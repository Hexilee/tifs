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

### Directory

We can store `name -> ino` records by a hash map, but the time complexity of deserializing a hash map is `O(n)`. Cache of directory may be neccessary.

### Serialize
We would use the serde framework to serialize meta, inode and directory. Taking both of human-readable and performance into consideration, we would use json in development and use bincode in production.

### Consistency

We would use the pessimistic transaction to confirm consistency. In most cases lock-on-write is enough. However, some actions like increasing ino-counter or decreasing nlinks needs get-for-update.

### Performance

The block size may be the key factor of performance. Small block size may cause high overhead in searching and transmitting big data while big block size may cause high overhead in altering little data.

Moreover, each block is a value in TiKV, and big value can cause bad performance in RocksDB, which is based on LSM tree. The [Titan](https://github.com/tikv/titan) plugin may reduce the overhead.

## Tracing

Refer to [TODO](https://github.com/Hexilee/tifs#todo).