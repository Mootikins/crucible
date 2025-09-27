use yrs::{Doc, Map, Text, Array, Transact};
use crate::Result;

pub struct CrdtManager {
    doc: Doc,
}

impl CrdtManager {
    pub fn new() -> Self {
        Self {
            doc: Doc::new(),
        }
    }

    pub fn get_document(&self) -> &Doc {
        &self.doc
    }

    pub fn get_map(&self, name: &str) -> Map {
        self.doc.get_map(name)
    }

    pub fn get_text(&self, name: &str) -> Text {
        self.doc.get_text(name)
    }

    pub fn get_array(&self, name: &str) -> Array {
        self.doc.get_array(name)
    }

    pub fn transact<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut Transact) -> Result<R>,
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

