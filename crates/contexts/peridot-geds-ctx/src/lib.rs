use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit},
    Aes256Gcm,
    Key,
    Nonce,
};
use async_trait::async_trait;
use geds_rs::{GEDSFile, GEDS};
use hex::decode;
use log::{error, info};
use peridot::context::DelegatingWasiCtx;
use peridot::plugin::{BoxedContextFuture, ContextConfig};
use std::collections::BTreeMap;
use std::env;
use std::ops::{Deref, DerefMut};
use wasmtime_wasi::p1::types::{
    CiovecArray, Errno, Error, Fd, Fdflags, Filesize, IovecArray, Lookupflags, Oflags, Rights, Size,
};
use peridot::memory;
use wiggle::{GuestMemory, GuestPtr};

#[derive(Default)]
struct GEDSDescriptors {
    used: BTreeMap<u32, GEDSFile>,
    free: Vec<u32>,
}

impl Deref for GEDSDescriptors {
    type Target = BTreeMap<u32, GEDSFile>;

    fn deref(&self) -> &Self::Target {
        &self.used
    }
}

impl DerefMut for GEDSDescriptors {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.used
    }
}

impl GEDSDescriptors {
    fn new() -> GEDSDescriptors {
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

    fn remove(&mut self, fd: Fd) -> Option<GEDSFile> {
        let fd = fd.into();
        let desc = self.used.remove(&fd)?;
        self.free.push(fd);
        Some(desc)
    }

    /// Pushes the [Descriptor] returning corresponding number.
    /// This operation will try to reuse numbers previously removed via [`Self::remove`]
    /// and rely on [`Self::unused`] if no free numbers are recorded
    fn push(&mut self, desc: GEDSFile) -> Result<u32, ()> {
        let fd = if let Some(fd) = self.free.pop() {
            fd
        } else {
            self.unused()?
        };
        assert!(self.insert(fd, desc).is_none());
        Ok(fd)
    }
}

fn create_cipher() -> anyhow::Result<Aes256Gcm> {
    let key_bytes;
    let key = match env::var("GEDS_CIPHER_KEY") {
        Ok(key) => {
            key_bytes = decode(key)?;
            Key::<Aes256Gcm>::from_slice(&key_bytes)
        },
        Err(_) => &Aes256Gcm::generate_key().unwrap(),
    };
    let cipher = Aes256Gcm::new(key);
    Ok(cipher)
}

pub struct PeridotGEDSCtx {
    next: Box<dyn DelegatingWasiCtx>,
    geds: Option<GEDS>,
    geds_descriptors: GEDSDescriptors,
    cipher: Aes256Gcm,
}
/// Registry entry point. Registered under the name `geds`.
pub fn factory(next: Box<dyn DelegatingWasiCtx>, _config: ContextConfig<'_>) -> BoxedContextFuture<'_> {
    Box::pin(async move { Ok(Box::new(PeridotGEDSCtx::new(next)) as Box<dyn DelegatingWasiCtx>) })
}

impl PeridotGEDSCtx {
    pub fn new(next: Box<dyn DelegatingWasiCtx>) -> Self {
        let mut config = GEDS::get_default_config();
        config.pub_sub_enabled = true;
        config.cache_objects_from_s3 = true;
        let geds = GEDS::new(&config);
        let mut opt = None;
        if let Err(e) = geds.start() {
            error!(
                "Error starting GEDS: {}. Falling back to local filesystem.",
                e
            );
        } else {
            if let Err(e) = geds.register_object_store_config(
                "geds-default",
                std::env::var("S3_ENDPOINT").unwrap().as_str(),
                std::env::var("S3_ACCESS_KEY").unwrap().as_str(),
                std::env::var("S3_SECRET_KEY").unwrap().as_str(),
            ) {
                error!("Error registering GEDS S3 config: {}", e);
            } else {
                info!("GEDS S3 config registered");
            }
            opt = Some(geds);
        }

        info!("Initialization complete!");
        Self {
            next,
            geds: opt,
            geds_descriptors: GEDSDescriptors::new(),
            cipher: create_cipher().unwrap()
        }
    }
}

#[async_trait]
impl DelegatingWasiCtx for PeridotGEDSCtx {
    fn next(&mut self) -> Option<&mut dyn DelegatingWasiCtx> {
        Some(&mut *self.next)
    }

    async fn fd_close(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        if self.geds_descriptors.contains_key(&u32::from(fd)) {
            self.geds_descriptors.remove(fd);
            self.geds.as_ref().unwrap().relocate(true);
            return Ok(());
        }
        self.next.fd_close(mem, fd).await
    }

    async fn fd_datasync(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        if self.geds_descriptors.contains_key(&u32::from(fd)) {
            self.geds.as_ref().unwrap().relocate(true);
            return Ok(());
        }
        self.next.fd_datasync(mem, fd).await
    }

    async fn fd_pread(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: IovecArray,
        offset: Filesize,
    ) -> Result<Size, Error> {
        if self.geds_descriptors.contains_key(&u32::from(fd)) {
            let geds_file = self.geds_descriptors.get(&u32::from(fd)).unwrap();
            let mut buf: Vec<u8> = vec![0; iovs.len() as usize];
            let len = buf.len();
            return match geds_file.read(&mut buf, offset.try_into().unwrap(), len) {
                Ok(size) => Ok(u32::try_from(size)?),
                Err(e) => {
                    error!("Error reading file: {}", e);
                    Err(Errno::Badf.into())
                }
            };
        }
        self.next.fd_pread(mem, fd, iovs, offset).await
    }

    async fn fd_pwrite(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
        offset: Filesize,
    ) -> Result<Size, Error> {
        if self.geds_descriptors.contains_key(&u32::from(fd)) {
            let geds_file = self.geds_descriptors.get(&u32::from(fd)).unwrap();
            // Consumed by encrypt before this call returns, so it can borrow guest memory.
            let buf = memory::payload(mem, iovs)?;

            let nonce = Aes256Gcm::generate_nonce().unwrap();
            let encrypted_buf = self.cipher.encrypt(&nonce, buf.as_ref()).unwrap();

            return match geds_file.write(&encrypted_buf, 0, encrypted_buf.len()) {
                Ok(()) => Ok(u32::try_from(encrypted_buf.len())?),
                Err(_) => Err(Errno::Fault.into()),
            };
        }
        self.next.fd_pwrite(mem, fd, iovs, offset).await
    }

    async fn fd_write(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
    ) -> Result<Size, Error> {
        if self.geds_descriptors.contains_key(&u32::from(fd)) {
            let geds_file = self.geds_descriptors.get(&u32::from(fd)).unwrap();
            // Consumed by encrypt before this call returns, so it can borrow guest memory.
            let buf = memory::payload(mem, iovs)?;

            let nonce = Aes256Gcm::generate_nonce().unwrap();
            let encrypted_buf = self.cipher.encrypt(&nonce, buf.as_ref()).unwrap();

            return match geds_file.write(&encrypted_buf, 0, encrypted_buf.len()) {
                Ok(()) => Ok(u32::try_from(encrypted_buf.len())?),
                Err(_) => Err(Errno::Fault.into()),
            };
        }
        self.next.fd_write(mem, fd, iovs).await
    }

    async fn path_open(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        dirflags: Lookupflags,
        path: GuestPtr<str>,
        oflags: Oflags,
        fs_rights_base: Rights,
        fs_rights_inheriting: Rights,
        fdflags: Fdflags,
    ) -> Result<Fd, Error> {
        let str_path = memory::read_path(mem, path)?;

        if let Some(start) = str_path.find("geds://") {
            let str_path = &str_path[start + 7..]; // Extrae lo que hay después de "geds://"

            let mut parts = str_path.splitn(2, '/'); // Divide solo en 2 partes (bucket y el resto)
            let bucket = parts.next().unwrap_or_default();
            let key = parts.next().unwrap_or_default();

            let result;
            if oflags.contains(Oflags::CREAT) {
                result = self.geds.as_ref().unwrap().create(bucket, &key, true);
            } else {
                result = self.geds.as_ref().unwrap().open(bucket, &key);
            }

            return match result {
                Ok(file) => {
                    let fd = self.geds_descriptors.push(file).unwrap();
                    Ok(fd.into())
                }
                Err(e) => {
                    error!("Could not open GEDSFile: {}", e);
                    Err(Errno::Noent.into())
                }
            };
        }
        self.next
            .path_open(
                mem,
                fd,
                dirflags,
                path,
                oflags,
                fs_rights_base,
                fs_rights_inheriting,
                fdflags,
            )
            .await
    }

    async fn path_unlink_file(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        path: GuestPtr<str>,
    ) -> Result<(), Error> {
        let str_path = memory::read_path(mem, path)?;
        if str_path.contains("geds://") {
            let path = str_path.replace("geds://", "");
            let bucket = path.split("/").next().unwrap();
            let key = path.split("/").skip(1).collect::<Vec<&str>>().join("/");
            let result = self.geds.as_ref().unwrap().delete_object(bucket, &key);
            return match result {
                Ok(_) => Ok(()),
                Err(e) => {
                    error!("Could not delete GEDSFile: {}", e);
                    Err(Errno::Noent.into())
                }
            };
        }
        self.next.path_unlink_file(mem, fd, path).await
    }
}

peridot::export_peridot_plugin! {
    "geds" => crate::factory,
}
