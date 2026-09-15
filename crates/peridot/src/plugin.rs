use crate::conf::PeridotConfig;
use crate::context::DelegatingWasiCtx;
use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::ffi::CStr;
use std::future::Future;
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::pin::Pin;

/// Identifies the toolchain and Peridot build a plugin was compiled against.
///
/// Contexts cross a dynamic-library boundary as ordinary Rust types — `Box<dyn DelegatingWasiCtx>`
/// and boxed futures — which have no stable ABI. A plugin built with a different compiler or
/// against a different Peridot would not fail to load, it would corrupt memory. Comparing this
/// string before calling into a library turns that into a refusal.
pub const ABI_FINGERPRINT: &str = concat!(
    "peridot ",
    env!("CARGO_PKG_VERSION"),
    " / rustc ",
    env!("PERIDOT_RUSTC_VERSION"),
    " / ",
    env!("PERIDOT_TARGET_TRIPLE"),
);

/// [`ABI_FINGERPRINT`] with a trailing NUL, for returning across the C boundary.
pub const ABI_FINGERPRINT_C: &str = concat!(
    "peridot ",
    env!("CARGO_PKG_VERSION"),
    " / rustc ",
    env!("PERIDOT_RUSTC_VERSION"),
    " / ",
    env!("PERIDOT_TARGET_TRIPLE"),
    "\0",
);

/// A type private to this crate, used only as a probe for [`abi_type_probe`].
pub struct AbiToken;

/// A hash of the [`TypeId`](std::any::TypeId) of a Peridot-owned type.
///
/// Stronger than the version string: Cargo derives a crate's `-Cmetadata` from its name, version,
/// features, profile *and the metadata of every dependency*, and `TypeId` is derived from that.
/// So this value changes if the plugin was compiled against a different `peridot`, a different
/// `wasmtime`/`wiggle`, or a different feature set — mismatches the version string alone cannot
/// see, because Peridot's own version would not have moved.
pub fn abi_type_probe() -> u64 {
    use std::hash::{Hash, Hasher};
    // `DefaultHasher::new` is documented to use fixed keys, so this is deterministic.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::any::TypeId::of::<AbiToken>().hash(&mut hasher);
    std::any::TypeId::of::<crate::context::PeridotContext>().hash(&mut hasher);
    hasher.finish()
}

/// Symbol a plugin exports to report the ABI it was built against.
pub const ABI_SYMBOL: &[u8] = b"peridot_plugin_abi\0";

/// Symbol a plugin exports to report its [`abi_type_probe`] value.
pub const TYPE_PROBE_SYMBOL: &[u8] = b"peridot_plugin_type_probe\0";

/// Symbol a plugin exports to add its contexts to the registry.
pub const REGISTER_SYMBOL: &[u8] = b"peridot_plugin_register\0";

/// Defines the entry points that make a `cdylib` loadable by Peridot.
///
/// Invoke once at the crate root, naming each context the plugin provides:
///
/// ```ignore
/// peridot::export_peridot_plugin! {
///     "trace" => crate::factory,
/// }
/// ```
///
/// The crate must be built with `crate-type = ["cdylib"]`, and — because the types crossing the
/// boundary have no stable ABI — from the same workspace and `Cargo.lock` as the runtime that
/// will load it, so both link one identical build of `peridot`.
#[macro_export]
macro_rules! export_peridot_plugin {
    ($($name:literal => $factory:path),+ $(,)?) => {
        /// Reports the ABI this plugin was compiled against. Checked before registration.
        #[no_mangle]
        pub extern "C" fn peridot_plugin_abi() -> *const ::std::os::raw::c_char {
            $crate::plugin::ABI_FINGERPRINT_C.as_ptr() as *const ::std::os::raw::c_char
        }

        /// Reports this plugin's view of Peridot's type identities. Checked before registration.
        #[no_mangle]
        pub extern "C" fn peridot_plugin_type_probe() -> u64 {
            $crate::plugin::abi_type_probe()
        }

        /// Adds this plugin's contexts to the runtime's registry.
        #[no_mangle]
        pub extern "C" fn peridot_plugin_register(registry: &mut $crate::plugin::PluginRegistry) {
            $( registry.register($name, $factory); )+
        }
    };
}

/// Guesses the cargo package name behind a plugin library, for use in error messages.
///
/// `target/release/libperidot_trace_plugin.so` -> `peridot-trace-plugin`. Only a hint: the file
/// may have been renamed, so it is never used for anything but the text of a suggestion.
fn package_hint(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.strip_prefix("lib").unwrap_or(stem).replace('_', "-"))
        .unwrap_or_else(|| "<plugin-package>".to_string())
}

/// A directory the runtime preopened for the guest.
///
/// The guest addresses these by file descriptor, but the fd-to-directory mapping lives inside
/// `WasiP1Ctx` and is not reachable from a context. The runtime knows it, so it passes it down.
#[derive(Debug, Clone)]
pub struct Preopen {
    /// Directory on the host.
    pub host: PathBuf,
    /// Name the guest sees, as given to `WasiCtxBuilder::preopened_dir`.
    pub guest: String,
}

/// What a context is handed when it is built.
///
/// Carries both the whole configuration and this context's own `config` block, so a context can
/// read global settings (`io.max_bandwidth`, the overseer address) as well as its own.
#[derive(Clone, Copy)]
pub struct ContextConfig<'a> {
    /// The whole Peridot configuration.
    pub global: &'a PeridotConfig,
    /// This context's own `config` block, or `Null` if the entry was a bare name.
    pub own: &'a serde_yaml::Value,
    /// Directories the runtime preopened for the guest.
    pub preopens: &'a [Preopen],
}

impl ContextConfig<'_> {
    /// Deserializes this context's own `config` block, falling back to `T::default()` when the
    /// entry carried no `config` at all.
    pub fn parse<T: DeserializeOwned + Default>(&self) -> Result<T> {
        if self.own.is_null() {
            return Ok(T::default());
        }
        serde_yaml::from_value(self.own.clone()).context("invalid context config block")
    }

    /// Resolves a path as the guest passed it to `path_open` into a host path.
    ///
    /// `path_open` supplies a directory fd plus a relative path, but the fd-to-directory mapping
    /// is private to wasmtime, so this matches on the path text instead: an absolute path is
    /// taken as-is, a path under a preopen's guest name resolves against that preopen, and a
    /// bare relative path resolves against the first preopen — which is how Peridot's runners
    /// are set up, with a single directory mounted at `"."`.
    ///
    /// Returns `None` when there are no preopens to resolve against.
    pub fn resolve(&self, guest_path: &str) -> Option<PathBuf> {
        let path = Path::new(guest_path);
        if path.is_absolute() {
            return Some(path.to_path_buf());
        }

        // Longest guest-name match first, so "/data/sub" beats "/data".
        let mut candidates: Vec<&Preopen> = self.preopens.iter().collect();
        candidates.sort_by_key(|p| std::cmp::Reverse(p.guest.len()));
        for preopen in &candidates {
            if let Ok(rest) = path.strip_prefix(&preopen.guest) {
                return Some(preopen.host.join(rest));
            }
        }

        self.preopens
            .first()
            .map(|preopen| preopen.host.join(path.strip_prefix("./").unwrap_or(path)))
    }
}

/// What a [`ContextFactory`] returns: construction may need to await (an S3 client, a remote
/// handshake), so the future is boxed rather than making every context construct synchronously.
pub type BoxedContextFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Box<dyn DelegatingWasiCtx>>> + Send + 'a>>;

/// Builds one link of a context chain, wrapping the link below it.
///
/// This is a plain `fn` pointer rather than a boxed closure so that the same signature can be
/// resolved from a dynamically loaded library later without changing any call site.
pub type ContextFactory =
    for<'a> fn(next: Box<dyn DelegatingWasiCtx>, config: ContextConfig<'a>) -> BoxedContextFuture<'a>;

/// Maps the context names a user writes in `config.yaml` to the code that builds them.
///
/// Factories arrive from two places: built-in contexts are registered directly by the runtime at
/// startup, and out-of-tree ones are resolved from a `cdylib` by [`load_library`]. Either way
/// *selection* is a runtime lookup, so which contexts run is a property of the configuration file
/// rather than of how the binary was built.
///
/// # Plugins are Rust-ABI, not C-ABI
///
/// A plugin hands back a `Box<dyn DelegatingWasiCtx>` whose hostcalls are `#[async_trait]` boxed
/// futures. Those are ordinary Rust types with no stable layout, so a plugin and the runtime that
/// loads it must be **built from this workspace in a single cargo invocation** — the same
/// compiler, the same `Cargo.lock`, the same resolved features:
///
/// ```text
/// cargo build --release -p peridot-cli -p <plugin-package>
/// ```
///
/// Building them separately can silently produce two different builds of `peridot`, and calling
/// across that boundary is undefined behaviour rather than a load error. [`load_library`] checks
/// for it and refuses rather than letting it happen; the checks cannot make a mismatched plugin
/// work.
///
/// [`load_library`]: PluginRegistry::load_library
#[derive(Default)]
pub struct PluginRegistry {
    factories: HashMap<String, ContextFactory>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, name: &str, factory: ContextFactory) -> &mut Self {
        self.factories.insert(name.to_string(), factory);
        self
    }

    /// Loads a plugin library and registers the contexts it provides, returning their names.
    ///
    /// # Safety
    ///
    /// Loading executes the library's initialisers and calls into it. The library must be a
    /// genuine Peridot plugin built from the same workspace and toolchain as this runtime; the
    /// ABI check below catches the common mismatches but cannot make an arbitrary file safe.
    ///
    /// The library is deliberately never unloaded: the contexts it builds contain its code and
    /// vtables, and they outlive this registry, so unloading would leave them dangling.
    pub unsafe fn load_library(&mut self, path: &Path) -> Result<Vec<String>> {
        let library = libloading::Library::new(path)
            .with_context(|| format!("failed to load plugin {}", path.display()))?;

        let abi: libloading::Symbol<extern "C" fn() -> *const c_char> = library
            .get(ABI_SYMBOL)
            .with_context(|| {
                format!(
                    "{} does not export `peridot_plugin_abi`; is it built with \
                     `peridot::export_peridot_plugin!`?",
                    path.display()
                )
            })?;
        let reported = CStr::from_ptr(abi())
            .to_str()
            .context("plugin reported a non-UTF-8 ABI fingerprint")?;
        if reported != ABI_FINGERPRINT {
            return Err(anyhow!(
                "plugin {} was built against a different Peridot ABI.\n  plugin:  {}\n  runtime: {}\n\
                 Rebuild both from this workspace in one cargo invocation:\n    \
                 cargo build --release -p peridot-cli -p {}",
                path.display(),
                reported,
                ABI_FINGERPRINT,
                package_hint(path)
            ));
        }

        // The version string catches a different compiler or Peridot release; this catches a
        // plugin built against different dependency versions or features, where that string
        // would be identical.
        let probe: libloading::Symbol<extern "C" fn() -> u64> = library
            .get(TYPE_PROBE_SYMBOL)
            .with_context(|| format!("{} does not export `peridot_plugin_type_probe`", path.display()))?;
        let reported_probe = probe();
        let expected_probe = abi_type_probe();
        if reported_probe != expected_probe {
            return Err(anyhow!(
                "plugin {} links a different build of Peridot or its dependencies \
                 (type probe {reported_probe:#x}, runtime {expected_probe:#x}).\n\
                 The version fingerprint matched, so this is a differing dependency version or \
                 feature set. Build both from this workspace in one cargo invocation so they \
                 share one Cargo.lock:\n    cargo build --release -p peridot-cli -p {}",
                path.display(),
                package_hint(path)
            ));
        }

        let register: libloading::Symbol<extern "C" fn(&mut PluginRegistry)> = library
            .get(REGISTER_SYMBOL)
            .with_context(|| format!("{} does not export `peridot_plugin_register`", path.display()))?;
        let register = *register;

        let before: Vec<String> = self.factories.keys().cloned().collect();
        register(self);
        let added: Vec<String> = self
            .factories
            .keys()
            .filter(|name| !before.contains(name))
            .cloned()
            .collect();

        // Kept resident for the life of the process; see the safety note above.
        std::mem::forget(library);
        Ok(added)
    }

    /// Names available to `config.contexts`, sorted for stable error messages.
    pub fn registered(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.factories.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    /// Assembles the chain named by `config.contexts` on top of `base`.
    ///
    /// The list reads outermost-first, like middleware: `[counter, token]` puts `counter`
    /// closest to the guest, so it sees a hostcall before `token` does and sees the result after.
    /// Building therefore walks the list in reverse, wrapping `base` from the inside out.
    pub async fn build_chain(
        &self,
        base: Box<dyn DelegatingWasiCtx>,
        config: &PeridotConfig,
        preopens: &[Preopen],
    ) -> Result<Box<dyn DelegatingWasiCtx>> {
        let mut chain = base;
        for spec in config.contexts.iter().rev() {
            let name = spec.name();
            let factory = self.factories.get(name).ok_or_else(|| {
                anyhow!(
                    "unknown context {:?}; registered contexts are {:?}",
                    name,
                    self.registered()
                )
            })?;
            let ctx_config = ContextConfig {
                global: config,
                own: spec.config(),
                preopens,
            };
            chain = factory(chain, ctx_config)
                .await
                .with_context(|| format!("failed to build context {name:?}"))?;
        }
        Ok(chain)
    }
}
