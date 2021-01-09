use fuser::FileAttr;
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};
use libc::{F_RDLCK, F_WRLCK, F_UNLCK, LOCK_SH, LOCK_EX, LOCK_UN, LOCK_NB};
use super::error::{FsError, Result};
use super::serialize::{deserialize, serialize, ENCODING};
use super::tikv_fs::TiFs;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LockType(pub i32);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LockState(pub HashSet<u64>, pub LockType);
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
        Inode{file_attr:attr, lock_state: LockState::new(HashSet::new(),LockType(LOCK_UN))}
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
    pub fn new(set: HashSet<u64>, typ: LockType) -> LockState {
        LockState(set, typ)
    }
}