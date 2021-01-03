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

### Product

```bash
cargo build --features "binc" --no-default-features --release
mkdir ~/mnt
RUST_LOG=info target/release/tifs --mount-point ~/mnt
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
        - [ ] git init
        - [ ] cargo build
        - [ ] npm install
        - [ ] sqlite
        - [ ] tikv on tifs
