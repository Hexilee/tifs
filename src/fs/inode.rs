use bincode::{deserialize, serialize, Result};
use fuser::FileAttr;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Inode(pub FileAttr);

impl Inode {
    pub fn serialize(&self) -> Result<Vec<u8>> {
        serialize(self)
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self> {
        deserialize(bytes)
    }
}
