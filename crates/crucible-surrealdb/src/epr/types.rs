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
pub enum EmbeddingRecord {}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TagRecord {}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityTagRecord {}

/// SurrealDB record identifier with an attached marker type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordId<T> {
    pub table: String,
    pub id: String,
    #[serde(skip)]
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

    pub fn to_string(&self) -> String {
        format!("{}:{}", self.table, self.id)
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

/// Namespaces allow multiple systems to define property keys without collisions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PropertyNamespace(pub String);

impl PropertyNamespace {
    pub fn core() -> Self {
        Self("core".to_string())
    }

    pub fn plugin(name: impl Into<String>) -> Self {
        Self(format!("plugin:{}", name.into()))
    }
}

impl fmt::Display for PropertyNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Enumerates the scalar column that a property primarily uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyValueType {
    Text,
    Number,
    Boolean,
    Date,
    Json,
}

impl PropertyValueType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PropertyValueType::Text => "text",
            PropertyValueType::Number => "number",
            PropertyValueType::Boolean => "boolean",
            PropertyValueType::Date => "date",
            PropertyValueType::Json => "json",
        }
    }
}

impl fmt::Display for PropertyValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A normalized representation of a property value, including typed columns.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropertyValue {
    pub value: Value,
    pub value_type: PropertyValueType,
    pub value_text: Option<String>,
    pub value_number: Option<f64>,
    pub value_bool: Option<bool>,
    pub value_date: Option<DateTime<Utc>>,
}

impl PropertyValue {
    pub fn text(value: impl Into<String>) -> Self {
        let text = value.into();
        Self {
            value: Value::String(text.clone()),
            value_type: PropertyValueType::Text,
            value_text: Some(text),
            value_number: None,
            value_bool: None,
            value_date: None,
        }
    }

    pub fn number(value: f64) -> Self {
        Self {
            value: Value::Number(
                serde_json::Number::from_f64(value).expect("finite floating point value"),
            ),
            value_type: PropertyValueType::Number,
            value_text: None,
            value_number: Some(value),
            value_bool: None,
            value_date: None,
        }
    }

    pub fn boolean(value: bool) -> Self {
        Self {
            value: Value::Bool(value),
            value_type: PropertyValueType::Boolean,
            value_text: None,
            value_number: None,
            value_bool: Some(value),
            value_date: None,
        }
    }

    pub fn date(value: DateTime<Utc>) -> Self {
        Self {
            value: Value::String(value.to_rfc3339()),
            value_type: PropertyValueType::Date,
            value_text: None,
            value_number: None,
            value_bool: None,
            value_date: Some(value),
        }
    }

    pub fn json(value: Value) -> Self {
        Self {
            value,
            value_type: PropertyValueType::Json,
            value_text: None,
            value_number: None,
            value_bool: None,
            value_date: None,
        }
    }

    /// Returns the canonical string that should be stored in the `value` column.
    pub fn as_json_string(&self) -> String {
        self.value.to_string()
    }
}

/// A single namespace/key/value record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Property {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RecordId<PropertyRecord>>,
    pub entity_id: RecordId<EntityRecord>,
    pub namespace: PropertyNamespace,
    pub key: String,
    pub value: PropertyValue,
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
    pub from_id: RecordId<EntityRecord>,
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
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
}

fn default_weight() -> f32 {
    1.0
}

fn default_directed() -> bool {
    true
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
    fn property_value_text_sets_typed_columns() {
        let pv = PropertyValue::text("hello");
        assert_eq!(pv.value_type, PropertyValueType::Text);
        assert_eq!(pv.value_text.as_deref(), Some("hello"));
        assert!(pv.value_number.is_none());
        assert!(pv.value_bool.is_none());
        assert!(pv.value_date.is_none());
        assert_eq!(pv.as_json_string(), "\"hello\"");
    }

    #[test]
    fn property_value_number_sets_numeric_column() {
        let pv = PropertyValue::number(42.5);
        assert_eq!(pv.value_type, PropertyValueType::Number);
        assert_eq!(pv.value_number, Some(42.5));
        assert!(pv.value_text.is_none());
        assert_eq!(pv.as_json_string(), "42.5");
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
