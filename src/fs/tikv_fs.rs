use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use anyhow::anyhow;
use async_trait::async_trait;
use fuser::*;
use tikv_client::{Config, Key, TransactionClient};

use super::async_fs::AsyncFileSystem;
use super::error::{FsError, Result};
use super::inode::Inode;
use super::key::{ScopedKey, ROOT_INODE};
use super::reply::*;

pub struct TiFs {
    inode_next: AtomicU64,
    pd_endpoints: Vec<String>,
    config: Config,
    client: TransactionClient,
}

impl TiFs {
    pub const SCAN_LIMIT: u32 = 1 << 10;
    pub const BLOCK_SIZE: usize = 1 << 12;

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
        })
    }
}

#[async_trait]
impl AsyncFileSystem for TiFs {
    async fn init(&self) -> Result<()> {
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

    async fn getattr(&self, ino: u64) -> Result<Attr> {
        let mut txn = self.client.begin().await?;
        let value = txn
            .get(ScopedKey::inode(ino).scoped())
            .await?
            .ok_or_else(|| FsError::InodeNotFound { inode: ino })?;
        txn.rollback().await?;
        Ok(Attr::new(Inode::deserialize(&value)?.0))
    }
}
