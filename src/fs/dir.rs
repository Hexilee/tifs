use std::collections::HashMap;
use std::ffi::OsString;
use std::ops::Deref;

use super::error::{FsError, Result};
use super::reply::DirItem;

use bincode::{deserialize, serialize};
use fuser::FileType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Directory(HashMap<OsString, DirItem>);

impl Directory {
    pub fn new(ino: u64, parent: u64) -> Self {
        Directory(HashMap::new())
            .add(DirItem {
                ino: ino,
                name: ".".into(),
                typ: FileType::Directory,
            })
            .add(DirItem {
                ino: parent,
                name: "..".into(),
                typ: FileType::Directory,
            })
    }

    pub fn add(mut self, item: DirItem) -> Self {
        self.0.insert(item.name.clone(), item);
        self
    }

    pub fn serialize(&self) -> Result<Vec<u8>> {
        serialize(self).map_err(|err| FsError::Serialize {
            target: "directory",
            typ: "bincode",
            msg: err.to_string(),
        })
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self> {
        deserialize(bytes).map_err(|err| FsError::Serialize {
            target: "directory",
            typ: "bincode",
            msg: err.to_string(),
        })
    }
}

impl Deref for Directory {
    type Target = HashMap<OsString, DirItem>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
