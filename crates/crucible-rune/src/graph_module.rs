//! Graph traversal module for Rune scripts
//!
//! Provides functions for traversing note graphs (outlinks, inlinks).
//!
//! # Example
//!
//! ```rune
//! use graph::{find, outlinks, inlinks};
//!
//! // Build a graph from notes
//! let g = #{
//!     notes: [
//!         #{ title: "Index", path: "Index.md", links: ["Project A", "Project B"] },
//!         #{ title: "Project A", path: "projects/a.md", links: ["Index"] },
//!         #{ title: "Project B", path: "projects/b.md", links: [] },
//!     ]
//! };
//!
//! // Get notes linked FROM a note (outlinks)
//! let out = graph::outlinks(g, "Index")?;  // returns [Project A, Project B]
//!
//! // Get notes linking TO a note (inlinks/backlinks)
//! let back = graph::inlinks(g, "Index")?;  // returns [Project A]
//!
//! // Find a note by title
//! let note = graph::find(g, "Index")?;
//! ```

use crate::mcp_types::{json_to_rune, rune_to_json};
use rune::runtime::{ToValue, VmResult};
use rune::{Any, ContextError, Module, Value};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Error type for graph operations (Rune-compatible)
#[derive(Debug, Clone, Any)]
#[rune(item = ::graph, name = GraphError)]
pub struct RuneGraphError {
    /// Error message
    #[rune(get)]
    pub message: String,
}

impl std::fmt::Display for RuneGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl RuneGraphError {
    fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

// =============================================================================
// Internal Helper Functions (work with JSON values)
// =============================================================================

/// Extract notes array from graph JSON object
fn get_notes_json(graph: &JsonValue) -> Result<&Vec<JsonValue>, String> {
    graph
        .get("notes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Graph must have 'notes' array field".to_string())
}

/// Get title from a note JSON object
fn get_title_json(note: &JsonValue) -> Result<&str, String> {
    note.get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Note must have 'title' string field".to_string())
}

/// Get links array from a note JSON object
fn get_links_json(note: &JsonValue) -> Vec<&str> {
    note.get("links")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_default()
}

/// Internal find implementation (JSON-based)
fn find_impl_json(graph: &JsonValue, title: &str) -> Result<Option<JsonValue>, String> {
    let notes = get_notes_json(graph)?;

    for note in notes {
        if get_title_json(note)? == title {
            return Ok(Some(note.clone()));
        }
    }

    Ok(None)
}

/// Internal outlinks implementation (JSON-based)
fn outlinks_impl_json(graph: &JsonValue, title: &str) -> Result<Vec<JsonValue>, String> {
    let notes = get_notes_json(graph)?;

    // Find the source note and get its links
    let mut source_links: Vec<&str> = Vec::new();
    for note in notes {
        if get_title_json(note)? == title {
            source_links = get_links_json(note);
            break;
        }
    }

    // Find notes that match the links
    let mut result: Vec<JsonValue> = Vec::new();
    for note in notes {
        let note_title = get_title_json(note)?;
        if source_links.contains(&note_title) {
            result.push(note.clone());
        }
    }

    Ok(result)
}

/// Internal inlinks implementation (JSON-based)
fn inlinks_impl_json(graph: &JsonValue, title: &str) -> Result<Vec<JsonValue>, String> {
    let notes = get_notes_json(graph)?;

    let mut result: Vec<JsonValue> = Vec::new();
    for note in notes {
        let links = get_links_json(note);
        if links.contains(&title) {
            result.push(note.clone());
        }
    }

    Ok(result)
}

// =============================================================================
// Rune Functions
// =============================================================================

/// Find a note by title
///
/// Returns the note object if found, or unit if not found.
#[rune::function]
fn find(graph: HashMap<String, Value>, title: String) -> Result<Value, RuneGraphError> {
    // Convert Rune graph to JSON
    let graph_value: Value = graph
        .to_value()
        .map_err(|e| RuneGraphError::new(format!("Failed to convert graph: {:?}", e)))?;

    let graph_json = rune_to_json(&graph_value)
        .map_err(|e| RuneGraphError::new(format!("Failed to convert to JSON: {:?}", e)))?;

    // Perform operation on JSON
    match find_impl_json(&graph_json, &title).map_err(RuneGraphError::new)? {
        Some(note_json) => {
            // Convert back to Rune
            match json_to_rune(&note_json) {
                VmResult::Ok(v) => Ok(v),
                VmResult::Err(e) => Err(RuneGraphError::new(format!("Conversion error: {:?}", e))),
            }
        }
        None => Ok(Value::empty()),
    }
}

/// Get outlinks (notes linked FROM the given note)
///
/// Returns an array of note objects that the source note links to.
#[rune::function]
fn outlinks(graph: HashMap<String, Value>, title: String) -> Result<Value, RuneGraphError> {
    // Convert Rune graph to JSON
    let graph_value: Value = graph
        .to_value()
        .map_err(|e| RuneGraphError::new(format!("Failed to convert graph: {:?}", e)))?;

    let graph_json = rune_to_json(&graph_value)
        .map_err(|e| RuneGraphError::new(format!("Failed to convert to JSON: {:?}", e)))?;

    // Perform operation on JSON
    let result_json = outlinks_impl_json(&graph_json, &title).map_err(RuneGraphError::new)?;

    // Convert back to Rune
    let result_array = JsonValue::Array(result_json);
    match json_to_rune(&result_array) {
        VmResult::Ok(v) => Ok(v),
        VmResult::Err(e) => Err(RuneGraphError::new(format!("Conversion error: {:?}", e))),
    }
}

/// Get inlinks/backlinks (notes linking TO the given note)
///
/// Returns an array of note objects that link to the target note.
#[rune::function]
fn inlinks(graph: HashMap<String, Value>, title: String) -> Result<Value, RuneGraphError> {
    // Convert Rune graph to JSON
    let graph_value: Value = graph
        .to_value()
        .map_err(|e| RuneGraphError::new(format!("Failed to convert graph: {:?}", e)))?;

    let graph_json = rune_to_json(&graph_value)
        .map_err(|e| RuneGraphError::new(format!("Failed to convert to JSON: {:?}", e)))?;

    // Perform operation on JSON
    let result_json = inlinks_impl_json(&graph_json, &title).map_err(RuneGraphError::new)?;

    // Convert back to Rune
    let result_array = JsonValue::Array(result_json);
    match json_to_rune(&result_array) {
        VmResult::Ok(v) => Ok(v),
        VmResult::Err(e) => Err(RuneGraphError::new(format!("Conversion error: {:?}", e))),
    }
}

// =============================================================================
// Module Registration
// =============================================================================

/// Create the graph module for Rune (in-memory mode)
///
/// This version operates on in-memory graph structures passed from Rune.
/// For database-backed queries, use `graph_module_with_executor`.
pub fn graph_module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate("graph")?;

    // Register the error type
    module.ty::<RuneGraphError>()?;

    // Register functions
    module.function_meta(find)?;
    module.function_meta(outlinks)?;
    module.function_meta(inlinks)?;

    Ok(module)
}

// =============================================================================
// Database-backed Module Registration
// =============================================================================

use crucible_core::storage::{GraphView, NoteStore};
use crucible_core::traits::GraphQueryExecutor;
use std::sync::Arc;

/// Create a graph module backed by a database executor
///
/// This version uses async functions that query the actual database
/// via the `GraphQueryExecutor` trait.
///
/// # Example
///
/// ```rust,ignore
/// use crucible_core::traits::GraphQueryExecutor;
/// use crucible_rune::graph_module_with_executor;
///
/// let executor: Arc<dyn GraphQueryExecutor> = create_graph_executor(client);
/// let module = graph_module_with_executor(executor)?;
///
/// // In Rune scripts:
/// // let note = graph::db_find("Index").await?;
/// // let links = graph::db_outlinks("Index").await?;
/// ```
pub fn graph_module_with_executor(
    executor: Arc<dyn GraphQueryExecutor>,
) -> Result<Module, ContextError> {
    let mut module = Module::with_crate("graph")?;

    // Register the error type
    module.ty::<RuneGraphError>()?;

    // Keep in-memory functions for backward compatibility
    module.function_meta(find)?;
    module.function_meta(outlinks)?;
    module.function_meta(inlinks)?;

    // Add database-backed async functions with db_ prefix
    // These query the actual database instead of in-memory structures

    // db_find - Find a note by title in the database
    let exec = executor.clone();
    module
        .function("db_find", move |title: String| {
            let exec = exec.clone();
            async move {
                let query = format!(r#"find("{}")"#, escape_quotes(&title));
                match exec.execute(&query).await {
                    Ok(results) => {
                        if let Some(first) = results.into_iter().next() {
                            json_to_rune(&first)
                        } else {
                            VmResult::Ok(Value::empty())
                        }
                    }
                    Err(e) => VmResult::Err(rune::runtime::VmError::panic(format!(
                        "Graph query error: {}",
                        e
                    ))),
                }
            }
        })
        .build()?;

    // db_outlinks - Get outlinks from database
    let exec = executor.clone();
    module
        .function("db_outlinks", move |title: String| {
            let exec = exec.clone();
            async move {
                let query = format!(r#"outlinks("{}")"#, escape_quotes(&title));
                match exec.execute(&query).await {
                    Ok(results) => {
                        let array = JsonValue::Array(results);
                        json_to_rune(&array)
                    }
                    Err(e) => VmResult::Err(rune::runtime::VmError::panic(format!(
                        "Graph query error: {}",
                        e
                    ))),
                }
            }
        })
        .build()?;

    // db_inlinks - Get inlinks from database
    let exec = executor.clone();
    module
        .function("db_inlinks", move |title: String| {
            let exec = exec.clone();
            async move {
                let query = format!(r#"inlinks("{}")"#, escape_quotes(&title));
                match exec.execute(&query).await {
                    Ok(results) => {
                        let array = JsonValue::Array(results);
                        json_to_rune(&array)
                    }
                    Err(e) => VmResult::Err(rune::runtime::VmError::panic(format!(
                        "Graph query error: {}",
                        e
                    ))),
                }
            }
        })
        .build()?;

    // db_neighbors - Get all connected notes from database
    let exec = executor.clone();
    module
        .function("db_neighbors", move |title: String| {
            let exec = exec.clone();
            async move {
                let query = format!(r#"neighbors("{}")"#, escape_quotes(&title));
                match exec.execute(&query).await {
                    Ok(results) => {
                        let array = JsonValue::Array(results);
                        json_to_rune(&array)
                    }
                    Err(e) => VmResult::Err(rune::runtime::VmError::panic(format!(
                        "Graph query error: {}",
                        e
                    ))),
                }
            }
        })
        .build()?;

    // db_query - Execute arbitrary graph query
    let exec = executor.clone();
    module
        .function("db_query", move |query: String| {
            let exec = exec.clone();
            async move {
                match exec.execute(&query).await {
                    Ok(results) => {
                        let array = JsonValue::Array(results);
                        json_to_rune(&array)
                    }
                    Err(e) => VmResult::Err(rune::runtime::VmError::panic(format!(
                        "Graph query error: {}",
                        e
                    ))),
                }
            }
        })
        .build()?;

    Ok(module)
}

// =============================================================================
// NoteStore Functions
// =============================================================================

/// Register note store functions on an existing module
///
/// This adds `note_get` and `note_list` functions that query the NoteStore.
/// Use with `graph_module_with_stores()` for a module that has both
/// graph traversal and note store functionality.
///
/// # Functions registered
///
/// - `note_get(path)` - Get a note by path, returns note record or nil
/// - `note_list(limit)` - List notes up to limit, returns array
///
/// # Example
///
/// ```rust,ignore
/// use crucible_core::storage::NoteStore;
/// use crucible_rune::register_note_functions;
///
/// let store: Arc<dyn NoteStore> = create_store();
/// let mut module = Module::with_crate("graph")?;
/// register_note_functions(&mut module, store)?;
/// ```
pub fn register_note_functions(
    module: &mut Module,
    store: Arc<dyn NoteStore>,
) -> Result<(), ContextError> {
    // note_get - Get a note by path
    let store_clone = Arc::clone(&store);
    module
        .function("note_get", move |path: String| {
            let s = Arc::clone(&store_clone);
            async move {
                match s.get(&path).await {
                    Ok(Some(record)) => {
                        // Serialize NoteRecord to JSON, then convert to Rune Value
                        match serde_json::to_value(&record) {
                            Ok(json) => json_to_rune(&json),
                            Err(e) => VmResult::Err(rune::runtime::VmError::panic(format!(
                                "Failed to serialize note record: {}",
                                e
                            ))),
                        }
                    }
                    Ok(None) => VmResult::Ok(Value::empty()),
                    Err(e) => VmResult::Err(rune::runtime::VmError::panic(format!(
                        "Note store error: {}",
                        e
                    ))),
                }
            }
        })
        .build()?;

    // note_list - List notes with optional limit
    let store_clone = Arc::clone(&store);
    module
        .function("note_list", move |limit: i64| {
            let s = Arc::clone(&store_clone);
            async move {
                match s.list().await {
                    Ok(records) => {
                        // Apply limit (if limit <= 0, return all)
                        let limited: Vec<_> = if limit > 0 {
                            records.into_iter().take(limit as usize).collect()
                        } else {
                            records
                        };

                        // Serialize to JSON array, then convert to Rune Value
                        match serde_json::to_value(&limited) {
                            Ok(json) => json_to_rune(&json),
                            Err(e) => VmResult::Err(rune::runtime::VmError::panic(format!(
                                "Failed to serialize note records: {}",
                                e
                            ))),
                        }
                    }
                    Err(e) => VmResult::Err(rune::runtime::VmError::panic(format!(
                        "Note store error: {}",
                        e
                    ))),
                }
            }
        })
        .build()?;

    Ok(())
}

// =============================================================================
// GraphView Functions (Fast Path)
// =============================================================================

/// Register fast graph traversal functions using GraphView
///
/// These functions provide O(1) lookups for graph traversal, bypassing
/// the query parser. Use these when you need fast, synchronous access
/// to link relationships.
///
/// # Functions registered
///
/// - `fast_outlinks(path)` - Get paths of notes this note links to
/// - `fast_backlinks(path)` - Get paths of notes linking to this note
/// - `fast_neighbors(path, depth)` - Get all connected notes within depth
///
/// # Example
///
/// ```rust,ignore
/// use crucible_core::storage::GraphView;
/// use crucible_rune::register_graph_view_functions;
///
/// let view: Arc<dyn GraphView> = create_graph_view();
/// let mut module = Module::with_crate("graph")?;
/// register_graph_view_functions(&mut module, view)?;
///
/// // In Rune scripts:
/// // let links = graph::fast_outlinks("notes/index.md");
/// // let backlinks = graph::fast_backlinks("notes/target.md");
/// // let nearby = graph::fast_neighbors("notes/hub.md", 2);
/// ```
pub fn register_graph_view_functions(
    module: &mut Module,
    view: Arc<dyn GraphView>,
) -> Result<(), ContextError> {
    // fast_outlinks - Get paths of notes this note links to
    let v = Arc::clone(&view);
    module
        .function("fast_outlinks", move |path: String| v.outlinks(&path))
        .build()?;

    // fast_backlinks - Get paths of notes linking to this note
    let v = Arc::clone(&view);
    module
        .function("fast_backlinks", move |path: String| v.backlinks(&path))
        .build()?;

    // fast_neighbors - Get all connected notes within depth
    let v = Arc::clone(&view);
    module
        .function("fast_neighbors", move |path: String, depth: i64| {
            v.neighbors(&path, depth as usize)
        })
        .build()?;

    Ok(())
}

/// Create a graph module with both executor and note store
///
/// This combines `graph_module_with_executor` functionality with NoteStore
/// functions, providing a unified module for graph traversal and note access.
///
/// # Functions available
///
/// From GraphQueryExecutor:
/// - `db_find(title)` - Find note by title
/// - `db_outlinks(title)` - Get outlinks
/// - `db_inlinks(title)` - Get inlinks
/// - `db_neighbors(title)` - Get all connected notes
/// - `db_query(query)` - Execute arbitrary graph query
///
/// From NoteStore:
/// - `note_get(path)` - Get note by path
/// - `note_list(limit)` - List notes
///
/// From in-memory (backward compat):
/// - `find(graph, title)`
/// - `outlinks(graph, title)`
/// - `inlinks(graph, title)`
///
/// # Example
///
/// ```rust,ignore
/// use crucible_core::traits::GraphQueryExecutor;
/// use crucible_core::storage::NoteStore;
/// use crucible_rune::graph_module_with_stores;
///
/// let executor: Arc<dyn GraphQueryExecutor> = create_executor();
/// let store: Arc<dyn NoteStore> = create_store();
/// let module = graph_module_with_stores(executor, store)?;
///
/// // In Rune scripts:
/// // let note = graph::note_get("notes/example.md").await?;
/// // let all = graph::note_list(100).await;
/// ```
pub fn graph_module_with_stores(
    executor: Arc<dyn GraphQueryExecutor>,
    store: Arc<dyn NoteStore>,
) -> Result<Module, ContextError> {
    // Start with the executor-based module
    let mut module = graph_module_with_executor(executor)?;

    // Add NoteStore functions
    register_note_functions(&mut module, store)?;

    Ok(module)
}

/// Create a graph module with executor, note store, and graph view
///
/// This is the most complete module, providing:
/// - Database-backed query functions (db_*)
/// - NoteStore access (note_*)
/// - Fast GraphView traversal (fast_*)
///
/// # Functions available
///
/// From GraphQueryExecutor:
/// - `db_find(title)` - Find note by title
/// - `db_outlinks(title)` - Get outlinks via query
/// - `db_inlinks(title)` - Get inlinks via query
/// - `db_neighbors(title)` - Get all connected notes via query
/// - `db_query(query)` - Execute arbitrary graph query
///
/// From NoteStore:
/// - `note_get(path)` - Get note by path
/// - `note_list(limit)` - List notes
///
/// From GraphView (fast path):
/// - `fast_outlinks(path)` - O(1) outlinks lookup
/// - `fast_backlinks(path)` - O(1) backlinks lookup
/// - `fast_neighbors(path, depth)` - O(k) neighbors lookup
///
/// # Example
///
/// ```rust,ignore
/// use crucible_core::traits::GraphQueryExecutor;
/// use crucible_core::storage::{NoteStore, GraphView};
/// use crucible_rune::graph_module_with_all;
///
/// let executor: Arc<dyn GraphQueryExecutor> = create_executor();
/// let store: Arc<dyn NoteStore> = create_store();
/// let view: Arc<dyn GraphView> = create_view();
/// let module = graph_module_with_all(executor, store, view)?;
/// ```
pub fn graph_module_with_all(
    executor: Arc<dyn GraphQueryExecutor>,
    store: Arc<dyn NoteStore>,
    view: Arc<dyn GraphView>,
) -> Result<Module, ContextError> {
    // Start with executor + store
    let mut module = graph_module_with_stores(executor, store)?;

    // Add fast GraphView functions
    register_graph_view_functions(&mut module, view)?;

    Ok(module)
}

/// Escape double quotes in a string for safe embedding in queries
fn escape_quotes(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // =========================================================================
    // Module creation test
    // =========================================================================

    #[test]
    fn test_graph_module_creation() {
        let module = graph_module();
        assert!(module.is_ok(), "Should create graph module");
    }

    // =========================================================================
    // JSON-based tests (test internal implementations)
    // =========================================================================

    #[test]
    fn test_find_existing_note() {
        let graph = json!({
            "notes": [
                { "title": "Index", "path": "Index.md", "links": [] },
                { "title": "Project A", "path": "a.md", "links": [] }
            ]
        });

        let result = find_impl_json(&graph, "Index").unwrap();
        assert!(result.is_some(), "Should find note");
        assert_eq!(result.unwrap()["title"], "Index");
    }

    #[test]
    fn test_find_missing_note_returns_none() {
        let graph = json!({
            "notes": [{ "title": "Index", "path": "Index.md", "links": [] }]
        });

        let result = find_impl_json(&graph, "NonExistent").unwrap();
        assert!(result.is_none(), "Should return None for missing note");
    }

    #[test]
    fn test_outlinks_returns_linked_notes() {
        let graph = json!({
            "notes": [
                { "title": "Index", "path": "Index.md", "links": ["Project A", "Project B"] },
                { "title": "Project A", "path": "a.md", "links": ["Index"] },
                { "title": "Project B", "path": "b.md", "links": [] },
                { "title": "Orphan", "path": "orphan.md", "links": [] }
            ]
        });

        let result = outlinks_impl_json(&graph, "Index").unwrap();
        assert_eq!(result.len(), 2, "Should return 2 notes");

        let mut titles: Vec<&str> = result.iter().filter_map(|n| n["title"].as_str()).collect();
        titles.sort();

        assert_eq!(titles, vec!["Project A", "Project B"]);
    }

    #[test]
    fn test_outlinks_empty_when_no_links() {
        let graph = json!({
            "notes": [{ "title": "Orphan", "path": "orphan.md", "links": [] }]
        });

        let result = outlinks_impl_json(&graph, "Orphan").unwrap();
        assert_eq!(result.len(), 0, "Should return empty vec");
    }

    #[test]
    fn test_inlinks_returns_notes_linking_to_target() {
        let graph = json!({
            "notes": [
                { "title": "Index", "path": "Index.md", "links": ["Project A", "Project B"] },
                { "title": "Project A", "path": "a.md", "links": ["Index"] },
                { "title": "Project B", "path": "b.md", "links": [] }
            ]
        });

        let result = inlinks_impl_json(&graph, "Index").unwrap();
        assert_eq!(result.len(), 1, "Only Project A links to Index");
        assert_eq!(result[0]["title"], "Project A");
    }

    #[test]
    fn test_inlinks_empty_when_no_backlinks() {
        let graph = json!({
            "notes": [
                { "title": "Orphan", "path": "orphan.md", "links": [] },
                { "title": "Another", "path": "another.md", "links": [] }
            ]
        });

        let result = inlinks_impl_json(&graph, "Orphan").unwrap();
        assert_eq!(result.len(), 0, "Should return empty vec");
    }

    #[test]
    fn test_inlinks_multiple_backlinks() {
        let graph = json!({
            "notes": [
                { "title": "Hub", "path": "hub.md", "links": [] },
                { "title": "A", "path": "a.md", "links": ["Hub"] },
                { "title": "B", "path": "b.md", "links": ["Hub"] },
                { "title": "C", "path": "c.md", "links": ["Hub"] }
            ]
        });

        let result = inlinks_impl_json(&graph, "Hub").unwrap();
        assert_eq!(result.len(), 3, "A, B, and C all link to Hub");
    }

    #[test]
    fn test_chained_traversal() {
        let graph = json!({
            "notes": [
                { "title": "Index", "path": "Index.md", "links": ["Project A"] },
                { "title": "Project A", "path": "a.md", "links": ["Sub Page"] },
                { "title": "Sub Page", "path": "sub.md", "links": [] }
            ]
        });

        // Get outlinks from Index
        let first_hop = outlinks_impl_json(&graph, "Index").unwrap();
        assert_eq!(first_hop.len(), 1);
        assert_eq!(first_hop[0]["title"], "Project A");

        // Get outlinks from Project A
        let second_hop = outlinks_impl_json(&graph, "Project A").unwrap();
        assert_eq!(second_hop.len(), 1);
        assert_eq!(second_hop[0]["title"], "Sub Page");
    }

    // =========================================================================
    // escape_quotes tests
    // =========================================================================

    #[test]
    fn test_escape_quotes_empty() {
        assert_eq!(escape_quotes(""), "");
    }

    #[test]
    fn test_escape_quotes_no_special_chars() {
        assert_eq!(escape_quotes("simple title"), "simple title");
    }

    #[test]
    fn test_escape_quotes_with_quotes() {
        assert_eq!(escape_quotes(r#"Note "A""#), r#"Note \"A\""#);
    }

    #[test]
    fn test_escape_quotes_with_backslash() {
        assert_eq!(escape_quotes(r#"C:\path"#), r#"C:\\path"#);
    }

    #[test]
    fn test_escape_quotes_with_both() {
        // Backslash before quote
        assert_eq!(escape_quotes(r#"say \"hello\""#), r#"say \\\"hello\\\""#);
    }

    #[test]
    fn test_escape_quotes_unicode() {
        assert_eq!(escape_quotes("日本語"), "日本語");
        assert_eq!(escape_quotes("émojis 🎉"), "émojis 🎉");
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use async_trait::async_trait;
    use crucible_core::traits::{GraphQueryError, GraphQueryExecutor, GraphQueryResult};
    use serde_json::json;

    /// Mock executor that returns predetermined results
    struct MockDbExecutor {
        results: Vec<serde_json::Value>,
    }

    #[async_trait]
    impl GraphQueryExecutor for MockDbExecutor {
        async fn execute(&self, _query: &str) -> GraphQueryResult<Vec<serde_json::Value>> {
            Ok(self.results.clone())
        }
    }

    /// Mock executor that always fails
    struct FailingExecutor {
        message: String,
    }

    #[async_trait]
    impl GraphQueryExecutor for FailingExecutor {
        async fn execute(&self, query: &str) -> GraphQueryResult<Vec<serde_json::Value>> {
            Err(GraphQueryError::with_query(&self.message, query))
        }
    }

    /// Helper to compile and run async Rune script
    async fn run_rune_async(
        module: Module,
        script: &str,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};

        let mut context = Context::with_default_modules()?;
        context.install(module)?;
        let runtime = std::sync::Arc::new(context.runtime()?);

        let mut sources = Sources::new();
        sources.insert(Source::new("test", script)?)?;

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources)?;
        }

        let unit = result?;
        let unit = std::sync::Arc::new(unit);

        // Use send_execute for async functions - this properly handles await
        let vm = Vm::new(runtime, unit);
        let execution = vm.send_execute(["main"], ())?;
        let output = execution.async_complete().await.into_result()?;

        // Convert to JSON
        let json = crate::mcp_types::rune_to_json(&output)?;
        Ok(json)
    }

    #[tokio::test]
    async fn test_db_find_returns_note() {
        let executor: Arc<dyn GraphQueryExecutor> = Arc::new(MockDbExecutor {
            results: vec![json!({"title": "Index", "path": "Index.md"})],
        });

        let module = graph_module_with_executor(executor).unwrap();

        let script = r#"
            use graph::db_find;

            pub async fn main() {
                db_find("Index").await
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        assert_eq!(result["title"], "Index");
        assert_eq!(result["path"], "Index.md");
    }

    #[tokio::test]
    async fn test_db_find_returns_empty_when_not_found() {
        let executor: Arc<dyn GraphQueryExecutor> = Arc::new(MockDbExecutor { results: vec![] });

        let module = graph_module_with_executor(executor).unwrap();

        let script = r#"
            use graph::db_find;

            pub async fn main() {
                let result = db_find("Missing").await;
                // Unit () becomes null in JSON
                result
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        assert!(result.is_null());
    }

    #[tokio::test]
    async fn test_db_outlinks_returns_array() {
        let executor: Arc<dyn GraphQueryExecutor> = Arc::new(MockDbExecutor {
            results: vec![
                json!({"title": "Project A", "path": "a.md"}),
                json!({"title": "Project B", "path": "b.md"}),
            ],
        });

        let module = graph_module_with_executor(executor).unwrap();

        let script = r#"
            use graph::db_outlinks;

            pub async fn main() {
                db_outlinks("Index").await
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        let arr = result.as_array().expect("Should be array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["title"], "Project A");
    }

    #[tokio::test]
    async fn test_db_query_with_raw_query() {
        let executor: Arc<dyn GraphQueryExecutor> = Arc::new(MockDbExecutor {
            results: vec![json!({"title": "Found"})],
        });

        let module = graph_module_with_executor(executor).unwrap();

        let script = r#"
            use graph::db_query;

            pub async fn main() {
                db_query("find(\"Index\") | ->wikilink[]").await
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        let arr = result.as_array().expect("Should be array");
        assert_eq!(arr.len(), 1);
    }

    #[tokio::test]
    async fn test_db_error_propagation() {
        let executor: Arc<dyn GraphQueryExecutor> = Arc::new(FailingExecutor {
            message: "Connection failed".to_string(),
        });

        let module = graph_module_with_executor(executor).unwrap();

        let script = r#"
            use graph::db_find;

            pub async fn main() {
                db_find("Index").await
            }
        "#;

        let result = run_rune_async(module, script).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Connection failed"),
            "Expected error message, got: {}",
            err
        );
    }
}

#[cfg(test)]
mod note_store_tests {
    use super::*;
    use async_trait::async_trait;
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::{
        NoteRecord, NoteStore, SearchResult, StorageError, StorageResult,
    };
    use serde_json::json;

    /// Mock NoteStore that returns predetermined results
    struct MockNoteStore {
        notes: Vec<NoteRecord>,
    }

    impl MockNoteStore {
        fn new(notes: Vec<NoteRecord>) -> Self {
            Self { notes }
        }

        fn with_sample_notes() -> Self {
            Self::new(vec![
                NoteRecord::new("notes/index.md", BlockHash::zero())
                    .with_title("Index".to_string())
                    .with_tags(vec!["home".to_string()]),
                NoteRecord::new("notes/project-a.md", BlockHash::zero())
                    .with_title("Project A".to_string())
                    .with_links(vec!["notes/index.md".to_string()]),
                NoteRecord::new("notes/project-b.md", BlockHash::zero())
                    .with_title("Project B".to_string()),
            ])
        }
    }

    #[async_trait]
    impl NoteStore for MockNoteStore {
        async fn upsert(&self, _note: NoteRecord) -> StorageResult<()> {
            Ok(())
        }

        async fn get(&self, path: &str) -> StorageResult<Option<NoteRecord>> {
            Ok(self.notes.iter().find(|n| n.path == path).cloned())
        }

        async fn delete(&self, _path: &str) -> StorageResult<()> {
            Ok(())
        }

        async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
            Ok(self.notes.clone())
        }

        async fn get_by_hash(&self, _hash: &BlockHash) -> StorageResult<Option<NoteRecord>> {
            Ok(None)
        }

        async fn search(
            &self,
            _embedding: &[f32],
            _limit: usize,
            _filter: Option<crucible_core::storage::Filter>,
        ) -> StorageResult<Vec<SearchResult>> {
            Ok(vec![])
        }
    }

    /// Mock NoteStore that always fails
    struct FailingNoteStore {
        message: String,
    }

    #[async_trait]
    impl NoteStore for FailingNoteStore {
        async fn upsert(&self, _note: NoteRecord) -> StorageResult<()> {
            Err(StorageError::backend(&self.message))
        }

        async fn get(&self, _path: &str) -> StorageResult<Option<NoteRecord>> {
            Err(StorageError::backend(&self.message))
        }

        async fn delete(&self, _path: &str) -> StorageResult<()> {
            Err(StorageError::backend(&self.message))
        }

        async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
            Err(StorageError::backend(&self.message))
        }

        async fn get_by_hash(&self, _hash: &BlockHash) -> StorageResult<Option<NoteRecord>> {
            Err(StorageError::backend(&self.message))
        }

        async fn search(
            &self,
            _embedding: &[f32],
            _limit: usize,
            _filter: Option<crucible_core::storage::Filter>,
        ) -> StorageResult<Vec<SearchResult>> {
            Err(StorageError::backend(&self.message))
        }
    }

    /// Helper to create a module with just NoteStore functions (no executor)
    fn note_store_module(store: Arc<dyn NoteStore>) -> Result<Module, ContextError> {
        let mut module = Module::with_crate("graph")?;
        register_note_functions(&mut module, store)?;
        Ok(module)
    }

    /// Helper to compile and run async Rune script
    async fn run_rune_async(
        module: Module,
        script: &str,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};

        let mut context = Context::with_default_modules()?;
        context.install(module)?;
        let runtime = std::sync::Arc::new(context.runtime()?);

        let mut sources = Sources::new();
        sources.insert(Source::new("test", script)?)?;

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources)?;
        }

        let unit = result?;
        let unit = std::sync::Arc::new(unit);

        let vm = Vm::new(runtime, unit);
        let execution = vm.send_execute(["main"], ())?;
        let output = execution.async_complete().await.into_result()?;

        let json = crate::mcp_types::rune_to_json(&output)?;
        Ok(json)
    }

    // =========================================================================
    // note_get tests
    // =========================================================================

    #[tokio::test]
    async fn test_note_get_returns_record() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_sample_notes());
        let module = note_store_module(store).unwrap();

        let script = r#"
            use graph::note_get;

            pub async fn main() {
                note_get("notes/index.md").await
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        assert_eq!(result["path"], "notes/index.md");
        assert_eq!(result["title"], "Index");
        assert!(result["tags"].as_array().unwrap().contains(&json!("home")));
    }

    #[tokio::test]
    async fn test_note_get_returns_nil_when_not_found() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_sample_notes());
        let module = note_store_module(store).unwrap();

        let script = r#"
            use graph::note_get;

            pub async fn main() {
                note_get("nonexistent/path.md").await
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        assert!(
            result.is_null(),
            "Expected null for missing note, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_note_get_propagates_error() {
        let store: Arc<dyn NoteStore> = Arc::new(FailingNoteStore {
            message: "Database connection lost".to_string(),
        });
        let module = note_store_module(store).unwrap();

        let script = r#"
            use graph::note_get;

            pub async fn main() {
                note_get("any/path.md").await
            }
        "#;

        let result = run_rune_async(module, script).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Database connection lost"),
            "Expected error message, got: {}",
            err
        );
    }

    // =========================================================================
    // note_list tests
    // =========================================================================

    #[tokio::test]
    async fn test_note_list_returns_all_notes() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_sample_notes());
        let module = note_store_module(store).unwrap();

        let script = r#"
            use graph::note_list;

            pub async fn main() {
                note_list(0).await  // 0 means no limit
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        let arr = result.as_array().expect("Should be array");
        assert_eq!(arr.len(), 3, "Should return all 3 notes");
    }

    #[tokio::test]
    async fn test_note_list_respects_limit() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_sample_notes());
        let module = note_store_module(store).unwrap();

        let script = r#"
            use graph::note_list;

            pub async fn main() {
                note_list(2).await
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        let arr = result.as_array().expect("Should be array");
        assert_eq!(arr.len(), 2, "Should return only 2 notes");
    }

    #[tokio::test]
    async fn test_note_list_with_limit_greater_than_count() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_sample_notes());
        let module = note_store_module(store).unwrap();

        let script = r#"
            use graph::note_list;

            pub async fn main() {
                note_list(100).await
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        let arr = result.as_array().expect("Should be array");
        assert_eq!(arr.len(), 3, "Should return all notes when limit > count");
    }

    #[tokio::test]
    async fn test_note_list_empty_store() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::new(vec![]));
        let module = note_store_module(store).unwrap();

        let script = r#"
            use graph::note_list;

            pub async fn main() {
                note_list(10).await
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        let arr = result.as_array().expect("Should be array");
        assert_eq!(arr.len(), 0, "Should return empty array");
    }

    #[tokio::test]
    async fn test_note_list_propagates_error() {
        let store: Arc<dyn NoteStore> = Arc::new(FailingNoteStore {
            message: "Storage unavailable".to_string(),
        });
        let module = note_store_module(store).unwrap();

        let script = r#"
            use graph::note_list;

            pub async fn main() {
                note_list(10).await
            }
        "#;

        let result = run_rune_async(module, script).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Storage unavailable"),
            "Expected error message, got: {}",
            err
        );
    }

    // =========================================================================
    // Combined module tests
    // =========================================================================

    #[tokio::test]
    async fn test_graph_module_with_stores_has_both_functions() {
        use crucible_core::traits::{GraphQueryExecutor, GraphQueryResult};

        struct MockExecutor;

        #[async_trait]
        impl GraphQueryExecutor for MockExecutor {
            async fn execute(&self, _query: &str) -> GraphQueryResult<Vec<serde_json::Value>> {
                Ok(vec![json!({"title": "From Executor"})])
            }
        }

        let executor: Arc<dyn GraphQueryExecutor> = Arc::new(MockExecutor);
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_sample_notes());

        let module = graph_module_with_stores(executor, store).unwrap();

        // Test that note_get works
        let script = r#"
            use graph::{note_get, db_find};

            pub async fn main() {
                let from_store = note_get("notes/index.md").await;
                let from_executor = db_find("Any").await;
                #{
                    store_path: from_store.path,
                    executor_title: from_executor.title,
                }
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        assert_eq!(result["store_path"], "notes/index.md");
        assert_eq!(result["executor_title"], "From Executor");
    }
}

#[cfg(test)]
mod graph_view_tests {
    use super::*;
    use crucible_core::storage::{GraphView, NoteRecord};

    /// Mock GraphView that returns predetermined results
    struct MockGraphView {
        outlinks_result: Vec<String>,
        backlinks_result: Vec<String>,
        neighbors_result: Vec<String>,
    }

    impl MockGraphView {
        fn new() -> Self {
            Self {
                outlinks_result: vec![
                    "linked/note-a.md".to_string(),
                    "linked/note-b.md".to_string(),
                ],
                backlinks_result: vec!["backlink/from-a.md".to_string()],
                neighbors_result: vec![
                    "linked/note-a.md".to_string(),
                    "linked/note-b.md".to_string(),
                    "backlink/from-a.md".to_string(),
                ],
            }
        }

        fn with_outlinks(mut self, links: Vec<String>) -> Self {
            self.outlinks_result = links;
            self
        }

        fn with_backlinks(mut self, links: Vec<String>) -> Self {
            self.backlinks_result = links;
            self
        }

        fn with_neighbors(mut self, links: Vec<String>) -> Self {
            self.neighbors_result = links;
            self
        }
    }

    impl GraphView for MockGraphView {
        fn outlinks(&self, _path: &str) -> Vec<String> {
            self.outlinks_result.clone()
        }

        fn backlinks(&self, _path: &str) -> Vec<String> {
            self.backlinks_result.clone()
        }

        fn neighbors(&self, _path: &str, _depth: usize) -> Vec<String> {
            self.neighbors_result.clone()
        }

        fn rebuild(&mut self, _notes: &[NoteRecord]) {
            // No-op for mock
        }
    }

    /// Helper to create module with GraphView functions
    fn graph_view_module(view: Arc<dyn GraphView>) -> Result<Module, ContextError> {
        let mut module = Module::with_crate("graph")?;
        register_graph_view_functions(&mut module, view)?;
        Ok(module)
    }

    /// Helper to compile and run async Rune script
    async fn run_rune_async(
        module: Module,
        script: &str,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
        use rune::termcolor::{ColorChoice, StandardStream};
        use rune::{Context, Diagnostics, Source, Sources, Vm};

        let mut context = Context::with_default_modules()?;
        context.install(module)?;
        let runtime = std::sync::Arc::new(context.runtime()?);

        let mut sources = Sources::new();
        sources.insert(Source::new("test", script)?)?;

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources)?;
        }

        let unit = result?;
        let unit = std::sync::Arc::new(unit);

        let vm = Vm::new(runtime, unit);
        let execution = vm.send_execute(["main"], ())?;
        let output = execution.async_complete().await.into_result()?;

        let json = crate::mcp_types::rune_to_json(&output)?;
        Ok(json)
    }

    // =========================================================================
    // outlinks tests
    // =========================================================================

    #[tokio::test]
    async fn test_fast_outlinks_returns_paths() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());
        let module = graph_view_module(view).unwrap();

        let script = r#"
            use graph::fast_outlinks;

            pub async fn main() {
                fast_outlinks("notes/index.md")
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        let arr = result.as_array().expect("Should be array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], "linked/note-a.md");
        assert_eq!(arr[1], "linked/note-b.md");
    }

    #[tokio::test]
    async fn test_fast_outlinks_empty_result() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new().with_outlinks(vec![]));
        let module = graph_view_module(view).unwrap();

        let script = r#"
            use graph::fast_outlinks;

            pub async fn main() {
                fast_outlinks("orphan.md")
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        let arr = result.as_array().expect("Should be array");
        assert!(arr.is_empty());
    }

    // =========================================================================
    // backlinks tests
    // =========================================================================

    #[tokio::test]
    async fn test_fast_backlinks_returns_paths() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());
        let module = graph_view_module(view).unwrap();

        let script = r#"
            use graph::fast_backlinks;

            pub async fn main() {
                fast_backlinks("notes/target.md")
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        let arr = result.as_array().expect("Should be array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0], "backlink/from-a.md");
    }

    #[tokio::test]
    async fn test_fast_backlinks_empty_result() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new().with_backlinks(vec![]));
        let module = graph_view_module(view).unwrap();

        let script = r#"
            use graph::fast_backlinks;

            pub async fn main() {
                fast_backlinks("orphan.md")
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        let arr = result.as_array().expect("Should be array");
        assert!(arr.is_empty());
    }

    // =========================================================================
    // neighbors tests
    // =========================================================================

    #[tokio::test]
    async fn test_fast_neighbors_returns_paths() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());
        let module = graph_view_module(view).unwrap();

        let script = r#"
            use graph::fast_neighbors;

            pub async fn main() {
                fast_neighbors("notes/hub.md", 1)
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        let arr = result.as_array().expect("Should be array");
        assert_eq!(arr.len(), 3);
    }

    #[tokio::test]
    async fn test_fast_neighbors_empty_result() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new().with_neighbors(vec![]));
        let module = graph_view_module(view).unwrap();

        let script = r#"
            use graph::fast_neighbors;

            pub async fn main() {
                fast_neighbors("isolated.md", 2)
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        let arr = result.as_array().expect("Should be array");
        assert!(arr.is_empty());
    }

    #[tokio::test]
    async fn test_fast_neighbors_depth_parameter() {
        // Verify that depth is passed correctly (mock doesn't use it, but signature works)
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());
        let module = graph_view_module(view).unwrap();

        let script = r#"
            use graph::fast_neighbors;

            pub async fn main() {
                // Test with different depths
                let depth1 = fast_neighbors("notes/hub.md", 1);
                let depth3 = fast_neighbors("notes/hub.md", 3);
                // Cast to i64 since u64 (from .len()) cannot be serialized to JSON
                #{
                    depth1_len: depth1.len() as i64,
                    depth3_len: depth3.len() as i64,
                }
            }
        "#;

        let result = run_rune_async(module, script).await.unwrap();
        // Both return same length since mock doesn't vary by depth
        assert_eq!(result["depth1_len"], 3);
        assert_eq!(result["depth3_len"], 3);
    }
}
