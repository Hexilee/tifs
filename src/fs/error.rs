use thiserror::Error;
use tikv_client::Key;
use tracing::error;

use super::key::ScopedKey;

#[derive(Error, Debug)]
pub enum FsError {
    #[error("unimplemented")]
    Unimplemented,

    #[error("fail to serialize/deserialize {target} as {typ}: `{msg}`")]
    Serialize {
        target: &'static str,
        typ: &'static str,
        msg: String,
    },

    #[error("cannot find path({file})")]
    FileNotFound { file: String },

    #[error("file({file}) already exist")]
    FileExist { file: String },

    #[error("cannot find inode({inode})")]
    InodeNotFound { inode: u64 },

    #[error("cannot find fh({fh})")]
    FhNotFound { fh: u64 },

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

    #[error("strip prefix error")]
    StripPrefixError(#[from] std::path::StripPrefixError),

    #[error("unknown error({0})")]
    UnknownError(String),

    #[error("invalid lock")]
    InvalidLock,
}

pub type Result<T> = std::result::Result<T, FsError>;

impl FsError {
    pub fn unimplemented() -> Self {
        Self::Unimplemented
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
        if let tikv_client::Error::RegionForKeyNotFound { key: key_data } = err {
            let key: Key = key_data.clone().into();
            let scoped_key: ScopedKey = key.into();
            Self::InodeNotFound {
                inode: scoped_key.key(),
            }
        } else {
            Self::UnknownError(err.to_string())
        }
    }
}

impl Into<libc::c_int> for FsError {
    fn into(self) -> libc::c_int {
        use FsError::*;

        match self {
            Unimplemented => libc::ENOSYS,
            FileNotFound { file: _ } => libc::ENOENT,
            FileExist { file: _ } => libc::EEXIST,
            InodeNotFound { inode: _ } => libc::EFAULT,
            FhNotFound { fh: _ } => libc::EBADF,
            InvalidOffset { ino: _, offset: _ } => libc::EINVAL,
            UnknownWhence { whence: _ } => libc::EINVAL,
            BlockNotFound { inode: _, block: _ } => libc::EINVAL,
            DirNotEmpty { dir: _ } => libc::ENOTEMPTY,
            UnknownFileType => libc::EINVAL,
            InvalidStr => libc::EINVAL,
            _ => libc::EFAULT,
        }
    }
}
