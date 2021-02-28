# TiFS

A distributed POSIX filesystem based on TiKV, with partition tolerance and strict consistency.

[![pjdfstest](https://github.com/Hexilee/tifs/workflows/pjdfstest/badge.svg)](https://github.com/Hexilee/tifs/actions)

## Installation

### Binary(linux-amd64 only)

```bash
mkdir tmp
cd tmp
wget https://github.com/Hexilee/tifs/releases/download/v0.1.0/tifs-linux-amd64.tar.gz
tar -xvf tifs-linux-amd64.tar.gz
sudo ./install.sh
```

### Source code

```bash
git clone https://github.com/Hexilee/tifs.git
cd tifs
cargo build --features "binc" --no-default-features --release
sudo install target/release/mount /sbin/mount.tifs
```

## Usage
You need a tikv cluster to run tifs. [tiup](https://github.com/pingcap/tiup) is convenient to deploy one, just install it and run `tiup playground`.

```bash
mkdir ~/mnt
mount -t tifs tifs:127.0.0.1:2379 ~/mnt
```

## Development

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

for now, `user_allow_other` and `auto unmount` does not work for `FreeBSD`, using as `root` and manually `umount` is needed.

## Contribution

### Design

Please refer to the [design.md](contribution/design.md)

### FUSE
There are little docs about FUSE, refer to the [example](https://github.com/cberner/fuser/blob/master/examples/simple.rs) for the meaning of FUSE API.

### Deploy TiKV
Please refer to the [tikv-deploy.md](contribution/tikv-deploy.md).

## TODO

- [x] FUSE API
    - [x] init
    - [x] lookup
    - [x] getattr
    - [x] setattr
    - [x] readlink
    - [x] readdir
    - [x] open
    - [x] release
    - [x] read
    - [x] write
    - [x] mkdir
    - [x] rmdir
    - [x] mknod
    - [x] lseek
    - [x] unlink
    - [x] symlink
    - [x] rename
    - [x] link
    - [x] statfs
    - [x] create
    - [x] fallocate
    - [x] getlk
    - [x] setlk

- [ ] Testing and Benchmarking
    - [x] pjdfstest
    - [ ] fio

- [ ] Real-world usage
    - [x] vim
    - [ ] emacs
    - [x] git
    - [x] gcc
    - [x] rustc
    - [ ] cargo build
    - [x] npm install
    - [x] sqlite
    - [ ] tikv on tifs
    - [x] client runs on FreeBSD: simple case works
