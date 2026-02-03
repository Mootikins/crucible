//! Kiln API module for Lua scripts
//!
//! Provides `cru.kiln.*` functions for accessing notes and knowledge graph
//! from Lua scripts. Also available as `cru.vault.*` for backwards compatibility.
//!
//! ## Usage in Lua
//!
//! ```lua
//! -- List notes (async)
//! local notes = cru.kiln.list(10)  -- optional limit
//! for _, note in ipairs(notes) do
//!     print(note.title, note.path)
//! end
//!
//! -- Get a specific note (async)
//! local note = cru.kiln.get("path/to/note.md")
//! if note then
//!     print(note.title)
//!     print(note.content_hash)
//!     for _, tag in ipairs(note.tags) do
//!         print("Tag:", tag)
//!     end
//! end
//!
//! -- Search notes (async) - semantic search
//! local results = cru.kiln.search("machine learning", {limit = 5})
//! for _, result in ipairs(results) do
//!     print(result.path, result.score)
//! end
//!
//! -- Get outgoing links from a note (sync)
//! local links = cru.kiln.outlinks("path/to/note.md")
//!
//! -- Get incoming links to a note (sync)
//! local backlinks = cru.kiln.backlinks("path/to/note.md")
//!
//! -- Get neighbors within depth (sync)
//! local nearby = cru.kiln.neighbors("path/to/note.md", 2)
//! ```

use crate::error::LuaError;
use crate::lua_util::register_in_namespaces;
use crucible_core::storage::{GraphView, NoteStore};
use mlua::{Lua, LuaSerdeExt, Table, Value};
use std::sync::Arc;

/// Register the kiln/vault module with a Lua state
///
/// This creates the `cru.kiln` namespace (and `cru.vault` alias) with stub functions.
/// Use `register_vault_module_with_store` to add database-backed functionality.
pub fn register_vault_module(lua: &Lua) -> Result<(), LuaError> {
    let vault = lua.create_table()?;

    let list_stub = lua.create_async_function(|lua, _limit: Option<usize>| async move {
        let table = lua.create_table()?;
        Ok(Value::Table(table))
    })?;
    vault.set("list", list_stub)?;

    let get_stub = lua.create_async_function(|_, _path: String| async move { Ok(Value::Nil) })?;
    vault.set("get", get_stub)?;

    let search_stub =
        lua.create_async_function(|lua, (_query, _opts): (String, Option<Table>)| async move {
            let table = lua.create_table()?;
            Ok(Value::Table(table))
        })?;
    vault.set("search", search_stub)?;

    let outlinks_stub = lua.create_function(|lua, _path: String| {
        let table = lua.create_table()?;
        Ok(Value::Table(table))
    })?;
    vault.set("outlinks", outlinks_stub)?;

    let backlinks_stub = lua.create_function(|lua, _path: String| {
        let table = lua.create_table()?;
        Ok(Value::Table(table))
    })?;
    vault.set("backlinks", backlinks_stub)?;

    let neighbors_stub = lua.create_function(|lua, (_path, _depth): (String, Option<usize>)| {
        let table = lua.create_table()?;
        Ok(Value::Table(table))
    })?;
    vault.set("neighbors", neighbors_stub)?;

    register_in_namespaces(lua, "kiln", vault.clone())?;
    register_in_namespaces(lua, "vault", vault)?;

    Ok(())
}

/// Register the vault module with NoteStore for database-backed queries
pub fn register_vault_module_with_store(
    lua: &Lua,
    store: Arc<dyn NoteStore>,
) -> Result<(), LuaError> {
    register_vault_module(lua)?;

    let globals = lua.globals();
    let cru: Table = globals.get("cru")?;
    let vault: Table = cru.get("vault")?;

    let s = Arc::clone(&store);
    let list_fn = lua.create_async_function(move |lua, limit: Option<usize>| {
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
                Err(e) => Err(mlua::Error::runtime(format!("Vault error: {}", e))),
            }
        }
    })?;
    vault.set("list", list_fn)?;

    let s = Arc::clone(&store);
    let get_fn = lua.create_async_function(move |lua, path: String| {
        let s = Arc::clone(&s);
        async move {
            match s.get(&path).await {
                Ok(Some(record)) => note_record_to_lua(&lua, &record),
                Ok(None) => Ok(Value::Nil),
                Err(e) => Err(mlua::Error::runtime(format!("Vault error: {}", e))),
            }
        }
    })?;
    vault.set("get", get_fn)?;

    let search_fn = lua.create_async_function(
        move |lua, (_query, _opts): (String, Option<Table>)| async move {
            let table = lua.create_table()?;
            Ok(Value::Table(table))
        },
    )?;
    vault.set("search", search_fn)?;

    Ok(())
}

/// Register GraphView functions on the vault module for graph traversal
pub fn register_vault_module_with_graph(
    lua: &Lua,
    view: Arc<dyn GraphView>,
) -> Result<(), LuaError> {
    let globals = lua.globals();
    let cru: Table = globals.get("cru")?;
    let vault: Table = cru.get("vault")?;

    let v = Arc::clone(&view);
    let outlinks_fn = lua.create_function(move |lua, path: String| {
        let paths = v.outlinks(&path);
        string_vec_to_lua_table(lua, &paths)
    })?;
    vault.set("outlinks", outlinks_fn)?;

    let v = Arc::clone(&view);
    let backlinks_fn = lua.create_function(move |lua, path: String| {
        let paths = v.backlinks(&path);
        string_vec_to_lua_table(lua, &paths)
    })?;
    vault.set("backlinks", backlinks_fn)?;

    let v = Arc::clone(&view);
    let neighbors_fn =
        lua.create_function(move |lua, (path, depth): (String, Option<usize>)| {
            let paths = v.neighbors(&path, depth.unwrap_or(1));
            string_vec_to_lua_table(lua, &paths)
        })?;
    vault.set("neighbors", neighbors_fn)?;

    Ok(())
}

/// Register the vault module with both NoteStore and GraphView
pub fn register_vault_module_full(
    lua: &Lua,
    store: Arc<dyn NoteStore>,
    view: Arc<dyn GraphView>,
) -> Result<(), LuaError> {
    register_vault_module_with_store(lua, store)?;
    register_vault_module_with_graph(lua, view)?;
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

    let tags = lua.create_table()?;
    for (i, tag) in record.tags.iter().enumerate() {
        tags.set(i + 1, tag.as_str())?;
    }
    table.set("tags", tags)?;

    let links = lua.create_table()?;
    for (i, link) in record.links_to.iter().enumerate() {
        links.set(i + 1, link.as_str())?;
    }
    table.set("links_to", links)?;

    let props = lua.create_table()?;
    for (k, v) in &record.properties {
        props.set(k.as_str(), lua.to_value(v)?)?;
    }
    table.set("properties", props)?;

    table.set("updated_at", record.updated_at.to_rfc3339())?;

    table.set("has_embedding", record.has_embedding())?;

    Ok(Value::Table(table))
}

fn string_vec_to_lua_table(lua: &Lua, values: &[String]) -> Result<Value, mlua::Error> {
    let table = lua.create_table()?;
    for (i, v) in values.iter().enumerate() {
        table.set(i + 1, v.as_str())?;
    }
    Ok(Value::Table(table))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        let cru = lua.create_table().unwrap();
        lua.globals().set("cru", cru).unwrap();
        let crucible = lua.create_table().unwrap();
        lua.globals().set("crucible", crucible).unwrap();
        register_vault_module(&lua).expect("Should register vault module");
        lua
    }

    #[test]
    fn test_register_vault_module() {
        let lua = setup_lua();

        let cru: Table = lua.globals().get("cru").expect("cru should exist");
        let vault: Table = cru.get("vault").expect("cru.vault should exist");

        assert!(vault.contains_key("list").unwrap());
        assert!(vault.contains_key("get").unwrap());
        assert!(vault.contains_key("search").unwrap());
        assert!(vault.contains_key("outlinks").unwrap());
        assert!(vault.contains_key("backlinks").unwrap());
        assert!(vault.contains_key("neighbors").unwrap());
    }

    #[test]
    fn test_kiln_alias_exists() {
        let lua = setup_lua();

        let cru: Table = lua.globals().get("cru").expect("cru should exist");
        let kiln: Table = cru.get("kiln").expect("cru.kiln should exist");

        assert!(kiln.contains_key("list").unwrap());
        assert!(kiln.contains_key("get").unwrap());
        assert!(kiln.contains_key("search").unwrap());
    }

    #[test]
    fn test_kiln_and_vault_share_same_table() {
        let lua = setup_lua();

        let cru: Table = lua.globals().get("cru").expect("cru should exist");
        let _kiln: Table = cru.get("kiln").expect("cru.kiln should exist");
        let _vault: Table = cru.get("vault").expect("cru.vault should exist");

        // Both point to the same Lua table — modifying one affects the other
        let result: bool = lua
            .load(r#"return cru.kiln.list == cru.vault.list"#)
            .eval()
            .unwrap();
        assert!(result, "cru.kiln and cru.vault should share the same functions");
    }

    #[test]
    fn test_vault_also_registered_as_crucible() {
        let lua = setup_lua();

        let crucible: Table = lua
            .globals()
            .get("crucible")
            .expect("crucible should exist");
        let vault: Table = crucible.get("vault").expect("crucible.vault should exist");

        assert!(vault.contains_key("list").unwrap());
    }

    #[tokio::test]
    async fn test_vault_list_stub_returns_empty() {
        let lua = setup_lua();

        let result: Table = lua
            .load(r#"return cru.vault.list()"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_vault_get_stub_returns_nil() {
        let lua = setup_lua();

        let result: Value = lua
            .load(r#"return cru.vault.get("test.md")"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result, Value::Nil));
    }

    #[test]
    fn test_vault_outlinks_stub_returns_empty() {
        let lua = setup_lua();

        let result: Table = lua
            .load(r#"return cru.vault.outlinks("test.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[test]
    fn test_vault_backlinks_stub_returns_empty() {
        let lua = setup_lua();

        let result: Table = lua
            .load(r#"return cru.vault.backlinks("test.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[test]
    fn test_vault_neighbors_stub_returns_empty() {
        let lua = setup_lua();

        let result: Table = lua
            .load(r#"return cru.vault.neighbors("test.md", 2)"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }
}

#[cfg(test)]
mod store_tests {
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

    fn setup_lua_with_store(store: Arc<dyn NoteStore>) -> Lua {
        let lua = Lua::new();
        let cru = lua.create_table().unwrap();
        lua.globals().set("cru", cru).unwrap();
        let crucible = lua.create_table().unwrap();
        lua.globals().set("crucible", crucible).unwrap();
        register_vault_module_with_store(&lua, store).expect("Should register vault module");
        lua
    }

    #[tokio::test]
    async fn test_vault_list_returns_notes() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![
            sample_note("a.md", "Note A"),
            sample_note("b.md", "Note B"),
            sample_note("c.md", "Note C"),
        ]));
        let lua = setup_lua_with_store(store);

        let result: Table = lua
            .load(r#"return cru.vault.list()"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_vault_list_with_limit() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![
            sample_note("a.md", "Note A"),
            sample_note("b.md", "Note B"),
            sample_note("c.md", "Note C"),
        ]));
        let lua = setup_lua_with_store(store);

        let result: Table = lua
            .load(r#"return cru.vault.list(2)"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 2);
    }

    #[tokio::test]
    async fn test_vault_get_returns_note() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![sample_note(
            "test.md",
            "Test Note",
        )]));
        let lua = setup_lua_with_store(store);

        let result: Table = lua
            .load(r#"return cru.vault.get("test.md")"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.get::<String>("path").unwrap(), "test.md");
        assert_eq!(result.get::<String>("title").unwrap(), "Test Note");
        assert!(result.get::<bool>("has_embedding").is_ok());
    }

    #[tokio::test]
    async fn test_vault_get_returns_nil_for_missing() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::new());
        let lua = setup_lua_with_store(store);

        let result: Value = lua
            .load(r#"return cru.vault.get("nonexistent.md")"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result, Value::Nil));
    }

    #[tokio::test]
    async fn test_vault_get_includes_tags() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![sample_note(
            "test.md", "Test",
        )]));
        let lua = setup_lua_with_store(store);

        let result: Table = lua
            .load(
                r#"
                local note = cru.vault.get("test.md")
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
    async fn test_vault_get_includes_links() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![sample_note(
            "test.md", "Test",
        )]));
        let lua = setup_lua_with_store(store);

        let result: Table = lua
            .load(
                r#"
                local note = cru.vault.get("test.md")
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
    async fn test_vault_note_has_all_fields() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![sample_note(
            "test.md", "Test",
        )]));
        let lua = setup_lua_with_store(store);

        let result: Table = lua
            .load(
                r#"
                local note = cru.vault.get("test.md")
                return {
                    has_path = note.path ~= nil,
                    has_title = note.title ~= nil,
                    has_tags = note.tags ~= nil,
                    has_links = note.links_to ~= nil,
                    has_updated = note.updated_at ~= nil,
                    has_embedding_flag = note.has_embedding ~= nil,
                    has_properties = note.properties ~= nil,
                    has_content_hash = note.content_hash ~= nil,
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
        assert!(result.get::<bool>("has_properties").unwrap());
        assert!(result.get::<bool>("has_content_hash").unwrap());
    }
}

#[cfg(test)]
mod graph_tests {
    use super::*;
    use crucible_core::storage::{GraphView, NoteRecord};

    /// Mock GraphView for testing
    struct MockGraphView {
        outlinks_result: Vec<String>,
        backlinks_result: Vec<String>,
        neighbors_result: Vec<String>,
    }

    impl MockGraphView {
        fn new() -> Self {
            Self {
                outlinks_result: vec!["linked/a.md".to_string(), "linked/b.md".to_string()],
                backlinks_result: vec!["backlink/from-a.md".to_string()],
                neighbors_result: vec![
                    "linked/a.md".to_string(),
                    "linked/b.md".to_string(),
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

        fn rebuild(&mut self, _notes: &[NoteRecord]) {}
    }

    fn setup_lua_with_graph(view: Arc<dyn GraphView>) -> Lua {
        let lua = Lua::new();
        let cru = lua.create_table().unwrap();
        lua.globals().set("cru", cru).unwrap();
        let crucible = lua.create_table().unwrap();
        lua.globals().set("crucible", crucible).unwrap();
        register_vault_module(&lua).expect("Should register vault module");
        register_vault_module_with_graph(&lua, view).expect("Should register graph functions");
        lua
    }

    #[test]
    fn test_vault_outlinks_returns_paths() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());
        let lua = setup_lua_with_graph(view);

        let result: Table = lua
            .load(r#"return cru.vault.outlinks("test.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 2);
        assert_eq!(result.get::<String>(1).unwrap(), "linked/a.md");
        assert_eq!(result.get::<String>(2).unwrap(), "linked/b.md");
    }

    #[test]
    fn test_vault_outlinks_empty() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new().with_outlinks(vec![]));
        let lua = setup_lua_with_graph(view);

        let result: Table = lua
            .load(r#"return cru.vault.outlinks("orphan.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[test]
    fn test_vault_backlinks_returns_paths() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());
        let lua = setup_lua_with_graph(view);

        let result: Table = lua
            .load(r#"return cru.vault.backlinks("test.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 1);
        assert_eq!(result.get::<String>(1).unwrap(), "backlink/from-a.md");
    }

    #[test]
    fn test_vault_backlinks_empty() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new().with_backlinks(vec![]));
        let lua = setup_lua_with_graph(view);

        let result: Table = lua
            .load(r#"return cru.vault.backlinks("orphan.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[test]
    fn test_vault_neighbors_returns_paths() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());
        let lua = setup_lua_with_graph(view);

        let result: Table = lua
            .load(r#"return cru.vault.neighbors("test.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 3);
    }

    #[test]
    fn test_vault_neighbors_with_depth() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());
        let lua = setup_lua_with_graph(view);

        let result: Table = lua
            .load(r#"return cru.vault.neighbors("test.md", 3)"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 3);
    }

    #[test]
    fn test_vault_neighbors_empty() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new().with_neighbors(vec![]));
        let lua = setup_lua_with_graph(view);

        let result: Table = lua
            .load(r#"return cru.vault.neighbors("isolated.md", 2)"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }
}
