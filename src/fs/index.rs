use std::convert::TryInto;
use std::mem::size_of;

use bytes::Bytes;
use bytestring::ByteString;
use serde::{Deserialize, Serialize};
use tikv_client::Key;

use super::error::{FsError, Result};
use super::serialize::{deserialize, serialize, ENCODING};

pub const INO_LEN: usize = size_of::<u64>();

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Hash, Clone)]
pub struct IndexKey {
    parent: u64,
    name: ByteString,
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Hash, Clone, Copy, Deserialize, Serialize)]
pub struct IndexValue {
    pub ino: u64,
}

impl IndexKey {
    pub fn new(parent: u64, name: ByteString) -> Self {
        Self { parent, name }
    }

    pub fn parent(&self) -> u64 {
        self.parent
    }

    pub fn name(&self) -> &str {
        &*self.name
    }
}

impl IndexValue {
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

impl From<Key> for IndexKey {
    fn from(key: Key) -> Self {
        let data: Vec<u8> = key.into();
        debug_assert!(data.len() >= INO_LEN);
        Self::new(
            u64::from_be_bytes(data[..INO_LEN].try_into().unwrap()),
            unsafe { ByteString::from_bytes_unchecked(Bytes::copy_from_slice(&data[INO_LEN..])) },
        )
    }
}

impl From<IndexKey> for Key {
    fn from(index: IndexKey) -> Self {
        let mut data = Vec::with_capacity(INO_LEN + index.name().len());
        data.extend(index.parent().to_be_bytes().iter());
        data.extend(index.name().as_bytes().iter());
        data.into()
    }
}
