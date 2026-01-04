//! Graph query executor for SQLite backend
//!
//! Implements `GraphQueryExecutor` for SQLite, supporting the jaq-like query syntax.
//!
//! ## Supported Queries
//!
//! - `find("title")` - Find a note by title
//! - `outlinks("path")` - Get notes that a note links to
//! - `inlinks("path")` - Get notes that link to a note
//! - `neighbors("path")` - Get all connected notes (outlinks + inlinks)
//!
//! ## Example
//!
//! ```ignore
//! let executor = SqliteGraphQueryExecutor::new(note_store, graph_view);
//! let results = executor.execute(r#"outlinks("notes/index.md")"#).await?;
//! ```

use async_trait::async_trait;
use crucible_core::storage::note_store::{GraphView, NoteStore};
use crucible_core::traits::graph_query::{GraphQueryError, GraphQueryExecutor, GraphQueryResult};
use regex::Regex;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

/// SQLite implementation of GraphQueryExecutor
pub struct SqliteGraphQueryExecutor<S, G>
where
    S: NoteStore,
    G: GraphView,
{
    store: Arc<S>,
    graph: Arc<RwLock<G>>,
}

impl<S, G> SqliteGraphQueryExecutor<S, G>
where
    S: NoteStore,
    G: GraphView,
{
    /// Create a new executor with a note store and graph view
    pub fn new(store: Arc<S>, graph: Arc<RwLock<G>>) -> Self {
        Self { store, graph }
    }

    /// Parse a function call from the query string
    fn parse_function_call(query: &str) -> Option<(&str, String)> {
        // Match patterns like: function("arg") or function('arg')
        let re = Regex::new(r#"^(\w+)\s*\(\s*["']([^"']+)["']\s*\)$"#).ok()?;
        let caps = re.captures(query.trim())?;
        let func_name = caps.get(1)?.as_str();
        let arg = caps.get(2)?.as_str().to_string();
        Some((func_name, arg))
    }

    /// Convert a note path to a JSON value for results
    async fn path_to_note_json(&self, path: &str) -> Option<Value> {
        match self.store.get(path).await {
            Ok(Some(note)) => Some(json!({
                "path": note.path,
                "title": note.title,
                "tags": note.tags,
            })),
            _ => Some(json!({
                "path": path,
                "title": path,
                "tags": [],
            })),
        }
    }
}

#[async_trait]
impl<S, G> GraphQueryExecutor for SqliteGraphQueryExecutor<S, G>
where
    S: NoteStore + Send + Sync,
    G: GraphView + Send + Sync,
{
    async fn execute(&self, query: &str) -> GraphQueryResult<Vec<Value>> {
        let (func_name, arg) = Self::parse_function_call(query).ok_or_else(|| {
            GraphQueryError::with_query("Invalid query syntax. Expected: function(\"arg\")", query)
        })?;

        match func_name {
            "find" => {
                // Find note by title - search through all notes
                let notes = self.store.list().await.map_err(|e| {
                    GraphQueryError::with_query(format!("Failed to list notes: {}", e), query)
                })?;

                let results: Vec<Value> = notes
                    .into_iter()
                    .filter(|n| n.title.to_lowercase().contains(&arg.to_lowercase()))
                    .map(|n| {
                        json!({
                            "path": n.path,
                            "title": n.title,
                            "tags": n.tags,
                        })
                    })
                    .collect();

                Ok(results)
            }

            "outlinks" => {
                let graph = self.graph.read().await;
                let paths = graph.outlinks(&arg);
                let mut results = Vec::with_capacity(paths.len());
                for path in paths {
                    if let Some(note) = self.path_to_note_json(&path).await {
                        results.push(note);
                    }
                }
                Ok(results)
            }

            "inlinks" => {
                let graph = self.graph.read().await;
                let paths = graph.backlinks(&arg);
                let mut results = Vec::with_capacity(paths.len());
                for path in paths {
                    if let Some(note) = self.path_to_note_json(&path).await {
                        results.push(note);
                    }
                }
                Ok(results)
            }

            "neighbors" => {
                let graph = self.graph.read().await;
                let paths = graph.neighbors(&arg, 1);
                let mut results = Vec::with_capacity(paths.len());
                for path in paths {
                    if let Some(note) = self.path_to_note_json(&path).await {
                        results.push(note);
                    }
                }
                Ok(results)
            }

            _ => Err(GraphQueryError::with_query(
                format!("Unknown function: {}", func_name),
                query,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::note_store::{Filter, NoteRecord, SearchResult};
    use crucible_core::storage::StorageResult;
    use std::collections::HashMap;
    use tokio::sync::Mutex;

    // Mock NoteStore for testing
    struct MockNoteStore {
        notes: Mutex<HashMap<String, NoteRecord>>,
    }

    impl MockNoteStore {
        fn new() -> Self {
            Self {
                notes: Mutex::new(HashMap::new()),
            }
        }

        async fn add_note(&self, path: &str, title: &str, links_to: Vec<&str>) {
            let note = NoteRecord {
                path: path.to_string(),
                content_hash: BlockHash::zero(),
                embedding: None,
                title: title.to_string(),
                tags: vec![],
                links_to: links_to.into_iter().map(String::from).collect(),
                properties: HashMap::new(),
                updated_at: Utc::now(),
            };
            self.notes.lock().await.insert(path.to_string(), note);
        }
    }

    #[async_trait]
    impl NoteStore for MockNoteStore {
        async fn upsert(&self, note: NoteRecord) -> StorageResult<()> {
            self.notes.lock().await.insert(note.path.clone(), note);
            Ok(())
        }

        async fn get(&self, path: &str) -> StorageResult<Option<NoteRecord>> {
            Ok(self.notes.lock().await.get(path).cloned())
        }

        async fn delete(&self, path: &str) -> StorageResult<()> {
            self.notes.lock().await.remove(path);
            Ok(())
        }

        async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
            Ok(self.notes.lock().await.values().cloned().collect())
        }

        async fn get_by_hash(&self, _hash: &BlockHash) -> StorageResult<Option<NoteRecord>> {
            Ok(None)
        }

        async fn search(
            &self,
            _embedding: &[f32],
            _k: usize,
            _filter: Option<Filter>,
        ) -> StorageResult<Vec<SearchResult>> {
            Ok(vec![])
        }
    }

    // Mock GraphView for testing
    struct MockGraphView {
        outlinks: HashMap<String, Vec<String>>,
        backlinks: HashMap<String, Vec<String>>,
    }

    impl MockGraphView {
        fn new() -> Self {
            Self {
                outlinks: HashMap::new(),
                backlinks: HashMap::new(),
            }
        }
    }

    impl GraphView for MockGraphView {
        fn outlinks(&self, path: &str) -> Vec<String> {
            self.outlinks.get(path).cloned().unwrap_or_default()
        }

        fn backlinks(&self, path: &str) -> Vec<String> {
            self.backlinks.get(path).cloned().unwrap_or_default()
        }

        fn neighbors(&self, path: &str, _depth: usize) -> Vec<String> {
            let mut result = self.outlinks(path);
            result.extend(self.backlinks(path));
            result.sort();
            result.dedup();
            result
        }

        fn rebuild(&mut self, notes: &[NoteRecord]) {
            self.outlinks.clear();
            self.backlinks.clear();

            for note in notes {
                if !note.links_to.is_empty() {
                    self.outlinks
                        .insert(note.path.clone(), note.links_to.clone());
                }
                for target in &note.links_to {
                    self.backlinks
                        .entry(target.clone())
                        .or_default()
                        .push(note.path.clone());
                }
            }
        }
    }

    async fn setup_test_executor() -> SqliteGraphQueryExecutor<MockNoteStore, MockGraphView> {
        let store = Arc::new(MockNoteStore::new());
        let mut graph = MockGraphView::new();

        // Add test notes
        store
            .add_note("index.md", "Index", vec!["a.md", "b.md"])
            .await;
        store.add_note("a.md", "Note A", vec!["b.md"]).await;
        store.add_note("b.md", "Note B", vec![]).await;
        store.add_note("c.md", "Note C", vec!["index.md"]).await;

        // Rebuild graph
        let notes = store.list().await.unwrap();
        graph.rebuild(&notes);

        SqliteGraphQueryExecutor::new(store, Arc::new(RwLock::new(graph)))
    }

    #[tokio::test]
    async fn test_find_query() {
        let executor = setup_test_executor().await;

        let results = executor.execute(r#"find("Index")"#).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["title"], "Index");
    }

    #[tokio::test]
    async fn test_find_partial_match() {
        let executor = setup_test_executor().await;

        let results = executor.execute(r#"find("Note")"#).await.unwrap();
        assert_eq!(results.len(), 3); // Note A, Note B, Note C
    }

    #[tokio::test]
    async fn test_outlinks_query() {
        let executor = setup_test_executor().await;

        let results = executor.execute(r#"outlinks("index.md")"#).await.unwrap();
        assert_eq!(results.len(), 2);

        let paths: Vec<&str> = results
            .iter()
            .map(|v| v["path"].as_str().unwrap())
            .collect();
        assert!(paths.contains(&"a.md"));
        assert!(paths.contains(&"b.md"));
    }

    #[tokio::test]
    async fn test_inlinks_query() {
        let executor = setup_test_executor().await;

        let results = executor.execute(r#"inlinks("b.md")"#).await.unwrap();
        assert_eq!(results.len(), 2); // index.md and a.md link to b.md

        let paths: Vec<&str> = results
            .iter()
            .map(|v| v["path"].as_str().unwrap())
            .collect();
        assert!(paths.contains(&"index.md"));
        assert!(paths.contains(&"a.md"));
    }

    #[tokio::test]
    async fn test_neighbors_query() {
        let executor = setup_test_executor().await;

        let results = executor.execute(r#"neighbors("a.md")"#).await.unwrap();
        // a.md links to b.md, and index.md links to a.md
        assert!(results.len() >= 2);
    }

    #[tokio::test]
    async fn test_invalid_syntax() {
        let executor = setup_test_executor().await;

        let result = executor.execute("invalid query").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Invalid query syntax"));
    }

    #[tokio::test]
    async fn test_unknown_function() {
        let executor = setup_test_executor().await;

        let result = executor.execute(r#"unknown("arg")"#).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Unknown function"));
    }

    #[tokio::test]
    async fn test_single_quotes() {
        let executor = setup_test_executor().await;

        // Should also accept single quotes
        let results = executor.execute(r#"find('Index')"#).await.unwrap();
        assert_eq!(results.len(), 1);
    }
}
