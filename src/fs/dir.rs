use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use fuser::FileType;
use serde::{Deserialize, Serialize};

use super::error::{FsError, Result};
use super::key::ROOT_INODE;
use super::reply::DirItem;
use super::serialize::{deserialize, serialize, ENCODING};

#[derive(Debug, Serialize, Deserialize)]
pub struct Directory(HashMap<String, DirItem>);

impl Directory {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn add(&mut self, item: DirItem) -> Option<DirItem> {
        self.0.insert(item.name.clone(), item)
    }

    pub fn serialize(&self) -> Result<Vec<u8>> {
        serialize(self).map_err(|err| FsError::Serialize {
            target: "directory",
            typ: ENCODING,
            msg: err.to_string(),
        })
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self> {
        deserialize(bytes).map_err(|err| FsError::Serialize {
            target: "directory",
            typ: ENCODING,
            msg: err.to_string(),
        })
    }

    pub fn into_map(self) -> HashMap<String, DirItem> {
        self.0
    }
}

impl Deref for Directory {
    type Target = HashMap<String, DirItem>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Directory {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
