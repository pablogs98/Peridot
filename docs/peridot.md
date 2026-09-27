# Peridot: An I/O-Extensible Execution Environment for WebAssembly Containers

![Rust workflow](https://github.com/pablogs98/peridot/actions/workflows/rust.yml/badge.svg)

Peridot runs a WebAssembly module under WASI preview 1 and lets you replace the implementation of
individual hostcalls with a **context**, a link in a chain that sits between the guest and the
real WASI implementation. A context can count bytes, rate-limit I/O, redirect `path_open` to
object storage, batch small writes, or anything else expressible at the hostcall boundary, without
the guest module being recompiled or even aware.

Which contexts run is decided **at run time**, in the configuration file. There are no cargo
feature flags.

---

## Artifact evaluation

If you are reviewing this work for Middleware '26, the guide you want is the
[README at the repository root](../README.md). It packages everything below
into one container image, so none of the requirements on this page need to be
installed.

This page is the reference manual: what Peridot is, how to configure it, how
to write a context, and how to deploy it under containerd.

---

## Requirements

| Requirement | Why |
|---|---|
| A recent stable Rust toolchain | The workspace tracks wasmtime 38, which sets a fairly recent floor. `rustup update` is the safe move. |
| `protoc` | `peridot-overseer-grpc` compiles its protobufs at build time. |
| A C toolchain and `cmake` | `aws-lc-sys`, pulled in by the AWS SDK that the `s3` and `batch` contexts use. |
| [wasi-sdk](https://github.com/WebAssembly/wasi-sdk) *(optional)* | Only to build the C guest modules under `c-wasm/`. Any `wasm32-wasip1` binary works. |

Install them with:

```bash
sudo apt install protobuf-compiler build-essential cmake pkg-config
```

Peridot is developed and tested on Ubuntu.

Two workspace members are **not** part of the default build: `peridot-geds-ctx`, which does not
compile until you have IBM GEDS installed (see [GEDS](#geds)), and `containerd-shim-peridot`,
which you only need for the deployment described in
[Deploying with containerd](#deploying-with-containerd). Both are still full workspace members,
so build either explicitly with `-p`.

---

## Build

```bash
cargo build --release
```

That produces `target/release/peridot`. Every built-in context is compiled in; none of them is
selected until a configuration file names it.

To build one of the two non-default members, name it:

```bash
cargo build --release -p containerd-shim-peridot
```

---

## Quick start: running a module with the `counter` context

`counter` is the reference context. It intercepts `fd_write` and `fd_pwrite`, adds up the bytes
the guest writes, and publishes the running total as a `written_bytes` metric. It needs no
external services, which makes it a good first run.

### 1. Get a WebAssembly module

Any `wasm32-wasip1` binary will do. To build one of the C examples in this repository:

```bash
$WASI_SDK_PATH/bin/clang --target=wasm32-wasip1 \
    c-wasm/helloworld/main.c -o hello.wasm
```

On wasi-sdk releases older than 24, the target triple is spelled `wasm32-wasi`.

The `c-wasm/` tree also carries CMake projects, buildable with the wasi-sdk toolchain file:

```bash
cmake -S c-wasm -B c-wasm/build \
    -DCMAKE_TOOLCHAIN_FILE=$WASI_SDK_PATH/share/cmake/wasi-sdk.cmake
cmake --build c-wasm/build
```

A Rust guest works just as well. Run `rustup target add wasm32-wasip1`, then
`cargo build --target wasm32-wasip1`.

### 2. Write a configuration file

```yaml
# config.yaml
args: []

contexts:
  - counter

io:
  demand: 150.0
  max_bandwidth: 300.0

cpu:
  demand: 75.0
  utilization: 0.85
```

### 3. Run it

```bash
./target/release/peridot hello.wasm config.yaml --log-level debug
```

The module path comes first, the configuration file second. At `debug` you will see the context
report each intercepted write, and the running total when the chain is torn down:

```
[INFO  peridot] Context chain: ["counter"]
[DEBUG peridot_counter_ctx] fd_write wrote 14 bytes, total 14
[INFO  peridot_counter_ctx] Total bytes written: 14
```

Peridot preopens the **module's own directory** for the guest as `.`, so a module that opens
`data.txt` gets the file sitting next to the `.wasm`.

---

## Command line

```
peridot <MODULE_PATH> <CONFIG_PATH> [OPTIONS]
```

| Argument | Required | Description |
|---|---|---|
| `<MODULE_PATH>` | ✔️ | The WebAssembly module to run |
| `<CONFIG_PATH>` | ✔️ | The YAML configuration file |
| `--log-level, -l` | ✖️ | Log verbosity, default `info`. Ignored if `RUST_LOG` is set. |

Contexts are configured in the YAML file.

---

## Configuration

```yaml
# Optional. Without it, no metrics thread is started.
overseer_address: "http://127.0.0.1:50051"

# Arguments passed through to the guest as argv.
args:
  - "foo"
  - "bar"

# Optional. Plugin libraries to load before the chain is built.
plugins:
  - ./target/release/libperidot_trace_plugin.so

# Optional. The context chain, outermost first.
# Absent or empty means plain WASI with no interception at all.
contexts:
  - counter
  - name: token
    config:
      max_bandwidth: 200

# Required.
io:
  demand: 150.0
  max_bandwidth: 300.0

cpu:
  demand: 75.0
  utilization: 0.85
```

`args`, `io` and `cpu` are required; `overseer_address`, `plugins` and `contexts` may be omitted.

Each entry in `contexts` is either a bare name or a `name` with its own `config` block. Contexts
that take no settings can always be written as a bare name.

---

## Contexts

A context chain composes like middleware. With `contexts: [counter, token]`, `counter` is
outermost: it sees each hostcall first and passes it to `token`, which passes it to the real WASI
implementation. Results travel back out in reverse. A context overrides only the hostcalls it
cares about; the other forty-odd forward automatically.

| Name | Replaces | Settings |
|---|---|---|
| `counter` | `fd_write`, `fd_pwrite`: counts bytes written | None |
| `token` | `fd_read`/`fd_write`/`fd_pread`/`fd_pwrite`: rate limiting | `max_bandwidth` (defaults to `io.max_bandwidth`) |
| `s3` | `path_open`/`fd_write`/`fd_close` on `s3://` paths | credentials from the environment |
| `geds` | the same, on `geds://` paths | see [GEDS](#geds) below |
| `batch` | `fd_write`: accumulates files, uploads them as one parquet object | `batch_size`, `bucket` (required) |
| `syscall-batching` | `path_open` and the write path: coalesces small writes | `num_writes` (default `1`, which disables batching) |

All of these except `geds` are built into the binary. `geds` ships as a plugin, because it cannot
be built without a native IBM GEDS installation.

Metrics a context publishes (`counter`'s `written_bytes`, for instance) are collected at startup
and reported to the overseer. With no `overseer_address` configured, no metrics thread runs and
contexts fall back to logging.

---

## GEDS

[IBM GEDS](https://github.com/IBM/GEDS) is a distributed ephemeral data store. The `geds` context
redirects guest file operations on `geds://` paths to it, encrypting object contents with AES-256-GCM
on the way out.

It is set up differently from every other context: GEDS is a native library that must be present on
the machine, so `peridot-geds-ctx` is **not** compiled into the binary and is **not** a member of
the default build. It is loaded at run time as a plugin instead. A machine without GEDS therefore
neither builds nor links it.

### 1. Install GEDS

Follow the upstream build instructions at <https://github.com/IBM/GEDS>. Install it to a prefix
whose Rust bindings land where the manifest expects them. With the default prefix `/usr/local`,
that is `/usr/local/rust`.

### 2. Enable the dependency

`crates/contexts/peridot-geds-ctx/Cargo.toml` ships with the binding commented out so the crate
stays buildable on machines without GEDS. Uncomment it, and correct the path if you installed
somewhere other than `/usr/local`:

```toml
geds_rs = { path = "/usr/local/rust" }
```

### 3. Build the plugin

```bash
cargo build --release -p peridot-cli -p peridot-geds-ctx
```

> Build the plugin **and** the runtime in one Cargo invocation. Contexts cross the library
> boundary as ordinary Rust types, which have no stable ABI, so both sides must link one identical
> build of `peridot`. See [Plugins](#plugins-adding-a-context-without-rebuilding-peridot) for what
> happens if they do not.

This produces `target/release/libperidot_geds_ctx.so`.

### 4. Set the environment

The context reads its S3 backing-store credentials from the environment and panics if they are
missing:

| Variable | Required | Meaning |
|---|---|---|
| `S3_ENDPOINT` | ✔️ | Endpoint of the object store GEDS caches from |
| `S3_ACCESS_KEY` | ✔️ | Access key for that store |
| `S3_SECRET_KEY` | ✔️ | Secret key for that store |
| `GEDS_CIPHER_KEY` | ✖️ | Hex-encoded 32-byte AES-256 key |

> **Set `GEDS_CIPHER_KEY` if you intend to read the data back.** When it is unset the context
> generates a fresh random key at startup, so objects written by one run cannot be decrypted by
> the next. Generate one with `openssl rand -hex 32`.

### 5. Point the configuration at it

```yaml
args: []

plugins:
  - ./target/release/libperidot_geds_ctx.so

contexts:
  - geds

io:
  demand: 150.0
  max_bandwidth: 300.0

cpu:
  demand: 75.0
  utilization: 0.85
```

The guest then addresses objects as `geds://<bucket>/<key>`. An ordinary `open`, `write`, `read`
or `unlink` on such a path is serviced by GEDS instead of the filesystem. Paths that do not start
with `geds://` fall through to the next context untouched, so a module can mix both freely.

If GEDS fails to start, the context logs the error and falls back to the local filesystem.

---

## Plugins: adding a context without rebuilding Peridot

A context can live outside the runtime, as a `cdylib` loaded at startup. Point `plugins:` at the
library and its contexts become usable in `contexts:`:

```yaml
plugins:
  - ./target/release/libperidot_trace_plugin.so
contexts:
  - name: trace
    config: { prefix: "T" }
```

To write one, add `crate-type = ["cdylib"]` to the plugin crate, implement the context as usual,
and declare the entry points:

```rust
peridot::export_peridot_plugin! {
    "trace" => crate::factory,
}
```

`crates/plugins/peridot-trace-plugin` is a complete working example. It is deliberately not a
dependency of the `peridot` binary, which is the point.

> **Build the plugin and the runtime in the same Cargo invocation:**
>
> ```bash
> cargo build --release -p peridot-cli -p peridot-trace-plugin
> ```
>
> Contexts cross the boundary as ordinary Rust types (`Box<dyn DelegatingWasiCtx>`, boxed
> futures), which have no stable ABI, so both sides must link one *identical* build of `peridot`.
> Cargo resolves features per invocation, so building them separately can silently produce two
> different builds of `peridot` even from the same workspace and lockfile.
>
> Peridot checks two things before calling into a library and refuses to load on either mismatch:
> a version fingerprint (Peridot version, rustc version, target triple), and a type probe derived
> from `TypeId`, which additionally catches differing dependency versions, features, or a
> separately compiled `peridot`, none of which the version string can see.

---

## Deploying with containerd

Peridot ships a containerd shim, so a Wasm workload can run as an ordinary container under `ctr`,
`nerdctl` or Kubernetes and still get its context chain.

The shim is built on [runwasi](https://github.com/containerd/runwasi), the containerd project's
framework for Wasm shims, and follows its Wasmtime shim closely. It currently tracks a fork,
<https://github.com/miqalvarez/runwasi>, pinned in `crates/containerd-shim-peridot/Cargo.toml`.
runwasi handles the containerd plumbing: sandbox lifecycle, image pulling, layer handling and
compilation caching. Peridot supplies the `Sandbox` implementation that assembles the context
chain around the guest.

`crates/containerd-shim-peridot/README.md` has the full walkthrough. What follows is the short
version.

### 1. Build and install the shim

```bash
cargo build --release -p containerd-shim-peridot
sudo cp target/release/containerd-shim-peridot-v1 /usr/local/bin/
```

containerd locates a shim by its binary name, so the file must keep the name
`containerd-shim-peridot-v1` and sit on containerd's `PATH`. That name maps to the runtime handler
`io.containerd.peridot.v1`.

### 2. Build the image

The shim runs OCI images only; a plain file path is not supported. Images follow the
[Wasm OCI artifact layout](https://tag-runtime.cncf.io/wgs/wasm/deliverables/wasm-oci-artifact/)
and carry two layers, told apart by media type:

| Layer | Media type |
|---|---|
| The Wasm module | `application/vnd.bytecodealliance.wasm.component.layer.v0+wasm` |
| The Peridot configuration | `application/vnd.peridot.image.layer.v1+json` |

The configuration layer holds the same YAML documented under [Configuration](#configuration),
despite the `+json` suffix on its media type. The shim writes it to `/peridot_conf.yaml` inside
the sandbox and reads it back from there.

Build the artifact with `oci-tar-builder`:

```bash
cargo install oci-tar-builder

oci-tar-builder --name wasi-helloworld \
                --repo localhost:5000 \
                --tag latest --module wasi-helloworld.wasm \
                --layer application/vnd.peridot.image.layer.v1+json=peridot-config.yaml \
                -o wasi-helloworld-oci.tar
```

Then import it into a registry. A local one is enough:

```bash
docker run -d -p 5000:5000 --name registry registry:2.7
regctl image import localhost:5000/wasi-helloworld:latest wasi-helloworld-oci.tar
```

### 3. Run it

```bash
sudo ctr run --rm --net-host --runtime=io.containerd.peridot.v1 \
    localhost:5000/wasi-helloworld:latest wasi-helloworld /wasi-helloworld.wasm
```

The final argument is the path of the module inside the image, so it follows the name given to
`--module`. The chain comes from the configuration layer, so the same `contexts:` list works
unchanged.

### Differences from the CLI

| | `peridot` CLI | containerd shim |
|---|---|---|
| Preopened directory | The module's own directory, as `.` | The host root, as `/` |
| Module source | A path on disk | An OCI image layer |
| Compilation | On every run | Precompiled and cached by containerd |
| `plugins:` | Paths on the host | Paths inside the container image |
| Metrics thread | Started only when `overseer_address` is set | Always attempted, failure logged as a warning |

Plugins work under the shim, but the shim runs inside the container, so a `plugins:` path is
resolved against the image's filesystem and the library has to be shipped in the image. Build it
against the shim rather than the CLI, since that is the binary it will be loaded into:

```bash
cargo build --release -p containerd-shim-peridot -p peridot-geds-ctx
```

Note also that the shim preopens the host root rather than a single directory, so a guest sees far
more of the filesystem than it does under the CLI.

Components are not supported, only modules.

---

## Writing a context

Implement `DelegatingWasiCtx` (from the `peridot` crate), overriding only the hostcalls you care
about. The default bodies forward everything else to the next context in the chain. Then expose a
`factory` function. To build it into the binary, register it in `build_registry` in `src/main.rs`
(and in `crates/containerd-shim-peridot/src/engine.rs` for the shim); to ship it separately, use
`export_peridot_plugin!` as above. See `crates/contexts/peridot-counter-ctx` for the reference
implementation.

Overrides receive `&mut GuestMemory<'_>`, a borrow of the guest's linear memory. Use the helpers in
`peridot::memory` (`payload`, `read_path`) rather than `to_vec`: they return a `Cow` that borrows
guest memory directly and only copies when the guest uses **shared** memory, where wiggle cannot
hand out a borrow safely.

Note that borrowing only pays off if the context consumes the bytes before the hostcall returns. A
context that defers work, such as a spawned upload or a buffer flushed later, must call
`into_owned()`, and that copy is unavoidable.

A context with in-flight background work should override `shutdown`, which runs after the guest's
`_start` returns, outermost link first. Flush there, then forward to the next link.

A context needing real host paths (to open files itself) gets the runtime's preopened directories
via `ContextConfig::preopens` and `ContextConfig::resolve`, since the descriptor-to-directory
mapping inside `WasiP1Ctx` is private to wasmtime.

---

## Licence

Peridot is released under the [Apache License 2.0](../LICENSE).

Some material in this repository is not ours to license and is redistributed
under its own terms; [THIRD-PARTY-NOTICES.md](../THIRD-PARTY-NOTICES.md) lists
it.
