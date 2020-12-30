use std::ffi::OsString;
use std::fmt::{self, Debug};
use std::future::Future;
use std::pin::Pin;

use anyhow::anyhow;
use async_trait::async_trait;
use fuser::*;
use tikv_client::{Config, TransactionClient};
use tracing::{debug, info, instrument, trace};

use super::async_fs::AsyncFileSystem;
use super::dir::Directory;
use super::error::{FsError, Result};
use super::file_handler::{FileHandler, FileHub};
use super::mode::{as_file_perm, make_mode};
use super::reply::*;
use super::transaction::Txn;

pub struct TiFs {
    pub pd_endpoints: Vec<String>,
    pub config: Config,
    pub client: TransactionClient,
    pub hub: FileHub,
    // inode_cache: RwLock<LruCache<u64, Inode>>,
    // block_cache: RwLock<LruCache<ScopedKey, Vec<u8>>>,
    // dir_cache: RwLock<LruCache<u64, Directory>>,
}

type BoxedFuture<'a, T> = Pin<Box<dyn 'a + Send + Future<Output = Result<T>>>>;

impl TiFs {
    pub const SCAN_LIMIT: u32 = 1 << 10;
    pub const BLOCK_SIZE: u64 = 1 << 12;
    pub const BLOCK_CACHE: usize = 1 << 25;
    pub const DIR_CACHE: usize = 1 << 24;
    pub const INODE_CACHE: usize = 1 << 24;

    #[instrument]
    pub async fn construct<S>(pd_endpoints: Vec<S>, cfg: Config) -> anyhow::Result<Self>
    where
        S: Clone + Debug + Into<String>,
    {
        let client = TransactionClient::new_with_config(pd_endpoints.clone(), cfg.clone())
            .await
            .map_err(|err| anyhow!("{}", err))?;
        info!("connected to pd endpoints: {:?}", pd_endpoints);
        Ok(TiFs {
            client,
            pd_endpoints: pd_endpoints.clone().into_iter().map(Into::into).collect(),
            config: cfg,
            hub: FileHub::new(),
            // inode_cache: RwLock::new(LruCache::new(Self::INODE_CACHE / size_of::<Inode>())),
            // block_cache: RwLock::new(LruCache::new(Self::BLOCK_CACHE / Self::BLOCK_SIZE)),
            // dir_cache: RwLock::new(LruCache::new(Self::DIR_CACHE / Self::BLOCK_SIZE)),
        })
    }

    async fn with_txn<F, T>(&self, f: F) -> Result<T>
    where
        T: 'static + Send,
        F: for<'a> FnOnce(&'a TiFs, &'a mut Txn) -> BoxedFuture<'a, T>,
    {
        let mut txn = Txn::begin_pessimistic(&self.client).await?;
        match f(self, &mut txn).await {
            Ok(v) => {
                txn.commit().await?;
                debug!("transaction committed");
                Ok(v)
            }
            Err(e) => {
                txn.rollback().await?;
                debug!("transaction rollbacked");
                Err(e)
            }
        }
    }

    async fn read_fh(&self, ino: u64, fh: u64) -> Result<FileHandler> {
        self.hub
            .get(ino, fh)
            .await
            .ok_or_else(|| FsError::FhNotFound { fh })
    }

    async fn read_data(&self, ino: u64, start: u64, chunk_size: Option<u64>) -> Result<Vec<u8>> {
        self.with_txn(move |_, txn| Box::pin(txn.read_data(ino, start, chunk_size)))
            .await
    }

    async fn clear_data(&self, ino: u64) -> Result<u64> {
        self.with_txn(move |_, txn| Box::pin(txn.clear_data(ino)))
            .await
    }

    async fn write_data(&self, ino: u64, start: u64, data: Vec<u8>) -> Result<usize> {
        self.with_txn(move |_, txn| Box::pin(txn.write_data(ino, start, data)))
            .await
    }

    async fn read_dir(&self, ino: u64) -> Result<Directory> {
        self.with_txn(move |_, txn| Box::pin(txn.read_dir(ino)))
            .await
    }

    async fn read_inode(&self, ino: u64) -> Result<FileAttr> {
        self.with_txn(move |_, txn| Box::pin(txn.read_inode(ino)))
            .await
    }

    async fn lookup_file(&self, parent: u64, name: OsString) -> Result<DirItem> {
        // TODO: use cache

        let dir = self.read_dir(parent).await?;
        let item = dir
            .get(&*name.to_string_lossy())
            .ok_or_else(|| FsError::FileNotFound {
                file: name.to_string_lossy().to_string(),
            })?
            .clone();

        debug!("get item({:?})", &item);
        Ok(item)
    }
}

impl Debug for TiFs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("tifs({:?})", self.pd_endpoints))
    }
}

#[async_trait]
impl AsyncFileSystem for TiFs {
    #[tracing::instrument]
    async fn init(&self, gid: u32, uid: u32) -> Result<()> {
        self.with_txn(move |fs, txn| {
            Box::pin(async move {
                info!("initializing tifs on {:?} ...", &fs.pd_endpoints);
                let attr = txn
                    .mkdir(
                        0,
                        OsString::default(),
                        make_mode(FileType::Directory, 0o777),
                        gid,
                        uid,
                    )
                    .await?;
                trace!("make root directory {:?}", &attr);
                Ok(())
            })
        })
        .await
    }

    #[tracing::instrument]
    async fn lookup(&self, parent: u64, name: OsString) -> Result<Entry> {
        // TODO: use cache

        let ino = self.lookup_file(parent, name).await?.ino;
        Ok(Entry::new(self.read_inode(ino).await?, 0))
    }

    #[tracing::instrument]
    async fn getattr(&self, ino: u64) -> Result<Attr> {
        Ok(Attr::new(self.read_inode(ino).await?))
    }

    #[tracing::instrument]
    async fn readdir(&self, ino: u64, _fh: u64, offset: i64) -> Result<Dir> {
        let directory = self.read_dir(ino).await?;
        let mut dir = Dir::offset(offset as usize);
        for (item) in directory
            .into_map()
            .into_values()
            .into_iter()
            .skip(offset as usize)
        {
            dir.push(item)
        }
        debug!("read directory {:?}", &dir);
        Ok(dir)
    }

    #[tracing::instrument]
    async fn open(&self, ino: u64, flags: i32) -> Result<Open> {
        // TODO: deal with flags
        let fh = self.hub.make(ino).await;
        Ok(Open::new(fh, flags as u32))
    }

    #[tracing::instrument]
    async fn read(
        &self,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
    ) -> Result<Data> {
        let handler = self.read_fh(ino, fh).await?;
        let mut cursor = handler.cursor().await;
        *cursor = ((*cursor) as i64 + offset) as usize;
        let data = self
            .read_data(ino, *cursor as u64, Some(size as u64))
            .await?;
        *cursor += data.len();
        Ok(Data::new(data))
    }

    #[tracing::instrument]
    async fn write(
        &self,
        ino: u64,
        fh: u64,
        offset: i64,
        data: Vec<u8>,
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
    ) -> Result<Write> {
        let handler = self.read_fh(ino, fh).await?;
        let mut cursor = handler.cursor().await;
        *cursor = (*cursor as i64 + offset) as usize;
        let data_len = data.len();
        let _ = self.write_data(ino, *cursor as u64, data).await?;
        *cursor += data_len;
        Ok(Write::new(*cursor as u32))
    }

    /// Create a directory.
    #[tracing::instrument]
    async fn mkdir(
        &self,
        parent: u64,
        name: OsString,
        mode: u32,
        gid: u32,
        uid: u32,
        _umask: u32,
    ) -> Result<Entry> {
        let attr = self
            .with_txn(move |_, txn| Box::pin(txn.mkdir(parent, name, mode, gid, uid)))
            .await?;
        Ok(Entry::new(attr, 0))
    }

    #[tracing::instrument]
    async fn rmdir(&self, parent: u64, raw_name: OsString) -> Result<()> {
        self.with_txn(move |_, txn| {
            Box::pin(async move {
                let mut dir = txn.read_dir(parent).await?;
                let name = raw_name.to_string_lossy();
                let item = dir.remove(&*name).ok_or_else(|| FsError::FileNotFound {
                    file: name.to_string(),
                })?;

                txn.save_dir(parent, &dir).await?;

                let dir_contents = txn.read_dir(item.ino).await?;
                if let Some(_) = dir_contents
                    .iter()
                    .find(|(key, _)| key.as_str() != "." && key.as_str() != "..")
                {
                    let name_str = name.to_string();
                    debug!("dir({}) not empty", &name_str);
                    return Err(FsError::DirNotEmpty { dir: name_str });
                }

                let mut attr = txn.read_inode(item.ino).await?;
                attr.nlink -= 1;
                txn.save_inode(&mut attr.into()).await?;
                Ok(())
            })
        })
        .await
    }

    #[tracing::instrument]
    async fn mknod(
        &self,
        parent: u64,
        name: OsString,
        mode: u32,
        gid: u32,
        uid: u32,
        _umask: u32,
        _rdev: u32,
    ) -> Result<Entry> {
        let attr = self
            .with_txn(move |_, txn| Box::pin(txn.make_inode(parent, name, mode, gid, uid)))
            .await?;
        Ok(Entry::new(attr, 0))
    }
}
