use thiserror::Error;
use tracing::error;

#[derive(Error, Debug)]
pub enum FsError {
    #[error("unimplemented")]
    Unimplemented,

    #[error("invalid scoped key: {0:?}")]
    InvalidScopedKey(Vec<u8>),

    #[error("fail to serialize/deserialize {target} as {typ}: `{msg}`")]
    Serialize {
        target: &'static str,
        typ: &'static str,
        msg: String,
    },

    #[error("name of file({file}) is too long")]
    NameTooLong { file: String },

    #[error("cannot find path({file})")]
    FileNotFound { file: String },

    #[error("file({file}) already exist")]
    FileExist { file: String },

    #[error("cannot find inode({inode})")]
    InodeNotFound { inode: u64 },

    #[error("cannot find {ino}({fh})")]
    FhNotFound { ino: u64, fh: u64 },

    #[error("invalid offset({offset}) of ino({ino})")]
    InvalidOffset { ino: u64, offset: i64 },

    #[error("unknown whence({whence})")]
    UnknownWhence { whence: i32 },

    #[error("cannot find block(<{inode}>[{block}])")]
    BlockNotFound { inode: u64, block: u64 },

    #[error("dir({dir}) not empty")]
    DirNotEmpty { dir: String },

    #[error("invalid string")]
    InvalidStr,

    #[error("unknown file type")]
    UnknownFileType,

    #[error("key error: {0}")]
    KeyError(String),

    #[error("excess max retry times: {0}")]
    RetryTimesExcess(u64),

    #[error("strip prefix error")]
    StripPrefixError(#[from] std::path::StripPrefixError),

    #[error("unknown error({0})")]
    UnknownError(String),

    #[error("invalid lock")]
    InvalidLock,

    #[error("block size conflicts: origin({origin}) != new({new})")]
    BlockSizeConflict { origin: u64, new: u64 },

    #[error("no space left: MaxSize({0})")]
    NoSpaceLeft(u64),
}

pub type Result<T> = std::result::Result<T, FsError>;

impl FsError {
    pub fn unimplemented() -> Self {
        Self::Unimplemented
    }

    pub fn block_size_conflict(origin: u64, new: u64) -> Self {
        Self::BlockSizeConflict { origin, new }
    }
}

impl From<std::ffi::NulError> for FsError {
    fn from(_: std::ffi::NulError) -> Self {
        Self::InvalidStr
    }
}

impl From<std::io::Error> for FsError {
    fn from(err: std::io::Error) -> Self {
        Self::UnknownError(err.to_string())
    }
}

impl From<tikv_client::Error> for FsError {
    fn from(err: tikv_client::Error) -> Self {
        use tikv_client::Error::*;

        match err {
            KeyError(err) => Self::KeyError(format!("{:?}", err)),
            MultipleKeyErrors(errs) => Self::KeyError(format!("{:?}", errs)),
            _ => Self::UnknownError(err.to_string()),
        }
    }
}

impl From<FsError> for libc::c_int {
    fn from(e: FsError) -> Self {
        use FsError::*;
        match e {
            Unimplemented => libc::ENOSYS,
            NameTooLong { file: _ } => libc::ENAMETOOLONG,
            FileNotFound { file: _ } => libc::ENOENT,
            FileExist { file: _ } => libc::EEXIST,
            InodeNotFound { inode: _ } => libc::EFAULT,
            FhNotFound { ino: _, fh: _ } => libc::EBADF,
            InvalidOffset { ino: _, offset: _ } => libc::EINVAL,
            UnknownWhence { whence: _ } => libc::EINVAL,
            BlockNotFound { inode: _, block: _ } => libc::EINVAL,
            DirNotEmpty { dir: _ } => libc::ENOTEMPTY,
            UnknownFileType => libc::EINVAL,
            KeyError(_) => libc::EAGAIN,
            RetryTimesExcess(_) => libc::EAGAIN,
            InvalidStr => libc::EINVAL,
            BlockSizeConflict { origin: _, new: _ } => libc::EINVAL,
            NoSpaceLeft(_) => libc::ENOSPC,
            _ => libc::EFAULT,
        }
    }
}
