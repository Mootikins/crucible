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
