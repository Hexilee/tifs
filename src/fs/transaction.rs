use std::ops::{Deref, DerefMut};
use std::time::SystemTime;

use bytes::Bytes;
use bytestring::ByteString;
use fuser::{FileAttr, FileType};
use tikv_client::{Transaction, TransactionClient};
use tracing::{debug, trace};

use super::block::empty_block;
use super::dir::Directory;
use super::error::{FsError, Result};
use super::file_handler::FileHandler;
use super::index::Index;
use super::inode::Inode;
use super::key::{ScopedKey, ROOT_INODE};
use super::meta::Meta;
use super::mode::{as_file_kind, as_file_perm, make_mode};
use super::reply::{DirItem, StatFs};

pub struct Txn {
    txn: Transaction,
    block_size: u64,
    max_blocks: Option<u64>,
    max_name_len: u32,
}

impl Txn {
    const INLINE_DATA_THRESHOLD_BASE: u64 = 1 << 4;

    fn inline_data_threshold(&self) -> u64 {
        self.block_size / Self::INLINE_DATA_THRESHOLD_BASE
    }

    pub fn block_size(&self) -> u64 {
        self.block_size
    }

    fn check_space_left(&self, meta: &Meta) -> Result<()> {
        match meta.last_stat {
            Some(ref stat) if stat.bavail == 0 => {
                Err(FsError::NoSpaceLeft(stat.bsize as u64 * stat.blocks))
            }
            _ => Ok(()),
        }
    }

    pub async fn begin_optimistic(
        client: &TransactionClient,
        block_size: u64,
        max_size: Option<u64>,
        max_name_len: u32,
    ) -> Result<Self> {
        Ok(Txn {
            txn: client.begin_optimistic().await?,
            block_size,
            max_blocks: max_size.map(|size| size / block_size),
            max_name_len,
        })
    }

    pub async fn open(&mut self, ino: u64) -> Result<u64> {
        let mut inode = self.read_inode(ino).await?;
        let fh = inode.next_fh;
        self.save_fh(ino, fh, &FileHandler::default()).await?;
        inode.next_fh += 1;
        inode.opened_fh += 1;
        self.save_inode(&inode).await?;
        Ok(fh)
    }

    pub async fn close(&mut self, ino: u64, fh: u64) -> Result<()> {
        self.read_fh(ino, fh).await?;
        self.delete(ScopedKey::handler(ino, fh)).await?;

        let mut inode = self.read_inode(ino).await?;
        inode.opened_fh -= 1;
        self.save_inode(&inode).await
    }

    pub async fn read_fh(&mut self, ino: u64, fh: u64) -> Result<FileHandler> {
        let data = self
            .get(ScopedKey::handler(ino, fh))
            .await?
            .ok_or_else(|| FsError::FhNotFound { ino, fh })?;
        FileHandler::deserialize(&data)
    }

    pub async fn save_fh(&mut self, ino: u64, fh: u64, handler: &FileHandler) -> Result<()> {
        Ok(self
            .put(ScopedKey::handler(ino, fh), handler.serialize()?)
            .await?)
    }

    pub async fn read(&mut self, ino: u64, fh: u64, offset: i64, size: u32) -> Result<Vec<u8>> {
        let handler = self.read_fh(ino, fh).await?;
        let start = handler.cursor as i64 + offset;
        if start < 0 {
            return Err(FsError::InvalidOffset { ino, offset: start });
        }
        self.read_data(ino, start as u64, Some(size as u64)).await
    }

    pub async fn write(&mut self, ino: u64, fh: u64, offset: i64, data: Bytes) -> Result<usize> {
        let handler = self.read_fh(ino, fh).await?;
        let start = handler.cursor as i64 + offset;
        if start < 0 {
            return Err(FsError::InvalidOffset { ino, offset: start });
        }

        self.write_data(ino, start as u64, data).await
    }

    pub async fn make_inode(
        &mut self,
        parent: u64,
        name: ByteString,
        mode: u32,
        gid: u32,
        uid: u32,
        rdev: u32,
    ) -> Result<Inode> {
        let mut meta = self
            .read_meta()
            .await?
            .unwrap_or_else(|| Meta::new(self.block_size));
        self.check_space_left(&meta)?;
        let ino = meta.inode_next;
        meta.inode_next += 1;

        debug!("get ino({})", ino);
        self.save_meta(&meta).await?;

        let file_type = as_file_kind(mode);
        if parent >= ROOT_INODE {
            if self.get_index(parent, name.clone()).await?.is_some() {
                return Err(FsError::FileExist {
                    file: name.to_string(),
                });
            }
            self.set_index(parent, name.clone(), ino).await?;

            let mut dir = self.read_dir(parent).await?;
            debug!("read dir({:?})", &dir);

            dir.push(DirItem {
                ino,
                name: name.to_string(),
                typ: file_type,
            });

            self.save_dir(parent, &dir).await?;
            // TODO: update attributes of directory
        }

        let inode = FileAttr {
            ino,
            size: 0,
            blocks: 0,
            atime: SystemTime::now(),
            mtime: SystemTime::now(),
            ctime: SystemTime::now(),
            crtime: SystemTime::now(),
            kind: file_type,
            perm: as_file_perm(mode),
            nlink: 1,
            uid,
            gid,
            rdev,
            blksize: self.block_size as u32,
            flags: 0,
        }
        .into();

        debug!("made inode ({:?})", &inode);

        self.save_inode(&inode).await?;
        Ok(inode.into())
    }

    pub async fn get_index(&mut self, parent: u64, name: ByteString) -> Result<Option<u64>> {
        let key = ScopedKey::index(parent, &name);
        self.get(key)
            .await
            .map_err(FsError::from)
            .and_then(|value| {
                value
                    .map(|data| Ok(Index::deserialize(&data)?.ino))
                    .transpose()
            })
    }

    pub async fn set_index(&mut self, parent: u64, name: ByteString, ino: u64) -> Result<()> {
        let key = ScopedKey::index(parent, &name);
        let value = Index::new(ino).serialize()?;
        Ok(self.put(key, value).await?)
    }

    pub async fn remove_index(&mut self, parent: u64, name: ByteString) -> Result<()> {
        let key = ScopedKey::index(parent, &name);
        Ok(self.delete(key).await?)
    }

    pub async fn read_inode(&mut self, ino: u64) -> Result<Inode> {
        let value = self
            .get(ScopedKey::inode(ino))
            .await?
            .ok_or_else(|| FsError::InodeNotFound { inode: ino })?;
        Ok(Inode::deserialize(&value)?)
    }

    pub async fn save_inode(&mut self, inode: &Inode) -> Result<()> {
        let key = ScopedKey::inode(inode.ino);

        if inode.nlink == 0 && inode.opened_fh == 0 {
            self.delete(key).await?;
        } else {
            self.put(key, inode.serialize()?).await?;
            debug!("save inode: {:?}", inode);
        }
        Ok(())
    }

    pub async fn remove_inode(&mut self, ino: u64) -> Result<()> {
        self.delete(ScopedKey::inode(ino)).await?;
        Ok(())
    }

    pub async fn read_meta(&mut self) -> Result<Option<Meta>> {
        let opt_data = self.get(ScopedKey::meta()).await?;
        opt_data.map(|data| Meta::deserialize(&data)).transpose()
    }

    pub async fn save_meta(&mut self, meta: &Meta) -> Result<()> {
        self.put(ScopedKey::meta(), meta.serialize()?).await?;
        Ok(())
    }

    async fn transfer_inline_data_to_block(&mut self, inode: &mut Inode) -> Result<()> {
        debug_assert!(inode.size <= self.inline_data_threshold());
        let key = ScopedKey::block(inode.ino, 0);
        let mut data = inode.inline_data.clone().unwrap();
        data.resize(self.block_size as usize, 0);
        self.put(key, data).await?;
        inode.inline_data = None;
        Ok(())
    }

    async fn write_inline_data(
        &mut self,
        inode: &mut Inode,
        start: u64,
        data: &[u8],
    ) -> Result<usize> {
        debug_assert!(inode.size <= self.inline_data_threshold());
        let size = data.len() as u64;
        debug_assert!(start + size <= self.inline_data_threshold());

        let size = data.len();
        let start = start as usize;

        let mut inlined = inode.inline_data.take().unwrap_or_else(Vec::new);
        if start + size > inlined.len() {
            inlined.resize(start + size, 0);
        }
        inlined[start..start + size].copy_from_slice(data);

        inode.atime = SystemTime::now();
        inode.mtime = SystemTime::now();
        inode.ctime = SystemTime::now();
        inode.set_size(inlined.len() as u64, self.block_size);
        inode.inline_data = Some(inlined);
        self.save_inode(inode).await?;

        Ok(size)
    }

    async fn read_inline_data(
        &mut self,
        inode: &mut Inode,
        start: u64,
        size: u64,
    ) -> Result<Vec<u8>> {
        debug_assert!(inode.size <= self.inline_data_threshold());

        let start = start as usize;
        let size = size as usize;

        let inlined = inode.inline_data.as_ref().unwrap();
        debug_assert!(inode.size as usize == inlined.len());
        let mut data: Vec<u8> = Vec::with_capacity(size);
        data.resize(size, 0);
        if inlined.len() > start {
            let to_copy = size.min(inlined.len() - start);
            data[..to_copy].copy_from_slice(&inlined[start..start + to_copy]);
        }

        inode.atime = SystemTime::now();
        self.save_inode(inode).await?;

        Ok(data)
    }

    pub async fn read_data(
        &mut self,
        ino: u64,
        start: u64,
        chunk_size: Option<u64>,
    ) -> Result<Vec<u8>> {
        let mut attr = self.read_inode(ino).await?;
        if start >= attr.size {
            return Ok(Vec::new());
        }

        let max_size = attr.size - start;
        let size = chunk_size.unwrap_or(max_size).min(max_size);

        if attr.inline_data.is_some() {
            return self.read_inline_data(&mut attr, start, size).await;
        }

        let target = start + size;
        let start_block = start / self.block_size;
        let end_block = (target + self.block_size - 1) / self.block_size;

        let pairs = self
            .scan(
                ScopedKey::block_range(ino, start_block..end_block),
                (end_block - start_block) as u32,
            )
            .await?;

        let mut data = pairs
            .enumerate()
            .flat_map(|(i, pair)| {
                let key = if let Ok(ScopedKey::Block { ino: _, block }) =
                    ScopedKey::parse(pair.key().into())
                {
                    block
                } else {
                    unreachable!("the keys from scanning should be always valid block keys")
                };
                let value = pair.into_value();
                (start_block as usize + i..key as usize)
                    .map(|_| empty_block(self.block_size))
                    .chain(vec![value])
            })
            .enumerate()
            .fold(
                Vec::with_capacity(
                    ((end_block - start_block) * self.block_size - start % self.block_size)
                        as usize,
                ),
                |mut data, (i, value)| {
                    let mut slice = value.as_slice();
                    if i == 0 {
                        slice = &slice[(start % self.block_size) as usize..]
                    }

                    data.extend_from_slice(slice);
                    data
                },
            );

        data.resize(size as usize, 0);
        attr.atime = SystemTime::now();
        self.save_inode(&attr).await?;
        Ok(data)
    }

    pub async fn clear_data(&mut self, ino: u64) -> Result<u64> {
        let mut attr = self.read_inode(ino).await?;
        let end_block = (attr.size + self.block_size - 1) / self.block_size;

        for block in 0..end_block {
            self.delete(ScopedKey::block(ino, block)).await?;
        }

        let clear_size = attr.size;
        attr.size = 0;
        attr.atime = SystemTime::now();
        self.save_inode(&attr).await?;
        Ok(clear_size)
    }

    pub async fn write_data(&mut self, ino: u64, start: u64, data: Bytes) -> Result<usize> {
        debug!("write data at ({})[{}]", ino, start);
        let meta = self.read_meta().await?.unwrap();
        self.check_space_left(&meta)?;

        let mut inode = self.read_inode(ino).await?;
        let size = data.len();
        let target = start + size as u64;

        if inode.inline_data.is_some() && target > self.block_size {
            self.transfer_inline_data_to_block(&mut inode).await?;
        }

        if (inode.inline_data.is_some() || inode.size == 0) && target <= self.block_size {
            return self.write_inline_data(&mut inode, start, &data).await;
        }

        let mut block_index = start / self.block_size;
        let start_key = ScopedKey::block(ino, block_index);
        let start_index = (start % self.block_size) as usize;

        let first_block_size = self.block_size as usize - start_index;

        let (first_block, mut rest) = data.split_at(first_block_size.min(data.len()));

        let mut start_value = self
            .get(start_key)
            .await?
            .unwrap_or_else(|| empty_block(self.block_size));

        start_value[start_index..start_index + first_block.len()].copy_from_slice(first_block);

        self.put(start_key, start_value).await?;

        while rest.len() != 0 {
            block_index += 1;
            let key = ScopedKey::block(ino, block_index);
            let (curent_block, current_rest) =
                rest.split_at((self.block_size as usize).min(rest.len()));
            let mut value = curent_block.to_vec();
            if value.len() < self.block_size as usize {
                let mut last_value = self
                    .get(key)
                    .await?
                    .unwrap_or_else(|| empty_block(self.block_size));
                last_value[..value.len()].copy_from_slice(&value);
                value = last_value;
            }
            self.put(key, value).await?;
            rest = current_rest;
        }

        inode.atime = SystemTime::now();
        inode.mtime = SystemTime::now();
        inode.ctime = SystemTime::now();
        inode.set_size(inode.size.max(target), self.block_size);
        self.save_inode(&inode.into()).await?;
        trace!("write data: {}", String::from_utf8_lossy(&data));
        Ok(size)
    }

    pub async fn write_link(&mut self, inode: &mut Inode, data: Bytes) -> Result<usize> {
        debug_assert!(inode.file_attr.kind == FileType::Symlink);
        inode.inline_data = None;
        inode.set_size(0, self.block_size);
        self.write_inline_data(inode, 0, &data).await
    }

    pub async fn read_link(&mut self, ino: u64) -> Result<Vec<u8>> {
        let mut inode = self.read_inode(ino).await?;
        debug_assert!(inode.file_attr.kind == FileType::Symlink);
        let size = inode.size;
        self.read_inline_data(&mut inode, 0, size).await
    }

    pub async fn link(&mut self, ino: u64, newparent: u64, newname: ByteString) -> Result<Inode> {
        if let Some(old_ino) = self.get_index(newparent, newname.clone()).await? {
            let inode = self.read_inode(old_ino).await?;
            match inode.kind {
                FileType::Directory => self.rmdir(newparent, newname.clone()).await?,
                _ => self.unlink(newparent, newname.clone()).await?,
            }
        }
        self.set_index(newparent, newname.clone(), ino).await?;

        let mut inode = self.read_inode(ino).await?;
        let mut dir = self.read_dir(newparent).await?;

        dir.push(DirItem {
            ino,
            name: newname.to_string(),
            typ: inode.kind,
        });

        self.save_dir(newparent, &dir).await?;
        inode.nlink += 1;
        inode.ctime = SystemTime::now();
        self.save_inode(&inode).await?;
        Ok(inode)
    }

    pub async fn unlink(&mut self, parent: u64, name: ByteString) -> Result<()> {
        match self.get_index(parent, name.clone()).await? {
            None => Err(FsError::FileNotFound {
                file: name.to_string(),
            }),
            Some(ino) => {
                self.remove_index(parent, name.clone()).await?;
                let parent_dir = self.read_dir(parent).await?;
                let new_parent_dir: Directory = parent_dir
                    .into_iter()
                    .filter(|item| item.name != &*name)
                    .collect();
                self.save_dir(parent, &new_parent_dir).await?;

                let mut inode = self.read_inode(ino).await?;
                inode.nlink -= 1;
                inode.ctime = SystemTime::now();
                self.save_inode(&inode).await?;
                Ok(())
            }
        }
    }

    pub async fn rmdir(&mut self, parent: u64, name: ByteString) -> Result<()> {
        match self.get_index(parent, name.clone()).await? {
            None => Err(FsError::FileNotFound {
                file: name.to_string(),
            }),
            Some(ino) => {
                let target_dir = self.read_dir(ino).await?;
                if target_dir.len() != 0 {
                    let name_str = name.to_string();
                    debug!("dir({}) not empty", &name_str);
                    return Err(FsError::DirNotEmpty { dir: name_str });
                }
                self.remove_index(parent, name.clone()).await?;
                self.remove_inode(ino).await?;

                let parent_dir = self.read_dir(parent).await?;
                let new_parent_dir: Directory = parent_dir
                    .into_iter()
                    .filter(|item| item.name != &*name)
                    .collect();
                self.save_dir(parent, &new_parent_dir).await?;
                Ok(())
            }
        }
    }

    pub async fn lookup(&mut self, parent: u64, name: ByteString) -> Result<u64> {
        self.get_index(parent, name.clone())
            .await?
            .ok_or_else(|| FsError::FileNotFound {
                file: name.to_string(),
            })
    }

    pub async fn fallocate(&mut self, inode: &mut Inode, offset: i64, length: i64) -> Result<()> {
        let target_size = (offset + length) as u64;
        if target_size <= inode.size {
            return Ok(());
        }

        if inode.inline_data.is_some() {
            if target_size <= self.inline_data_threshold() {
                let original_size = inode.size;
                let data = vec![0; (target_size - original_size) as usize];
                self.write_inline_data(inode, original_size, &data).await?;
                return Ok(());
            } else {
                self.transfer_inline_data_to_block(inode).await?;
            }
        }

        inode.set_size(target_size, self.block_size);
        inode.mtime = SystemTime::now();
        self.save_inode(inode).await?;
        Ok(())
    }

    pub async fn mkdir(
        &mut self,
        parent: u64,
        name: ByteString,
        mode: u32,
        gid: u32,
        uid: u32,
    ) -> Result<Inode> {
        let dir_mode = make_mode(FileType::Directory, mode as _);
        let mut inode = self.make_inode(parent, name, dir_mode, gid, uid, 0).await?;
        inode.perm = mode as _;
        self.save_inode(&inode).await?;
        self.save_dir(inode.ino, &Directory::new()).await
    }

    pub async fn read_dir(&mut self, ino: u64) -> Result<Directory> {
        let data =
            self.get(ScopedKey::block(ino, 0))
                .await?
                .ok_or_else(|| FsError::BlockNotFound {
                    inode: ino,
                    block: 0,
                })?;
        trace!("read data: {}", String::from_utf8_lossy(&data));
        super::dir::decode(&data)
    }

    pub async fn save_dir(&mut self, ino: u64, dir: &Directory) -> Result<Inode> {
        let data = super::dir::encode(dir)?;
        let mut inode = self.read_inode(ino).await?;
        inode.set_size(data.len() as u64, self.block_size);
        inode.atime = SystemTime::now();
        inode.mtime = SystemTime::now();
        inode.ctime = SystemTime::now();
        self.save_inode(&inode).await?;
        self.put(ScopedKey::block(ino, 0), data).await?;
        Ok(inode)
    }

    pub async fn statfs(&mut self) -> Result<StatFs> {
        let bsize = self.block_size as u32;
        let mut meta = self
            .read_meta()
            .await?
            .expect("meta should not be none after fs initialized");
        let next_inode = meta.inode_next;
        let (used_blocks, files) = self
            .scan(
                ScopedKey::inode_range(ROOT_INODE..next_inode),
                (next_inode - ROOT_INODE) as u32,
            )
            .await?
            .map(|pair| Inode::deserialize(pair.value()))
            .try_fold((0, 0), |(blocks, files), inode| {
                Ok::<_, FsError>((blocks + inode?.blocks, files + 1))
            })?;
        let ffree = std::u64::MAX - next_inode;
        let bfree = match self.max_blocks {
            Some(max_blocks) if max_blocks > used_blocks => max_blocks - used_blocks,
            Some(_) => 0,
            None => std::u64::MAX,
        };
        let blocks = match self.max_blocks {
            Some(max_blocks) => max_blocks,
            None => used_blocks,
        };

        let stat = StatFs::new(
            blocks,
            bfree,
            bfree,
            files,
            ffree,
            bsize,
            self.max_name_len,
            0,
        );
        trace!("statfs: {:?}", stat);
        meta.last_stat = Some(stat.clone());
        self.save_meta(&meta).await?;
        Ok(stat)
    }
}

impl Deref for Txn {
    type Target = Transaction;

    fn deref(&self) -> &Self::Target {
        &self.txn
    }
}

impl DerefMut for Txn {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.txn
    }
}
