use std::collections::HashSet;
use std::ops::{Deref, DerefMut};

use fuser::FileAttr;
use libc::F_UNLCK;
use serde::{Deserialize, Serialize};

use super::error::{FsError, Result};
use super::serialize::{deserialize, serialize, ENCODING};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LockState {
    pub owner_set: HashSet<u64>,
    #[cfg(target_os = "linux")]
    pub lk_type: i32,
    #[cfg(any(target_os = "freebsd", target_os = "macos"))]
    pub lk_type: i16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Inode {
    pub file_attr: FileAttr,
    pub lock_state: LockState,
    pub inline_data: Option<Vec<u8>>,
    pub next_fh: u64,
    pub opened_fh: u64,
}

impl Inode {
    fn update_blocks(&mut self, block_size: u64) {
        self.blocks = (self.size + block_size - 1) / block_size;
    }

    pub fn set_size(&mut self, size: u64, block_size: u64) {
        self.size = size;
        self.update_blocks(block_size);
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
        Inode {
            file_attr: attr,
            lock_state: LockState::new(HashSet::new(), F_UNLCK),
            inline_data: None,
            next_fh: 0,
            opened_fh: 0,
        }
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
    #[cfg(target_os = "linux")]
    pub fn new(owner_set: HashSet<u64>, lk_type: i32) -> LockState {
        LockState { owner_set, lk_type }
    }
    #[cfg(any(target_os = "freebsd", target_os = "macos"))]
    pub fn new(owner_set: HashSet<u64>, lk_type: i16) -> LockState {
        LockState { owner_set, lk_type }
    }
}
