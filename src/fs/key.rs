use std::convert::TryInto;
use std::mem::size_of;
use std::ops::Range;

use tikv_client::Key;

pub const INODE_SCOPE: u64 = 0;
pub const ROOT_INODE: u64 = 1;
pub const KEY_LEN: usize = size_of::<u64>() * 2;

pub struct ScopedKey {
    scope: u64,
    key: u64,
}

impl ScopedKey {
    pub fn new(scope: u64, key: u64) -> Self {
        Self { scope, key }
    }

    pub fn inode(inode: u64) -> Self {
        Self::new(INODE_SCOPE, inode)
    }

    pub fn root() -> Self {
        Self::inode(ROOT_INODE)
    }

    pub fn block_range(inode: u64, block_range: Range<u64>) -> Range<Key> {
        debug_assert_ne!(0, inode);
        Self::new(inode, block_range.start).scoped()..Self::new(inode, block_range.end).scoped()
    }

    pub fn scoped(&self) -> Key {
        let mut data = Vec::with_capacity(KEY_LEN);
        data.extend(self.scope.to_be_bytes().into_iter());
        data.extend(self.key.to_be_bytes().into_iter());
        data.into()
    }

    pub fn scope(&self) -> u64 {
        self.scope
    }

    pub fn key(&self) -> u64 {
        self.key
    }
}

impl From<Key> for ScopedKey {
    fn from(key: Key) -> Self {
        let data: Vec<u8> = key.into();
        debug_assert_eq!(KEY_LEN, data.len());
        Self::new(
            u64::from_be_bytes(data[..8].try_into().unwrap()),
            u64::from_be_bytes(data[8..].try_into().unwrap()),
        )
    }
}
