//! Type definitions matching the SurrealDB schema.
//! These types directly correspond to the schema defined in schema.surql.
//!
//! Design principles:
//! - Strong typing where schema is fixed (path, title, content)
//! - Flexible types where schema is dynamic (metadata object)
//! - Serde compatibility for JSON serialization to/from SurrealDB
//! - Record ID types for graph relations

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Core Types
// ============================================================================

/// Represents a SurrealDB record ID with typed table name
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordId<T> {
    pub table: String,
    pub id: String,
    #[serde(skip)]
    _phantom: std::marker::PhantomData<T>,
}

impl<T> RecordId<T> {
    pub fn new(table: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            id: id.into(),
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn to_string(&self) -> String {
        format!("{}:{}", self.table, self.id)
    }
}

impl<T> std::fmt::Display for RecordId<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.table, self.id)
    }
}

// ============================================================================
// Notes Table
// ============================================================================

/// A note/note in the knowledge kiln
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    /// Record ID (format: "notes:path/to/file.md")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RecordId<Note>>,

    /// File path (relative to kiln root)
    pub path: String,

    /// BLAKE3 hash of file content as hex string (64 characters) for change detection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_hash: Option<String>,

    /// Note title (extracted from frontmatter or first heading)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Full markdown content
    pub content: String,

    /// Creation timestamp
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// Last modification timestamp
    #[serde(default = "Utc::now")]
    pub modified_at: DateTime<Utc>,

    /// Copy of content for full-text indexing (auto-synced via event)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_text: Option<String>,

    /// Copy of title for full-text indexing (auto-synced via event)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_text: Option<String>,

    /// Tags (e.g., ["#rust", "#database"])
    #[serde(default)]
    pub tags: Vec<String>,

    /// Embedding vector (typically 384 or 1536 dimensions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,

    /// Name of the embedding model used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,

    /// When the embedding was last updated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_updated_at: Option<DateTime<Utc>>,

    /// Flexible metadata from frontmatter (YAML properties)
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,

    /// Computed field: status from metadata (read-only)
    #[serde(skip_serializing)]
    pub status: Option<String>,

    /// Computed field: top-level folder (read-only)
    #[serde(skip_serializing)]
    pub folder: Option<String>,

    /// Computed field: filename from path (read-only)
    #[serde(skip_serializing)]
    pub file_name: Option<String>,
}

impl Note {
    /// Create a new note with minimal required fields
    pub fn new(path: impl Into<String>, content: impl Into<String>) -> Self {
        let path = path.into();
        let content = content.into();
        let now = Utc::now();

        Self {
            id: None,
            path,
            file_hash: None,
            title: None,
            content: content.clone(),
            created_at: now,
            modified_at: now,
            content_text: Some(content),
            title_text: None,
            tags: Vec::new(),
            embedding: None,
            embedding_model: None,
            embedding_updated_at: None,
            metadata: HashMap::new(),
            status: None,
            folder: None,
            file_name: None,
        }
    }

    /// Set file hash
    pub fn with_file_hash(mut self, hash: impl Into<String>) -> Self {
        self.file_hash = Some(hash.into());
        self
    }

    /// Set title (also updates title_text)
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        let title = title.into();
        self.title_text = Some(title.clone());
        self.title = Some(title);
        self
    }

    /// Add tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Set embedding
    pub fn with_embedding(mut self, embedding: Vec<f32>, model: impl Into<String>) -> Self {
        self.embedding = Some(embedding);
        self.embedding_model = Some(model.into());
        self.embedding_updated_at = Some(Utc::now());
        self
    }

    /// Add metadata property
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

// ============================================================================
// Tags Table
// ============================================================================

/// A tag with metadata and hierarchy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tag {
    /// Record ID (format: "tags:rust" or "tags:project-crucible")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RecordId<Tag>>,

    /// Tag name (without # prefix, normalized to lowercase)
    pub name: String,

    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional color (hex code or color name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,

    /// Number of notes using this tag
    #[serde(default)]
    pub usage_count: i32,

    /// Last time this tag was used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<DateTime<Utc>>,

    /// Parent tag for hierarchical tags (e.g., "project" for "project/crucible")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_tag: Option<RecordId<Tag>>,

    /// Creation timestamp
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
}

impl Tag {
    /// Create a new tag
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: None,
            name: name.into(),
            description: None,
            color: None,
            usage_count: 0,
            last_used: None,
            parent_tag: None,
            created_at: Utc::now(),
        }
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set color
    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Set parent tag
    pub fn with_parent(mut self, parent: RecordId<Tag>) -> Self {
        self.parent_tag = Some(parent);
        self
    }
}

// ============================================================================
// Graph Relations (Edges)
// ============================================================================

/// Wikilink edge: note -> note
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wikilink {
    /// Source note
    #[serde(rename = "in")]
    pub from: RecordId<Note>,

    /// Target note
    #[serde(rename = "out")]
    pub to: RecordId<Note>,

    /// The text inside [[ ]]
    pub link_text: String,

    /// Surrounding paragraph or sentence
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,

    /// Character offset in source note
    pub position: i32,

    /// When the link was created
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,

    /// Weight for graph algorithms (default: 1.0)
    #[serde(default = "default_weight")]
    pub weight: f32,
}

fn default_weight() -> f32 {
    1.0
}

impl Wikilink {
    pub fn new(
        from: RecordId<Note>,
        to: RecordId<Note>,
        link_text: impl Into<String>,
        position: i32,
    ) -> Self {
        Self {
            from,
            to,
            link_text: link_text.into(),
            context: None,
            position,
            created_at: Utc::now(),
            weight: 1.0,
        }
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }
}

/// Tagged_with edge: note -> tag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaggedWith {
    /// Source note
    #[serde(rename = "in")]
    pub from: RecordId<Note>,

    /// Target tag
    #[serde(rename = "out")]
    pub to: RecordId<Tag>,

    /// When the tag was added
    #[serde(default = "Utc::now")]
    pub added_at: DateTime<Utc>,

    /// Who/what added the tag ("user", "auto", etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_by: Option<String>,
}

impl TaggedWith {
    pub fn new(from: RecordId<Note>, to: RecordId<Tag>) -> Self {
        Self {
            from,
            to,
            added_at: Utc::now(),
            added_by: None,
        }
    }

    pub fn with_added_by(mut self, added_by: impl Into<String>) -> Self {
        self.added_by = Some(added_by.into());
        self
    }
}

/// Relates_to edge: note -> note (semantic similarity, citations, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatesTo {
    /// Source note
    #[serde(rename = "in")]
    pub from: RecordId<Note>,

    /// Target note
    #[serde(rename = "out")]
    pub to: RecordId<Note>,

    /// Type of relation ("similar", "references", "contradicts", etc.)
    pub relation_type: String,

    /// Score/strength of the relation (e.g., cosine similarity)
    #[serde(default)]
    pub score: f32,

    /// When the relation was computed
    #[serde(default = "Utc::now")]
    pub computed_at: DateTime<Utc>,

    /// Optional metadata (algorithm used, parameters, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl RelatesTo {
    pub fn new(
        from: RecordId<Note>,
        to: RecordId<Note>,
        relation_type: impl Into<String>,
        score: f32,
    ) -> Self {
        Self {
            from,
            to,
            relation_type: relation_type.into(),
            score,
            computed_at: Utc::now(),
            metadata: None,
        }
    }

    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

// ============================================================================
// Query Results
// ============================================================================

/// Search result with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Note details
    #[serde(flatten)]
    pub note: Note,

    /// Relevance score (0.0 - 1.0 or higher)
    pub score: f64,

    /// Text snippet with highlighting (for full-text search)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

/// Graph traversal result (note with relationship metadata)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// Note details
    #[serde(flatten)]
    pub note: Note,

    /// Distance from query origin (hop count)
    pub depth: u32,

    /// Relationship type that led to this node
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation_type: Option<String>,

    /// Edge weight/score
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_weight: Option<f32>,
}

/// Statistics about the kiln
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KilnStats {
    /// Total number of notes
    pub total_notes: i64,

    /// Number of notes with embeddings
    pub notes_with_embeddings: i64,

    /// Total number of wikilinks
    pub total_wikilinks: i64,

    /// Total number of unique tags
    pub total_tags: i64,

    /// Average backlink count
    pub avg_backlinks: f64,

    /// Most linked notes (top 10)
    pub hub_notes: Vec<(String, i64)>,

    /// Most used tags (top 10)
    pub popular_tags: Vec<(String, i32)>,

    /// Schema version
    pub schema_version: i32,

    /// Last updated timestamp
    pub last_updated: DateTime<Utc>,
}

// ============================================================================
// Query Builders (for type-safe query construction)
// ============================================================================

/// Builder for semantic search queries
#[derive(Debug, Clone)]
pub struct SemanticSearchQuery {
    pub embedding: Vec<f32>,
    pub limit: u32,
    pub min_similarity: Option<f32>,
    pub tags: Option<Vec<String>>,
    pub folder: Option<String>,
}

impl SemanticSearchQuery {
    pub fn new(embedding: Vec<f32>) -> Self {
        Self {
            embedding,
            limit: 10,
            min_similarity: None,
            tags: None,
            folder: None,
        }
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    pub fn min_similarity(mut self, threshold: f32) -> Self {
        self.min_similarity = Some(threshold);
        self
    }

    pub fn filter_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = Some(tags);
        self
    }

    pub fn filter_folder(mut self, folder: impl Into<String>) -> Self {
        self.folder = Some(folder.into());
        self
    }
}

/// Builder for full-text search queries
#[derive(Debug, Clone)]
pub struct FullTextSearchQuery {
    pub query: String,
    pub search_title: bool,
    pub search_content: bool,
    pub limit: u32,
    pub tags: Option<Vec<String>>,
    pub folder: Option<String>,
}

impl FullTextSearchQuery {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            search_title: true,
            search_content: true,
            limit: 20,
            tags: None,
            folder: None,
        }
    }

    pub fn title_only(mut self) -> Self {
        self.search_title = true;
        self.search_content = false;
        self
    }

    pub fn content_only(mut self) -> Self {
        self.search_title = false;
        self.search_content = true;
        self
    }

    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    pub fn filter_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = Some(tags);
        self
    }

    pub fn filter_folder(mut self, folder: impl Into<String>) -> Self {
        self.folder = Some(folder.into());
        self
    }
}

/// Builder for graph traversal queries
#[derive(Debug, Clone)]
pub struct GraphTraversalQuery {
    pub start_node: RecordId<Note>,
    pub relation_type: Option<String>,
    pub max_depth: u32,
    pub direction: TraversalDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalDirection {
    /// Follow outgoing edges (->)
    Outgoing,
    /// Follow incoming edges (<-)
    Incoming,
    /// Follow both directions
    Both,
}

impl GraphTraversalQuery {
    pub fn new(start_node: RecordId<Note>) -> Self {
        Self {
            start_node,
            relation_type: None,
            max_depth: 2,
            direction: TraversalDirection::Outgoing,
        }
    }

    pub fn relation_type(mut self, relation_type: impl Into<String>) -> Self {
        self.relation_type = Some(relation_type.into());
        self
    }

    pub fn max_depth(mut self, depth: u32) -> Self {
        self.max_depth = depth;
        self
    }

    pub fn direction(mut self, direction: TraversalDirection) -> Self {
        self.direction = direction;
        self
    }

    pub fn backlinks(mut self) -> Self {
        self.direction = TraversalDirection::Incoming;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_builder() {
        let note = Note::new("test.md", "# Test\nContent")
            .with_title("Test Note")
            .with_tags(vec!["#rust".to_string(), "#test".to_string()])
            .with_metadata("status", serde_json::json!("draft"));

        assert_eq!(note.path, "test.md");
        assert_eq!(note.title, Some("Test Note".to_string()));
        assert_eq!(note.tags.len(), 2);
        assert_eq!(
            note.metadata.get("status").unwrap(),
            &serde_json::json!("draft")
        );
    }

    #[test]
    fn test_tag_builder() {
        let parent_id = RecordId::new("tags", "project");
        let tag = Tag::new("project/crucible")
            .with_description("Crucible knowledge management system")
            .with_color("#ff5733")
            .with_parent(parent_id.clone());

        assert_eq!(tag.name, "project/crucible");
        assert_eq!(tag.parent_tag, Some(parent_id));
    }

    #[test]
    fn test_record_id() {
        let id: RecordId<Note> = RecordId::new("notes", "test.md");
        assert_eq!(id.to_string(), "notes:test.md");
        assert_eq!(id.table, "notes");
        assert_eq!(id.id, "test.md");
    }

    #[test]
    fn test_wikilink_builder() {
        let from = RecordId::new("notes", "a.md");
        let to = RecordId::new("notes", "b.md");
        let link = Wikilink::new(from, to, "Link Text", 123)
            .with_context("This is a link to another note")
            .with_weight(2.0);

        assert_eq!(link.link_text, "Link Text");
        assert_eq!(link.position, 123);
        assert_eq!(link.weight, 2.0);
    }

    #[test]
    fn test_semantic_search_builder() {
        let query = SemanticSearchQuery::new(vec![0.1, 0.2, 0.3])
            .limit(5)
            .min_similarity(0.8)
            .filter_tags(vec!["#rust".to_string()])
            .filter_folder("Projects");

        assert_eq!(query.limit, 5);
        assert_eq!(query.min_similarity, Some(0.8));
        assert_eq!(query.tags, Some(vec!["#rust".to_string()]));
    }

    #[test]
    fn test_graph_traversal_builder() {
        let start = RecordId::new("notes", "start.md");
        let query = GraphTraversalQuery::new(start).max_depth(3).backlinks();

        assert_eq!(query.max_depth, 3);
        assert_eq!(query.direction, TraversalDirection::Incoming);
    }
}
