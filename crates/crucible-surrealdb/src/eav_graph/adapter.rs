//! Adapter between crucible-core and SurrealDB EAV+Graph types
//!
//! This module provides conversion functions between the database-agnostic
//! core types and the SurrealDB-specific types that use RecordId<T>.

use crucible_core::storage as core;

use super::types::{
    EntityRecord, Property as SurrealProperty, PropertyNamespace as SurrealPropertyNamespace,
    PropertyRecord, RecordId,
};

// ============================================================================
// ID Conversion
// ============================================================================

/// Convert a String ID to a RecordId<EntityRecord>
///
/// # Arguments
///
/// * `id` - String ID (e.g., "note:test123" or "entities:note:test123")
///
/// # Returns
///
/// RecordId with "entities" table and the provided ID
///
/// # Examples
///
/// - "note:test123" → RecordId { table: "entities", id: "note:test123" }
/// - "entities:note:test123" → RecordId { table: "entities", id: "note:test123" }
pub fn string_to_entity_id(id: &str) -> RecordId<EntityRecord> {
    // If ID already has "entities:" prefix, extract just the ID part
    let id_part = if id.starts_with("entities:") {
        id.strip_prefix("entities:").unwrap_or(id)
    } else {
        id
    };

    RecordId::new("entities", id_part)
}

/// Convert a RecordId<EntityRecord> to a String ID
///
/// # Arguments
///
/// * `record_id` - The RecordId to convert
///
/// # Returns
///
/// String in format "table:id"
pub fn entity_id_to_string<T>(record_id: &RecordId<T>) -> String {
    format!("{}:{}", record_id.table, record_id.id)
}

// ============================================================================
// Property Conversion
// ============================================================================

/// Convert core Property to SurrealDB Property
///
/// # Arguments
///
/// * `core_prop` - Property from crucible-core
/// * `property_id` - Optional RecordId for the property (if None, generates one)
///
/// # Returns
///
/// SurrealDB Property with RecordId entity_id
pub fn core_property_to_surreal(
    core_prop: core::Property,
    property_id: Option<RecordId<PropertyRecord>>,
) -> SurrealProperty {
    let entity_id = string_to_entity_id(&core_prop.entity_id);

    SurrealProperty {
        id: property_id,
        entity_id,
        namespace: SurrealPropertyNamespace(core_prop.namespace.0.to_string()),
        key: core_prop.key,
        value: core_prop.value,
        source: "parser".to_string(),
        confidence: 1.0,
        created_at: core_prop.created_at,
        updated_at: core_prop.updated_at,
    }
}

/// Convert SurrealDB Property to core Property
///
/// # Arguments
///
/// * `surreal_prop` - Property from SurrealDB
///
/// # Returns
///
/// Core Property with String entity_id
pub fn surreal_property_to_core(surreal_prop: SurrealProperty) -> core::Property {
    core::Property {
        entity_id: entity_id_to_string(&surreal_prop.entity_id),
        namespace: core::PropertyNamespace(surreal_prop.namespace.0.into()),
        key: surreal_prop.key,
        value: surreal_prop.value,
        created_at: surreal_prop.created_at,
        updated_at: surreal_prop.updated_at,
    }
}

/// Batch convert core Properties to SurrealDB Properties
///
/// # Arguments
///
/// * `core_props` - Vector of properties from crucible-core
///
/// # Returns
///
/// Vector of SurrealDB Properties with generated IDs
pub fn core_properties_to_surreal(core_props: Vec<core::Property>) -> Vec<SurrealProperty> {
    core_props
        .into_iter()
        .enumerate()
        .map(|(idx, prop)| {
            // Generate a property ID: entities:note:test:frontmatter:title
            let prop_id = RecordId::new(
                "properties",
                format!("{}:{}:{}", prop.entity_id, prop.namespace.0.as_ref(), prop.key),
            );
            core_property_to_surreal(prop, Some(prop_id))
        })
        .collect()
}

/// Batch convert SurrealDB Properties to core Properties
///
/// # Arguments
///
/// * `surreal_props` - Vector of properties from SurrealDB
///
/// # Returns
///
/// Vector of core Properties
pub fn surreal_properties_to_core(surreal_props: Vec<SurrealProperty>) -> Vec<core::Property> {
    surreal_props
        .into_iter()
        .map(surreal_property_to_core)
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_string_to_entity_id_simple() {
        let id = string_to_entity_id("note:test123");
        assert_eq!(id.table, "entities");
        assert_eq!(id.id, "note:test123");
    }

    #[test]
    fn test_string_to_entity_id_without_prefix() {
        let id = string_to_entity_id("test123");
        assert_eq!(id.table, "entities");
        assert_eq!(id.id, "test123");
    }

    #[test]
    fn test_entity_id_to_string() {
        let record_id = RecordId::<EntityRecord>::new("entities", "note:test123");
        let string_id = entity_id_to_string(&record_id);
        assert_eq!(string_id, "entities:note:test123");
    }

    #[test]
    fn test_round_trip_entity_id() {
        let original = "note:test123";
        let record_id = string_to_entity_id(original);
        let result = entity_id_to_string(&record_id);
        assert_eq!(result, "entities:note:test123");
    }

    #[test]
    fn test_core_property_to_surreal() {
        let now = Utc::now();
        let core_prop = core::Property {
            entity_id: "note:test".to_string(),
            namespace: core::PropertyNamespace::frontmatter(),
            key: "title".to_string(),
            value: core::PropertyValue::Text("Test Note".to_string()),
            created_at: now,
            updated_at: now,
        };

        let surreal_prop = core_property_to_surreal(core_prop.clone(), None);

        assert_eq!(surreal_prop.entity_id.table, "entities");
        assert_eq!(surreal_prop.entity_id.id, "note:test");
        assert_eq!(surreal_prop.namespace.0.as_str(), "frontmatter");
        assert_eq!(surreal_prop.key, "title");
        assert_eq!(
            surreal_prop.value,
            core::PropertyValue::Text("Test Note".to_string())
        );
    }

    #[test]
    fn test_surreal_property_to_core() {
        let now = Utc::now();
        let surreal_prop = SurrealProperty {
            id: Some(RecordId::new("properties", "prop1")),
            entity_id: RecordId::new("entities", "note:test"),
            namespace: SurrealPropertyNamespace("frontmatter".to_string()),
            key: "author".to_string(),
            value: core::PropertyValue::Text("John Doe".to_string()),
            source: "parser".to_string(),
            confidence: 1.0,
            created_at: now,
            updated_at: now,
        };

        let core_prop = surreal_property_to_core(surreal_prop);

        assert_eq!(core_prop.entity_id, "entities:note:test");
        assert_eq!(core_prop.namespace.0.as_ref(), "frontmatter");
        assert_eq!(core_prop.key, "author");
        assert_eq!(
            core_prop.value,
            core::PropertyValue::Text("John Doe".to_string())
        );
    }

    #[test]
    fn test_round_trip_property() {
        let now = Utc::now();
        let original = core::Property {
            entity_id: "note:test".to_string(),
            namespace: core::PropertyNamespace::frontmatter(),
            key: "count".to_string(),
            value: core::PropertyValue::Number(42.0),
            created_at: now,
            updated_at: now,
        };

        let surreal = core_property_to_surreal(original.clone(), None);
        let result = surreal_property_to_core(surreal);

        assert_eq!(result.entity_id, "entities:note:test");
        assert_eq!(result.namespace.0.as_ref(), original.namespace.0.as_ref());
        assert_eq!(result.key, original.key);
        assert_eq!(result.value, original.value);
    }

    #[test]
    fn test_batch_conversion() {
        let now = Utc::now();
        let core_props = vec![
            core::Property {
                entity_id: "note:test".to_string(),
                namespace: core::PropertyNamespace::frontmatter(),
                key: "title".to_string(),
                value: core::PropertyValue::Text("Test".to_string()),
                created_at: now,
                updated_at: now,
            },
            core::Property {
                entity_id: "note:test".to_string(),
                namespace: core::PropertyNamespace::frontmatter(),
                key: "count".to_string(),
                value: core::PropertyValue::Number(42.0),
                created_at: now,
                updated_at: now,
            },
        ];

        let surreal_props = core_properties_to_surreal(core_props.clone());
        assert_eq!(surreal_props.len(), 2);

        let result_props = surreal_properties_to_core(surreal_props);
        assert_eq!(result_props.len(), 2);

        // Check first property
        assert_eq!(result_props[0].key, "title");
        assert_eq!(
            result_props[0].value,
            core::PropertyValue::Text("Test".to_string())
        );

        // Check second property
        assert_eq!(result_props[1].key, "count");
        assert_eq!(result_props[1].value, core::PropertyValue::Number(42.0));
    }
}
