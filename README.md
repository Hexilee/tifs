# tifs
A distributed file system based on tikv

## Run

You need a tikv cluster to run tifs. [tiup](https://github.com/pingcap/tiup) is convenient to deploy one, just install it and run `tiup playground`.

### Develop

```bash
cargo build
mkdir ~/mnt
RUST_LOG=debug target/debug/tifs --mount-point ~/mnt
```

Then you can open another shell and do play with tifs in `~/mnt`.

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
    - [ ] setattr
    - [ ] readlink
    - [x] readdir
    - [x] open ?
    - [x] read ?
    - [x] write ?
    - [x] mkdir
    - [x] rmdir !
    - [x] mknod
    - [ ] lseek
    - [ ] unlink
    - [ ] symlink
    - [ ] rename
    - [ ] link
    - [ ] statfs
    - [ ] access
    - [ ] create
    - [ ] fallocate
    - [ ] getlk
    - [ ] setlk 
    - [ ] copy_file_range

- [ ] Transaction
    - [ ] select for update
        - [ ] next inode
        - [ ] nlinks

- [ ] Performance
    - [ ] cache
        - [ ] inode table
        - [ ] directory