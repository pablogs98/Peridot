use log::{error, info};
use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::time::Duration;
use async_trait::async_trait;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::put_object::{PutObjectError, PutObjectOutput};
use aws_sdk_s3::primitives::ByteStream;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use wasmtime_wasi::p1::types::{Error, CiovecArray, Fd, Fdflags, Lookupflags, Oflags, Rights, Size};
use wasmtime_wasi::p1::{WasiP1Ctx};
use wiggle::{GuestError, GuestMemory, GuestPtr};
use peridot::context::{DelegatingWasiCtx, PeridotContext};

#[derive(Default)]
struct S3Descriptors {
    used: BTreeMap<u32, String>,
    free: Vec<u32>,
}

impl Deref for S3Descriptors {
    type Target = BTreeMap<u32, String>;

    fn deref(&self) -> &Self::Target {
        &self.used
    }
}

impl DerefMut for S3Descriptors {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.used
    }
}

impl S3Descriptors {
    fn new() -> S3Descriptors {
        let descriptors = Self::default();
        descriptors
    }

    /// Returns next descriptor number, which was never assigned
    fn unused(&self) -> Result<u32, ()> {
        match self.last_key_value() {
            Some((fd, _)) => {
                if let Some(fd) = fd.checked_add(1) {
                    return Ok(fd);
                }
                if self.len() == u32::MAX as usize {
                    return Err(());
                }
                // TODO: Optimize
                Ok((8192..u32::MAX)
                    .rev()
                    .find(|fd| !self.contains_key(fd))
                    .expect("failed to find an unused file descriptor"))
            }
            None => Ok(0),
        }
    }

    fn remove(&mut self, fd: Fd) -> Option<String> {
        let fd = fd.into();
        let desc = self.used.remove(&fd)?;
        self.free.push(fd);
        Some(desc)
    }

    /// Pushes the [Descriptor] returning corresponding number.
    /// This operation will try to reuse numbers previously removed via [`Self::remove`]
    /// and rely on [`Self::unused`] if no free numbers are recorded
    fn push(&mut self, desc: String) -> Result<u32, ()> {
        let fd = if let Some(fd) = self.free.pop() {
            fd
        } else {
            self.unused()?
        };
        assert!(self.insert(fd, desc).is_none());
        Ok(fd)
    }
}

pub struct PeridotS3Ctx {
    base: PeridotContext,
    s3: aws_sdk_s3::Client,
    s3_descriptors: S3Descriptors,
    futures: Vec<JoinHandle<(Result<PutObjectOutput, SdkError<PutObjectError>>, Duration, Instant)>>,
}

impl PeridotS3Ctx {
    pub async fn new(base: PeridotContext) -> Self {
        std::env::set_var("WASMTIME_LOG", "wasmtime_wasi=trace");
        let config = aws_config::load_from_env().await;
        let client = aws_sdk_s3::Client::new(&config);
        Self {
            base,
            s3: client,
            s3_descriptors: S3Descriptors::new(),
            futures: Vec::new(),
        }
    }

    fn spawn_s3_upload(&mut self, bucket: String, key: String, body: ByteStream) {
        let client = self.s3.clone();
        let handle = tokio::spawn(async move {
            let start = Instant::now();
            let result = client.put_object()
                .bucket(bucket)
                .key(key)
                .body(body)
                .send()
                .await;
            let elapsed = start.elapsed();
            (result, elapsed, start)
        });
        self.futures.push(handle);
    }
}

#[async_trait]
impl DelegatingWasiCtx for PeridotS3Ctx {
    fn inner(&mut self) -> &mut WasiP1Ctx {
        self.base.inner()
    }

    async fn fd_close(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        if self.s3_descriptors.contains_key(&u32::from(fd)) {
            self.s3_descriptors.remove(fd);
            return Ok(());
        }
        self.base.fd_close(mem, fd).await
    }

    async fn fd_write(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
    ) -> Result<Size, Error> {
        println!("fd_write called");
        if self.s3_descriptors.contains_key(&u32::from(fd)) {
            let s3_file = self.s3_descriptors.get(&u32::from(fd)).unwrap();
            let buf = first_non_empty_ciovec(mem, iovs)?;
            let buf = mem.to_vec(buf)?;
            let len = buf.len();
            let body = aws_sdk_s3::primitives::ByteStream::from(buf);
            let bucket = s3_file.split("//").nth(1).unwrap().split("/").next().unwrap();
            let key = s3_file.split("/").skip(3).collect::<Vec<&str>>().join("/");
            self.spawn_s3_upload(bucket.to_string(), key.to_string(), body);
            return Ok(u32::try_from(len)?);
        }
        self.base.fd_write(mem, fd, iovs).await
    }

    async fn path_open(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, dirflags: Lookupflags, path: GuestPtr<str>, oflags: Oflags, fs_rights_base: Rights, fs_rights_inheriting: Rights, fdflags: Fdflags) -> Result<Fd, Error> {
        println!("path_open called");
        let str_path = read_string(mem, path)?;
        if let Some(start) = str_path.find("s3://") {
            let str_path = &str_path[start..];
            let fd = self.s3_descriptors.push(str_path.to_string()).unwrap();
            let fd = fd.into();
            return Ok(fd);
        }
        self.base.path_open(mem, fd, dirflags, path, oflags, fs_rights_base, fs_rights_inheriting, fdflags).await
    }
}

fn read_string(memory: &GuestMemory, ptr: GuestPtr<str>) -> Result<String, GuestError> {
    Ok(memory.as_cow_str(ptr)?.into_owned())
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

impl PeridotS3Ctx {
    pub async fn drain_uploads(&mut self) {
        if self.futures.is_empty() {
            return;
        }

        info!("Waiting for {} pending S3 uploads...", self.futures.len());

        let mut total_elapsed = Duration::ZERO;
        let mut completed = 0;
        let mut first_start: Option<Instant> = None;
        let mut last_end: Option<Instant> = None;

        for handle in self.futures.drain(..) {
            match handle.await {
                Ok((Ok(_), elapsed, start)) => {
                    let end = start + elapsed;
                    info!("Upload completed in {:.2?}", elapsed);

                    // track earliest start
                    if first_start.map_or(true, |fst| start < fst) {
                        first_start = Some(start);
                    }

                    // track latest end
                    if last_end.map_or(true, |led| end > led) {
                        last_end = Some(end);
                    }

                    total_elapsed += elapsed;
                    completed += 1;
                }

                Ok((Err(e), _, _)) => {
                    error!("S3 upload failed: {:?}", e);
                }

                Err(join_err) => {
                    error!("Task join error: {:?}", join_err);
                }
            }
        }

        // Summary statistics
        if completed > 0 {
            let avg = total_elapsed / completed as u32;
            info!("Average S3 upload time: {:.2?}", avg);

            if let (Some(start), Some(end)) = (first_start, last_end) {
                let duration_sec = end.duration_since(start).as_secs_f64();
                if duration_sec > 0.0 {
                    let sps = completed as f64 / duration_sec;
                    info!("Avg. uploads per second: {:.2}", sps);
                }
            }
        }
    }
}


impl Deref for PeridotS3Ctx {
    type Target = WasiP1Ctx;

    fn deref(&self) -> &Self::Target {
        &self.base.inner
    }
}