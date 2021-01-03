use std::ffi::OsString;
use std::fmt::{self, Debug};
use std::future::Future;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::SystemTime;

use anyhow::anyhow;
use async_trait::async_trait;
use fuser::*;
use libc::{SEEK_CUR, SEEK_END, SEEK_SET};
use tikv_client::{Config, TransactionClient};
use tracing::{debug, info, instrument, trace};

use super::async_fs::AsyncFileSystem;
use super::dir::Directory;
use super::error::{FsError, Result};
use super::file_handler::{FileHandler, FileHub};
use super::key::ROOT_INODE;
use super::mode::{as_file_perm, make_mode};
use super::reply::get_time;
use super::reply::{Attr, Create, Data, Dir, DirItem, Entry, Lseek, Open, Write};
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
                trace!("transaction committed");
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
            .map(Into::into)
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
                let root_inode = txn.read_inode(ROOT_INODE).await;
                if let Err(FsError::InodeNotFound { inode: _ }) = root_inode {
                    let attr = txn
                        .mkdir(
                            0,
                            OsString::default(),
                            make_mode(FileType::Directory, 0o777),
                            gid,
                            uid,
                        )
                        .await?;
                    debug!("make root directory {:?}", &attr);
                    Ok(())
                } else {
                    root_inode.map(|_| ())
                }
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
    async fn setattr(
        &self,
        ino: u64,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        ctime: Option<SystemTime>,
        fh: Option<u64>,
        crtime: Option<SystemTime>,
        chgtime: Option<SystemTime>,
        bkuptime: Option<SystemTime>,
        flags: Option<u32>,
    ) -> Result<Attr> {
        self.with_txn(move |_, txn| {
            Box::pin(async move {
                let mut attr = txn.read_inode_for_update(ino).await?;
                match uid {
                    Some(uid_) => attr.uid = uid_,
                    _ => (),
                }
                match gid {
                    Some(gid_) => attr.gid = gid_,
                    _ => (),
                }
                // TODO: how to deal with size, fh, chgtime, bkuptime?
                match atime {
                    Some(atime_) => match atime_ {
                        TimeOrNow::SpecificTime(t) => attr.atime = t,
                        TimeOrNow::Now => attr.atime = SystemTime::now(),
                    },
                    _ => attr.atime = SystemTime::now(),
                }
                match mtime {
                    Some(mtime_) => match mtime_ {
                        TimeOrNow::SpecificTime(t) => attr.mtime = t,
                        TimeOrNow::Now => attr.mtime = SystemTime::now(),
                    },
                    _ => attr.mtime = SystemTime::now(),
                }
                match ctime {
                    Some(t) => attr.ctime = t,
                    _ => (),
                }
                match crtime {
                    Some(t) => attr.crtime = t,
                    _ => (),
                }
                match flags {
                    Some(f) => attr.flags = f,
                    _ => (),
                }
                txn.save_inode(&mut attr).await?;
                Ok(Attr {
                    time: get_time(),
                    attr: attr.into(),
                })
            })
        })
        .await
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
        Ok(Entry::new(attr.into(), 0))
    }

    #[tracing::instrument]
    async fn rmdir(&self, parent: u64, raw_name: OsString) -> Result<()> {
        self.with_txn(move |_, txn| {
            Box::pin(async move {
                let mut dir = txn.read_dir_for_update(parent).await?;
                let name = raw_name.to_string_lossy();
                let item = dir.remove(&*name).ok_or_else(|| FsError::FileNotFound {
                    file: name.to_string(),
                })?;

                let dir_contents = txn.read_dir(item.ino).await?;
                if let Some(_) = dir_contents
                    .iter()
                    .find(|(key, _)| key.as_str() != "." && key.as_str() != "..")
                {
                    let name_str = name.to_string();
                    debug!("dir({}) not empty", &name_str);
                    return Err(FsError::DirNotEmpty { dir: name_str });
                }

                txn.save_dir(parent, &dir).await?;
                txn.remove_inode(item.ino).await?;
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
        Ok(Entry::new(attr.into(), 0))
    }

    #[tracing::instrument]
    async fn access(&self, ino: u64, mask: i32) -> Result<()> {
        let attr = self.read_inode(ino).await?;
        Ok(())
    }

    async fn create(
        &self,
        uid: u32,
        gid: u32,
        parent: u64,
        name: OsString,
        mode: u32,
        umask: u32,
        flags: i32,
    ) -> Result<Create> {
        let entry = self.mknod(parent, name, mode, gid, uid, umask, 0).await?;
        let open = self.open(entry.stat.ino, flags).await?;
        Ok(Create::new(
            entry.stat,
            entry.generation,
            open.fh,
            open.flags,
        ))
    }

    async fn lseek(&self, ino: u64, fh: u64, offset: i64, whence: i32) -> Result<Lseek> {
        let file_handler = self.read_fh(ino, fh).await?;
        let mut cursor = file_handler.cursor().await;
        let inode = self.read_inode(ino).await?;
        let target_cursor = match whence {
            SEEK_SET => offset,
            SEEK_CUR => *cursor as i64 + offset,
            SEEK_END => inode.size as i64 + offset,
            _ => return Err(FsError::UnknownWhence { whence }),
        };

        if target_cursor < 0 {
            return Err(FsError::InvalidOffset {
                ino: inode.ino,
                offset: target_cursor,
            });
        }

        *cursor = target_cursor as usize;
        Ok(Lseek::new(target_cursor))
    }

    async fn release(
        &self,
        ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
    ) -> Result<()> {
        self.hub
            .close(ino, fh)
            .await
            .ok_or_else(|| FsError::FhNotFound { fh })
            .map(|_| ())
    }

    /// Create a hard link.
    async fn link(&self, ino: u64, newparent: u64, newname: OsString) -> Result<Entry> {
        self.with_txn(move |_, txn| {
            Box::pin(async move {
                let mut attr = txn.read_inode_for_update(ino).await?;
                let mut dir = txn.read_dir(newparent).await?;
                let name = newname.to_string_lossy();

                if let Some(item) = dir.add(DirItem {
                    ino,
                    name: name.to_string(),
                    typ: attr.kind,
                }) {
                    return Err(FsError::FileExist { file: item.name });
                }

                txn.save_dir(newparent, &dir).await?;
                attr.nlink += 1;
                txn.save_inode(&mut attr.into()).await?;
                Ok(Entry::new(attr.into(), 0))
            })
        })
        .await
    }

    async fn unlink(&self, parent: u64, raw_name: OsString) -> Result<()> {
        self.with_txn(move |_, txn| {
            Box::pin(async move {
                let mut dir = txn.read_dir_for_update(parent).await?;
                let name = raw_name.to_string_lossy();
                let item = dir.remove(&*name).ok_or_else(|| FsError::FileNotFound {
                    file: name.to_string(),
                })?;

                txn.save_dir(parent, &dir).await?;
                let mut attr = txn.read_inode_for_update(item.ino).await?;
                attr.nlink -= 1;
                txn.save_inode(&mut attr.into()).await?;

                Ok(())
            })
        })
        .await
    }

    async fn rename(
        &self,
        parent: u64,
        raw_name: OsString,
        newparent: u64,
        new_raw_name: OsString,
        _flags: u32,
    ) -> Result<()> {
        self.with_txn(move |_, txn| {
            Box::pin(async move {
                let mut dir = txn.read_dir_for_update(parent).await?;
                let name = raw_name.to_string_lossy();

                match dir.remove(&*name) {
                    None => Err(FsError::FileNotFound {
                        file: name.to_string(),
                    }),
                    Some(mut item) => {
                        txn.save_dir(newparent, &dir).await?;

                        let mut new_dir = if parent == newparent {
                            dir
                        } else {
                            txn.read_dir_for_update(newparent).await?
                        };

                        item.name = new_raw_name.to_string_lossy().to_string();
                        if let Some(old_item) = new_dir.add(item) {
                            return Err(FsError::FileExist {
                                file: old_item.name,
                            });
                        }

                        txn.save_dir(newparent, &new_dir).await?;
                        Ok(())
                    }
                }
            })
        })
        .await
    }

    async fn symlink(
        &self,
        gid: u32,
        uid: u32,
        parent: u64,
        name: OsString,
        link: PathBuf,
    ) -> Result<Entry> {
        self.with_txn(move |_, txn| {
            Box::pin(async move {
                let mut attr = txn
                    .make_inode(parent, name, make_mode(FileType::Symlink, 0o777), gid, uid)
                    .await?;

                attr.size = txn
                    .write_data(attr.ino, 0, link.as_os_str().as_bytes().to_vec())
                    .await? as u64;

                txn.save_inode(&mut attr).await?;
                Ok(Entry::new(attr.into(), 0))
            })
        })
        .await
    }

    async fn readlink(&self, ino: u64) -> Result<Data> {
        self.with_txn(move |_, txn| {
            Box::pin(async move { Ok(Data::new(txn.read_data(ino, 0, None).await?)) })
        })
        .await
    }

    async fn fallocate(
        &self,
        ino: u64,
        fh: u64,
        offset: i64,
        length: i64,
        _mode: i32,
    ) -> Result<()> {
        self.with_txn(move |_, txn| {
            Box::pin(async move {
                let mut inode = txn.read_inode_for_update(ino).await?;
                txn.fallocate(&mut inode, offset, length).await
            })
        })
        .await?;
        let handler = self.read_fh(ino, fh).await?;
        *handler.cursor().await = (offset + length) as usize;
        Ok(())
    }
}
