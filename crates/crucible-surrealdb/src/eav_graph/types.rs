use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

/// Marker types for strongly typed record identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityRecord {}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyRecord {}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelationRecord {}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockRecord {}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum EmbeddingRecord {}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TagRecord {}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityTagRecord {}

/// SurrealDB record identifier with an attached marker type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecordId<T> {
    pub table: String,
    pub id: String,
    _marker: std::marker::PhantomData<T>,
}

impl<T> RecordId<T> {
    pub fn new(table: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            id: id.into(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Parse from "table:id" string format
    pub fn from_string(s: &str) -> Result<Self, String> {
        if let Some((table, id)) = s.split_once(':') {
            Ok(Self::new(table, id))
        } else {
            Err(format!("Invalid RecordId format: {}", s))
        }
    }
}

impl<T> From<RecordId<T>> for String {
    fn from(record_id: RecordId<T>) -> String {
        format!("{}:{}", record_id.table, record_id.id)
    }
}

impl<T> TryFrom<String> for RecordId<T> {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_string(&s)
    }
}

impl<T> TryFrom<&str> for RecordId<T> {
    type Error = String;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::from_string(s)
    }
}

impl<T> Serialize for RecordId<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize as an object with table and id fields
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("RecordId", 2)?;
        state.serialize_field("table", &self.table)?;
        state.serialize_field("id", &self.id)?;
        state.end()
    }
}

/// Deserialize RecordId from multiple formats:
/// - String format: "table:id"
/// - Object format: {"table": "entities", "id": "note:123"}
/// - SurrealDB Thing format: {"tb": "entities", "id": {...}}
///
/// This flexibility allows seamless integration with SurrealDB's internal
/// representation while maintaining clean string-based IDs in application code.
impl<'de, T> Deserialize<'de> for RecordId<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct RecordIdVisitor<T>(std::marker::PhantomData<T>);

        impl<'de, T> Visitor<'de> for RecordIdVisitor<T> {
            type Value = RecordId<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a RecordId string or object")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                RecordId::from_string(value).map_err(E::custom)
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: de::MapAccess<'de>,
            {
                let mut table: Option<String> = None;
                let mut id: Option<String> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "table" | "tb" => {
                            table = Some(map.next_value()?);
                        }
                        "id" => {
                            // The id field might be a String or might be nested
                            let value: serde_json::Value = map.next_value()?;
                            id = Some(match value {
                                serde_json::Value::String(s) => s,
                                _ => value.to_string(),
                            });
                        }
                        _ => {
                            let _: serde_json::Value = map.next_value()?;
                        }
                    }
                }

                match (table, id) {
                    (Some(t), Some(i)) => Ok(RecordId::new(t, i)),
                    _ => Err(de::Error::custom("missing table or id field")),
                }
            }
        }

        deserializer.deserialize_any(RecordIdVisitor(std::marker::PhantomData))
    }
}

impl<T> fmt::Display for RecordId<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.table, self.id)
    }
}

/// Entity types supported by the new schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Note,
    Block,
    Tag,
    Section,
    Media,
    Person,
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityType::Note => "note",
            EntityType::Block => "block",
            EntityType::Tag => "tag",
            EntityType::Section => "section",
            EntityType::Media => "media",
            EntityType::Person => "person",
        }
    }
}

impl fmt::Display for EntityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Canonical representation of an entity row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entity {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RecordId<EntityRecord>>,
    pub entity_type: EntityType,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    #[serde(default = "default_version")]
    pub version: i32,
    pub content_hash: Option<String>,
    pub created_by: Option<String>,
    pub vault_id: Option<String>,
    pub data: Option<Value>,
    pub search_text: Option<String>,
}

fn default_version() -> i32 {
    1
}

impl Entity {
    pub fn new(id: RecordId<EntityRecord>, entity_type: EntityType) -> Self {
        Self {
            id: Some(id),
            entity_type,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
            version: default_version(),
            content_hash: None,
            created_by: None,
            vault_id: None,
            data: None,
            search_text: None,
        }
    }

    #[must_use = "builder methods consume self and return a new value"]
    pub fn with_content_hash(mut self, hash: impl Into<String>) -> Self {
        self.content_hash = Some(hash.into());
        self
    }

    #[must_use = "builder methods consume self and return a new value"]
    pub fn with_search_text(mut self, text: impl Into<String>) -> Self {
        self.search_text = Some(text.into());
        self
    }
}

/// Namespaces allow multiple systems to define property keys without collisions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PropertyNamespace(pub String);

impl PropertyNamespace {
    #[allow(dead_code)]
    pub fn core() -> Self {
        Self("core".to_string())
    }

    #[allow(dead_code)]
    pub fn plugin(name: impl Into<String>) -> Self {
        Self(format!("plugin:{}", name.into()))
    }
}

impl fmt::Display for PropertyNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Re-export shared AttributeValue from core
pub use crucible_core::storage::AttributeValue;

/// A single namespace/key/value record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Property {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RecordId<PropertyRecord>>,
    pub entity_id: RecordId<EntityRecord>,
    pub namespace: PropertyNamespace,
    pub key: String,
    pub value: AttributeValue,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

fn default_source() -> String {
    "parser".to_string()
}

fn default_confidence() -> f32 {
    1.0
}

/// Directed relation between two entities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Relation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RecordId<RelationRecord>>,
    #[serde(rename = "in")]
    pub from_id: RecordId<EntityRecord>,
    #[serde(rename = "out")]
    pub to_id: RecordId<EntityRecord>,
    pub relation_type: String,
    #[serde(default = "default_weight")]
    pub weight: f32,
    #[serde(default = "default_directed")]
    pub directed: bool,
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    #[serde(default = "default_source")]
    pub source: String,
    pub position: Option<i32>,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default = "default_content_category")]
    pub content_category: String,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
}

fn default_weight() -> f32 {
    1.0
}

fn default_directed() -> bool {
    true
}

fn default_content_category() -> String {
    "note".to_string()
}

/// Block-level storage for AST nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockNode {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RecordId<BlockRecord>>,
    pub entity_id: RecordId<EntityRecord>,
    pub block_index: i32,
    pub block_type: String,
    pub content: String,
    pub content_hash: String,
    pub start_offset: Option<i32>,
    pub end_offset: Option<i32>,
    pub start_line: Option<i32>,
    pub end_line: Option<i32>,
    pub parent_block_id: Option<RecordId<BlockRecord>>,
    pub depth: Option<i32>,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

/// Embedding vectors for either entities or individual blocks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg(test)]
pub struct EmbeddingVector {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RecordId<EmbeddingRecord>>,
    pub entity_id: RecordId<EntityRecord>,
    pub block_id: Option<RecordId<BlockRecord>>,
    pub embedding: Vec<f32>,
    pub dimensions: i32,
    pub model: String,
    pub model_version: String,
    pub content_used: String,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
}

impl Property {
    pub fn new(
        id: RecordId<PropertyRecord>,
        entity_id: RecordId<EntityRecord>,
        namespace: impl Into<String>,
        key: impl Into<String>,
        value: AttributeValue,
    ) -> Self {
        Self {
            id: Some(id),
            entity_id,
            namespace: PropertyNamespace(namespace.into()),
            key: key.into(),
            value,
            source: default_source(),
            confidence: default_confidence(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

impl BlockNode {
    pub fn new(
        id: RecordId<BlockRecord>,
        entity_id: RecordId<EntityRecord>,
        block_index: i32,
        block_type: impl Into<String>,
        content: impl Into<String>,
        content_hash: impl Into<String>,
    ) -> Self {
        Self {
            id: Some(id),
            entity_id,
            block_index,
            block_type: block_type.into(),
            content: content.into(),
            content_hash: content_hash.into(),
            start_offset: None,
            end_offset: None,
            start_line: None,
            end_line: None,
            parent_block_id: None,
            depth: None,
            metadata: Value::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

#[cfg(test)]
impl EmbeddingVector {
    pub fn new(
        id: RecordId<EmbeddingRecord>,
        entity_id: RecordId<EntityRecord>,
        embedding: Vec<f32>,
        dimensions: i32,
        model: impl Into<String>,
        model_version: impl Into<String>,
        content_used: impl Into<String>,
    ) -> Self {
        Self {
            id: Some(id),
            entity_id,
            block_id: None,
            embedding,
            dimensions,
            model: model.into(),
            model_version: model_version.into(),
            content_used: content_used.into(),
            created_at: Utc::now(),
        }
    }
}

/// Hierarchical tag definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tag {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RecordId<TagRecord>>,
    pub name: String,
    pub parent_id: Option<RecordId<TagRecord>>,
    pub path: String,
    pub depth: i32,
    pub description: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
}

/// Mapping between entities and tags.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntityTag {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RecordId<EntityTagRecord>>,
    pub entity_id: RecordId<EntityRecord>,
    pub tag_id: RecordId<TagRecord>,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default = "default_confidence")]
    pub confidence: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_value_variants_compile() {
        // Test that we can use the shared AttributeValue enum
        let _text = AttributeValue::Text("hello".to_string());
        let _number = AttributeValue::Number(42.5);
        let _bool = AttributeValue::Bool(true);
        let _date = AttributeValue::Date(chrono::NaiveDate::from_ymd_opt(2024, 11, 8).unwrap());
        let _json = AttributeValue::Json(serde_json::json!({"key": "value"}));
    }

    #[test]
    fn property_namespace_helpers_generate_expected_values() {
        let core = PropertyNamespace::core();
        assert_eq!(core.0, "core");

        let plugin = PropertyNamespace::plugin("tasks");
        assert_eq!(plugin.0, "plugin:tasks");
        assert_eq!(plugin.to_string(), "plugin:tasks");
    }
}
