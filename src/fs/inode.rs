use bincode::{deserialize, serialize};
use fuser::FileAttr;
use serde::{Deserialize, Serialize};

use super::error::{FsError, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Inode(pub FileAttr);

impl Inode {
    pub fn serialize(&self) -> Result<Vec<u8>> {
        serialize(self).map_err(|err| FsError::Serialize {
            target: "inode",
            typ: "bincode",
            msg: err.to_string(),
        })
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self> {
        deserialize(bytes).map_err(|err| FsError::Serialize {
            target: "inode",
            typ: "bincode",
            msg: err.to_string(),
        })
    }
}

impl From<FileAttr> for Inode {
    fn from(attr: FileAttr) -> Self {
        Inode(attr)
    }
}
