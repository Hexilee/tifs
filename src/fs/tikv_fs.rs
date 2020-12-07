use std::ffi::OsString;
use std::fmt::{self, Debug};
use std::mem::size_of;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use anyhow::anyhow;
use async_std::sync::RwLock;
use async_trait::async_trait;
use fuser::*;
use lru::LruCache;
use tikv_client::{Config, Key, TransactionClient};
use tracing::trace;

use super::async_fs::AsyncFileSystem;
use super::dir::Directory;
use super::error::{FsError, Result};
use super::inode::Inode;
use super::key::{ScopedKey, ROOT_INODE};
use super::reply::*;

pub struct TiFs {
    inode_next: AtomicU64,
    pd_endpoints: Vec<String>,
    config: Config,
    client: TransactionClient,

    inode_cache: RwLock<LruCache<u64, Inode>>,
    block_cache: RwLock<LruCache<ScopedKey, Vec<u8>>>,
    dir_cache: RwLock<LruCache<u64, Directory>>,
}

impl TiFs {
    pub const SCAN_LIMIT: u32 = 1 << 10;
    pub const BLOCK_SIZE: usize = 1 << 12;
    pub const BLOCK_CACHE: usize = 1 << 30;
    pub const DIR_CACHE: usize = 1 << 29;
    pub const INODE_CACHE: usize = 1 << 29;

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

            inode_cache: RwLock::new(LruCache::new(Self::INODE_CACHE / size_of::<Inode>())),
            block_cache: RwLock::new(LruCache::new(Self::BLOCK_CACHE / Self::BLOCK_SIZE)),
            dir_cache: RwLock::new(LruCache::new(Self::DIR_CACHE / Self::BLOCK_SIZE)),
        })
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
        if let Some(dir) = self.dir_cache.read().await.get(&parent) {
            return match dir.get(&name) {
                None => Err(FsError::FileNotFound {
                    file: name.to_string_lossy().to_string(),
                }),
                Some(item) => {
                    let attr = self.getattr(item.ino).await?;
                    Ok(Entry::new(attr.attr, 0))
                }
            };
        }

        let dir = self.getattr(parent).await?.attr;
        if dir.size == 0 {
            return Err(FsError::FileNotFound {
                file: name.to_string_lossy().to_string(),
            });
        }
    }

    #[tracing::instrument]
    async fn getattr(&self, ino: u64) -> Result<Attr> {
        let mut txn = self.client.begin().await?;
        let value = txn
            .get(ScopedKey::inode(ino).scoped())
            .await?
            .ok_or_else(|| FsError::InodeNotFound { inode: ino })?;
        txn.rollback().await?;
        Ok(Attr::new(Inode::deserialize(&value)?.0))
    }

    async fn readdir(&self, _ino: u64, _fh: u64, offset: i64) -> Result<Dir> {
        Ok(Dir::offset(offset as usize))
    }
}
