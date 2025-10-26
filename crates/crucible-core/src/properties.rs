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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_property_map_new() {
        let map = PropertyMap::new();
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn test_property_map_set_and_get() {
        let mut map = PropertyMap::new();
        map.set(
            "name".to_string(),
            PropertyValue::String("Test".to_string()),
        );

        assert_eq!(map.len(), 1);
        assert!(!map.is_empty());

        match map.get("name") {
            Some(PropertyValue::String(s)) => assert_eq!(s, "Test"),
            _ => panic!("Expected string value"),
        }
    }

    #[test]
    fn test_property_value_types() {
        let mut map = PropertyMap::new();

        // String
        map.set(
            "string".to_string(),
            PropertyValue::String("value".to_string()),
        );

        // Number
        map.set("number".to_string(), PropertyValue::Number(42.5));

        // Boolean
        map.set("boolean".to_string(), PropertyValue::Boolean(true));

        // Null
        map.set("null".to_string(), PropertyValue::Null);

        // Array
        map.set(
            "array".to_string(),
            PropertyValue::Array(vec![PropertyValue::Number(1.0), PropertyValue::Number(2.0)]),
        );

        // Object
        let mut obj = HashMap::new();
        obj.insert("key".to_string(), PropertyValue::String("val".to_string()));
        map.set("object".to_string(), PropertyValue::Object(obj));

        assert_eq!(map.len(), 6);
    }

    #[test]
    fn test_property_map_remove() {
        let mut map = PropertyMap::new();
        map.set(
            "key".to_string(),
            PropertyValue::String("value".to_string()),
        );

        assert_eq!(map.len(), 1);

        let removed = map.remove("key");
        assert!(removed.is_some());
        assert_eq!(map.len(), 0);
        assert!(map.is_empty());

        let removed_again = map.remove("key");
        assert!(removed_again.is_none());
    }

    #[test]
    fn test_property_map_keys() {
        let mut map = PropertyMap::new();
        map.set("key1".to_string(), PropertyValue::Number(1.0));
        map.set("key2".to_string(), PropertyValue::Number(2.0));
        map.set("key3".to_string(), PropertyValue::Number(3.0));

        let keys: Vec<_> = map.keys().cloned().collect();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"key1".to_string()));
        assert!(keys.contains(&"key2".to_string()));
        assert!(keys.contains(&"key3".to_string()));
    }

    #[test]
    fn test_property_map_overwrite() {
        let mut map = PropertyMap::new();
        map.set("key".to_string(), PropertyValue::String("old".to_string()));
        map.set("key".to_string(), PropertyValue::String("new".to_string()));

        assert_eq!(map.len(), 1);
        match map.get("key") {
            Some(PropertyValue::String(s)) => assert_eq!(s, "new"),
            _ => panic!("Expected string value"),
        }
    }

    #[test]
    fn test_property_map_default() {
        let map = PropertyMap::default();
        assert!(map.is_empty());
    }

    #[test]
    fn test_property_value_serialization() {
        let mut map = PropertyMap::new();
        map.set(
            "test".to_string(),
            PropertyValue::String("value".to_string()),
        );

        // Test that serialization works
        let json = serde_json::to_string(&map).unwrap();
        assert!(json.contains("test"));
        assert!(json.contains("value"));

        // Test deserialization
        let deserialized: PropertyMap = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.len(), 1);
    }

    #[test]
    fn test_nested_property_values() {
        let mut inner_map = HashMap::new();
        inner_map.insert("nested".to_string(), PropertyValue::Number(123.0));

        let mut map = PropertyMap::new();
        map.set("outer".to_string(), PropertyValue::Object(inner_map));

        match map.get("outer") {
            Some(PropertyValue::Object(obj)) => match obj.get("nested") {
                Some(PropertyValue::Number(n)) => assert_eq!(*n, 123.0),
                _ => panic!("Expected nested number"),
            },
            _ => panic!("Expected object"),
        }
    }
}
