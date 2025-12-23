use crate::Result;
use yrs::{Array, Doc, Map, Text, Transact};

pub struct CrdtManager {
    doc: Doc,
}

impl CrdtManager {
    pub fn new() -> Self {
        Self { doc: Doc::new() }
    }

    pub fn get_document(&self) -> &Doc {
        &self.doc
    }

    pub fn get_map(&self, name: &str) -> impl Map {
        self.doc.get_or_insert_map(name)
    }

    pub fn get_text(&self, name: &str) -> impl Text {
        self.doc.get_or_insert_text(name)
    }

    pub fn get_array(&self, name: &str) -> impl Array {
        self.doc.get_or_insert_array(name)
    }

    pub fn transact<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut yrs::TransactionMut) -> Result<R>,
    {
        let mut txn = self.doc.transact_mut();
        f(&mut txn)
    }
}

impl Default for CrdtManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crdt_manager_new() {
        let manager = CrdtManager::new();
        // client_id is a u64, just verify it exists
        let _client_id = manager.get_document().client_id();
    }

    #[test]
    fn test_crdt_manager_default() {
        let manager = CrdtManager::default();
        // client_id is a u64, just verify it exists
        let _client_id = manager.get_document().client_id();
    }

    #[test]
    fn test_get_text() {
        let manager = CrdtManager::new();
        let text = manager.get_text("test_text");

        // Verify we can use the text
        manager
            .transact(|txn| {
                text.insert(txn, 0, "Hello");
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn test_get_map() {
        let manager = CrdtManager::new();
        let map = manager.get_map("test_map");

        // Verify we can use the map
        manager
            .transact(|txn| {
                map.insert(txn, "key", "value");
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn test_get_array() {
        let manager = CrdtManager::new();
        let array = manager.get_array("test_array");

        // Verify we can use the array
        manager
            .transact(|txn| {
                array.insert(txn, 0, "item1");
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn test_transact() {
        let manager = CrdtManager::new();
        let text = manager.get_text("content");

        let result = manager.transact(|txn| {
            text.insert(txn, 0, "Test");
            Ok(42)
        });

        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_multiple_operations() {
        let manager = CrdtManager::new();
        let text = manager.get_text("doc");
        let map = manager.get_map("metadata");

        // Perform multiple operations
        manager
            .transact(|txn| {
                text.insert(txn, 0, "Hello");
                map.insert(txn, "author", "test");
                Ok(())
            })
            .unwrap();

        manager
            .transact(|txn| {
                text.insert(txn, 5, " World");
                map.insert(txn, "version", 1);
                Ok(())
            })
            .unwrap();

        // Verify operations completed without error (no panics means success)
    }
}
