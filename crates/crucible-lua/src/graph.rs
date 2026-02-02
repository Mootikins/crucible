//! Graph traversal module for Lua scripts
//!
//! Provides functions for traversing note graphs (outlinks, inlinks).
//!
//! ## Usage in Lua
//!
//! ```lua
//! local graph = require("graph")
//!
//! -- Build a graph from notes
//! local g = {
//!     notes = {
//!         { title = "Index", path = "Index.md", links = {"Project A", "Project B"} },
//!         { title = "Project A", path = "projects/a.md", links = {"Index"} },
//!         { title = "Project B", path = "projects/b.md", links = {} },
//!     }
//! }
//!
//! -- Get notes linked FROM a note (outlinks)
//! local outlinks = graph.outlinks(g, "Index")  -- returns {Project A, Project B}
//!
//! -- Get notes linking TO a note (inlinks/backlinks)
//! local inlinks = graph.inlinks(g, "Index")    -- returns {Project A}
//!
//! -- Find a note by title
//! local note = graph.find(g, "Index")
//!
//! -- Database-backed queries (async)
//! local note = graph.db_find("Index")
//! local links = graph.db_outlinks("Index")
//! ```

use crate::error::LuaError;
use crucible_core::storage::{GraphView, NoteStore};
use crucible_core::traits::GraphQueryExecutor;
use mlua::{Lua, LuaSerdeExt, Table, Value};
use std::sync::Arc;

/// Register the graph module with a Lua state
pub fn register_graph_module(lua: &Lua) -> Result<(), LuaError> {
    let graph = lua.create_table()?;

    // graph.find(g, title) -> note or nil
    let find_fn = lua.create_function(|_lua, (g, title): (Table, String)| {
        let notes: Table = g.get("notes")?;

        for pair in notes.pairs::<i64, Table>() {
            let (_, note) = pair?;
            let note_title: String = note.get("title")?;
            if note_title == title {
                return Ok(Value::Table(note));
            }
        }

        Ok(Value::Nil)
    })?;
    graph.set("find", find_fn)?;

    // graph.outlinks(g, title) -> array of notes
    let outlinks_fn = lua.create_function(|lua, (g, title): (Table, String)| {
        let notes: Table = g.get("notes")?;
        let result = lua.create_table()?;

        // Find the source note and get its links
        let mut source_links: Vec<String> = Vec::new();
        for pair in notes.pairs::<i64, Table>() {
            let (_, note) = pair?;
            let note_title: String = note.get("title")?;
            if note_title == title {
                // Get the links array
                if let Ok(links) = note.get::<Table>("links") {
                    for link_pair in links.pairs::<i64, String>() {
                        let (_, link) = link_pair?;
                        source_links.push(link);
                    }
                }
                break;
            }
        }

        // Find notes that match the links
        let mut result_idx = 1;
        for pair in notes.pairs::<i64, Table>() {
            let (_, note) = pair?;
            let note_title: String = note.get("title")?;
            if source_links.contains(&note_title) {
                result.set(result_idx, note)?;
                result_idx += 1;
            }
        }

        Ok(result)
    })?;
    graph.set("outlinks", outlinks_fn)?;

    // graph.inlinks(g, title) -> array of notes (backlinks)
    let inlinks_fn = lua.create_function(|lua, (g, title): (Table, String)| {
        let notes: Table = g.get("notes")?;
        let result = lua.create_table()?;

        // Find all notes that link TO the target
        let mut result_idx = 1;
        for pair in notes.pairs::<i64, Table>() {
            let (_, note) = pair?;

            // Check if this note's links contain the target title
            if let Ok(links) = note.get::<Table>("links") {
                for link_pair in links.pairs::<i64, String>() {
                    let (_, link) = link_pair?;
                    if link == title {
                        result.set(result_idx, note.clone())?;
                        result_idx += 1;
                        break;
                    }
                }
            }
        }

        Ok(result)
    })?;
    graph.set("inlinks", inlinks_fn)?;

    // Register graph module globally
    lua.globals().set("graph", graph)?;

    Ok(())
}

/// Register the graph module with database-backed async queries
///
/// This version adds async functions that query the actual database
/// via the `GraphQueryExecutor` trait.
///
/// # Example
///
/// ```lua
/// -- In Lua scripts:
/// local note = graph.db_find("Index")
/// local links = graph.db_outlinks("Index")
/// local backlinks = graph.db_inlinks("Index")
/// local all = graph.db_neighbors("Index")
/// local custom = graph.db_query('find("Index") | ->wikilink[]')
/// ```
pub fn register_graph_module_with_executor(
    lua: &Lua,
    executor: Arc<dyn GraphQueryExecutor>,
) -> Result<(), LuaError> {
    // First register the in-memory functions
    register_graph_module(lua)?;

    // Get the graph table we just created
    let graph: Table = lua.globals().get("graph")?;

    // Add database-backed async functions with db_ prefix

    // db_find - Find a note by title in the database
    let exec = executor.clone();
    let db_find = lua.create_async_function(move |lua, title: String| {
        let exec = exec.clone();
        async move {
            let query = format!(r#"find("{}")"#, escape_quotes(&title));
            match exec.execute(&query).await {
                Ok(results) => {
                    if let Some(first) = results.into_iter().next() {
                        lua.to_value(&first)
                    } else {
                        Ok(Value::Nil)
                    }
                }
                Err(e) => Err(mlua::Error::runtime(format!("Graph query error: {}", e))),
            }
        }
    })?;
    graph.set("db_find", db_find)?;

    // db_outlinks - Get outlinks from database
    let exec = executor.clone();
    let db_outlinks = lua.create_async_function(move |lua, title: String| {
        let exec = exec.clone();
        async move {
            let query = format!(r#"outlinks("{}")"#, escape_quotes(&title));
            match exec.execute(&query).await {
                Ok(results) => json_array_to_lua_table(&lua, &results),
                Err(e) => Err(mlua::Error::runtime(format!("Graph query error: {}", e))),
            }
        }
    })?;
    graph.set("db_outlinks", db_outlinks)?;

    // db_inlinks - Get inlinks from database
    let exec = executor.clone();
    let db_inlinks = lua.create_async_function(move |lua, title: String| {
        let exec = exec.clone();
        async move {
            let query = format!(r#"inlinks("{}")"#, escape_quotes(&title));
            match exec.execute(&query).await {
                Ok(results) => json_array_to_lua_table(&lua, &results),
                Err(e) => Err(mlua::Error::runtime(format!("Graph query error: {}", e))),
            }
        }
    })?;
    graph.set("db_inlinks", db_inlinks)?;

    // db_neighbors - Get all connected notes from database
    let exec = executor.clone();
    let db_neighbors = lua.create_async_function(move |lua, title: String| {
        let exec = exec.clone();
        async move {
            let query = format!(r#"neighbors("{}")"#, escape_quotes(&title));
            match exec.execute(&query).await {
                Ok(results) => json_array_to_lua_table(&lua, &results),
                Err(e) => Err(mlua::Error::runtime(format!("Graph query error: {}", e))),
            }
        }
    })?;
    graph.set("db_neighbors", db_neighbors)?;

    // db_query - Execute arbitrary graph query
    let exec = executor.clone();
    let db_query = lua.create_async_function(move |lua, query: String| {
        let exec = exec.clone();
        async move {
            match exec.execute(&query).await {
                Ok(results) => json_array_to_lua_table(&lua, &results),
                Err(e) => Err(mlua::Error::runtime(format!("Graph query error: {}", e))),
            }
        }
    })?;
    graph.set("db_query", db_query)?;

    Ok(())
}

/// Register NoteStore functions on the graph module
///
/// This adds async functions that query the NoteStore directly:
///
/// # Example
///
/// ```lua
/// -- Get a note by path
/// local note = graph.note_get("path/to/note.md")
/// if note then
///     print(note.title)
///     print(note.path)
///     print(#note.tags)
/// end
///
/// -- List all notes (with optional limit)
/// local notes = graph.note_list(10)
/// for i, note in ipairs(notes) do
///     print(note.title)
/// end
/// ```
pub fn register_note_store_functions(lua: &Lua, store: Arc<dyn NoteStore>) -> Result<(), LuaError> {
    // Get the graph table (must exist from prior registration)
    let graph: Table = lua.globals().get("graph")?;

    // note_get - Get a note by path
    let s = Arc::clone(&store);
    let note_get = lua.create_async_function(move |lua, path: String| {
        let s = Arc::clone(&s);
        async move {
            match s.get(&path).await {
                Ok(Some(record)) => {
                    // Convert NoteRecord to Lua table
                    note_record_to_lua(&lua, &record)
                }
                Ok(None) => Ok(Value::Nil),
                Err(e) => Err(mlua::Error::runtime(format!("NoteStore error: {}", e))),
            }
        }
    })?;
    graph.set("note_get", note_get)?;

    // note_list - List notes with optional limit
    let s = Arc::clone(&store);
    let note_list = lua.create_async_function(move |lua, limit: Option<usize>| {
        let s = Arc::clone(&s);
        async move {
            match s.list().await {
                Ok(records) => {
                    let table = lua.create_table()?;
                    let iter = records.iter();
                    let iter: Box<dyn Iterator<Item = _>> = if let Some(lim) = limit {
                        Box::new(iter.take(lim))
                    } else {
                        Box::new(iter)
                    };

                    for (i, record) in iter.enumerate() {
                        let lua_record = note_record_to_lua(&lua, record)?;
                        table.set(i + 1, lua_record)?;
                    }
                    Ok(Value::Table(table))
                }
                Err(e) => Err(mlua::Error::runtime(format!("NoteStore error: {}", e))),
            }
        }
    })?;
    graph.set("note_list", note_list)?;

    Ok(())
}

/// Register the graph module with NoteStore-backed async queries (`note_get`, `note_list`).
///
/// Ensures the base graph module exists first (idempotent).
/// Call when storage becomes available (e.g., daemon kiln.open).
pub fn register_graph_module_with_store(
    lua: &Lua,
    store: Arc<dyn NoteStore>,
) -> Result<(), LuaError> {
    if lua.globals().get::<Option<Table>>("graph")?.is_none() {
        register_graph_module(lua)?;
    }

    register_note_store_functions(lua, store)?;

    Ok(())
}

/// Register the graph module with both executor and note store
///
/// This is a convenience function that combines `register_graph_module_with_executor`
/// and `register_note_store_functions`.
pub fn register_graph_module_full(
    lua: &Lua,
    executor: Arc<dyn GraphQueryExecutor>,
    store: Arc<dyn NoteStore>,
) -> Result<(), LuaError> {
    register_graph_module_with_executor(lua, executor)?;
    register_note_store_functions(lua, store)?;
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
/// ```lua
/// -- Get outlinks (synchronous, fast)
/// local links = graph.fast_outlinks("notes/index.md")
/// for _, link in ipairs(links) do
///     print(link)
/// end
///
/// -- Get backlinks
/// local backlinks = graph.fast_backlinks("notes/target.md")
///
/// -- Get neighbors within depth
/// local nearby = graph.fast_neighbors("notes/hub.md", 2)
/// ```
pub fn register_graph_view_functions(lua: &Lua, view: Arc<dyn GraphView>) -> Result<(), LuaError> {
    // Get the graph table (must exist from prior registration)
    let graph: Table = lua.globals().get("graph")?;

    // fast_outlinks - Get paths of notes this note links to
    let v = Arc::clone(&view);
    let fast_outlinks = lua.create_function(move |lua, path: String| {
        let paths = v.outlinks(&path);
        string_vec_to_lua_table(lua, &paths)
    })?;
    graph.set("fast_outlinks", fast_outlinks)?;

    // fast_backlinks - Get paths of notes linking to this note
    let v = Arc::clone(&view);
    let fast_backlinks = lua.create_function(move |lua, path: String| {
        let paths = v.backlinks(&path);
        string_vec_to_lua_table(lua, &paths)
    })?;
    graph.set("fast_backlinks", fast_backlinks)?;

    // fast_neighbors - Get all connected notes within depth
    let v = Arc::clone(&view);
    let fast_neighbors = lua.create_function(move |lua, (path, depth): (String, usize)| {
        let paths = v.neighbors(&path, depth);
        string_vec_to_lua_table(lua, &paths)
    })?;
    graph.set("fast_neighbors", fast_neighbors)?;

    Ok(())
}

/// Register the graph module with executor, note store, and graph view
///
/// This is the most complete module, providing:
/// - Database-backed query functions (db_*)
/// - NoteStore access (note_*)
/// - Fast GraphView traversal (fast_*)
///
/// # Example
///
/// ```lua
/// -- Fast path functions
/// local links = graph.fast_outlinks("notes/index.md")
/// local backlinks = graph.fast_backlinks("notes/target.md")
/// local nearby = graph.fast_neighbors("notes/hub.md", 2)
///
/// -- Database queries
/// local note = graph.db_find("Index")
///
/// -- NoteStore access
/// local record = graph.note_get("notes/index.md")
/// ```
pub fn register_graph_module_with_all(
    lua: &Lua,
    executor: Arc<dyn GraphQueryExecutor>,
    store: Arc<dyn NoteStore>,
    view: Arc<dyn GraphView>,
) -> Result<(), LuaError> {
    register_graph_module_full(lua, executor, store)?;
    register_graph_view_functions(lua, view)?;
    Ok(())
}

/// Convert a Vec<String> to a Lua table
fn string_vec_to_lua_table(lua: &Lua, values: &[String]) -> Result<Value, mlua::Error> {
    let table = lua.create_table()?;
    for (i, v) in values.iter().enumerate() {
        table.set(i + 1, v.as_str())?;
    }
    Ok(Value::Table(table))
}

/// Convert a NoteRecord to a Lua table
fn note_record_to_lua(
    lua: &Lua,
    record: &crucible_core::storage::NoteRecord,
) -> Result<Value, mlua::Error> {
    let table = lua.create_table()?;

    table.set("path", record.path.as_str())?;
    table.set("title", record.title.as_str())?;
    table.set("content_hash", record.content_hash.to_string())?;

    // Tags as array
    let tags = lua.create_table()?;
    for (i, tag) in record.tags.iter().enumerate() {
        tags.set(i + 1, tag.as_str())?;
    }
    table.set("tags", tags)?;

    // Links as array
    let links = lua.create_table()?;
    for (i, link) in record.links_to.iter().enumerate() {
        links.set(i + 1, link.as_str())?;
    }
    table.set("links_to", links)?;

    // Properties as table (convert serde_json::Value to Lua)
    let props = lua.create_table()?;
    for (k, v) in &record.properties {
        props.set(k.as_str(), lua.to_value(v)?)?;
    }
    table.set("properties", props)?;

    // Updated timestamp as ISO string
    table.set("updated_at", record.updated_at.to_rfc3339())?;

    // Embedding: skip for now (large vectors not useful in Lua)
    // We set has_embedding boolean instead
    table.set("has_embedding", record.has_embedding())?;

    Ok(Value::Table(table))
}

/// Escape double quotes in a string for safe embedding in queries
fn escape_quotes(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Convert a JSON array to a Lua table
fn json_array_to_lua_table(lua: &Lua, values: &[serde_json::Value]) -> Result<Value, mlua::Error> {
    lua.to_value(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        register_graph_module(&lua).unwrap();
        lua
    }

    // =========================================================================
    // RED: Write failing tests first
    // =========================================================================

    #[test]
    fn test_graph_find_existing_note() {
        let lua = setup_lua();
        let result: Table = lua
            .load(
                r#"
                local g = {
                    notes = {
                        { title = "Index", path = "Index.md", links = {} },
                        { title = "Project A", path = "a.md", links = {} },
                    }
                }
                return graph.find(g, "Index")
                "#,
            )
            .eval()
            .unwrap();

        assert_eq!(result.get::<String>("title").unwrap(), "Index");
        assert_eq!(result.get::<String>("path").unwrap(), "Index.md");
    }

    #[test]
    fn test_graph_find_missing_note_returns_nil() {
        let lua = setup_lua();
        let result: Value = lua
            .load(
                r#"
                local g = { notes = { { title = "Index", path = "Index.md", links = {} } } }
                return graph.find(g, "NonExistent")
                "#,
            )
            .eval()
            .unwrap();

        assert!(matches!(result, Value::Nil));
    }

    #[test]
    fn test_graph_outlinks_returns_linked_notes() {
        let lua = setup_lua();
        let result: Table = lua
            .load(
                r#"
                local g = {
                    notes = {
                        { title = "Index", path = "Index.md", links = {"Project A", "Project B"} },
                        { title = "Project A", path = "a.md", links = {"Index"} },
                        { title = "Project B", path = "b.md", links = {} },
                        { title = "Orphan", path = "orphan.md", links = {} },
                    }
                }
                return graph.outlinks(g, "Index")
                "#,
            )
            .eval()
            .unwrap();

        // Should return 2 notes (Project A and Project B)
        assert_eq!(result.len().unwrap(), 2);

        // Collect titles
        let mut titles: Vec<String> = Vec::new();
        for pair in result.pairs::<i64, Table>() {
            let (_, note) = pair.unwrap();
            titles.push(note.get::<String>("title").unwrap());
        }
        titles.sort();

        assert_eq!(titles, vec!["Project A", "Project B"]);
    }

    #[test]
    fn test_graph_outlinks_empty_when_no_links() {
        let lua = setup_lua();
        let result: Table = lua
            .load(
                r#"
                local g = {
                    notes = {
                        { title = "Orphan", path = "orphan.md", links = {} },
                    }
                }
                return graph.outlinks(g, "Orphan")
                "#,
            )
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[test]
    fn test_graph_inlinks_returns_notes_linking_to_target() {
        let lua = setup_lua();
        let result: Table = lua
            .load(
                r#"
                local g = {
                    notes = {
                        { title = "Index", path = "Index.md", links = {"Project A", "Project B"} },
                        { title = "Project A", path = "a.md", links = {"Index"} },
                        { title = "Project B", path = "b.md", links = {} },
                    }
                }
                return graph.inlinks(g, "Index")
                "#,
            )
            .eval()
            .unwrap();

        // Only Project A links to Index
        assert_eq!(result.len().unwrap(), 1);

        let note: Table = result.get(1).unwrap();
        assert_eq!(note.get::<String>("title").unwrap(), "Project A");
    }

    #[test]
    fn test_graph_inlinks_empty_when_no_backlinks() {
        let lua = setup_lua();
        let result: Table = lua
            .load(
                r#"
                local g = {
                    notes = {
                        { title = "Orphan", path = "orphan.md", links = {} },
                        { title = "Another", path = "another.md", links = {} },
                    }
                }
                return graph.inlinks(g, "Orphan")
                "#,
            )
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[test]
    fn test_graph_inlinks_multiple_backlinks() {
        let lua = setup_lua();
        let result: Table = lua
            .load(
                r#"
                local g = {
                    notes = {
                        { title = "Hub", path = "hub.md", links = {} },
                        { title = "A", path = "a.md", links = {"Hub"} },
                        { title = "B", path = "b.md", links = {"Hub"} },
                        { title = "C", path = "c.md", links = {"Hub"} },
                    }
                }
                return graph.inlinks(g, "Hub")
                "#,
            )
            .eval()
            .unwrap();

        // A, B, and C all link to Hub
        assert_eq!(result.len().unwrap(), 3);
    }

    #[test]
    fn test_graph_chained_traversal() {
        let lua = setup_lua();
        // Two-hop: Index -> Project A -> Index (back)
        let result: bool = lua
            .load(
                r#"
                local g = {
                    notes = {
                        { title = "Index", path = "Index.md", links = {"Project A"} },
                        { title = "Project A", path = "a.md", links = {"Sub Page"} },
                        { title = "Sub Page", path = "sub.md", links = {} },
                    }
                }

                -- Get outlinks from Index
                local first_hop = graph.outlinks(g, "Index")

                -- Get outlinks from Project A (first result)
                local project_a = first_hop[1]
                local second_hop = graph.outlinks(g, project_a.title)

                -- Should find Sub Page
                return second_hop[1].title == "Sub Page"
                "#,
            )
            .eval()
            .unwrap();

        assert!(result);
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

    #[tokio::test]
    async fn test_db_find_returns_note() {
        let lua = Lua::new();
        let executor: Arc<dyn GraphQueryExecutor> = Arc::new(MockDbExecutor {
            results: vec![json!({"title": "Index", "path": "Index.md"})],
        });

        register_graph_module_with_executor(&lua, executor).unwrap();

        let result: Table = lua
            .load(r#"return graph.db_find("Index")"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.get::<String>("title").unwrap(), "Index");
        assert_eq!(result.get::<String>("path").unwrap(), "Index.md");
    }

    #[tokio::test]
    async fn test_db_find_returns_nil_when_not_found() {
        let lua = Lua::new();
        let executor: Arc<dyn GraphQueryExecutor> = Arc::new(MockDbExecutor { results: vec![] });

        register_graph_module_with_executor(&lua, executor).unwrap();

        let result: Value = lua
            .load(r#"return graph.db_find("Missing")"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result, Value::Nil));
    }

    #[tokio::test]
    async fn test_db_outlinks_returns_array() {
        let lua = Lua::new();
        let executor: Arc<dyn GraphQueryExecutor> = Arc::new(MockDbExecutor {
            results: vec![
                json!({"title": "Project A", "path": "a.md"}),
                json!({"title": "Project B", "path": "b.md"}),
            ],
        });

        register_graph_module_with_executor(&lua, executor).unwrap();

        let result: Table = lua
            .load(r#"return graph.db_outlinks("Index")"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 2);

        let first: Table = result.get(1).unwrap();
        assert_eq!(first.get::<String>("title").unwrap(), "Project A");
    }

    #[tokio::test]
    async fn test_db_inlinks_returns_backlinks() {
        let lua = Lua::new();
        let executor: Arc<dyn GraphQueryExecutor> = Arc::new(MockDbExecutor {
            results: vec![json!({"title": "Project A", "path": "a.md"})],
        });

        register_graph_module_with_executor(&lua, executor).unwrap();

        let result: Table = lua
            .load(r#"return graph.db_inlinks("Index")"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 1);
        let first: Table = result.get(1).unwrap();
        assert_eq!(first.get::<String>("title").unwrap(), "Project A");
    }

    #[tokio::test]
    async fn test_db_query_raw() {
        let lua = Lua::new();
        let executor: Arc<dyn GraphQueryExecutor> = Arc::new(MockDbExecutor {
            results: vec![json!({"title": "Found"})],
        });

        register_graph_module_with_executor(&lua, executor).unwrap();

        let result: Table = lua
            .load(r#"return graph.db_query('find("Index") | ->wikilink[]')"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_db_error_propagation() {
        let lua = Lua::new();
        let executor: Arc<dyn GraphQueryExecutor> = Arc::new(FailingExecutor {
            message: "Connection failed".to_string(),
        });

        register_graph_module_with_executor(&lua, executor).unwrap();

        let result = lua
            .load(r#"return graph.db_find("Index")"#)
            .eval_async::<Value>()
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Connection failed"));
    }
}

#[cfg(test)]
mod note_store_tests {
    use super::*;
    use async_trait::async_trait;
    use crucible_core::events::SessionEvent;
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::{Filter, NoteRecord, NoteStore, SearchResult, StorageResult};
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Mock NoteStore for testing
    struct MockNoteStore {
        notes: Mutex<HashMap<String, NoteRecord>>,
    }

    impl MockNoteStore {
        fn new() -> Self {
            Self {
                notes: Mutex::new(HashMap::new()),
            }
        }

        fn with_notes(notes: Vec<NoteRecord>) -> Self {
            let store = Self::new();
            {
                let mut map = store.notes.lock().unwrap();
                for note in notes {
                    map.insert(note.path.clone(), note);
                }
            }
            store
        }
    }

    #[async_trait]
    impl NoteStore for MockNoteStore {
        async fn upsert(&self, note: NoteRecord) -> StorageResult<Vec<SessionEvent>> {
            let title = note.title.clone();
            let path = note.path.clone();
            let mut map = self.notes.lock().unwrap();
            map.insert(note.path.clone(), note);
            let event = SessionEvent::NoteCreated {
                path: path.into(),
                title: Some(title),
            };
            Ok(vec![event])
        }

        async fn get(&self, path: &str) -> StorageResult<Option<NoteRecord>> {
            let map = self.notes.lock().unwrap();
            Ok(map.get(path).cloned())
        }

        async fn delete(&self, path: &str) -> StorageResult<SessionEvent> {
            let mut map = self.notes.lock().unwrap();
            map.remove(path);
            Ok(SessionEvent::NoteDeleted {
                path: path.into(),
                existed: false,
            })
        }

        async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
            let map = self.notes.lock().unwrap();
            Ok(map.values().cloned().collect())
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

    fn sample_note(path: &str, title: &str) -> NoteRecord {
        NoteRecord::new(path, BlockHash::zero())
            .with_title(title)
            .with_tags(vec!["rust".to_string(), "test".to_string()])
            .with_links(vec!["other/note.md".to_string()])
    }

    #[tokio::test]
    async fn test_note_get_returns_record() {
        let lua = Lua::new();
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![
            sample_note("Index.md", "Index"),
            sample_note("other.md", "Other"),
        ]));

        // First register the basic graph module, then add note store functions
        register_graph_module(&lua).unwrap();
        register_note_store_functions(&lua, store).unwrap();

        let result: Table = lua
            .load(r#"return graph.note_get("Index.md")"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.get::<String>("path").unwrap(), "Index.md");
        assert_eq!(result.get::<String>("title").unwrap(), "Index");
        assert!(result.get::<bool>("has_embedding").is_ok());
    }

    #[tokio::test]
    async fn test_note_get_returns_nil_when_not_found() {
        let lua = Lua::new();
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::new());

        register_graph_module(&lua).unwrap();
        register_note_store_functions(&lua, store).unwrap();

        let result: Value = lua
            .load(r#"return graph.note_get("nonexistent.md")"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result, Value::Nil));
    }

    #[tokio::test]
    async fn test_note_get_includes_tags() {
        let lua = Lua::new();
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![sample_note(
            "Index.md", "Index",
        )]));

        register_graph_module(&lua).unwrap();
        register_note_store_functions(&lua, store).unwrap();

        let result: Table = lua
            .load(
                r#"
                local note = graph.note_get("Index.md")
                return note.tags
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 2);
        assert_eq!(result.get::<String>(1).unwrap(), "rust");
        assert_eq!(result.get::<String>(2).unwrap(), "test");
    }

    #[tokio::test]
    async fn test_note_get_includes_links() {
        let lua = Lua::new();
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![sample_note(
            "Index.md", "Index",
        )]));

        register_graph_module(&lua).unwrap();
        register_note_store_functions(&lua, store).unwrap();

        let result: Table = lua
            .load(
                r#"
                local note = graph.note_get("Index.md")
                return note.links_to
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 1);
        assert_eq!(result.get::<String>(1).unwrap(), "other/note.md");
    }

    #[tokio::test]
    async fn test_note_list_returns_all_notes() {
        let lua = Lua::new();
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![
            sample_note("a.md", "Note A"),
            sample_note("b.md", "Note B"),
            sample_note("c.md", "Note C"),
        ]));

        register_graph_module(&lua).unwrap();
        register_note_store_functions(&lua, store).unwrap();

        let result: Table = lua
            .load(r#"return graph.note_list()"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_note_list_with_limit() {
        let lua = Lua::new();
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![
            sample_note("a.md", "Note A"),
            sample_note("b.md", "Note B"),
            sample_note("c.md", "Note C"),
        ]));

        register_graph_module(&lua).unwrap();
        register_note_store_functions(&lua, store).unwrap();

        let result: Table = lua
            .load(r#"return graph.note_list(2)"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 2);
    }

    #[tokio::test]
    async fn test_note_list_empty_store() {
        let lua = Lua::new();
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::new());

        register_graph_module(&lua).unwrap();
        register_note_store_functions(&lua, store).unwrap();

        let result: Table = lua
            .load(r#"return graph.note_list()"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_note_list_items_have_expected_fields() {
        let lua = Lua::new();
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![sample_note(
            "test.md",
            "Test Note",
        )]));

        register_graph_module(&lua).unwrap();
        register_note_store_functions(&lua, store).unwrap();

        let result: Table = lua
            .load(
                r#"
                local notes = graph.note_list()
                local note = notes[1]
                return {
                    has_path = note.path ~= nil,
                    has_title = note.title ~= nil,
                    has_tags = note.tags ~= nil,
                    has_links = note.links_to ~= nil,
                    has_updated = note.updated_at ~= nil,
                    has_embedding_flag = note.has_embedding ~= nil,
                }
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert!(result.get::<bool>("has_path").unwrap());
        assert!(result.get::<bool>("has_title").unwrap());
        assert!(result.get::<bool>("has_tags").unwrap());
        assert!(result.get::<bool>("has_links").unwrap());
        assert!(result.get::<bool>("has_updated").unwrap());
        assert!(result.get::<bool>("has_embedding_flag").unwrap());
    }

    #[tokio::test]
    async fn test_register_graph_module_full() {
        use crucible_core::traits::{GraphQueryExecutor, GraphQueryResult};

        struct MockExecutor;

        #[async_trait]
        impl GraphQueryExecutor for MockExecutor {
            async fn execute(&self, _query: &str) -> GraphQueryResult<Vec<serde_json::Value>> {
                Ok(vec![])
            }
        }

        let lua = Lua::new();
        let executor: Arc<dyn GraphQueryExecutor> = Arc::new(MockExecutor);
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![sample_note(
            "test.md", "Test",
        )]));

        // Use the combined registration function
        register_graph_module_full(&lua, executor, store).unwrap();

        // Both db_find and note_get should be available
        let graph: Table = lua.globals().get("graph").unwrap();
        assert!(graph.contains_key("db_find").unwrap());
        assert!(graph.contains_key("note_get").unwrap());
        assert!(graph.contains_key("note_list").unwrap());
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

    // =========================================================================
    // fast_outlinks tests
    // =========================================================================

    #[test]
    fn test_fast_outlinks_returns_paths() {
        let lua = Lua::new();
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());

        register_graph_module(&lua).unwrap();
        register_graph_view_functions(&lua, view).unwrap();

        let result: Table = lua
            .load(r#"return graph.fast_outlinks("notes/index.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 2);
        assert_eq!(result.get::<String>(1).unwrap(), "linked/note-a.md");
        assert_eq!(result.get::<String>(2).unwrap(), "linked/note-b.md");
    }

    #[test]
    fn test_fast_outlinks_empty_result() {
        let lua = Lua::new();
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new().with_outlinks(vec![]));

        register_graph_module(&lua).unwrap();
        register_graph_view_functions(&lua, view).unwrap();

        let result: Table = lua
            .load(r#"return graph.fast_outlinks("orphan.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    // =========================================================================
    // fast_backlinks tests
    // =========================================================================

    #[test]
    fn test_fast_backlinks_returns_paths() {
        let lua = Lua::new();
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());

        register_graph_module(&lua).unwrap();
        register_graph_view_functions(&lua, view).unwrap();

        let result: Table = lua
            .load(r#"return graph.fast_backlinks("notes/target.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 1);
        assert_eq!(result.get::<String>(1).unwrap(), "backlink/from-a.md");
    }

    #[test]
    fn test_fast_backlinks_empty_result() {
        let lua = Lua::new();
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new().with_backlinks(vec![]));

        register_graph_module(&lua).unwrap();
        register_graph_view_functions(&lua, view).unwrap();

        let result: Table = lua
            .load(r#"return graph.fast_backlinks("orphan.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    // =========================================================================
    // fast_neighbors tests
    // =========================================================================

    #[test]
    fn test_fast_neighbors_returns_paths() {
        let lua = Lua::new();
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());

        register_graph_module(&lua).unwrap();
        register_graph_view_functions(&lua, view).unwrap();

        let result: Table = lua
            .load(r#"return graph.fast_neighbors("notes/hub.md", 1)"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 3);
    }

    #[test]
    fn test_fast_neighbors_empty_result() {
        let lua = Lua::new();
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new().with_neighbors(vec![]));

        register_graph_module(&lua).unwrap();
        register_graph_view_functions(&lua, view).unwrap();

        let result: Table = lua
            .load(r#"return graph.fast_neighbors("isolated.md", 2)"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[test]
    fn test_fast_neighbors_depth_parameter() {
        // Verify that depth is passed correctly (mock doesn't use it, but signature works)
        let lua = Lua::new();
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());

        register_graph_module(&lua).unwrap();
        register_graph_view_functions(&lua, view).unwrap();

        let result: Table = lua
            .load(
                r#"
                local depth1 = graph.fast_neighbors("notes/hub.md", 1)
                local depth3 = graph.fast_neighbors("notes/hub.md", 3)
                return {
                    depth1_len = #depth1,
                    depth3_len = #depth3,
                }
                "#,
            )
            .eval()
            .unwrap();

        // Both return same length since mock doesn't vary by depth
        assert_eq!(result.get::<i64>("depth1_len").unwrap(), 3);
        assert_eq!(result.get::<i64>("depth3_len").unwrap(), 3);
    }

    // =========================================================================
    // Combined module tests
    // =========================================================================

    #[tokio::test]
    async fn test_register_graph_module_with_all() {
        use async_trait::async_trait;
        use crucible_core::events::SessionEvent;
        use crucible_core::parser::BlockHash;
        use crucible_core::storage::{Filter, NoteRecord, NoteStore, SearchResult, StorageResult};
        use crucible_core::traits::{GraphQueryExecutor, GraphQueryResult};

        struct MockExecutor;

        #[async_trait]
        impl GraphQueryExecutor for MockExecutor {
            async fn execute(&self, _query: &str) -> GraphQueryResult<Vec<serde_json::Value>> {
                Ok(vec![])
            }
        }

        struct MockNoteStore;

        #[async_trait]
        impl NoteStore for MockNoteStore {
            async fn upsert(&self, note: NoteRecord) -> StorageResult<Vec<SessionEvent>> {
                let event = SessionEvent::NoteCreated {
                    path: note.path.into(),
                    title: Some(note.title),
                };
                Ok(vec![event])
            }
            async fn get(&self, _path: &str) -> StorageResult<Option<NoteRecord>> {
                Ok(None)
            }
            async fn delete(&self, path: &str) -> StorageResult<SessionEvent> {
                Ok(SessionEvent::NoteDeleted {
                    path: path.into(),
                    existed: false,
                })
            }
            async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
                Ok(vec![])
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

        let lua = Lua::new();
        let executor: Arc<dyn GraphQueryExecutor> = Arc::new(MockExecutor);
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore);
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());

        // Use the combined registration function
        register_graph_module_with_all(&lua, executor, store, view).unwrap();

        // All functions should be available
        let graph: Table = lua.globals().get("graph").unwrap();
        assert!(graph.contains_key("db_find").unwrap());
        assert!(graph.contains_key("note_get").unwrap());
        assert!(graph.contains_key("note_list").unwrap());
        assert!(graph.contains_key("fast_outlinks").unwrap());
        assert!(graph.contains_key("fast_backlinks").unwrap());
        assert!(graph.contains_key("fast_neighbors").unwrap());
    }
}
