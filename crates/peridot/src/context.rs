use crate::metrics::{MetricsProducer, MetricsSubscriber};
use async_trait::async_trait;
use std::ops::Deref;
use std::sync::{Arc, Mutex};
use wasmtime::Linker;
use wasmtime_wasi::p1::types::{Advice, CiovecArray, Clockid, Dircookie, Error, Event, Exitcode, Fd, Fdflags, Fdstat, Filedelta, Filesize, Filestat, Fstflags, IovecArray, Lookupflags, Oflags, Prestat, Riflags, Rights, Roflags, Sdflags, Siflags, Signal, Size, Subscription, Timestamp, Whence};
use wasmtime_wasi::p1::wasi_snapshot_preview1::WasiSnapshotPreview1;
use wasmtime_wasi::p1::{wasi_snapshot_preview1, WasiP1Ctx};
use wiggle::{anyhow, GuestMemory, GuestPtr};

/// Declares the full `wasi_snapshot_preview1` hostcall surface once, and generates the three
/// pieces of boilerplate that have to stay in lockstep with it:
///
/// 1. the [`DelegatingWasiCtx`] trait, whose default bodies forward to the next link in the chain,
/// 2. [`PeridotContext`]'s implementation, which terminates the chain at the real [`WasiP1Ctx`],
/// 3. [`WasiWrapper`]'s [`WasiSnapshotPreview1`] implementation, which is what wiggle links.
///
/// Every hostcall takes `&mut GuestMemory<'_>` as its first argument, so the macro inserts it
/// rather than making each declaration repeat it.
macro_rules! peridot_hostcalls {
    (
        sync_calls: { $( fn $sn:ident( $($sa:ident : $st:ty),* $(,)? ) -> $sr:ty; )* }
        async_calls: { $( fn $an:ident( $($aa:ident : $at:ty),* $(,)? ) -> $ar:ty; )* }
    ) => {
        /// One link in a chain of WASI contexts.
        ///
        /// A context overrides the hostcalls it cares about and lets the default bodies forward
        /// everything else to [`next`](DelegatingWasiCtx::next). The chain is built outermost-first
        /// by [`PluginRegistry`](crate::plugin::PluginRegistry) and always bottoms out in a
        /// [`PeridotContext`], which is the only link that returns `None` from `next`.
        ///
        /// Overrides receive `&mut GuestMemory<'_>` — a borrow of the guest's linear memory — so
        /// reading and writing guest buffers costs nothing beyond a bounds check. See
        /// [`GuestMemory::as_slice`] for the zero-copy path and [`GuestMemory::to_vec`] for the
        /// copying one.
        #[async_trait]
        pub trait DelegatingWasiCtx: Send {
            /// The next link down the chain, or `None` for a base context that services
            /// hostcalls itself. A base context must override every hostcall; the default
            /// bodies panic if one reaches a link with no `next`.
            fn next(&mut self) -> Option<&mut dyn DelegatingWasiCtx>;

            /// The runtime's own WASI context, at the base of the chain.
            ///
            /// Escape hatch for contexts that need the runtime-managed state the hostcall
            /// signatures do not carry — the file descriptor table, most importantly. Prefer
            /// overriding hostcalls; reach for this only when what you need genuinely lives
            /// inside [`WasiP1Ctx`]. Returns `None` only if the chain has no base, which
            /// cannot happen for a chain built by the registry.
            fn wasi_p1(&mut self) -> Option<&mut WasiP1Ctx> {
                self.next()?.wasi_p1()
            }

            /// Metrics this link contributes to the publisher. Collected once at startup by
            /// [`collect_metrics`].
            fn metrics_producers(&self) -> Vec<Arc<dyn MetricsProducer + Send + Sync>> {
                Vec::new()
            }

            /// Objects in this link that want to be notified of metric updates.
            fn metrics_subscribers(&self) -> Vec<Arc<Mutex<dyn MetricsSubscriber + Send + Sync>>> {
                Vec::new()
            }

            /// Runs after the guest's `_start` returns, outermost link first. Contexts with
            /// in-flight background work (uploads, buffered batches) should flush it here.
            async fn shutdown(&mut self) {
                if let Some(next) = self.next() {
                    next.shutdown().await;
                }
            }

            $(
                fn $sn(&mut self, mem: &mut GuestMemory<'_>, $($sa: $st),*) -> $sr {
                    match self.next() {
                        Some(next) => next.$sn(mem, $($sa),*),
                        None => panic!(
                            "base context did not override {}", stringify!($sn)
                        ),
                    }
                }
            )*

            $(
                async fn $an(&mut self, mem: &mut GuestMemory<'_>, $($aa: $at),*) -> $ar {
                    match self.next() {
                        Some(next) => next.$an(mem, $($aa),*).await,
                        None => panic!(
                            "base context did not override {}", stringify!($an)
                        ),
                    }
                }
            )*
        }

        #[async_trait]
        impl DelegatingWasiCtx for PeridotContext {
            fn next(&mut self) -> Option<&mut dyn DelegatingWasiCtx> {
                None
            }

            fn wasi_p1(&mut self) -> Option<&mut WasiP1Ctx> {
                Some(&mut self.inner)
            }

            $(
                fn $sn(&mut self, mem: &mut GuestMemory<'_>, $($sa: $st),*) -> $sr {
                    WasiSnapshotPreview1::$sn(&mut self.inner, mem, $($sa),*)
                }
            )*

            $(
                async fn $an(&mut self, mem: &mut GuestMemory<'_>, $($aa: $at),*) -> $ar {
                    WasiSnapshotPreview1::$an(&mut self.inner, mem, $($aa),*).await
                }
            )*
        }

        #[async_trait]
        impl WasiSnapshotPreview1 for WasiWrapper {
            $(
                fn $sn(&mut self, mem: &mut GuestMemory<'_>, $($sa: $st),*) -> $sr {
                    self.ctx.$sn(mem, $($sa),*)
                }
            )*

            $(
                async fn $an(&mut self, mem: &mut GuestMemory<'_>, $($aa: $at),*) -> $ar {
                    self.ctx.$an(mem, $($aa),*).await
                }
            )*
        }
    };
}

peridot_hostcalls! {
    sync_calls: {
        fn args_get(argv: GuestPtr<GuestPtr<u8>>, argv_buf: GuestPtr<u8>) -> Result<(), Error>;
        fn args_sizes_get() -> Result<(Size, Size), Error>;
        fn environ_get(environ: GuestPtr<GuestPtr<u8>>, environ_buf: GuestPtr<u8>) -> Result<(), Error>;
        fn environ_sizes_get() -> Result<(Size, Size), Error>;
        fn clock_res_get(id: Clockid) -> Result<Timestamp, Error>;
        fn clock_time_get(id: Clockid, precision: Timestamp) -> Result<Timestamp, Error>;
        fn fd_allocate(fd: Fd, offset: Filesize, len: Filesize) -> Result<(), Error>;
        fn fd_fdstat_set_flags(fd: Fd, flags: Fdflags) -> Result<(), Error>;
        fn fd_fdstat_set_rights(fd: Fd, fs_rights_base: Rights, fs_rights_inheriting: Rights) -> Result<(), Error>;
        fn fd_prestat_get(fd: Fd) -> Result<Prestat, Error>;
        fn fd_prestat_dir_name(fd: Fd, path: GuestPtr<u8>, path_len: Size) -> Result<(), Error>;
        fn fd_renumber(fd: Fd, to: Fd) -> Result<(), Error>;
        fn fd_tell(fd: Fd) -> Result<Filesize, Error>;
        fn proc_exit(rval: Exitcode) -> anyhow::Error;
        fn proc_raise(sig: Signal) -> Result<(), Error>;
        fn sched_yield() -> Result<(), Error>;
        fn random_get(buf: GuestPtr<u8>, buf_len: Size) -> Result<(), Error>;
        fn sock_accept(fd: Fd, flags: Fdflags) -> Result<Fd, Error>;
        fn sock_recv(fd: Fd, ri_data: IovecArray, ri_flags: Riflags) -> Result<(Size, Roflags), Error>;
        fn sock_send(fd: Fd, si_data: CiovecArray, si_flags: Siflags) -> Result<Size, Error>;
        fn sock_shutdown(fd: Fd, how: Sdflags) -> Result<(), Error>;
    }
    async_calls: {
        fn fd_advise(fd: Fd, offset: Filesize, len: Filesize, advice: Advice) -> Result<(), Error>;
        fn fd_close(fd: Fd) -> Result<(), Error>;
        fn fd_datasync(fd: Fd) -> Result<(), Error>;
        fn fd_fdstat_get(fd: Fd) -> Result<Fdstat, Error>;
        fn fd_filestat_get(fd: Fd) -> Result<Filestat, Error>;
        fn fd_filestat_set_size(fd: Fd, size: Filesize) -> Result<(), Error>;
        fn fd_filestat_set_times(fd: Fd, atim: Timestamp, mtim: Timestamp, fst_flags: Fstflags) -> Result<(), Error>;
        fn fd_pread(fd: Fd, iovs: IovecArray, offset: Filesize) -> Result<Size, Error>;
        fn fd_pwrite(fd: Fd, iovs: CiovecArray, offset: Filesize) -> Result<Size, Error>;
        fn fd_read(fd: Fd, iovs: IovecArray) -> Result<Size, Error>;
        fn fd_readdir(fd: Fd, buf: GuestPtr<u8>, buf_len: Size, cookie: Dircookie) -> Result<Size, Error>;
        fn fd_seek(fd: Fd, offset: Filedelta, whence: Whence) -> Result<Filesize, Error>;
        fn fd_sync(fd: Fd) -> Result<(), Error>;
        fn fd_write(fd: Fd, iovs: CiovecArray) -> Result<Size, Error>;
        fn path_create_directory(fd: Fd, path: GuestPtr<str>) -> Result<(), Error>;
        fn path_filestat_get(fd: Fd, flags: Lookupflags, path: GuestPtr<str>) -> Result<Filestat, Error>;
        fn path_filestat_set_times(fd: Fd, flags: Lookupflags, path: GuestPtr<str>, atim: Timestamp, mtim: Timestamp, fst_flags: Fstflags) -> Result<(), Error>;
        fn path_link(old_fd: Fd, old_flags: Lookupflags, old_path: GuestPtr<str>, new_fd: Fd, new_path: GuestPtr<str>) -> Result<(), Error>;
        fn path_open(fd: Fd, dirflags: Lookupflags, path: GuestPtr<str>, oflags: Oflags, fs_rights_base: Rights, fs_rights_inheriting: Rights, fdflags: Fdflags) -> Result<Fd, Error>;
        fn path_readlink(fd: Fd, path: GuestPtr<str>, buf: GuestPtr<u8>, buf_len: Size) -> Result<Size, Error>;
        fn path_remove_directory(fd: Fd, path: GuestPtr<str>) -> Result<(), Error>;
        fn path_rename(fd: Fd, old_path: GuestPtr<str>, new_fd: Fd, new_path: GuestPtr<str>) -> Result<(), Error>;
        fn path_symlink(old_path: GuestPtr<str>, fd: Fd, new_path: GuestPtr<str>) -> Result<(), Error>;
        fn path_unlink_file(fd: Fd, path: GuestPtr<str>) -> Result<(), Error>;
        fn poll_oneoff(in_: GuestPtr<Subscription>, out: GuestPtr<Event>, nsubscriptions: Size) -> Result<Size, Error>;
    }
}

/// The base of every context chain: services hostcalls from the real WASI preview 1 context.
pub struct PeridotContext {
    pub inner: WasiP1Ctx,
}

impl PeridotContext {
    pub fn new(inner: WasiP1Ctx) -> Self {
        Self { inner }
    }

    /// Convenience for starting a chain, since callers always want it boxed.
    pub fn boxed(inner: WasiP1Ctx) -> Box<dyn DelegatingWasiCtx> {
        Box::new(Self::new(inner))
    }
}

/// Adapts a context chain to the trait wiggle actually links against; this is the `Store`
/// data type.
///
/// Deliberately concrete rather than generic over the chain's type. Contexts are selected at
/// run time, so the store always holds a `Box<dyn DelegatingWasiCtx>` anyway, and a `?Sized`
/// type parameter here forces callers to prove region bounds the trait object cannot express
/// in every position (notably inside `-> impl Future` in a trait).
pub struct WasiWrapper {
    pub ctx: Box<dyn DelegatingWasiCtx>,
}

impl WasiWrapper {
    pub fn new(ctx: Box<dyn DelegatingWasiCtx>) -> Self {
        Self { ctx }
    }
}

impl Deref for WasiWrapper {
    type Target = dyn DelegatingWasiCtx;

    fn deref(&self) -> &Self::Target {
        &*self.ctx
    }
}

/// Metrics contributed by every link in `chain`, walked outermost link first.
pub fn collect_metrics(
    chain: &mut dyn DelegatingWasiCtx,
) -> (
    Vec<Arc<dyn MetricsProducer + Send + Sync>>,
    Vec<Arc<Mutex<dyn MetricsSubscriber + Send + Sync>>>,
) {
    fn walk(
        ctx: &mut dyn DelegatingWasiCtx,
        producers: &mut Vec<Arc<dyn MetricsProducer + Send + Sync>>,
        subscribers: &mut Vec<Arc<Mutex<dyn MetricsSubscriber + Send + Sync>>>,
    ) {
        producers.extend(ctx.metrics_producers());
        subscribers.extend(ctx.metrics_subscribers());
        if let Some(next) = ctx.next() {
            walk(next, producers, subscribers);
        }
    }

    let mut producers = Vec::new();
    let mut subscribers = Vec::new();
    walk(chain, &mut producers, &mut subscribers);
    (producers, subscribers)
}

pub fn add_to_linker_async<T: Send + 'static + WasiSnapshotPreview1>(
    linker: &mut Linker<T>,
    f: impl Fn(&mut T) -> &mut T + Copy + Send + Sync + 'static,
) -> anyhow::Result<()> {
    wasi_snapshot_preview1::add_to_linker(linker, f)
}
