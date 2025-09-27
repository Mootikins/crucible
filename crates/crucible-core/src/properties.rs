use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropertyValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Array(Vec<PropertyValue>),
    Object(HashMap<String, PropertyValue>),
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyMap {
    inner: HashMap<String, PropertyValue>,
}

impl PropertyMap {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&PropertyValue> {
        self.inner.get(key)
    }

    pub fn set(&mut self, key: String, value: PropertyValue) {
        self.inner.insert(key, value);
    }

    pub fn remove(&mut self, key: &str) -> Option<PropertyValue> {
        self.inner.remove(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.inner.keys()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for PropertyMap {
    fn default() -> Self {
        Self::new()
    }
}

