# TiFS

A distributed file system based on TiKV.

## Run

You need a tikv cluster to run tifs. [tiup](https://github.com/pingcap/tiup) is convenient to deploy one, just install it and run `tiup playground`.

### Develop

```bash
cargo build
mkdir ~/mnt
RUST_LOG=debug target/debug/tifs --mount-point ~/mnt
```

Then you can open another shell and play with tifs in `~/mnt`.

Maybe you should enable `user_allow_other` in `/etc/fuse.conf`.

for developing under `FreeBSD`, make sure the following dependencies are met.

```bash
pkg install llvm protobuf pkgconf fusefs-libs3 cmake
```

### Product

```bash
cargo build --features "binc" --no-default-features --release
mkdir ~/mnt
RUST_LOG=info target/release/tifs --mount-point ~/mnt
```

### Installation

```bash
cargo build --features "binc" --no-default-features --release
sudo install target/release/mount /sbin/mount.tifs
mkdir ~/mnt
mount -t tifs tifs:127.0.0.1:2379 ~/mnt
```

## FUSE
There is little docs about FUSE, refer to [example](https://github.com/cberner/fuser/blob/master/examples/simple.rs) for the meaning of FUSE API.

## TODO

> Item end with '?' means there are probably some bugs; Item end with '!' means there must be some bugs.

- [ ] FUSE API
    - [x] init
    - [x] lookup
    - [x] getattr
    - [x] setattr ?
    - [x] readlink !
    - [x] readdir
    - [x] open
    - [x] release
    - [x] read
    - [x] write
    - [x] mkdir
    - [x] rmdir !
    - [x] mknod
    - [x] lseek
    - [x] unlink
    - [x] symlink
    - [x] rename
    - [x] link
    - [x] statfs ???
    - [ ] access
    - [x] create
    - [x] fallocate
    - [ ] getlk
    - [ ] setlk
    - [ ] copy\_file\_range

- [x] Consistency
    - [x] select for update
        - [x] next inode
        - [x] nlinks
        - [x] directory
        - [x] start and end block
    - [x] direct io

- [ ] Performance
    - [ ] cache
        - [ ] inode table
        - [ ] directory

- [ ] Testing and Benchmarking
    - [ ] unit test
    - [ ] benchmark

- [ ] Other
    - [ ] real-world usage
        - [x] vim
        - [ ] emacs
        - [x] git
        - [x] gcc
        - [x] rustc
        - [ ] cargo build
        - [ ] npm install
        - [x] sqlite
        - [ ] tikv on tifs
