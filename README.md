# TiFS

A distributed POSIX filesystem based on TiKV, with partition tolerance and strict consistency.

[![pjdfstest](https://github.com/Hexilee/tifs/workflows/pjdfstest/badge.svg)](https://github.com/Hexilee/tifs/actions)

## Environment

### Build 

- Linux
`libfuse` and `build-essential` are required, in ubuntu/debian:

```
sudo apt install -y libfuse-dev libfuse3-dev build-essential
```

- macOS
```
brew install --cask osxfuse
```

### Runtime
- Linux
`fuse3` and `openssl` are required, in ubuntu/debian:

```
sudo apt-get install -y libfuse3-dev fuse3 libssl-dev
```

- macOS

```
brew install --cask osxfuse
```

In Catalina or former version, you need to load osxfuse into the kernel:

```
/Library/Filesystems/osxfuse.fs/Contents/Resources/load_osxfuse
```

## Installation

### Container
You can use the image on [docker hub](https://hub.docker.com/repository/docker/hexilee/tifs) or build from the [Dockerfile](Dockerfile).

### Binary(linux-amd64 or darwin-amd64)

```bash
mkdir tmp
cd tmp
wget https://github.com/Hexilee/tifs/releases/download/v0.2.1/tifs-linux-amd64.tar.gz
tar -xvf tifs-linux-amd64.tar.gz
sudo ./install.sh
```

> The `install.sh` may fail in macOS Catalina or Big Sur because of the 
> [SIP](https://developer.apple.com/documentation/security/disabling_and_enabling_system_integrity_protection). 
> 
> You can just use the `target/release/mount` to mount tifs.
> ### Example
> ```
> target/release/mount tifs:127.0.0.1:2379 ~/mnt
> ```

### Source code

```bash
git clone https://github.com/Hexilee/tifs.git
cd tifs
sudo make install
```

## Usage
You need a tikv cluster to run tifs. [tiup](https://github.com/pingcap/tiup) is convenient to deploy one, just install it and run `tiup playground`.

### Container

```bash
docker run -d --device /dev/fuse \
    --cap-add SYS_ADMIN \
    -v <mount point>:/mnt:shared \
    hexilee/tifs:0.2.2 --mount-point /mnt --pd-endpoints <endpoints>
```

#### TLS
You need ca.crt, client.crt and client.key to access TiKV cluster on TLS. 

> It will be convenient to get self-signed certificates by [sign-cert.sh](sign-cert.sh)(based on the [easy-rsa](https://github.com/OpenVPN/easy-rsa)).

You should place them into a directory <cert dir> and execute following docker command.

```bash
docker run -d --device /dev/fuse \
    --cap-add SYS_ADMIN \
    -v <cert dir>:/root/.tifs/tls \
    -v <mount point>:/mnt:shared \
    hexilee/tifs:0.2.2 --mount-point /mnt --pd-endpoints <endpoints>
```

### Binary

```bash
mkdir <mount point>
mount -t tifs tifs:<pd endpoints> <mount point>
```

#### TLS

```bash
mount -t tifs -o tls=<tls config file> tifs:<pd endpoints> <mount point>
```

By default, the tls-config should be located in `~/.tifs/tls.toml`, refer to the [tls.toml](config-examples/tls.toml) for detailed configuration.

## Other Custom Mount Options

### `direct_io`

Enable global direct io, to avoid page cache.

```bash
mount -t tifs -o direct_io tifs:<pd endpoints> <mount point>
```
### `blksize`

Set block size in KiB, 64 by default.

```bash
mount -t tifs -o blksize=4 tifs:<pd endpoints> <mount point>
```

### `maxsize`

The quota of fs capacity, could be human-readable.

```bash
mount -t tifs -o maxsize=1GiB tifs:<pd endpoints> <mount point>
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

- [x] Testing and Benchmarking
    - [x] pjdfstest
    - [x] fio

- [x] Real-world usage
    - [x] vim
    - [x] emacs
    - [x] git
    - [x] gcc
    - [x] rustc
    - [x] cargo build
    - [x] npm install
    - [x] sqlite
    - [x] tikv on tifs
    - [x] client runs on FreeBSD: simple case works
