use async_trait::async_trait;
use peridot::context::{DelegatingWasiCtx, PeridotContext};
use std::collections::HashMap;
use std::io::IoSlice;
use std::ops::Deref;
use wasmtime_wasi::p1::types::{CiovecArray, Error, Fd, Size};
use wasmtime_wasi::p1::WasiP1Ctx;
use wasmtime_wasi_io::IoView;
use wiggle::{GuestError, GuestMemory, GuestPtr};

pub struct PeridotSyscallBatchingCtx {
    inner: PeridotContext,
    num_writes: usize,
    io_slices_buffer: HashMap<Fd, (Vec<Vec<u8>>, usize)>,
    writes: usize
}

impl PeridotSyscallBatchingCtx {
    pub fn new(inner: PeridotContext, num_writes: usize) -> Self {
        Self {
            inner,
            num_writes,
            io_slices_buffer: HashMap::new(),
            writes: 0,
        }
    }

    async fn flush_buffer(&mut self, fd: Fd) -> Result<(), Error> {
        if let Some((vecs, _)) = self.io_slices_buffer.remove(&fd) {
            // concatenate all buffered writes for this fd
            let data = vecs.concat();

            // get the file handle temporarily
            let file = {
                let wasi_ctx = self.inner.inner();
                let entry = wasi_ctx.table().get_file(fd)?;
                entry.file.clone()  // clone or take ownership
            };

            // write outside the borrow of table()
            let ioslices = [IoSlice::new(&data)];
            file.write_vectored(&ioslices).await?;
        }
        Ok(())
    }
}

fn first_non_empty_ciovec(
    memory: &GuestMemory<'_>,
    ciovs: CiovecArray,
) -> Result<GuestPtr<[u8]>, GuestError> {
    for iov in ciovs.iter() {
        let iov = memory.read(iov?)?;
        if iov.buf_len == 0 {
            continue;
        }
        return Ok(iov.buf.as_array(iov.buf_len));
    }
    Ok(GuestPtr::new((0, 0)))
}

#[async_trait]
impl DelegatingWasiCtx for PeridotSyscallBatchingCtx {
    
    fn inner(&mut self) -> &mut WasiP1Ctx {
        self.inner.inner()
    }
    
    async fn fd_datasync(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        let fd_u: u32 = fd.into();
        if fd_u > 3 && self.num_writes > 1 {
            self.flush_buffer(fd).await?;
        }
        self.inner.fd_datasync(mem, fd).await
    }
    
    async fn fd_write(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
    ) -> Result<Size, Error> {
        let fd_u: u32 = fd.into();

        if fd_u < 4 || self.num_writes <= 1 {
            return self.inner.fd_write(mem, fd, iovs).await;
        }

        let ciovec = first_non_empty_ciovec(mem, iovs)?;
        let vec = mem.to_vec(ciovec)?;
        let len = vec.len();

        self.io_slices_buffer
            .entry(fd)
            .or_insert((Vec::new(), 0))
            .0
            .push(vec);
        self.writes += 1;

        if self.writes == self.num_writes {
            self.flush_buffer(fd).await?;
            self.writes = 0;
        }

        Ok(u32::try_from(len)?)
    }

}

impl Deref for PeridotSyscallBatchingCtx {
    type Target = PeridotContext;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}