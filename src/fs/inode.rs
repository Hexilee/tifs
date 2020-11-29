use std::convert::TryInto;
use std::mem::{size_of, transmute};

use fuse::FileAttr;

#[derive(Clone, Copy, Debug)]
pub struct Inode(pub FileAttr);

impl From<Inode> for Vec<u8> {
    fn from(inode: Inode) -> Self {
        unsafe { transmute::<_, [u8; size_of::<Inode>()]>(inode)[..].to_owned() }
    }
}

impl From<Vec<u8>> for Inode {
    fn from(data: Vec<u8>) -> Self {
        debug_assert_eq!(size_of::<Self>(), data.len());
        unsafe { transmute::<[u8; size_of::<Self>()], _>(data.try_into().unwrap()) }
    }
}
