use fuser::FileAttr;
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};
use libc::{LOCK_UN};
use super::error::{FsError, Result};
use super::serialize::{deserialize, serialize, ENCODING};
use super::tikv_fs::TiFs;
use std::collections::HashSet;


#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LockState {
    pub owner_set: HashSet<u64>,
    pub lk_type: i32
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Inode {
    pub file_attr: FileAttr,
    pub lock_state: LockState
}

impl Inode {
    fn update_blocks(&mut self) {
        self.blocks = (self.size + TiFs::BLOCK_SIZE - 1) / TiFs::BLOCK_SIZE;
    }

    pub fn set_size(&mut self, size: u64) {
        self.size = size;
        self.update_blocks();
    }

    pub fn serialize(&self) -> Result<Vec<u8>> {
        serialize(self).map_err(|err| FsError::Serialize {
            target: "inode",
            typ: ENCODING,
            msg: err.to_string(),
        })
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self> {
        deserialize(bytes).map_err(|err| FsError::Serialize {
            target: "inode",
            typ: ENCODING,
            msg: err.to_string(),
        })
    }
}

impl From<FileAttr> for Inode {
    fn from(attr: FileAttr) -> Self {
        Inode{file_attr:attr, lock_state: LockState::new(HashSet::new(), LOCK_UN)}
    }
}

impl From<Inode> for FileAttr {
    fn from(inode: Inode) -> Self {
        inode.file_attr
    }
}

impl From<Inode> for LockState {
    fn from(inode: Inode) -> Self {
        inode.lock_state
    }
}

impl Deref for Inode {
    type Target = FileAttr;

    fn deref(&self) -> &Self::Target {
        &self.file_attr
    }
}

impl DerefMut for Inode {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.file_attr
    }
}

impl LockState {
    pub fn new(owner_set: HashSet<u64>, lk_type: i32) -> LockState {
        LockState{
            owner_set,
            lk_type
        }
    }
}
