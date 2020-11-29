use nix::errno::Errno;
use nix::Error;
use thiserror::Error;
use tikv_client::Key;
use tracing::error;

use super::key::ScopedKey;

#[derive(Error, Debug)]
pub enum FsError {
    #[error("errno {0}")]
    Sys(Errno),

    #[error("cannot find inode({inode})")]
    InodeNotFound { inode: u64 },

    #[error("cannot find fh({fh})")]
    FhNotFound { fh: u64 },

    #[error("invalid string")]
    InvalidStr,

    #[error("unknown file type")]
    UnknownFileType,

    #[error("strip prefix error")]
    StripPrefixError(#[from] std::path::StripPrefixError),

    #[error("unknown error")]
    UnknownError,
}

pub type Result<T> = std::result::Result<T, FsError>;

impl FsError {
    pub fn last() -> Self {
        nix::Error::last().into()
    }
}

impl From<nix::Error> for FsError {
    fn from(err: Error) -> Self {
        // TODO: match more error types
        match err {
            Error::Sys(errno) => Self::Sys(errno),
            _ => {
                error!("unknown error {:?}", err);
                Self::UnknownError
            }
        }
    }
}

impl From<std::ffi::NulError> for FsError {
    fn from(_: std::ffi::NulError) -> Self {
        Self::InvalidStr
    }
}

impl From<std::io::Error> for FsError {
    fn from(err: std::io::Error) -> Self {
        error!("unknown error {:?}", err);
        Self::UnknownError
    }
}

impl From<tikv_client::Error> for FsError {
    fn from(err: tikv_client::Error) -> Self {
        use tikv_client::ErrorKind;

        if let ErrorKind::RegionForKeyNotFound { key: key_data } = err.kind() {
            let key: Key = key_data.clone().into();
            let scoped_key: ScopedKey = key.into();
            Self::InodeNotFound {
                inode: scoped_key.key(),
            }
        } else {
            Self::UnknownError
        }
    }
}

impl Into<libc::c_int> for FsError {
    fn into(self) -> libc::c_int {
        use FsError::*;

        match self {
            Sys(errno) => errno as i32,
            InodeNotFound { inode: _ } => libc::EFAULT,
            FhNotFound { fh: _ } => libc::EFAULT,
            UnknownFileType => libc::EINVAL,
            InvalidStr => libc::EINVAL,
            _ => libc::EFAULT,
        }
    }
}
