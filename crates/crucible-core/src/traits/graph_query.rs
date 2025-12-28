//! Graph query execution abstraction
//!
//! This trait abstracts graph query execution, allowing the scripting layers
//! (Rune, Lua) to execute graph queries without depending on specific database
//! implementations.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────┐
//! │ crucible-core                   │
//! │   GraphQueryExecutor trait      │  ← Abstraction
//! └─────────────────────────────────┘
//!               ▲
//!               │ implements
//! ┌─────────────────────────────────┐
//! │ crucible-surrealdb              │
//! │   SurrealGraphExecutor          │  ← Concrete implementation
//! └─────────────────────────────────┘
//!               ▲
//!               │ injected into
//! ┌─────────────┴───────────────────┐
//! │ crucible-rune / crucible-lua    │
//! │   graph_module_with_executor()  │  ← Module factories
//! └─────────────────────────────────┘
//! ```
//!
//! ## Supported Query Syntax
//!
//! The executor accepts jaq-like query syntax:
//!
//! - `find("Note Title")` - Find a note by title
//! - `outlinks("Note Title")` - Get notes linked from a note
//! - `inlinks("Note Title")` - Get notes linking to a note
//! - `neighbors("Note Title")` - Get all connected notes
//! - `find("Index") | ->wikilink[]` - Arrow traversal syntax
//!
//! ## Example
//!
//! ```rust,ignore
//! use crucible_core::traits::GraphQueryExecutor;
//!
//! async fn search_graph(executor: &dyn GraphQueryExecutor) {
//!     // Find all notes linked from Index
//!     let results = executor.execute(r#"outlinks("Index")"#).await?;
//!
//!     for note in results {
//!         println!("Found: {}", note["title"]);
//!     }
//! }
//! ```

use async_trait::async_trait;
use serde_json::Value;
use std::fmt;

/// Error type for graph query operations
#[derive(Debug, Clone)]
pub struct GraphQueryError {
    /// Error message
    pub message: String,
    /// Optional query that caused the error
    pub query: Option<String>,
}

impl GraphQueryError {
    /// Create a new error with a message
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            query: None,
        }
    }

    /// Create a new error with a message and query context
    pub fn with_query(message: impl Into<String>, query: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            query: Some(query.into()),
        }
    }
}

impl fmt::Display for GraphQueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(query) = &self.query {
            write!(f, "{} (query: {})", self.message, query)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for GraphQueryError {}

/// Result type for graph query operations
pub type GraphQueryResult<T> = Result<T, GraphQueryError>;

/// Trait for executing graph queries
///
/// This trait abstracts the execution of graph queries, allowing scripting
/// layers to query the knowledge graph without depending on specific database
/// implementations.
///
/// Implementations should translate the jaq-like query syntax to the native
/// database query language and return results as JSON values.
#[async_trait]
pub trait GraphQueryExecutor: Send + Sync {
    /// Execute a graph query and return matching notes
    ///
    /// # Arguments
    ///
    /// * `query` - A jaq-like query string (e.g., `outlinks("Index")`)
    ///
    /// # Returns
    ///
    /// A vector of JSON values representing the matched notes, or an error.
    ///
    /// Each note value is a JSON object with at least a `title` field.
    async fn execute(&self, query: &str) -> GraphQueryResult<Vec<Value>>;

    /// Execute a graph query and return a single result
    ///
    /// Convenience method for queries expected to return a single result.
    /// Returns `None` if no results found.
    async fn execute_one(&self, query: &str) -> GraphQueryResult<Option<Value>> {
        let results = self.execute(query).await?;
        Ok(results.into_iter().next())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // GraphQueryError tests
    // =========================================================================

    #[test]
    fn test_error_new() {
        let err = GraphQueryError::new("Query failed");
        assert_eq!(err.message, "Query failed");
        assert!(err.query.is_none());
    }

    #[test]
    fn test_error_with_query() {
        let err = GraphQueryError::with_query("Syntax error", "outlinks(bad)");
        assert_eq!(err.message, "Syntax error");
        assert_eq!(err.query, Some("outlinks(bad)".to_string()));
    }

    #[test]
    fn test_error_display_without_query() {
        let err = GraphQueryError::new("Query failed");
        assert_eq!(format!("{}", err), "Query failed");
    }

    #[test]
    fn test_error_display_with_query() {
        let err = GraphQueryError::with_query("Syntax error", "outlinks(bad)");
        assert_eq!(format!("{}", err), "Syntax error (query: outlinks(bad))");
    }

    // =========================================================================
    // Mock executor for testing the trait
    // =========================================================================

    /// Mock executor that returns predetermined results
    struct MockGraphExecutor {
        results: Vec<Value>,
    }

    #[async_trait]
    impl GraphQueryExecutor for MockGraphExecutor {
        async fn execute(&self, _query: &str) -> GraphQueryResult<Vec<Value>> {
            Ok(self.results.clone())
        }
    }

    #[tokio::test]
    async fn test_execute_returns_results() {
        let executor = MockGraphExecutor {
            results: vec![
                serde_json::json!({"title": "Note A", "path": "a.md"}),
                serde_json::json!({"title": "Note B", "path": "b.md"}),
            ],
        };

        let results = executor.execute("outlinks(\"Index\")").await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["title"], "Note A");
    }

    #[tokio::test]
    async fn test_execute_one_returns_first() {
        let executor = MockGraphExecutor {
            results: vec![
                serde_json::json!({"title": "Note A"}),
                serde_json::json!({"title": "Note B"}),
            ],
        };

        let result = executor.execute_one("find(\"Note A\")").await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap()["title"], "Note A");
    }

    #[tokio::test]
    async fn test_execute_one_returns_none_on_empty() {
        let executor = MockGraphExecutor { results: vec![] };

        let result = executor.execute_one("find(\"Missing\")").await.unwrap();
        assert!(result.is_none());
    }

    // =========================================================================
    // Error propagation tests
    // =========================================================================

    /// Mock executor that always returns an error
    struct FailingExecutor {
        error_message: String,
    }

    #[async_trait]
    impl GraphQueryExecutor for FailingExecutor {
        async fn execute(&self, query: &str) -> GraphQueryResult<Vec<Value>> {
            Err(GraphQueryError::with_query(&self.error_message, query))
        }
    }

    #[tokio::test]
    async fn test_error_propagation() {
        let executor = FailingExecutor {
            error_message: "Connection failed".to_string(),
        };

        let result = executor.execute("outlinks(\"X\")").await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.message.contains("Connection failed"));
        assert_eq!(err.query, Some("outlinks(\"X\")".to_string()));
    }

    #[tokio::test]
    async fn test_error_propagation_through_execute_one() {
        let executor = FailingExecutor {
            error_message: "Database offline".to_string(),
        };

        let result = executor.execute_one("find(\"Note\")").await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.message.contains("Database offline"));
    }
}
