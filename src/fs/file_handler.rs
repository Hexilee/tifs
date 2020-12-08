use std::collections::HashMap;
use std::sync::Arc;

use async_std::sync::{RwLock, RwLockWriteGuard};
use slab::Slab;

type FileSlab = RwLock<Slab<FileHandler>>;
pub struct FileHub {
    table: RwLock<HashMap<u64, FileSlab>>,
}

impl FileHub {
    pub fn new() -> Self {
        Self {
            table: RwLock::new(HashMap::new()),
        }
    }

    pub async fn make(&self, ino: u64) -> u64 {
        let mut hub = self.table.write().await;
        if !hub.contains_key(&ino) {
            hub.insert(ino, RwLock::new(Slab::new()));
        }

        let mut slab = hub[&ino].write().await;
        slab.insert(FileHandler::new()) as u64
    }

    pub async fn get(&self, ino: u64, fh: u64) -> Option<FileHandler> {
        self.table
            .read()
            .await
            .get(&ino)?
            .read()
            .await
            .get(fh as usize)
            .cloned()
    }

    pub async fn close(&self, ino: u64, fh: u64) -> Option<FileHandler> {
        let hub = self.table.read().await;
        let mut slab = hub.get(&ino)?.write().await;
        if !slab.contains(fh as usize) {
            None
        } else {
            Some(slab.remove(fh as usize))
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileHandler {
    cursor: Arc<RwLock<Cursor>>,
}

pub type Cursor = usize;

impl FileHandler {
    fn new() -> Self {
        Self {
            cursor: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn cursor(&self) -> RwLockWriteGuard<'_, Cursor> {
        self.cursor.write().await
    }

    pub async fn pos(&self) -> usize {
        *self.cursor.read().await
    }
}
