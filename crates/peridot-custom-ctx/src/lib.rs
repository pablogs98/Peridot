pub mod clock_ctx;
pub mod token_ctx;
mod geds_ctx;

#[macro_export]
macro_rules! define_wasi {
    ($async_mode:tt $($bounds:tt)*) => {

    use wasmtime::Linker;

    pub fn add_to_linker<T, U>(
        linker: &mut Linker<T>,
        get_cx: impl Fn(&mut T) -> &mut U + Send + Sync + Copy + 'static,
    ) -> anyhow::Result<()>
        where U: Send
                    + wasi_common::snapshots::preview_1::wasi_snapshot_preview1::WasiSnapshotPreview1,
            $($bounds)*
    {
        snapshots::preview_1::add_wasi_snapshot_preview1_to_linker(linker, get_cx)?;
        Ok(())
    }

    pub mod snapshots {
        pub mod preview_1 {
            wiggle::wasmtime_integration!({
                // The wiggle code to integrate with lives here:
                target: wasi_common::snapshots::preview_1,
                witx: ["$CARGO_MANIFEST_DIR/witx/wasi_snapshot_preview1.witx"],
                errors: { errno => trappable Error },
                $async_mode: *
            });
        }
    }
}}