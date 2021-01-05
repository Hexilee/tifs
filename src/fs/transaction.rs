use std::ffi::OsString;
use std::ops::{Deref, DerefMut};
use std::time::SystemTime;

use fuser::{FileAttr, FileType};
use tikv_client::{Transaction, TransactionClient};
use tracing::{debug, instrument};

use super::dir::Directory;
use super::error::{FsError, Result};
use super::inode::Inode;
use super::key::{ScopedKey, ROOT_INODE};
use super::meta::Meta;
use super::mode::{as_file_kind, as_file_perm, make_mode};
use super::reply::DirItem;
use super::tikv_fs::TiFs;

pub struct Txn(Transaction);

impl Txn {
    pub async fn begin(client: &TransactionClient) -> Result<Self> {
        Ok(Txn(client.begin().await?))
    }

    pub async fn begin_pessimistic(client: &TransactionClient) -> Result<Self> {
        Ok(Txn(client.begin_pessimistic().await?))
    }

    pub async fn make_inode(
        &mut self,
        parent: u64,
        raw_name: OsString,
        mode: u32,
        gid: u32,
        uid: u32,
    ) -> Result<Inode> {
        let mut meta = self.read_meta_for_update().await?.unwrap_or_else(|| Meta {
            inode_next: ROOT_INODE,
        });
        let ino = meta.inode_next;
        meta.inode_next += 1;

        debug!("get ino({})", ino);
        self.save_meta(&meta).await?;

        let file_type = as_file_kind(mode);
        let name = raw_name.to_string_lossy();

        if parent >= ROOT_INODE {
            let mut dir = self.read_dir_for_update(parent).await?;
            debug!("read dir({:?})", &dir);

            if let Some(item) = dir.add(DirItem {
                ino,
                name: name.to_string(),
                typ: file_type,
            }) {
                return Err(FsError::FileExist { file: item.name });
            }

            self.save_dir(parent, &dir).await?;
            // TODO: update attributes of directory
        }

        let inode = Inode(FileAttr {
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
            rdev: 0,
            blksize: TiFs::BLOCK_SIZE as u32,
            padding: 0,
            flags: 0,
        });

        debug!("made inode ({:?})", &inode);

        self.save_inode(&inode).await?;
        Ok(inode.into())
    }

    pub async fn read_inode(&self, ino: u64) -> Result<Inode> {
        let value = self
            .get(ScopedKey::inode(ino).scoped())
            .await?
            .ok_or_else(|| FsError::InodeNotFound { inode: ino })?;
        Ok(Inode::deserialize(&value)?)
    }

    pub async fn read_inode_for_update(&mut self, ino: u64) -> Result<Inode> {
        let value = self
            .get_for_update(ScopedKey::inode(ino).scoped())
            .await?
            .ok_or_else(|| FsError::InodeNotFound { inode: ino })?;
        Ok(Inode::deserialize(&value)?)
    }

    pub async fn save_inode(&mut self, inode: &Inode) -> Result<()> {
        let key = ScopedKey::inode(inode.0.ino).scoped();

        if inode.0.nlink == 0 {
            self.delete(key).await?;
        } else {
            self.put(key, inode.serialize()?).await?;
            debug!("save inode: {:?}", inode);
        }
        Ok(())
    }

    pub async fn remove_inode(&mut self, ino: u64) -> Result<()> {
        self.delete(ScopedKey::inode(ino).scoped()).await?;
        Ok(())
    }

    pub async fn read_meta(&self) -> Result<Option<Meta>> {
        let opt_data = self.get(ScopedKey::meta().scoped()).await?;
        opt_data.map(|data| Meta::deserialize(&data)).transpose()
    }

    pub async fn read_meta_for_update(&mut self) -> Result<Option<Meta>> {
        let opt_data = self.get_for_update(ScopedKey::meta().scoped()).await?;
        opt_data.map(|data| Meta::deserialize(&data)).transpose()
    }

    pub async fn save_meta(&mut self, meta: &Meta) -> Result<()> {
        self.put(ScopedKey::meta().scoped(), meta.serialize()?)
            .await?;
        Ok(())
    }

    pub async fn read_data(
        &mut self,
        ino: u64,
        start: u64,
        chunk_size: Option<u64>,
    ) -> Result<Vec<u8>> {
        let mut attr = self.read_inode(ino).await?;
        let size = chunk_size.unwrap_or_else(|| attr.size - start);
        let target = start + size;

        let data_size = target - start;
        let start_block = start / TiFs::BLOCK_SIZE;
        let end_block = (target + TiFs::BLOCK_SIZE - 1) / TiFs::BLOCK_SIZE;

        let pairs = self
            .scan(
                ScopedKey::block_range(ino, start_block..end_block),
                (end_block - start_block) as u32,
            )
            .await?;

        let is_last_block = |index| (index as u64 + 1) * TiFs::BLOCK_SIZE > data_size;

        let mut data = pairs
            .enumerate()
            .flat_map(|(i, pair)| {
                let key: ScopedKey = pair.key().clone().into();
                let value = pair.into_value();
                (i..key.key() as usize)
                    .map(|_| vec![0; TiFs::BLOCK_SIZE as usize])
                    .chain(vec![value])
            })
            .enumerate()
            .fold(
                Vec::with_capacity(data_size as usize),
                |mut data, (i, mut value)| {
                    if !is_last_block(i) && value.len() < TiFs::BLOCK_SIZE as usize {
                        value.resize(TiFs::BLOCK_SIZE as usize, 0);
                    }

                    let slice = match i {
                        0 => &value[(start_block % TiFs::BLOCK_SIZE) as usize..],
                        n if is_last_block(n) => &value[..(data_size % TiFs::BLOCK_SIZE) as usize],
                        _ => value.as_slice(),
                    };

                    data.extend_from_slice(slice);
                    data
                },
            );

        if data.len() < size as usize {
            data.resize(size as usize, 0);
        }

        attr.atime = SystemTime::now();
        self.save_inode(&attr).await?;
        Ok(data)
    }

    pub async fn clear_data(&mut self, ino: u64) -> Result<u64> {
        let mut attr = self.read_inode(ino).await?;
        let end_block = (attr.size + TiFs::BLOCK_SIZE - 1) / TiFs::BLOCK_SIZE;

        for block in 0..end_block {
            self.delete(ScopedKey::new(ino, block).scoped()).await?;
        }

        let clear_size = attr.size;
        attr.size = 0;
        attr.atime = SystemTime::now();
        self.save_inode(&attr).await?;
        Ok(clear_size)
    }

    pub async fn write_data(&mut self, ino: u64, start: u64, data: Vec<u8>) -> Result<usize> {
        debug!("write data at ({})[{}]", ino, start);
        let mut attr = self.read_inode(ino).await?;
        let size = data.len();
        let target = start + size as u64;

        let mut block_index = start / TiFs::BLOCK_SIZE;
        let start_key = ScopedKey::new(ino, block_index).scoped();
        let start_index = start % TiFs::BLOCK_SIZE;

        let first_block_size = (TiFs::BLOCK_SIZE - start_index) as usize;

        let (first_block, mut rest) = data.split_at(first_block_size.min(data.len()));

        let mut start_value = if start_index > 0 {
            self.get_for_update(start_key.clone())
                .await?
                .unwrap_or_else(|| vec![0; start_index as usize])
        } else {
            Vec::with_capacity(first_block.len())
        };

        start_value.extend_from_slice(first_block);
        self.put(start_key, start_value).await?;

        while rest.len() != 0 {
            block_index += 1;
            let key = ScopedKey::new(ino, block_index).scoped();
            let (curent_block, current_rest) =
                rest.split_at((TiFs::BLOCK_SIZE as usize).min(rest.len()));
            let mut value = curent_block.to_vec();
            if value.len() != TiFs::BLOCK_SIZE as usize
                && (block_index * TiFs::BLOCK_SIZE + value.len() as u64) < attr.size
            {
                let last_value = self
                    .get_for_update(key.clone())
                    .await?
                    .unwrap_or_else(|| vec![0; (attr.size % TiFs::BLOCK_SIZE) as usize]);

                value.extend_from_slice(&last_value[value.len()..]);
            }
            self.put(key, value).await?;
            rest = current_rest;
        }

        attr.atime = SystemTime::now();
        attr.mtime = SystemTime::now();
        attr.ctime = SystemTime::now();
        attr.set_size(attr.size.max(target));
        self.save_inode(&attr.into()).await?;
        debug!("write data: {}", String::from_utf8_lossy(&data));
        Ok(size)
    }

    pub async fn fallocate(&mut self, inode: &mut Inode, offset: i64, length: i64) -> Result<()> {
        let target_size = (offset + length) as u64;
        if target_size > inode.size {
            inode.set_size(target_size);
            inode.mtime = SystemTime::now();
            self.save_inode(inode).await?;
        }
        Ok(())
    }

    pub async fn mkdir(
        &mut self,
        parent: u64,
        name: OsString,
        mode: u32,
        gid: u32,
        uid: u32,
    ) -> Result<Inode> {
        let dir_mode = make_mode(FileType::Directory, as_file_perm(mode));
        let attr = self.make_inode(parent, name, dir_mode, gid, uid).await?;
        self.save_dir(attr.ino, &Directory::new()).await?;
        Ok(attr)
    }

    pub async fn read_dir(&mut self, ino: u64) -> Result<Directory> {
        let data = self
            .get(ScopedKey::dir(ino))
            .await?
            .ok_or_else(|| FsError::BlockNotFound {
                inode: ino,
                block: 0,
            })?;
        debug!("read data: {}", String::from_utf8_lossy(&data));
        Directory::deserialize(&data)
    }

    pub async fn read_dir_for_update(&mut self, ino: u64) -> Result<Directory> {
        let data = self
            .get_for_update(ScopedKey::dir(ino))
            .await?
            .ok_or_else(|| FsError::BlockNotFound {
                inode: ino,
                block: 0,
            })?;
        debug!("read data: {}", String::from_utf8_lossy(&data));
        Directory::deserialize(&data)
    }

    pub async fn save_dir(&mut self, ino: u64, dir: &Directory) -> Result<()> {
        let data = dir.serialize()?;
        let mut attr = self.read_inode(ino).await?;
        attr.set_size(data.len() as u64);
        attr.atime = SystemTime::now();
        attr.mtime = SystemTime::now();
        attr.ctime = SystemTime::now();
        self.save_inode(&attr).await?;
        self.put(ScopedKey::dir(ino), data).await?;
        Ok(())
    }
}

impl Deref for Txn {
    type Target = Transaction;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Txn {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
