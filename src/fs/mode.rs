use fuser::FileType;

pub const fn as_file_perm(mode: u32) -> u16 {
    (mode & !(libc::S_ISUID | libc::S_ISGID) as u32) as _
}

#[cfg(any(target_os = "freebsd", target_os = "macos"))]
pub fn as_file_kind(mode: u32) -> FileType {
    use FileType::*;

    match mode as u16 & libc::S_IFMT {
        libc::S_IFREG => RegularFile,
        libc::S_IFLNK => Symlink,
        libc::S_IFDIR => Directory,
        libc::S_IFIFO => NamedPipe,
        libc::S_IFBLK => BlockDevice,
        libc::S_IFCHR => CharDevice,
        libc::S_IFSOCK => Socket,
        _ => unimplemented!("{}", mode),
    }
}

#[cfg(target_os = "linux")]
pub fn as_file_kind(mode: u32) -> FileType {
    use FileType::*;

    match mode & libc::S_IFMT as u32 {
        libc::S_IFREG => RegularFile,
        libc::S_IFLNK => Symlink,
        libc::S_IFDIR => Directory,
        libc::S_IFIFO => NamedPipe,
        libc::S_IFBLK => BlockDevice,
        libc::S_IFCHR => CharDevice,
        libc::S_IFSOCK => Socket,
        _ => unimplemented!("{}", mode),
    }
}

#[cfg(any(target_os = "freebsd", target_os = "macos"))]
pub fn make_mode(tpy: FileType, perm: u16) -> u32 {
    use FileType::*;

    let kind = match tpy {
        RegularFile => libc::S_IFREG,
        Symlink => libc::S_IFLNK,
        Directory => libc::S_IFDIR,
        NamedPipe => libc::S_IFIFO,
        BlockDevice => libc::S_IFBLK,
        CharDevice => libc::S_IFCHR,
        Socket => libc::S_IFSOCK,
    };

    kind as u32 | perm as u32
}

#[cfg(target_os = "linux")]
pub fn make_mode(tpy: FileType, perm: u16) -> u32 {
    use FileType::*;

    let kind = match tpy {
        RegularFile => libc::S_IFREG,
        Symlink => libc::S_IFLNK,
        Directory => libc::S_IFDIR,
        NamedPipe => libc::S_IFIFO,
        BlockDevice => libc::S_IFBLK,
        CharDevice => libc::S_IFCHR,
        Socket => libc::S_IFSOCK,
    };

    kind | perm as u32
}
