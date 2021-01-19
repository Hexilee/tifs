use std::ffi::OsString;
use std::fmt::{self, Debug};
use std::future::Future;
use std::matches;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::SystemTime;

use anyhow::anyhow;
use async_trait::async_trait;
use fuser::consts::FOPEN_DIRECT_IO;
use fuser::*;
use libc::{F_RDLCK, F_UNLCK, F_WRLCK, O_DIRECT, SEEK_CUR, SEEK_END, SEEK_SET};
use tikv_client::{Config, TransactionClient};
use tracing::{debug, info, instrument, trace, warn};

use super::dir::Directory;
use super::error::{FsError, Result};
use super::file_handler::{FileHandler, FileHub};
use super::inode::Inode;
use super::key::{ScopedKey, ROOT_INODE};
use super::mode::{as_file_perm, make_mode};
use super::reply::get_time;
use super::reply::{Attr, Create, Data, Dir, DirItem, Entry, Lseek, Open, StatFs, Write};
use super::transaction::Txn;
use super::{async_fs::AsyncFileSystem, reply::Lock};
use crate::MountOption;

pub struct TiFs {
    pub pd_endpoints: Vec<String>,
    pub config: Config,
    pub client: TransactionClient,
    pub hub: FileHub,
    pub direct_io: bool,
}

type BoxedFuture<'a, T> = Pin<Box<dyn 'a + Send + Future<Output = Result<T>>>>;

impl TiFs {
    pub const SCAN_LIMIT: u32 = 1 << 10;
    pub const BLOCK_SIZE: u64 = 1 << 20;
    pub const BLOCK_CACHE: usize = 1 << 25;
    pub const DIR_CACHE: usize = 1 << 24;
    pub const INODE_CACHE: usize = 1 << 24;
    pub const MAX_NAME_LEN: u32 = 1 << 8;
    pub const INLINE_DATA_THRESHOLD: u64 = 1 << 16;

    #[instrument]
    pub async fn construct<S>(
        pd_endpoints: Vec<S>,
        cfg: Config,
        options: Vec<MountOption>,
    ) -> anyhow::Result<Self>
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
            direct_io: options
                .iter()
                .find(|option| matches!(option, MountOption::DirectIO))
                .is_some(),
        })
    }

    async fn with_pessimistic<F, T>(&self, f: F) -> Result<T>
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
        self.with_pessimistic(move |_, txn| Box::pin(txn.read_data(ino, start, chunk_size)))
            .await
    }

    async fn clear_data(&self, ino: u64) -> Result<u64> {
        self.with_pessimistic(move |_, txn| Box::pin(txn.clear_data(ino)))
            .await
    }

    async fn write_data(&self, ino: u64, start: u64, data: Vec<u8>) -> Result<usize> {
        self.with_pessimistic(move |_, txn| Box::pin(txn.write_data(ino, start, data)))
            .await
    }

    async fn read_dir(&self, ino: u64) -> Result<Directory> {
        self.with_pessimistic(move |_, txn| Box::pin(txn.read_dir(ino)))
            .await
    }

    async fn read_inode(&self, ino: u64) -> Result<FileAttr> {
        let ino = self
            .with_pessimistic(move |_, txn| Box::pin(txn.read_inode(ino)))
            .await?;
        Ok(ino.file_attr)
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

    async fn setlkw(&self, ino: u64, lock_owner: u64, typ: i32) -> Result<bool> {
        loop {
            let res = self
                .with_pessimistic(move |_, txn| {
                    Box::pin(async move {
                        let mut inode = txn.read_inode_for_update(ino).await?;
                        match typ {
                            F_WRLCK => {
                                if inode.lock_state.owner_set.len() > 1 {
                                    return Ok(false);
                                }
                                if inode.lock_state.owner_set.is_empty() {
                                    inode.lock_state.lk_type = F_WRLCK;
                                    inode.lock_state.owner_set.insert(lock_owner);
                                    txn.save_inode(&inode).await?;
                                    return Ok(true);
                                }
                                if inode.lock_state.owner_set.get(&lock_owner) == Some(&lock_owner)
                                {
                                    inode.lock_state.lk_type = F_WRLCK;
                                    txn.save_inode(&inode).await?;
                                    return Ok(true);
                                }
                                Err(FsError::InvalidLock)
                            }
                            F_RDLCK => {
                                if inode.lock_state.lk_type == F_WRLCK {
                                    return Ok(false);
                                } else {
                                    inode.lock_state.lk_type = F_RDLCK;
                                    inode.lock_state.owner_set.insert(lock_owner);
                                    txn.save_inode(&inode).await?;
                                    return Ok(true);
                                }
                            }
                            _ => return Err(FsError::InvalidLock),
                        }
                    })
                })
                .await?;
            if res {
                break;
            }
        }

        Ok(true)
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
    async fn init(&self, gid: u32, uid: u32, config: &mut KernelConfig) -> Result<()> {
        // config
        //     .add_capabilities(fuser::consts::FUSE_POSIX_LOCKS)
        //     .expect("kernel config failed to add cap_fuse FUSE_POSIX_LOCKS");
        config
            .add_capabilities(fuser::consts::FUSE_FLOCK_LOCKS)
            .expect("kernel config failed to add cap_fuse FUSE_CAP_FLOCK_LOCKS");

        self.with_pessimistic(move |fs, txn| {
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
        warn!("getattr, inode:{:?}", ino);
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
        self.with_pessimistic(move |_, txn| {
            Box::pin(async move {
                // TODO: how to deal with fh, chgtime, bkuptime?
                let mut attr = txn.read_inode_for_update(ino).await?;
                attr.perm = match mode {
                    Some(m) => as_file_perm(m),
                    None => attr.perm,
                };
                attr.uid = uid.unwrap_or(attr.uid);
                attr.gid = gid.unwrap_or(attr.gid);
                attr.set_size(size.unwrap_or(attr.size));
                attr.atime = match atime {
                    Some(TimeOrNow::SpecificTime(t)) => t,
                    Some(TimeOrNow::Now) | None => SystemTime::now(),
                };
                attr.mtime = match mtime {
                    Some(TimeOrNow::SpecificTime(t)) => t,
                    Some(TimeOrNow::Now) | None => SystemTime::now(),
                };
                attr.ctime = ctime.unwrap_or(attr.ctime);
                attr.crtime = crtime.unwrap_or(attr.crtime);
                attr.flags = flags.unwrap_or(attr.flags);
                txn.save_inode(&attr).await?;
                Ok(Attr {
                    time: get_time(),
                    attr: attr.into(),
                })
            })
        })
        .await
    }

    #[tracing::instrument]
    async fn readdir(&self, ino: u64, _fh: u64, mut offset: i64) -> Result<Dir> {
        let mut dir = Dir::offset(offset as usize);

        if offset == 0 {
            dir.push(DirItem {
                ino: ROOT_INODE,
                name: "..".to_string(),
                typ: FileType::Directory,
            });
        }

        if offset <= 1 {
            dir.push(DirItem {
                ino,
                name: ".".to_string(),
                typ: FileType::Directory,
            });
        }

        offset -= 2.min(offset);

        let directory = self.read_dir(ino).await?;
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
        let mut open_flags = 0;
        if self.direct_io || flags | O_DIRECT != 0 {
            open_flags |= FOPEN_DIRECT_IO;
        }

        Ok(Open::new(fh, open_flags))
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
        let start = *handler.read_cursor().await as i64 + offset;
        if start < 0 {
            return Err(FsError::InvalidOffset {
                ino: ino,
                offset: start,
            });
        }
        let data = self.read_data(ino, start as u64, Some(size as u64)).await?;
        Ok(Data::new(data))
    }

    #[tracing::instrument(skip(data))]
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
        let start = *handler.read_cursor().await as i64 + offset;
        if start < 0 {
            return Err(FsError::InvalidOffset {
                ino: ino,
                offset: start,
            });
        }

        let data_len = data.len();
        let _ = self.write_data(ino, start as u64, data).await?;
        Ok(Write::new(data_len as u32))
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
            .with_pessimistic(move |_, txn| Box::pin(txn.mkdir(parent, name, mode, gid, uid)))
            .await?;
        Ok(Entry::new(attr.into(), 0))
    }

    #[tracing::instrument]
    async fn rmdir(&self, parent: u64, raw_name: OsString) -> Result<()> {
        self.with_pessimistic(move |_, txn| {
            Box::pin(async move {
                let mut dir = txn.read_dir_for_update(parent).await?;
                let name = raw_name.to_string_lossy();
                let item = dir.remove(&*name).ok_or_else(|| FsError::FileNotFound {
                    file: name.to_string(),
                })?;

                let dir_contents = txn.read_dir(item.ino).await?;
                if dir_contents.len() != 0 {
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
            .with_pessimistic(move |_, txn| Box::pin(txn.make_inode(parent, name, mode, gid, uid)))
            .await?;
        Ok(Entry::new(attr.into(), 0))
    }

    #[tracing::instrument]
    async fn access(&self, ino: u64, mask: i32) -> Result<()> {
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
        self.with_pessimistic(move |_, txn| {
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
                txn.save_inode(&attr).await?;
                Ok(Entry::new(attr.into(), 0))
            })
        })
        .await
    }

    async fn unlink(&self, parent: u64, raw_name: OsString) -> Result<()> {
        self.with_pessimistic(move |_, txn| {
            Box::pin(async move {
                let mut dir = txn.read_dir_for_update(parent).await?;
                let name = raw_name.to_string_lossy();
                let item = dir.remove(&*name).ok_or_else(|| FsError::FileNotFound {
                    file: name.to_string(),
                })?;

                txn.save_dir(parent, &dir).await?;
                let mut attr = txn.read_inode_for_update(item.ino).await?;
                attr.nlink -= 1;
                txn.save_inode(&attr).await?;
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
        self.with_pessimistic(move |_, txn| {
            Box::pin(async move {
                let mut dir = txn.read_dir_for_update(parent).await?;
                let name = raw_name.to_string_lossy();
                match dir.remove(&*name) {
                    None => Err(FsError::FileNotFound {
                        file: name.to_string(),
                    }),
                    Some(mut item) => {
                        txn.save_dir(parent, &dir).await?;

                        let mut new_dir = if parent == newparent {
                            dir
                        } else {
                            txn.read_dir_for_update(newparent).await?
                        };

                        item.name = new_raw_name.to_string_lossy().to_string();
                        if let Some(old_item) = new_dir.add(item) {
                            let mut old_inode = txn.read_inode_for_update(old_item.ino).await?;
                            old_inode.nlink -= 1;
                            txn.save_inode(&old_inode).await?;
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
        self.with_pessimistic(move |_, txn| {
            Box::pin(async move {
                let mut attr = txn
                    .make_inode(parent, name, make_mode(FileType::Symlink, 0o777), gid, uid)
                    .await?;

                attr.set_size(
                    txn.write_data(attr.ino, 0, link.as_os_str().as_bytes().to_vec())
                        .await? as u64,
                );
                txn.save_inode(&attr).await?;
                Ok(Entry::new(attr.into(), 0))
            })
        })
        .await
    }

    async fn readlink(&self, ino: u64) -> Result<Data> {
        self.with_pessimistic(move |_, txn| {
            Box::pin(async move { Ok(Data::new(txn.read_data(ino, 0, None).await?)) })
        })
        .await
    }

    #[tracing::instrument]
    async fn fallocate(
        &self,
        ino: u64,
        fh: u64,
        offset: i64,
        length: i64,
        _mode: i32,
    ) -> Result<()> {
        self.with_pessimistic(move |_, txn| {
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
    // TODO: Find an api to calculate total and available space on tikv.
    async fn statfs(&self, _ino: u64) -> Result<StatFs> {
        let bsize = Self::BLOCK_SIZE as u32;
        let namelen = Self::MAX_NAME_LEN;
        let (ffree, blocks, files) = self
            .with_pessimistic(move |_, txn| {
                Box::pin(async move {
                    let next_inode = txn
                        .read_meta()
                        .await?
                        .map(|meta| meta.inode_next)
                        .unwrap_or(ROOT_INODE);
                    let (b, f) = txn
                        .scan(
                            ScopedKey::inode_range(ROOT_INODE..next_inode),
                            (next_inode - ROOT_INODE) as u32,
                        )
                        .await?
                        .map(|pair| Inode::deserialize(pair.value()))
                        .try_fold((0, 0), |(blocks, files), inode| {
                            Ok::<_, FsError>((blocks + inode?.blocks, files + 1))
                        })?;
                    Ok((std::u64::MAX - next_inode, b, f))
                })
            })
            .await?;
        Ok(StatFs::new(
            blocks,
            std::u64::MAX,
            std::u64::MAX,
            files,
            ffree,
            bsize,
            namelen,
            0,
        ))
    }

    #[tracing::instrument]
    async fn setlk(
        &self,
        ino: u64,
        fh: u64,
        lock_owner: u64,
        start: u64,
        end: u64,
        typ: i32,
        pid: u32,
        sleep: bool,
    ) -> Result<()> {
        let not_again = self.with_pessimistic(move |_, txn| {
            Box::pin(async move {
                let mut inode = txn.read_inode_for_update(ino).await?;
                warn!("setlk, inode:{:?}, pid:{:?}, typ para: {:?}, state type: {:?}, owner: {:?}, sleep: {:?},", inode, pid, typ, inode.lock_state.lk_type, lock_owner, sleep);
                if inode.file_attr.kind == FileType::Directory {
                    return Err(FsError::InvalidLock);
                }
                match typ {
                    F_RDLCK => {
                        if inode.lock_state.lk_type == F_WRLCK {
                            if sleep {
                                warn!("setlk F_RDLCK return sleep, inode:{:?}, pid:{:?}, typ para: {:?}, state type: {:?}, owner: {:?}, sleep: {:?},", inode, pid, typ, inode.lock_state.lk_type, lock_owner, sleep);
                                return Ok(false)
                            }
                            return Err(FsError::InvalidLock);
                        }
                        inode.lock_state.owner_set.insert(lock_owner);
                        inode.lock_state.lk_type = F_RDLCK;
                        txn.save_inode(&inode).await?;
                        warn!("setlk F_RDLCK return, inode:{:?}, pid:{:?}, typ para: {:?}, state type: {:?}, owner: {:?}, sleep: {:?},", inode, pid, typ, inode.lock_state.lk_type, lock_owner, sleep);
                        Ok(true)
                    }
                    F_WRLCK => match inode.lock_state.lk_type {
                        F_RDLCK => {
                            if inode.lock_state.owner_set.len() == 1
                                && inode.lock_state.owner_set.get(&lock_owner) == Some(&lock_owner)
                            {
                                inode.lock_state.lk_type = F_WRLCK;
                                txn.save_inode(&inode).await?;
                                warn!("setlk F_WRLCK on F_RDLCK return, inode:{:?}, pid:{:?}, typ para: {:?}, state type: {:?}, owner: {:?}, sleep: {:?},", inode, pid, typ, inode.lock_state.lk_type, lock_owner, sleep);
                                return Ok(true);
                            }
                            if sleep {
                                warn!("setlk F_WRLCK on F_RDLCK sleep return, inode:{:?}, pid:{:?}, typ para: {:?}, state type: {:?}, owner: {:?}, sleep: {:?},", inode, pid, typ, inode.lock_state.lk_type, lock_owner, sleep);
                                return Ok(false)
                            }
                            return Err(FsError::InvalidLock);
                        },
                        F_UNLCK => {
                            inode.lock_state.owner_set.clear();
                            inode.lock_state.owner_set.insert(lock_owner);
                            inode.lock_state.lk_type = F_WRLCK;
                            warn!("setlk F_WRLCK on F_UNLCK return, inode:{:?}, pid:{:?}, typ para: {:?}, state type: {:?}, owner: {:?}, sleep: {:?},", inode, pid, typ, inode.lock_state.lk_type, lock_owner, sleep);
                            txn.save_inode(&inode).await?;
                            Ok(true)
                        },
                        F_WRLCK => {
                            if sleep {
                                warn!("setlk F_WRLCK on F_WRLCK return sleep, inode:{:?}, pid:{:?}, typ para: {:?}, state type: {:?}, owner: {:?}, sleep: {:?},", inode, pid, typ, inode.lock_state.lk_type, lock_owner, sleep);
                                return Ok(false)
                            }
                            return Err(FsError::InvalidLock);
                        },
                        _ => return Err(FsError::InvalidLock)
                    },
                    F_UNLCK => {
                        inode.lock_state.owner_set.remove(&lock_owner);
                        if inode.lock_state.owner_set.is_empty() {
                            inode.lock_state.lk_type = F_UNLCK;
                        }
                        txn.save_inode(&inode).await?;
                        warn!("setlk F_UNLCK return, inode:{:?}, pid:{:?}, typ para: {:?}, state type: {:?}, owner: {:?}, sleep: {:?},", inode, pid, typ, inode.lock_state.lk_type, lock_owner, sleep);
                        Ok(true)
                    }
                    _ => return Err(FsError::InvalidLock)
                }
            })
        })
        .await?;
        if !not_again {
            if self.setlkw(ino, lock_owner, typ).await? {
                return Ok(());
            }
            return Err(FsError::InvalidLock);
        }
        return Ok(());
    }

    #[tracing::instrument]
    async fn getlk(
        &self,
        ino: u64,
        fh: u64,
        lock_owner: u64,
        start: u64,
        end: u64,
        typ: i32,
        pid: u32,
    ) -> Result<Lock> {
        // TODO: read only operation need not txn?
        self.with_pessimistic(move |_, txn| {
            Box::pin(async move {
                let inode = txn.read_inode(ino).await?;
                warn!("getlk, inode:{:?}, pid:{:?}", inode, pid);
                Ok(Lock::_new(0, 0, inode.lock_state.lk_type, 0))
            })
        })
        .await
    }
}
