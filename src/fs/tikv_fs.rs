use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::anyhow;
use async_trait::async_trait;
use fuse::*;
use tikv_client::{Config, Key, TransactionClient};
use time::Timespec;

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
        }

        Ok(txn.rollback().await?)
    }

    async fn destroy(&self) {}

    async fn lookup(&self, parent: u64, name: OsString) -> Result<Entry> {
        unimplemented!()
    }

    async fn forget(&self, ino: u64, nlookup: u64) {
        unimplemented!()
    }

    async fn getattr(&self, ino: u64) -> Result<Attr> {
        let mut txn = self.client.begin().await?;
        let value = txn
            .get(ScopedKey::inode(ino).scoped())
            .await?
            .ok_or_else(|| FsError::InodeNotFound { inode: ino })?;
        txn.rollback().await?;
        Ok(Attr::new(Inode::from(value).0))
    }

    async fn setattr(
        &self,
        ino: u64,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<Timespec>,
        mtime: Option<Timespec>,
        fh: Option<u64>,
        crtime: Option<Timespec>,
        chgtime: Option<Timespec>,
        bkuptime: Option<Timespec>,
        flags: Option<u32>,
    ) -> Result<Attr> {
        unimplemented!()
    }

    async fn readlink(&self, ino: u64) -> Result<Data> {
        unimplemented!()
    }

    async fn mknod(&self, parent: u64, name: OsString, mode: u32, rdev: u32) -> Result<Entry> {
        unimplemented!()
    }

    async fn mkdir(&self, parent: u64, name: OsString, mode: u32) -> Result<Entry> {
        unimplemented!()
    }

    async fn unlink(&self, parent: u64, name: OsString) -> Result<()> {
        unimplemented!()
    }

    async fn rmdir(&self, parent: u64, name: OsString) -> Result<()> {
        unimplemented!()
    }

    async fn symlink(&self, parent: u64, name: OsString, link: PathBuf) -> Result<Entry> {
        unimplemented!()
    }

    async fn rename(
        &self,
        parent: u64,
        name: OsString,
        newparent: u64,
        newname: OsString,
    ) -> Result<()> {
        unimplemented!()
    }

    async fn link(&self, ino: u64, newparent: u64, newname: OsString) -> Result<Entry> {
        unimplemented!()
    }

    async fn open(&self, ino: u64, flags: u32) -> Result<Open> {
        unimplemented!()
    }

    async fn read(&self, ino: u64, fh: u64, offset: i64, size: u32) -> Result<Data> {
        unimplemented!()
    }

    async fn write(
        &self,
        ino: u64,
        fh: u64,
        offset: i64,
        data: Vec<u8>,
        flags: u32,
    ) -> Result<Write> {
        unimplemented!()
    }

    async fn flush(&self, ino: u64, fh: u64, lock_owner: u64) -> Result<()> {
        unimplemented!()
    }

    async fn release(
        &self,
        ino: u64,
        fh: u64,
        flags: u32,
        lock_owner: u64,
        flush: bool,
    ) -> Result<()> {
        unimplemented!()
    }

    async fn fsync(&self, ino: u64, fh: u64, datasync: bool) -> Result<()> {
        unimplemented!()
    }

    async fn opendir(&self, ino: u64, flags: u32) -> Result<Open> {
        unimplemented!()
    }

    async fn readdir(&self, ino: u64, fh: u64, offset: i64, reply: ReplyDirectory) {
        unimplemented!()
    }

    async fn releasedir(&self, ino: u64, fh: u64, flags: u32) -> Result<()> {
        unimplemented!()
    }

    async fn fsyncdir(&self, ino: u64, fh: u64, datasync: bool) -> Result<()> {
        unimplemented!()
    }

    async fn statfs(&self, ino: u64) -> Result<StatFs> {
        unimplemented!()
    }

    async fn setxattr(
        &self,
        ino: u64,
        name: OsString,
        value: Vec<u8>,
        flags: u32,
        position: u32,
    ) -> Result<()> {
        unimplemented!()
    }

    async fn getxattr(&self, ino: u64, name: OsString, size: u32) -> Result<Xattr> {
        unimplemented!()
    }

    async fn listxattr(&self, ino: u64, size: u32) -> Result<Xattr> {
        unimplemented!()
    }

    async fn removexattr(&self, ino: u64, name: OsString) -> Result<()> {
        unimplemented!()
    }

    async fn access(&self, ino: u64, mask: u32) -> Result<()> {
        unimplemented!()
    }

    async fn create(
        &self,
        parent: u64,
        name: OsString,
        mode: u32,
        flags: u32,
        uid: u32,
        gid: u32,
    ) -> Result<Create> {
        unimplemented!()
    }

    async fn getlk(
        &self,
        ino: u64,
        fh: u64,
        lock_owner: u64,
        start: u64,
        end: u64,
        typ: u32,
        pid: u32,
    ) -> Result<Lock> {
        unimplemented!()
    }

    async fn setlk(
        &self,
        ino: u64,
        fh: u64,
        lock_owner: u64,
        start: u64,
        end: u64,
        typ: u32,
        pid: u32,
        sleep: bool,
    ) -> Result<()> {
        unimplemented!()
    }

    async fn bmap(&self, ino: u64, blocksize: u32, idx: u64, reply: ReplyBmap) {
        unimplemented!()
    }
}
