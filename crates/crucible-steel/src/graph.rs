//! Graph traversal module for Steel scripts
//!
//! Provides database-backed graph queries via Steel functions.
//!
//! ## Steel Usage
//!
//! ```scheme
//! ;; Find a note by title (database query)
//! (db-find "Index")  ; => note hash or #f
//!
//! ;; Get outlinks from database
//! (db-outlinks "Index")  ; => list of notes
//!
//! ;; Get inlinks from database
//! (db-inlinks "Index")   ; => list of notes
//!
//! ;; Execute arbitrary graph query
//! (db-query "find(\"Index\") | ->wikilink[]")
//! ```

use crate::error::SteelError;
use crucible_core::traits::GraphQueryExecutor;
use std::sync::Arc;

/// Graph module that provides database-backed queries
///
/// This is designed to be registered with a Steel executor to provide
/// db-find, db-outlinks, db-inlinks functions.
pub struct GraphModule {
    executor: Arc<dyn GraphQueryExecutor>,
}

impl GraphModule {
    /// Create a new graph module with a database executor
    pub fn new(executor: Arc<dyn GraphQueryExecutor>) -> Self {
        Self { executor }
    }

    /// Find a note by title
    pub async fn find(&self, title: &str) -> Result<Option<serde_json::Value>, SteelError> {
        let query = format!(r#"find("{}")"#, escape_quotes(title));
        let results = self
            .executor
            .execute(&query)
            .await
            .map_err(|e| SteelError::Execution(format!("Graph query error: {}", e)))?;

        Ok(results.into_iter().next())
    }

    /// Get outlinks from a note
    pub async fn outlinks(&self, title: &str) -> Result<Vec<serde_json::Value>, SteelError> {
        let query = format!(r#"outlinks("{}")"#, escape_quotes(title));
        self.executor
            .execute(&query)
            .await
            .map_err(|e| SteelError::Execution(format!("Graph query error: {}", e)))
    }

    /// Get inlinks to a note
    pub async fn inlinks(&self, title: &str) -> Result<Vec<serde_json::Value>, SteelError> {
        let query = format!(r#"inlinks("{}")"#, escape_quotes(title));
        self.executor
            .execute(&query)
            .await
            .map_err(|e| SteelError::Execution(format!("Graph query error: {}", e)))
    }

    /// Get all neighbors (outlinks + inlinks)
    pub async fn neighbors(&self, title: &str) -> Result<Vec<serde_json::Value>, SteelError> {
        let query = format!(r#"neighbors("{}")"#, escape_quotes(title));
        self.executor
            .execute(&query)
            .await
            .map_err(|e| SteelError::Execution(format!("Graph query error: {}", e)))
    }

    /// Execute an arbitrary graph query
    pub async fn query(&self, query: &str) -> Result<Vec<serde_json::Value>, SteelError> {
        self.executor
            .execute(query)
            .await
            .map_err(|e| SteelError::Execution(format!("Graph query error: {}", e)))
    }

    /// Generate Steel code that defines the db-* functions
    ///
    /// These functions are stubs that will be replaced by Rust implementations
    /// when registered with an executor that has database access.
    pub fn steel_stubs() -> &'static str {
        r#"
;; Database-backed graph functions (stubs - replaced by Rust)
;; These provide the same interface as the pure Steel graph functions
;; but query the actual database.

(define (db-find title)
  (error "db-find not available: no database connection"))

(define (db-outlinks title)
  (error "db-outlinks not available: no database connection"))

(define (db-inlinks title)
  (error "db-inlinks not available: no database connection"))

(define (db-neighbors title)
  (error "db-neighbors not available: no database connection"))

(define (db-query q)
  (error "db-query not available: no database connection"))
"#
    }
}

/// Escape quotes in a string for safe embedding in queries
fn escape_quotes(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use crate::SteelExecutor;
    use serde_json::json;

    // Include the graph library source
    const GRAPH_LIB: &str = include_str!("../lib/graph.scm");

    #[tokio::test]
    async fn test_graph_find_existing() {
        let executor = SteelExecutor::new().unwrap();

        // Load the graph library
        executor.execute_source(GRAPH_LIB).await.unwrap();

        // Create test data and find a note
        let result = executor
            .execute_source(
                r#"
                (define notes
                  (list
                    (hash 'title "Index" 'path "Index.md" 'links '("Project A"))
                    (hash 'title "Project A" 'path "a.md" 'links '())))
                (note-title (graph-find notes "Index"))
                "#,
            )
            .await
            .unwrap();

        assert_eq!(result, json!("Index"));
    }

    #[tokio::test]
    async fn test_graph_find_missing() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(GRAPH_LIB).await.unwrap();

        let result = executor
            .execute_source(
                r#"
                (define notes
                  (list (hash 'title "Index" 'path "Index.md" 'links '())))
                (graph-find notes "Missing")
                "#,
            )
            .await
            .unwrap();

        assert_eq!(result, json!(false));
    }

    #[tokio::test]
    async fn test_graph_outlinks() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(GRAPH_LIB).await.unwrap();

        let result = executor
            .execute_source(
                r#"
                (define notes
                  (list
                    (hash 'title "Index" 'path "Index.md" 'links '("Project A" "Project B"))
                    (hash 'title "Project A" 'path "a.md" 'links '())
                    (hash 'title "Project B" 'path "b.md" 'links '())))
                (map note-title (graph-outlinks notes "Index"))
                "#,
            )
            .await
            .unwrap();

        // Should return titles of linked notes
        let titles: Vec<String> = serde_json::from_value(result).unwrap();
        assert!(titles.contains(&"Project A".to_string()));
        assert!(titles.contains(&"Project B".to_string()));
    }

    #[tokio::test]
    async fn test_graph_inlinks() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(GRAPH_LIB).await.unwrap();

        let result = executor
            .execute_source(
                r#"
                (define notes
                  (list
                    (hash 'title "Index" 'path "Index.md" 'links '("Project A"))
                    (hash 'title "Project A" 'path "a.md" 'links '("Index"))
                    (hash 'title "Project B" 'path "b.md" 'links '())))
                (map note-title (graph-inlinks notes "Index"))
                "#,
            )
            .await
            .unwrap();

        // Only Project A links to Index
        assert_eq!(result, json!(["Project A"]));
    }

    #[tokio::test]
    async fn test_graph_neighbors() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(GRAPH_LIB).await.unwrap();

        let result = executor
            .execute_source(
                r#"
                (define notes
                  (list
                    (hash 'title "Hub" 'path "hub.md" 'links '("A"))
                    (hash 'title "A" 'path "a.md" 'links '())
                    (hash 'title "B" 'path "b.md" 'links '("Hub"))))
                (length (graph-neighbors notes "Hub"))
                "#,
            )
            .await
            .unwrap();

        // Hub links to A, B links to Hub => 2 neighbors
        assert_eq!(result, json!(2));
    }

    #[tokio::test]
    async fn test_graph_filter_by_tag() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(GRAPH_LIB).await.unwrap();

        let result = executor
            .execute_source(
                r#"
                (define notes
                  (list
                    (hash 'title "Index" 'path "Index.md" 'links '() 'tags '("important"))
                    (hash 'title "Project A" 'path "a.md" 'links '() 'tags '("project" "important"))
                    (hash 'title "Draft" 'path "draft.md" 'links '() 'tags '("draft"))))
                (map note-title (graph-filter-by-tag notes "important"))
                "#,
            )
            .await
            .unwrap();

        let titles: Vec<String> = serde_json::from_value(result).unwrap();
        assert_eq!(titles.len(), 2);
        assert!(titles.contains(&"Index".to_string()));
        assert!(titles.contains(&"Project A".to_string()));
    }
}
