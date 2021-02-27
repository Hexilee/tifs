# TiFS: FUSE based on TiKV

## Summary
As we can build a DBMS based on TiKV, we can also build a filesystem based on it.

## Detailed design

### Key

We would store different kinds of data in diffrent scope, so we design a `ScopedKey` enumeration following.

```rust
pub enum ScopedKey<'a> {
    Meta,
    Inode(u64),
    Block {
        ino: u64,
        block: u64,
    },
    FileHandler {
        ino: u64,
        handler: u64,
    },
    FileIndex {
        parent: u64,
        name: &'a str,
    },
}
```

We can encode a scoped key into a byte array as a TiKV key. Following is the common layout of an encoded key.

```
+ 1byte +<--------------------------------+ dynamic size +---------------------------------------->+
|       |                                                                                          |
|       |                                                                                          |
|       |                                                                                          |
|       |                                                                                          |
|       |                                                                                          |
|       |                                                                                          |
|       v                                                                                          v
+--------------------------------------------------------------------------------------------------+
|       |                                                                                          |
| scope |                                       body                                               |
|       |                                                                                          |
+-------+------------------------------------------------------------------------------------------+
```

#### Meta

There is only one key in the meta scope. The meta key is designed to store meta data of our filesystem, following is the layout of an encoded meta key.

```
+ 1byte +
|       |
|       |
|       |
|       |
|       |
|       |
|       v
+-------+
|       |
|   0   |
|       |
+-------+
```

#### Inode

Keys in the inode scope are designed to store attributes of files, following is the layout of an encoded inode key.

```
+ 1byte +<-----------------------------------+ 8bytes +------------------------------------------->+
|       |                                                                                          |
|       |                                                                                          |
|       |                                                                                          |
|       |                                                                                          |
|       |                                                                                          |
|       |                                                                                          |
|       v                                                                                          v
+--------------------------------------------------------------------------------------------------+
|       |                                                                                          |
|   1   |                                   inode number                                           |
|       |                                                                                          |
+-------+------------------------------------------------------------------------------------------+
```

#### Block

Keys in the block scope are designed to store blocks of file, following is the layout of an encoded block key.

```
+ 1byte +<----------------- 8bytes ---------------->+<------------------- 8bytes ----------------->+
|       |                                           |                                              |
|       |                                           |                                              |
|       |                                           |                                              |
|       |                                           |                                              |
|       |                                           |                                              |
|       |                                           |                                              |
|       v                                           v                                              v
+--------------------------------------------------------------------------------------------------+
|       |                                           |                                              |
|   2   |              inode number                 |                  block index                 |
|       |                                           |                                              |
+-------+-------------------------------------------+----------------------------------------------+
```

As we encode keys in big-endian, the blocks of a file will be stored continously in TiKV, we can read big data by a scan request.

#### FileHandler

Keys in the file handler scope are designed to store file handler of file, following is the layout of an encoded file handler key.

```
+ 1byte +<----------------- 8bytes ---------------->+<------------------- 8bytes ----------------->+
|       |                                           |                                              |
|       |                                           |                                              |
|       |                                           |                                              |
|       |                                           |                                              |
|       |                                           |                                              |
|       |                                           |                                              |
|       v                                           v                                              v
+--------------------------------------------------------------------------------------------------+
|       |                                           |                                              |
|   3   |              inode number                 |                  file handler                |
|       |                                           |                                              |
+-------+-------------------------------------------+----------------------------------------------+
```

#### FileIndex

Keys in the file index scope are designed to store file index of file, following is the layout of an encoded file index key.

```
+ 1byte +<----------------- 8bytes ---------------->+<-------------- dynamic size ---------------->+
|       |                                           |                                              |
|       |                                           |                                              |
|       |                                           |                                              |
|       |                                           |                                              |
|       |                                           |                                              |
|       |                                           |                                              |
|       v                                           v                                              v
+--------------------------------------------------------------------------------------------------+
|       |                                           |                                              |
|   4   |     inode number of parent directory      |         file name in utf-8 encoding          |
|       |                                           |                                              |
+-------+-------------------------------------------+----------------------------------------------+
```

### Value

### Serialize
We would use the [serde framework](https://github.com/serde-rs/serde) to serialize/deserialize the meta, inodes, directories, file handlers and file indexes. Taking both of human-readable and performance into consideration, we would use json in development and use bincode in production.

#### Meta

```rust
pub struct Meta {
    pub inode_next: u64,
}
```
The meta structure contains only an auto-increasing counter `inode_next`, designed to generate inode number and implement [mknod](https://docs.rs/fuser/0.7.0/fuser/trait.Filesystem.html#method.mknod).

#### Inode

```rust
pub struct Inode {
    pub file_attr: FileAttr,
    pub lock_state: LockState,
    pub inline_data: Option<Vec<u8>>,
    pub next_fh: u64,
    pub opened_fh: u64,
}
```

The inode structure consists of 5 fields. The `file_attr` field contains basic attributes like inode number, file size, blocks and so on, you can refer to the [fuser docs](https://docs.rs/fuser/0.7.0/fuser/struct.FileAttr.html) for more details.

The `lock_state` field contains current lock type and owner set of this file, designed to implement [getlk](https://docs.rs/fuser/0.7.0/fuser/trait.Filesystem.html#method.getlk) and [setlk](https://docs.rs/fuser/0.7.0/fuser/trait.Filesystem.html#method.setlk). Following is its structure.

```rust
pub struct LockState {
    pub owner_set: HashSet<u64>,
    pub lk_type: i32,
}
```

The `inline_data` field shoud contains file contents when the total size is small enough. The `next_fn` field is an auto-increasing counter, designed to generate file handler, while the `opened_fh` field records the numbers of opened file handler.

#### FileHandler

```rust
pub struct FileHandler {
    pub cursor: u64,
    pub flags: i32,
}
```

Each file handler contains a cursor and open flags. The `cursor` field stores current position of the cursor, and the `flags` field is designed to manage read/write permission.

#### Directory

```rust
pub type Directory = Vec<DirItem>;

pub struct DirItem {
    pub ino: u64,
    pub name: String,
    pub typ: FileType,
}
```

The directory contains all mappings from the file name to the inode number and file type, designed to implement the [readdir](https://docs.rs/fuser/0.7.0/fuser/trait.Filesystem.html#method.readdir).

#### FileIndex

```rust
pub struct Index {
    pub ino: u64,
}
```

We can just store all items in a directory by a vector, but the time complexities of deserializing vector and searching it are both `O(n)`. So we need indices to optimize [lookup](https://docs.rs/fuser/0.7.0/fuser/trait.Filesystem.html#method.lookup).

The index value contains only an inode number. We can construct an [index key](#fileindex) by a file name and inode number of the parent directory, then we can get inode number of the file by this key much faster. 

### Consistency

As the pessimistic transaction of client library is not well tested, we would use the optimistic transaction to confirm consistency.

### Performance

The block size may be the key factor of performance. Small block size may cause high overhead in searching and transmitting big data while big block size may cause high overhead in altering little data.

Moreover, each block is a value in TiKV, and big value can cause bad performance in RocksDB, which is based on LSM tree. The [Titan](https://github.com/tikv/titan) plugin may reduce the overhead.

## Tracing

Refer to [TODO](https://github.com/Hexilee/tifs#todo).