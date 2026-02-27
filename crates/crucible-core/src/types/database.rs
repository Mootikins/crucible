//! Database domain types consumed across the Crucible workspace.
//!
//! These types originated in the `database` module and are consumed by multiple
//! crates (crucible-cli, crucible-rpc, crucible-sqlite, crucible-daemon).
//! They live here as the canonical definitions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Common result type for database operations
pub type DbResult<T> = Result<T, DbError>;

/// Database operation errors
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum DbError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Query error: {0}")]
    Query(String),

    #[error("Schema error: {0}")]
    Schema(String),

    #[error("Transaction error: {0}")]
    Transaction(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Document identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DocumentId(pub String);

impl std::fmt::Display for DocumentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Database record (row)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    /// Optional record identifier
    pub id: Option<RecordId>,
    /// Record field data (column values or note fields)
    pub data: HashMap<String, serde_json::Value>,
}

impl Record {
    /// Create a new record without an ID
    pub fn new(data: HashMap<String, serde_json::Value>) -> Self {
        Self { id: None, data }
    }

    /// Create a new record with an ID
    pub fn with_id(id: RecordId, data: HashMap<String, serde_json::Value>) -> Self {
        Self { id: Some(id), data }
    }
}

/// Record identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RecordId(pub String);

impl std::fmt::Display for RecordId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for RecordId {
    fn from(s: String) -> Self {
        RecordId(s)
    }
}

impl From<&str> for RecordId {
    fn from(s: &str) -> Self {
        RecordId(s.to_string())
    }
}

/// Query result containing records and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub records: Vec<Record>,
    pub total_count: Option<u64>,
    pub execution_time_ms: Option<u64>,
    pub has_more: bool,
}

impl QueryResult {
    /// Create a new empty query result
    pub fn empty() -> Self {
        Self {
            records: Vec::new(),
            total_count: Some(0),
            execution_time_ms: None,
            has_more: false,
        }
    }

    /// Create a query result with records
    pub fn with_records(records: Vec<Record>) -> Self {
        let total_count = records.len() as u64;
        Self {
            records,
            total_count: Some(total_count),
            execution_time_ms: None,
            has_more: false,
        }
    }
}

/// Search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub document_id: DocumentId,
    pub score: f64,
    pub highlights: Option<Vec<String>>,
    pub snippet: Option<String>,
    pub kiln_path: Option<PathBuf>,
}

/// Unified search result that can represent either a note or a skill
///
/// This enum supports searching across both notes and agent skills, allowing
/// semantic search to return relevant results from either source. Results can
/// be merged and sorted by relevance score.
///
/// # JSON Serialization
///
/// The enum uses `#[serde(tag = "type")]` to include a discriminator field:
///
/// ```json
/// // Note result
/// {
///   "type": "note",
///   "document_id": "notes/rust.md",
///   "score": 0.85,
///   "snippet": "...",
///   "highlights": [...]
/// }
///
/// // Skill result
/// {
///   "type": "skill",
///   "name": "git-commit",
///   "description": "Create well-formatted git commits",
///   "scope": "personal",
///   "score": 0.82
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UnifiedSearchResult {
    /// A note search result
    Note {
        #[serde(flatten)]
        result: SearchResult,
    },
    /// A skill search result
    Skill {
        name: String,
        description: String,
        scope: String,
        score: f64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_search_result_note_serialization() {
        let note_result = UnifiedSearchResult::Note {
            result: SearchResult {
                document_id: DocumentId("notes/rust.md".to_string()),
                score: 0.85,
                highlights: Some(vec!["memory safety".to_string()]),
                snippet: Some("Rust is a systems programming language...".to_string()),
                kiln_path: None,
            },
        };

        let json = serde_json::to_value(&note_result).unwrap();

        // Check discriminator field
        assert_eq!(json["type"], "note");
        assert_eq!(json["document_id"], "notes/rust.md");
        assert_eq!(json["score"], 0.85);

        // Verify round-trip
        let deserialized: UnifiedSearchResult = serde_json::from_value(json).unwrap();
        match deserialized {
            UnifiedSearchResult::Note { result } => {
                assert_eq!(result.document_id.0, "notes/rust.md");
                assert_eq!(result.score, 0.85);
            }
            _ => panic!("Expected Note variant"),
        }
    }

    #[test]
    fn test_unified_search_result_skill_serialization() {
        let skill_result = UnifiedSearchResult::Skill {
            name: "git-commit".to_string(),
            description: "Create well-formatted git commits".to_string(),
            scope: "personal".to_string(),
            score: 0.82,
        };

        let json = serde_json::to_value(&skill_result).unwrap();

        // Check discriminator field
        assert_eq!(json["type"], "skill");
        assert_eq!(json["name"], "git-commit");
        assert_eq!(json["description"], "Create well-formatted git commits");
        assert_eq!(json["scope"], "personal");
        assert_eq!(json["score"], 0.82);

        // Verify round-trip
        let deserialized: UnifiedSearchResult = serde_json::from_value(json).unwrap();
        match deserialized {
            UnifiedSearchResult::Skill {
                name,
                description,
                scope,
                score,
            } => {
                assert_eq!(name, "git-commit");
                assert_eq!(description, "Create well-formatted git commits");
                assert_eq!(scope, "personal");
                assert_eq!(score, 0.82);
            }
            _ => panic!("Expected Skill variant"),
        }
    }

    #[test]
    fn test_unified_search_result_sorting() {
        let mut results = [
            UnifiedSearchResult::Note {
                result: SearchResult {
                    document_id: DocumentId("notes/low.md".to_string()),
                    score: 0.60,
                    highlights: None,
                    snippet: None,
                    kiln_path: None,
                },
            },
            UnifiedSearchResult::Skill {
                name: "high-skill".to_string(),
                description: "High scoring skill".to_string(),
                scope: "personal".to_string(),
                score: 0.90,
            },
            UnifiedSearchResult::Note {
                result: SearchResult {
                    document_id: DocumentId("notes/medium.md".to_string()),
                    score: 0.75,
                    highlights: None,
                    snippet: None,
                    kiln_path: None,
                },
            },
        ];

        // Sort by score descending
        results.sort_by(|a, b| {
            let score_a = match a {
                UnifiedSearchResult::Note { result } => result.score,
                UnifiedSearchResult::Skill { score, .. } => *score,
            };
            let score_b = match b {
                UnifiedSearchResult::Note { result } => result.score,
                UnifiedSearchResult::Skill { score, .. } => *score,
            };
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Verify order: 0.90, 0.75, 0.60
        match &results[0] {
            UnifiedSearchResult::Skill { score, .. } => assert_eq!(*score, 0.90),
            _ => panic!("Expected highest score first"),
        }
        match &results[1] {
            UnifiedSearchResult::Note { result } => assert_eq!(result.score, 0.75),
            _ => panic!("Expected second highest score"),
        }
        match &results[2] {
            UnifiedSearchResult::Note { result } => assert_eq!(result.score, 0.60),
            _ => panic!("Expected lowest score last"),
        }
    }
}
