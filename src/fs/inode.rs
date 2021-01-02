use fuser::FileAttr;
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

use super::error::{FsError, Result};
use super::serialize::{deserialize, serialize, ENCODING};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Inode(pub FileAttr);

impl Inode {
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
        Inode(attr)
    }
}

impl From<Inode> for FileAttr {
    fn from(inode: Inode) -> Self {
        inode.0
    }
}

impl Deref for Inode {
    type Target = FileAttr;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Inode {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
