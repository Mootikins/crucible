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
//! ```

use crate::error::LuaError;
use crucible_core::storage::NoteStore;
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

/// Register scoped NoteStore functions on the graph module: every read through
/// `graph.note_get` / `graph.note_list` is filtered by the given authority.
///
/// Production callers (`daemon_plugins::upgrade_with_storage`) pass
/// `Scope::Workspace { path: kiln_path }` so a Lua plugin running inside
/// kiln A cannot read notes scoped to kiln B even via the lower-level
/// `graph.*` surface.
pub fn register_note_store_functions_scoped(
    lua: &Lua,
    store: Arc<dyn NoteStore>,
    authority: crucible_core::storage::Scope,
) -> Result<(), LuaError> {
    // Get the graph table (must exist from prior registration)
    let graph: Table = lua.globals().get("graph")?;

    // note_get - Get a note by path
    let s = Arc::clone(&store);
    let auth = authority.clone();
    let note_get = lua.create_async_function(move |lua, path: String| {
        let s = Arc::clone(&s);
        let auth = auth.clone();
        async move {
            match s.get(&path, &auth).await {
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
    let auth = authority.clone();
    let note_list = lua.create_async_function(move |lua, limit: Option<usize>| {
        let s = Arc::clone(&s);
        let auth = auth.clone();
        async move {
            match s.list(&auth).await {
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

/// Register the graph module with scoped NoteStore-backed async queries — see
/// [`register_note_store_functions_scoped`] for security semantics.
pub fn register_graph_module_with_store_scoped(
    lua: &Lua,
    store: Arc<dyn NoteStore>,
    authority: crucible_core::storage::Scope,
) -> Result<(), LuaError> {
    if lua.globals().get::<Option<Table>>("graph")?.is_none() {
        register_graph_module(lua)?;
    }

    register_note_store_functions_scoped(lua, store, authority)?;

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestLuaBuilder;

    // =========================================================================
    // RED: Write failing tests first
    // =========================================================================

    #[test]
    fn test_graph_find_existing_note() {
        let lua = TestLuaBuilder::new().with_graph().build();
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
        let lua = TestLuaBuilder::new().with_graph().build();
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
        let lua = TestLuaBuilder::new().with_graph().build();
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
        let lua = TestLuaBuilder::new().with_graph().build();
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
        let lua = TestLuaBuilder::new().with_graph().build();
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
        let lua = TestLuaBuilder::new().with_graph().build();
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
        let lua = TestLuaBuilder::new().with_graph().build();
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
        let lua = TestLuaBuilder::new().with_graph().build();
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
mod test_mocks {
    use async_trait::async_trait;
    use crucible_core::events::{InternalSessionEvent, SessionEvent};
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::{Filter, NoteRecord, NoteStore, SearchResult, StorageResult};
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory mock `NoteStore` keyed by note path.
    pub(super) struct MockNoteStore {
        notes: Mutex<HashMap<String, NoteRecord>>,
    }

    impl MockNoteStore {
        pub(super) fn new() -> Self {
            Self {
                notes: Mutex::new(HashMap::new()),
            }
        }

        pub(super) fn with_notes(notes: Vec<NoteRecord>) -> Self {
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
            let event = SessionEvent::internal(InternalSessionEvent::NoteCreated {
                path: path.into(),
                title: Some(title),
            });
            Ok(vec![event])
        }

        async fn get(
            &self,
            path: &str,
            _authority: &crucible_core::storage::Scope,
        ) -> StorageResult<Option<NoteRecord>> {
            let map = self.notes.lock().unwrap();
            Ok(map.get(path).cloned())
        }

        async fn delete(&self, path: &str) -> StorageResult<SessionEvent> {
            let mut map = self.notes.lock().unwrap();
            map.remove(path);
            Ok(SessionEvent::internal(InternalSessionEvent::NoteDeleted {
                path: path.into(),
                existed: false,
            }))
        }

        async fn list(
            &self,
            _authority: &crucible_core::storage::Scope,
        ) -> StorageResult<Vec<NoteRecord>> {
            let map = self.notes.lock().unwrap();
            Ok(map.values().cloned().collect())
        }

        async fn get_by_hash(
            &self,
            _hash: &BlockHash,
            _authority: &crucible_core::storage::Scope,
        ) -> StorageResult<Option<NoteRecord>> {
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
}

#[cfg(test)]
mod note_store_tests {
    use super::*;

    /// Unscoped registration for tests: unbound workspace authority.
    fn register_note_store_unscoped(lua: &Lua, store: Arc<dyn NoteStore>) {
        register_note_store_functions_scoped(
            lua,
            store,
            crucible_core::storage::Scope::workspace_unchecked(std::path::PathBuf::new()),
        )
        .unwrap();
    }

    use crucible_core::parser::BlockHash;
    use crucible_core::storage::{NoteRecord, NoteStore};
    use test_mocks::MockNoteStore;

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
        register_note_store_unscoped(&lua, store);

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
        register_note_store_unscoped(&lua, store);

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
        register_note_store_unscoped(&lua, store);

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
        register_note_store_unscoped(&lua, store);

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
        register_note_store_unscoped(&lua, store);

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
        register_note_store_unscoped(&lua, store);

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
        register_note_store_unscoped(&lua, store);

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
        register_note_store_unscoped(&lua, store);

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
}
