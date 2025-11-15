//! Frontmatter to Property Mapper
//!
//! Phase 1.2: Maps frontmatter key-value pairs to Property objects following the
//! flat frontmatter convention.
//!
//! ## Design Principles
//!
//! 1. **Flat Structure**: All frontmatter properties use "frontmatter" namespace
//! 2. **Type Inference**: Automatically maps to PropertyValue types
//! 3. **No Nesting**: Complex objects serialized to JSON (following Obsidian conventions)
//! 4. **Timestamps**: All properties get created_at/updated_at metadata

use crate::storage::{Property, PropertyNamespace, PropertyValue};
use chrono::{NaiveDate, Utc};
use serde_json::Value;
use std::collections::HashMap;

/// Maps frontmatter key-value pairs to Property objects
///
/// See `FrontmatterPropertyMapper::new()` and `map_to_properties()` for usage.
pub struct FrontmatterPropertyMapper {
    entity_id: String,
    namespace: PropertyNamespace,
}

impl FrontmatterPropertyMapper {
    /// Create a new mapper for the given entity
    ///
    /// # Arguments
    ///
    /// * `entity_id` - The ID of the entity these properties belong to
    pub fn new(entity_id: impl Into<String>) -> Self {
        Self {
            entity_id: entity_id.into(),
            namespace: PropertyNamespace::frontmatter(),
        }
    }

    /// Map a HashMap of frontmatter to Property objects
    ///
    /// Performs type inference for each value:
    /// - String → PropertyValue::Text
    /// - Number → PropertyValue::Number
    /// - Boolean → PropertyValue::Bool
    /// - Date string (YYYY-MM-DD) → PropertyValue::Date
    /// - Array → PropertyValue::Json
    /// - Object → PropertyValue::Json (with warning logged)
    ///
    /// # Arguments
    ///
    /// * `frontmatter` - HashMap of key-value pairs from frontmatter
    ///
    /// # Returns
    ///
    /// Vector of Property objects ready for storage
    pub fn map_to_properties(&self, frontmatter: HashMap<String, Value>) -> Vec<Property> {
        let now = Utc::now();
        let mut properties = Vec::with_capacity(frontmatter.len());

        for (key, value) in frontmatter {
            let property_value = self.infer_property_value(&key, &value);

            properties.push(Property {
                entity_id: self.entity_id.clone(),
                namespace: self.namespace.clone(),
                key,
                value: property_value,
                created_at: now,
                updated_at: now,
            });
        }

        properties
    }

    /// Infer PropertyValue type from serde_json::Value
    fn infer_property_value(&self, key: &str, value: &Value) -> PropertyValue {
        match value {
            // String - check if it's a date format first
            Value::String(s) => {
                // Try to parse as ISO 8601 date
                if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                    return PropertyValue::Date(date);
                }
                // Try RFC 3339 datetime
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                    return PropertyValue::Date(dt.date_naive());
                }
                // Regular string
                PropertyValue::Text(s.clone())
            }

            // Number (JSON numbers are always f64)
            Value::Number(n) => PropertyValue::Number(n.as_f64().unwrap_or(0.0)),

            // Boolean
            Value::Bool(b) => PropertyValue::Bool(*b),

            // Array - serialize to JSON
            Value::Array(_) => PropertyValue::Json(value.clone()),

            // Object - serialize to JSON (log warning about nesting)
            Value::Object(_) => {
                tracing::warn!(
                    key = %key,
                    entity = %self.entity_id,
                    "Nested object in frontmatter - consider flattening keys (e.g., 'author.name' instead of nested 'author' object)"
                );
                PropertyValue::Json(value.clone())
            }

            // Null - treat as empty string
            Value::Null => PropertyValue::Text(String::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ============================================================================
    // Phase 1.2.1: String → PropertyValue::Text
    // ============================================================================

    #[test]
    fn test_map_string_to_value_text() {
        let mapper = FrontmatterPropertyMapper::new("note:test");
        let mut frontmatter = HashMap::new();
        frontmatter.insert("title".to_string(), json!("My Note"));
        frontmatter.insert("author".to_string(), json!("John Doe"));

        let properties = mapper.map_to_properties(frontmatter);

        assert_eq!(properties.len(), 2);

        let title_prop = properties.iter().find(|p| p.key == "title").unwrap();
        assert_eq!(title_prop.entity_id, "note:test");
        assert_eq!(title_prop.namespace, PropertyNamespace::frontmatter());
        assert_eq!(title_prop.value, PropertyValue::Text("My Note".to_string()));

        let author_prop = properties.iter().find(|p| p.key == "author").unwrap();
        assert_eq!(
            author_prop.value,
            PropertyValue::Text("John Doe".to_string())
        );
    }

    // ============================================================================
    // Phase 1.2.2: Number → PropertyValue::Number
    // ============================================================================

    #[test]
    fn test_map_number_to_value_number() {
        let mapper = FrontmatterPropertyMapper::new("note:test");
        let mut frontmatter = HashMap::new();
        frontmatter.insert("count".to_string(), json!(42));
        frontmatter.insert("rating".to_string(), json!(4.5));
        frontmatter.insert("negative".to_string(), json!(-10));

        let properties = mapper.map_to_properties(frontmatter);

        assert_eq!(properties.len(), 3);

        let count_prop = properties.iter().find(|p| p.key == "count").unwrap();
        assert_eq!(count_prop.value, PropertyValue::Number(42.0));

        let rating_prop = properties.iter().find(|p| p.key == "rating").unwrap();
        assert_eq!(rating_prop.value, PropertyValue::Number(4.5));

        let neg_prop = properties.iter().find(|p| p.key == "negative").unwrap();
        assert_eq!(neg_prop.value, PropertyValue::Number(-10.0));
    }

    // ============================================================================
    // Phase 1.2.3: Boolean → PropertyValue::Bool
    // ============================================================================

    #[test]
    fn test_map_boolean_to_value_bool() {
        let mapper = FrontmatterPropertyMapper::new("note:test");
        let mut frontmatter = HashMap::new();
        frontmatter.insert("published".to_string(), json!(true));
        frontmatter.insert("draft".to_string(), json!(false));

        let properties = mapper.map_to_properties(frontmatter);

        assert_eq!(properties.len(), 2);

        let pub_prop = properties.iter().find(|p| p.key == "published").unwrap();
        assert_eq!(pub_prop.value, PropertyValue::Bool(true));

        let draft_prop = properties.iter().find(|p| p.key == "draft").unwrap();
        assert_eq!(draft_prop.value, PropertyValue::Bool(false));
    }

    // ============================================================================
    // Phase 1.2.4: Date String → PropertyValue::Date
    // ============================================================================

    #[test]
    fn test_map_date_string_to_value_date() {
        let mapper = FrontmatterPropertyMapper::new("note:test");
        let mut frontmatter = HashMap::new();
        frontmatter.insert("created".to_string(), json!("2024-11-08"));
        frontmatter.insert("modified".to_string(), json!("2024-11-09"));

        let properties = mapper.map_to_properties(frontmatter);

        assert_eq!(properties.len(), 2);

        let created_prop = properties.iter().find(|p| p.key == "created").unwrap();
        assert_eq!(
            created_prop.value,
            PropertyValue::Date(NaiveDate::from_ymd_opt(2024, 11, 8).unwrap())
        );

        let modified_prop = properties.iter().find(|p| p.key == "modified").unwrap();
        assert_eq!(
            modified_prop.value,
            PropertyValue::Date(NaiveDate::from_ymd_opt(2024, 11, 9).unwrap())
        );
    }

    // ============================================================================
    // Phase 1.2.5: Array → PropertyValue::Json
    // ============================================================================

    #[test]
    fn test_map_array_to_value_json() {
        let mapper = FrontmatterPropertyMapper::new("note:test");
        let mut frontmatter = HashMap::new();
        frontmatter.insert("tags".to_string(), json!(["rust", "testing", "tdd"]));
        frontmatter.insert("aliases".to_string(), json!(["note", "example"]));

        let properties = mapper.map_to_properties(frontmatter);

        assert_eq!(properties.len(), 2);

        let tags_prop = properties.iter().find(|p| p.key == "tags").unwrap();
        assert_eq!(
            tags_prop.value,
            PropertyValue::Json(json!(["rust", "testing", "tdd"]))
        );

        let aliases_prop = properties.iter().find(|p| p.key == "aliases").unwrap();
        assert_eq!(
            aliases_prop.value,
            PropertyValue::Json(json!(["note", "example"]))
        );
    }

    // ============================================================================
    // Phase 1.2.6: Nested Object → PropertyValue::Json (with warning)
    // ============================================================================

    #[test]
    fn test_map_nested_object_to_value_json() {
        let mapper = FrontmatterPropertyMapper::new("note:test");
        let mut frontmatter = HashMap::new();
        frontmatter.insert(
            "author".to_string(),
            json!({"name": "John Doe", "email": "john@example.com"}),
        );

        let properties = mapper.map_to_properties(frontmatter);

        assert_eq!(properties.len(), 1);

        let author_prop = properties.iter().find(|p| p.key == "author").unwrap();
        assert_eq!(
            author_prop.value,
            PropertyValue::Json(json!({"name": "John Doe", "email": "john@example.com"}))
        );
    }

    // ============================================================================
    // Phase 1.2: Verify Namespace
    // ============================================================================

    #[test]
    fn test_all_properties_use_frontmatter_namespace() {
        let mapper = FrontmatterPropertyMapper::new("note:test");
        let mut frontmatter = HashMap::new();
        frontmatter.insert("title".to_string(), json!("Test"));
        frontmatter.insert("count".to_string(), json!(42));
        frontmatter.insert("published".to_string(), json!(true));
        frontmatter.insert("created".to_string(), json!("2024-11-08"));
        frontmatter.insert("tags".to_string(), json!(["test"]));

        let properties = mapper.map_to_properties(frontmatter);

        // All properties should use "frontmatter" namespace
        assert_eq!(properties.len(), 5);
        for prop in properties {
            assert_eq!(prop.namespace, PropertyNamespace::frontmatter());
            assert_eq!(prop.entity_id, "note:test");
        }
    }

    #[test]
    fn test_timestamps_are_set() {
        let mapper = FrontmatterPropertyMapper::new("note:test");
        let mut frontmatter = HashMap::new();
        frontmatter.insert("title".to_string(), json!("Test"));

        let properties = mapper.map_to_properties(frontmatter);

        assert_eq!(properties.len(), 1);
        let prop = &properties[0];

        // Timestamps should be set
        assert!(prop.created_at.timestamp() > 0);
        assert!(prop.updated_at.timestamp() > 0);
        assert_eq!(prop.created_at, prop.updated_at); // Should be the same initially
    }
}
