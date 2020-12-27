# TiFS: FUSE based on TiKV

---

## Summary
As we can build a DBMS based on TiKV, we can also build a filesystem based on it.

---

## Detailed design

### Key

We would store different kinds of data in diffrent scope, so we use a `ScopedKey` structure following.

```rust
struct ScopedKey {
    scope: u64,
    key: u64
}
```

Then we can encode it to a TiKV key in big-endian.

#### Block

We store data blocks scoped by `ino`(inode ID) of their owners. For example, `ScopedKey{ scope: 1, key: 0}` means the first block of `ino(1)`, and `ScopedKey{ scope: 2, key: 64 }` means the 65th blocks of `ino(2)`.

As we encode keys in big-endian, the blocks of a ino will be stored continously in TiKV, we can read big data by scan.

#### Inode

As the `ino` of POSIX filesystems usually start from 1 (so does TiFS), we can store inodes with `scope(0)`. For example, `ScopedKey{ scope: 0, key: 1 }` means the inode of `ino(1)`, and `ScopedKey{ scope: 0, key: 64 }` means the inode of `ino(64)`.

#### Meta

As block scopes and inode scope are allocated, the `ScopedKey{ scope: 0, key: 0}` is left, we can store the metadata like ino-counter by this key.

### Directory

We can store `name -> ino` records by a `HashMap`, but the time complexity of deserialize a hash map is `O(n)`. Cache of directory may be neccessary.

### Serialize
We would use the serde framework to serialize meta, inode and directory. Taking both of human-readable and performance into consideration, we would use json in development and use bincode in production.

### Consistency

We would use the pessimistic transaction to confirm consistency. In most cases lock-on-write is enough. However, some actions like increasing ino-counter or decreasing nlinks needs get-for-update.

### Performance

The block size may be the key factor of performance. Small block size may cause high overhead in searching and transmitting big data while big block size may cause high overhead in altering little data.

Moreover, each block is a value in TiKV, and big value can cause bad performance in RocksDB, which is based on LSM tree. The [Titan](https://github.com/tikv/titan) plugin may reduce the overhead.

## TODO and Tracing

Refer to [README](https://github.com/Hexilee/tifs#todo).