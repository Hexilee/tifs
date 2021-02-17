use serde::{Deserialize, Serialize};

use super::error::{FsError, Result};
use super::serialize::{deserialize, serialize, ENCODING};

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Hash, Clone, Copy, Deserialize, Serialize)]
pub struct Index {
    pub ino: u64,
}

impl Index {
    pub const fn new(ino: u64) -> Self {
        Self { ino }
    }

    pub fn serialize(&self) -> Result<Vec<u8>> {
        serialize(self).map_err(|err| FsError::Serialize {
            target: "index",
            typ: ENCODING,
            msg: err.to_string(),
        })
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self> {
        deserialize(bytes).map_err(|err| FsError::Serialize {
            target: "index",
            typ: ENCODING,
            msg: err.to_string(),
        })
    }
}
