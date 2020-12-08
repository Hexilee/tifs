use std::ffi::OsString;
use std::fmt::{self, Debug};
use std::mem::size_of;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use anyhow::anyhow;
use async_trait::async_trait;
use fuser::*;
use tikv_client::{Config, Key, TransactionClient};
use tracing::trace;

use super::async_fs::AsyncFileSystem;
use super::dir::Directory;
use super::error::{FsError, Result};
use super::file_handler::FileHub;
use super::inode::Inode;
use super::key::{ScopedKey, ROOT_INODE};
use super::reply::*;

pub struct TiFs {
    inode_next: AtomicU64,
    pd_endpoints: Vec<String>,
    config: Config,
    client: TransactionClient,

    hub: FileHub,
    // inode_cache: RwLock<LruCache<u64, Inode>>,
    // block_cache: RwLock<LruCache<ScopedKey, Vec<u8>>>,
    // dir_cache: RwLock<LruCache<u64, Directory>>,
}

impl TiFs {
    pub const SCAN_LIMIT: u32 = 1 << 10;
    pub const BLOCK_SIZE: usize = 1 << 12;
    pub const BLOCK_CACHE: usize = 1 << 25;
    pub const DIR_CACHE: usize = 1 << 24;
    pub const INODE_CACHE: usize = 1 << 24;

    pub async fn construct<S>(pd_endpoints: Vec<S>, cfg: Config) -> anyhow::Result<Self>
    where
        S: Clone + Into<String>,
    {
        Ok(TiFs {
            inode_next: AtomicU64::new(ROOT_INODE),
            pd_endpoints: pd_endpoints.clone().into_iter().map(Into::into).collect(),
            config: cfg.clone(),
            client: TransactionClient::new_with_config(pd_endpoints, cfg)
                .await
                .map_err(|err| anyhow!("{}", err))?,

            hub: FileHub::new(),
            // inode_cache: RwLock::new(LruCache::new(Self::INODE_CACHE / size_of::<Inode>())),
            // block_cache: RwLock::new(LruCache::new(Self::BLOCK_CACHE / Self::BLOCK_SIZE)),
            // dir_cache: RwLock::new(LruCache::new(Self::DIR_CACHE / Self::BLOCK_SIZE)),
        })
    }

    async fn read_dir(&self, ino: u64) -> Result<Directory> {
        let dir = self.getattr(ino).await?.attr;
        let upper = (dir.size + Self::BLOCK_SIZE as u64 - 1) / Self::BLOCK_SIZE as u64;
        let mut txn = self.client.begin().await?;
        let entries = txn
            .scan(ScopedKey::block_range(ino, 0..upper), upper as u32)
            .await?;
        let data = entries.fold(Vec::with_capacity(dir.size as usize), |mut data, block| {
            data.extend(block.into_value());
            data
        });
        txn.rollback().await?;
        Directory::deserialize(&data)
    }

    async fn read_inode(&self, ino: u64) -> Result<FileAttr> {
        let mut txn = self.client.begin().await?;
        let value = txn
            .get(ScopedKey::inode(ino).scoped())
            .await?
            .ok_or_else(|| FsError::InodeNotFound { inode: ino })?;
        txn.rollback().await?;
        Ok(Inode::deserialize(&value)?.0)
    }

    async fn save_inode(&self, mut inode: Inode) -> Result<()> {
        inode.0.mtime = SystemTime::now();

        let mut txn = self.client.begin().await?;
        txn.put(ScopedKey::inode(inode.0.ino).scoped(), inode.serialize()?)
            .await?;
        Ok(txn.commit().await?)
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
    async fn init(&self) -> Result<()> {
        trace!("initializing tifs on {:?} ...", self.pd_endpoints);
        let mut txn = self.client.begin().await?;
        let mut start_inode = ROOT_INODE;
        let mut max_key: Option<Key> = None;
        loop {
            let keys: Vec<Key> = txn
                .scan_keys(ScopedKey::inode(start_inode).scoped().., Self::SCAN_LIMIT)
                .await?
                .collect();
            if keys.is_empty() {
                break;
            }
            max_key = Some(keys[keys.len() - 1].clone());
            start_inode += Self::SCAN_LIMIT as u64
        }

        if let Some(key) = max_key {
            self.inode_next
                .store(ScopedKey::from(key).key() + 1, Ordering::Relaxed)
        } else {
            let root = Inode(FileAttr {
                ino: FUSE_ROOT_ID,
                size: 0,
                blocks: 0,
                atime: SystemTime::now(),
                mtime: SystemTime::now(),
                ctime: SystemTime::now(),
                crtime: SystemTime::now(),
                kind: FileType::Directory,
                perm: 0o777,
                nlink: 2,
                uid: 0,
                gid: 0,
                rdev: 0,
                blksize: Self::BLOCK_SIZE as u32,
                padding: 0,
                flags: 0,
            });
            txn.put(ScopedKey::root(), root.serialize()?).await?;
        }

        Ok(txn.commit().await?)
    }

    #[tracing::instrument]
    async fn lookup(&self, parent: u64, name: OsString) -> Result<Entry> {
        // TODO: use cache
        let dir = self.read_dir(parent).await?;
        let file = dir.get(&name).ok_or_else(|| FsError::FileNotFound {
            file: name.to_string_lossy().to_string(),
        })?;
        Ok(Entry::new(self.read_inode(file.ino).await?, 0))
    }

    #[tracing::instrument]
    async fn getattr(&self, ino: u64) -> Result<Attr> {
        Ok(Attr::new(self.read_inode(ino).await?))
    }

    async fn readdir(&self, ino: u64, _fh: u64, offset: i64) -> Result<Dir> {
        let directory = self.read_dir(ino).await?;
        let mut dir = Dir::offset(offset as usize);
        for (i, item) in directory.into_map().into_values().into_iter().enumerate() {
            if i >= offset as usize {
                dir.push(item)
            }
        }
        Ok(dir)
    }

    async fn open(&self, ino: u64, flags: i32) -> Result<Open> {
        // TODO: deal with flags
        let fh = self.hub.make(ino).await;
        Ok(Open::new(fh, flags as u32))
    }

    async fn read(
        &self,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
    ) -> Result<Data> {
        let mut attr = self.read_inode(ino).await?;
        let handler = self
            .hub
            .get(ino, fh)
            .await
            .ok_or_else(|| FsError::FhNotFound { fh })?;
        let mut cursor = handler.cursor().await;
        *cursor = ((*cursor) as i64 + offset) as usize;

        let target = (attr.size as usize).min(*cursor + size as usize);

        let mut data = Vec::with_capacity(target - *cursor);

        let start_block = *cursor / Self::BLOCK_SIZE;
        let end_block = (target + Self::BLOCK_SIZE - 1) / Self::BLOCK_SIZE;

        let mut txn = self.client.begin().await?;
        let pairs = txn
            .scan(
                ScopedKey::block_range(ino, (start_block as u64)..(end_block as u64)),
                (end_block - start_block) as u32,
            )
            .await?;

        for (i, pair) in pairs.enumerate() {
            let value = pair.into_value();
            let mut slice = value.as_slice();
            slice = match i {
                0 => &slice[(start_block % Self::BLOCK_SIZE)..],
                n if (n + 1) * Self::BLOCK_SIZE > data.capacity() => {
                    &slice[..(data.capacity() % Self::BLOCK_SIZE)]
                }
                _ => slice,
            };

            data.extend(slice);
        }

        txn.rollback().await?;
        *cursor = target;
        attr.atime = SystemTime::now();
        self.save_inode(attr.into()).await?;

        Ok(Data::new(data))
    }
}
