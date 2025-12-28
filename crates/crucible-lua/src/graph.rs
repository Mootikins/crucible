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
use crucible_core::traits::GraphQueryExecutor;
use mlua::{Lua, Table, Value};
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
                        json_to_lua_value(&lua, &first)
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

/// Escape double quotes in a string for safe embedding in queries
fn escape_quotes(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Convert a JSON value to a Lua value
fn json_to_lua_value(lua: &Lua, value: &serde_json::Value) -> Result<Value, mlua::Error> {
    match value {
        serde_json::Value::Null => Ok(Value::Nil),
        serde_json::Value::Bool(b) => Ok(Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(f))
            } else {
                Ok(Value::Nil)
            }
        }
        serde_json::Value::String(s) => lua.create_string(s).map(Value::String),
        serde_json::Value::Array(arr) => {
            let table = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                table.set(i + 1, json_to_lua_value(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
        serde_json::Value::Object(map) => {
            let table = lua.create_table()?;
            for (k, v) in map {
                table.set(k.as_str(), json_to_lua_value(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}

/// Convert a JSON array to a Lua table
fn json_array_to_lua_table(lua: &Lua, values: &[serde_json::Value]) -> Result<Value, mlua::Error> {
    let table = lua.create_table()?;
    for (i, v) in values.iter().enumerate() {
        table.set(i + 1, json_to_lua_value(lua, v)?)?;
    }
    Ok(Value::Table(table))
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
