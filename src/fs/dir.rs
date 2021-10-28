use super::error::{FsError, Result};
use super::reply::DirItem;
use super::serialize::{deserialize, serialize, ENCODING};

pub type Directory = Vec<DirItem>;

pub fn encode(dir: &[DirItem]) -> Result<Vec<u8>> {
    serialize(dir).map_err(|err| FsError::Serialize {
        target: "directory",
        typ: ENCODING,
        msg: err.to_string(),
    })
}

pub fn decode(bytes: &[u8]) -> Result<Directory> {
    deserialize(bytes).map_err(|err| FsError::Serialize {
        target: "directory",
        typ: ENCODING,
        msg: err.to_string(),
    })
}

pub fn encode_item(item: &DirItem) -> Result<Vec<u8>> {
    serialize(item).map_err(|err| FsError::Serialize {
        target: "dir item",
        typ: ENCODING,
        msg: err.to_string(),
    })
}

pub fn decode_item(bytes: &[u8]) -> Result<DirItem> {
    deserialize(bytes).map_err(|err| FsError::Serialize {
        target: "dir item",
        typ: ENCODING,
        msg: err.to_string(),
    })
}
