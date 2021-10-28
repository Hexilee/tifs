use std::fmt::Debug;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fuser::*;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, trace};

use super::error::Result;

pub fn get_time() -> Duration {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap()
}

#[derive(Debug)]
pub struct Entry {
    pub time: Duration,
    pub stat: FileAttr,
    pub generation: u64,
}

impl Entry {
    pub fn new(stat: FileAttr, generation: u64) -> Self {
        Self {
            time: get_time(),
            stat,
            generation,
        }
    }
}

#[derive(Debug)]
pub struct Open {
    pub fh: u64,
    pub flags: u32,
}
impl Open {
    pub fn new(fh: u64, flags: u32) -> Self {
        Self { fh, flags }
    }
}

#[derive(Debug)]
pub struct Attr {
    pub time: Duration,
    pub attr: FileAttr,
}
impl Attr {
    pub fn new(attr: FileAttr) -> Self {
        Self {
            time: get_time(),
            attr,
        }
    }
}

#[derive(Debug)]
pub struct Data {
    pub data: Vec<u8>,
}
impl Data {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirItem {
    pub ino: u64,
    pub name: String,
    pub typ: FileType,
}
#[derive(Debug, Default)]
pub struct Dir {
    offset: usize,
    items: Vec<DirItem>,
}

impl Dir {
    pub fn offset(offset: usize) -> Self {
        Self {
            offset,
            items: Default::default(),
        }
    }

    pub fn new() -> Self {
        Default::default()
    }

    pub fn push(&mut self, item: DirItem) {
        self.items.push(item)
    }
}

#[derive(Debug, Default)]
pub struct DirPlus {
    offset: usize,
    items: Vec<(DirItem, Entry)>,
}

impl DirPlus {
    pub fn offset(offset: usize) -> Self {
        Self {
            offset,
            items: Default::default(),
        }
    }

    pub fn new() -> Self {
        Default::default()
    }

    pub fn push(&mut self, item: DirItem, entry: Entry) {
        self.items.push((item, entry))
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct StatFs {
    pub blocks: u64,
    pub bfree: u64,
    pub bavail: u64,
    pub files: u64,
    pub ffree: u64,
    pub bsize: u32,
    pub namelen: u32,
    pub frsize: u32,
}

impl StatFs {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        blocks: u64,
        bfree: u64,
        bavail: u64,
        files: u64,
        ffree: u64,
        bsize: u32,
        namelen: u32,
        frsize: u32,
    ) -> Self {
        Self {
            blocks,
            bfree,
            bavail,
            files,
            ffree,
            bsize,
            namelen,
            frsize,
        }
    }
}

#[derive(Debug)]
pub struct Write {
    pub size: u32,
}
impl Write {
    pub fn new(size: u32) -> Self {
        Self { size }
    }
}

#[derive(Debug)]
pub struct Create {
    pub ttl: Duration,
    pub attr: FileAttr,
    pub generation: u64,
    pub fh: u64,
    pub flags: u32,
}
impl Create {
    pub fn new(attr: FileAttr, generation: u64, fh: u64, flags: u32) -> Self {
        Self {
            ttl: get_time(),
            attr,
            generation,
            fh,
            flags,
        }
    }
}

#[derive(Debug)]
pub struct Lock {
    pub start: u64,
    pub end: u64,
    pub typ: i32,
    pub pid: u32,
}

impl Lock {
    pub fn _new(start: u64, end: u64, typ: i32, pid: u32) -> Self {
        Self {
            start,
            end,
            typ,
            pid,
        }
    }
}

#[derive(Debug)]
pub enum Xattr {
    Data { data: Vec<u8> },
    Size { size: u32 },
}
impl Xattr {
    pub fn data(data: Vec<u8>) -> Self {
        Xattr::Data { data }
    }
    pub fn size(size: u32) -> Self {
        Xattr::Size { size }
    }
}

#[derive(Debug)]
pub struct Bmap {
    block: u64,
}

impl Bmap {
    pub fn new(block: u64) -> Self {
        Self { block }
    }
}

#[derive(Debug)]
pub struct Lseek {
    offset: i64,
}

impl Lseek {
    pub fn new(offset: i64) -> Self {
        Self { offset }
    }
}

pub trait FsReply<T: Debug>: Sized {
    fn reply_ok(self, item: T);
    fn reply_err(self, err: libc::c_int);

    fn reply(self, id: u64, result: Result<T>) {
        match result {
            Ok(item) => {
                trace!("ok. reply for request({})", id);
                self.reply_ok(item)
            }
            Err(err) => {
                debug!("err. reply with {} for request ({})", err, id);

                let err = err.into();
                if err == -1 {
                    error!("returned -1");
                }
                self.reply_err(err)
            }
        }
    }
}

impl FsReply<Entry> for ReplyEntry {
    fn reply_ok(self, item: Entry) {
        self.entry(&item.time, &item.stat, item.generation);
    }
    fn reply_err(self, err: libc::c_int) {
        self.error(err);
    }
}

impl FsReply<Open> for ReplyOpen {
    fn reply_ok(self, item: Open) {
        self.opened(item.fh, item.flags);
    }
    fn reply_err(self, err: libc::c_int) {
        self.error(err);
    }
}

impl FsReply<Attr> for ReplyAttr {
    fn reply_ok(self, item: Attr) {
        self.attr(&item.time, &item.attr);
    }
    fn reply_err(self, err: libc::c_int) {
        self.error(err);
    }
}

impl FsReply<Data> for ReplyData {
    fn reply_ok(self, item: Data) {
        self.data(item.data.as_slice());
    }
    fn reply_err(self, err: libc::c_int) {
        self.error(err);
    }
}

impl FsReply<Dir> for ReplyDirectory {
    fn reply_ok(mut self, dir: Dir) {
        for (index, item) in dir.items.into_iter().enumerate() {
            if self.add(
                item.ino,
                (index + 1 + dir.offset) as i64,
                item.typ,
                item.name,
            ) {
                break;
            }
        }
        self.ok()
    }
    fn reply_err(self, err: libc::c_int) {
        self.error(err);
    }
}

impl FsReply<DirPlus> for ReplyDirectoryPlus {
    fn reply_ok(mut self, dir: DirPlus) {
        for (index, (item, entry)) in dir.items.into_iter().enumerate() {
            if self.add(
                item.ino,
                (dir.offset + index) as i64,
                item.name,
                &entry.time,
                &entry.stat,
                entry.generation,
            ) {
                break;
            }
        }
        self.ok()
    }
    fn reply_err(self, err: libc::c_int) {
        self.error(err);
    }
}

impl FsReply<StatFs> for ReplyStatfs {
    fn reply_ok(self, item: StatFs) {
        self.statfs(
            item.blocks,
            item.bfree,
            item.bavail,
            item.files,
            item.ffree,
            item.bsize,
            item.namelen,
            item.frsize,
        )
    }
    fn reply_err(self, err: libc::c_int) {
        self.error(err);
    }
}

impl FsReply<Write> for ReplyWrite {
    fn reply_ok(self, item: Write) {
        self.written(item.size);
    }
    fn reply_err(self, err: libc::c_int) {
        self.error(err);
    }
}

impl FsReply<Create> for ReplyCreate {
    fn reply_ok(self, item: Create) {
        self.created(&item.ttl, &item.attr, item.generation, item.fh, item.flags);
    }
    fn reply_err(self, err: libc::c_int) {
        self.error(err);
    }
}

impl FsReply<Lock> for ReplyLock {
    fn reply_ok(self, item: Lock) {
        self.locked(item.start, item.end, item.typ, item.pid);
    }
    fn reply_err(self, err: libc::c_int) {
        self.error(err);
    }
}

impl FsReply<Xattr> for ReplyXattr {
    fn reply_ok(self, item: Xattr) {
        use Xattr::*;
        match item {
            Data { data } => self.data(data.as_slice()),
            Size { size } => self.size(size),
        }
    }
    fn reply_err(self, err: libc::c_int) {
        self.error(err);
    }
}

impl FsReply<Bmap> for ReplyBmap {
    fn reply_ok(self, item: Bmap) {
        self.bmap(item.block)
    }
    fn reply_err(self, err: libc::c_int) {
        self.error(err);
    }
}

impl FsReply<Lseek> for ReplyLseek {
    fn reply_ok(self, item: Lseek) {
        self.offset(item.offset)
    }
    fn reply_err(self, err: libc::c_int) {
        self.error(err);
    }
}

impl FsReply<()> for ReplyEmpty {
    fn reply_ok(self, _: ()) {
        self.ok();
    }
    fn reply_err(self, err: libc::c_int) {
        self.error(err);
    }
}
