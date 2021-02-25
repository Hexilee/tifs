use serde::{Deserialize, Serialize};

use super::error::{FsError, Result};
use super::serialize::{deserialize, serialize, ENCODING};

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Hash, Clone, Copy, Deserialize, Serialize)]
pub struct FileHandler {
    // TODO: add open flags
    pub cursor: u64,
}

impl FileHandler {
    pub const fn new(cursor: u64) -> Self {
        Self { cursor }
    }

    pub fn serialize(&self) -> Result<Vec<u8>> {
        serialize(self).map_err(|err| FsError::Serialize {
            target: "file handler",
            typ: ENCODING,
            msg: err.to_string(),
        })
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self> {
        deserialize(bytes).map_err(|err| FsError::Serialize {
            target: "file handler",
            typ: ENCODING,
            msg: err.to_string(),
        })
    }
}

impl Default for FileHandler {
    fn default() -> Self {
        Self::new(0)
    }
}
