//! Kiln API module for Lua scripts
//!
//! Provides `cru.kiln.*` functions for accessing notes and knowledge graph
//! from Lua scripts.
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
//! local results = cru.kiln.search("machine learning", {limit = 5, threshold = 0.6})
//! for _, result in ipairs(results) do
//!     print(result.path, result.title, result.score, result.snippet)
//! end
//!
//! -- Create a note (daemon-only; writes file and indexes synchronously)
//! local path = cru.kiln.create_note({
//!   path = "Entities/Jane Doe.md",
//!   body = "# Jane Doe\n\nWorks on [[Crucible]].",
//!   frontmatter = { type = "entity", aliases = { "jane", "JD" } },
//!   overwrite = false,           -- default; set true to replace existing
//! })
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
use crate::json_query::lua_to_json;
use crate::lua_util::register_in_namespaces;
use crucible_core::storage::{GraphView, NoteStore};
use mlua::{Lua, LuaSerdeExt, Table, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// One hit returned by [`DaemonVaultApi::search`].
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub path: String,
    pub title: String,
    pub score: f64,
    pub snippet: Option<String>,
}

/// Daemon-side vault API consumed by `cru.kiln.create_note` and `cru.kiln.search`.
///
/// Implemented by `crucible-daemon` so the Lua crate stays free of pipeline /
/// embedding-provider concerns. Mirrors the stub-then-upgrade pattern used by
/// [`crate::team::DaemonTeamApi`] and friends.
pub trait DaemonVaultApi: Send + Sync {
    /// Write a note to disk under the kiln, then parse + index synchronously
    /// so callers can immediately `search`/`get` it. Returns the absolute path
    /// of the written file on success.
    ///
    /// * `relative_path` is kiln-relative (e.g. `"Entities/Jane Doe.md"`).
    /// * `frontmatter` is an optional JSON object; serialized to YAML.
    /// * If `overwrite` is `false` and the file already exists, returns
    ///   `Err(...)` without touching the file.
    fn create_note(
        &self,
        relative_path: String,
        body: String,
        frontmatter: Option<serde_json::Value>,
        overwrite: bool,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;

    /// Embed `query` and return the top `limit` notes by similarity. Hits
    /// below `threshold` (cosine score) are filtered out. `threshold = 0.0`
    /// returns all top-k results.
    fn search(
        &self,
        query: String,
        limit: usize,
        threshold: f64,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<SearchHit>, String>> + Send>>;
}

/// Default top-N when callers omit `limit`.
const DEFAULT_SEARCH_LIMIT: usize = 10;

/// Register the kiln module with a Lua state
///
/// This creates the `cru.kiln` namespace with stub functions.
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

    // create_note stub: error when no daemon is connected. This is async
    // because the real implementation goes through the pipeline (which is
    // async), and stub callers should hit the same shape.
    let create_note_stub = lua.create_async_function(|_, _opts: Table| async move {
        Err::<Value, _>(mlua::Error::runtime(
            "cru.kiln.create_note: no daemon connected (vault API not initialized)",
        ))
    })?;
    vault.set("create_note", create_note_stub)?;

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

    register_in_namespaces(lua, "kiln", vault)?;

    Ok(())
}

/// Register the kiln module with NoteStore for database-backed queries.
pub fn register_vault_module_with_store(
    lua: &Lua,
    store: Arc<dyn NoteStore>,
) -> Result<(), LuaError> {
    register_vault_module(lua)?;

    let globals = lua.globals();
    let cru: Table = globals.get("cru")?;
    let vault: Table = cru.get("kiln")?;

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
                Err(e) => Err(mlua::Error::runtime(format!("Kiln error: {}", e))),
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
                Err(e) => Err(mlua::Error::runtime(format!("Kiln error: {}", e))),
            }
        }
    })?;
    vault.set("get", get_fn)?;

    // `search` remains a stub here; `register_vault_module_with_api` upgrades
    // it. We re-install the stub so a partial upgrade (store but no api)
    // doesn't leak a previously-registered real fn.
    let search_fn = lua.create_async_function(
        move |lua, (_query, _opts): (String, Option<Table>)| async move {
            let table = lua.create_table()?;
            Ok(Value::Table(table))
        },
    )?;
    vault.set("search", search_fn)?;

    Ok(())
}

/// Upgrade `cru.kiln.create_note` and `cru.kiln.search` to call into a
/// daemon-backed [`DaemonVaultApi`].
///
/// `register_vault_module` (or `register_vault_module_with_store`) must be
/// called first so the `cru.kiln` table exists.
pub fn register_vault_module_with_api(
    lua: &Lua,
    api: Arc<dyn DaemonVaultApi>,
) -> Result<(), LuaError> {
    let globals = lua.globals();
    let cru: Table = globals.get("cru")?;
    let vault: Table = cru.get("kiln")?;

    // -- create_note ------------------------------------------------------
    let api_create = Arc::clone(&api);
    let create_note_fn = lua.create_async_function(move |lua, opts: Table| {
        let api = Arc::clone(&api_create);
        async move {
            let path: String = opts
                .get("path")
                .map_err(|_| mlua::Error::runtime("create_note: 'path' (string) is required"))?;
            let body: String = opts
                .get("body")
                .map_err(|_| mlua::Error::runtime("create_note: 'body' (string) is required"))?;
            let overwrite: bool = opts.get("overwrite").unwrap_or(false);

            // frontmatter is optional. When present it must be a table; we
            // convert through json so YAML serialization on the daemon side
            // matches what the existing MCP create_note tool does.
            let frontmatter_json = match opts.get::<Value>("frontmatter") {
                Ok(Value::Nil) | Err(_) => None,
                Ok(Value::Table(t)) => {
                    let json = lua_to_json(&lua, Value::Table(t)).map_err(|e| {
                        mlua::Error::runtime(format!("create_note: invalid frontmatter table: {e}"))
                    })?;
                    Some(json)
                }
                Ok(other) => {
                    return Err(mlua::Error::runtime(format!(
                        "create_note: 'frontmatter' must be a table, got {}",
                        other.type_name()
                    )))
                }
            };

            match api
                .create_note(path, body, frontmatter_json, overwrite)
                .await
            {
                Ok(abs_path) => Ok(Value::String(lua.create_string(&abs_path)?)),
                Err(e) => Err(mlua::Error::runtime(format!("create_note: {e}"))),
            }
        }
    })?;
    vault.set("create_note", create_note_fn)?;

    // -- search -----------------------------------------------------------
    let api_search = Arc::clone(&api);
    let search_fn =
        lua.create_async_function(move |lua, (query, opts): (String, Option<Table>)| {
            let api = Arc::clone(&api_search);
            async move {
                let (limit, threshold) = match opts {
                    Some(t) => {
                        let limit: usize = t
                            .get::<Option<usize>>("limit")
                            .ok()
                            .flatten()
                            .unwrap_or(DEFAULT_SEARCH_LIMIT);
                        // Accept either f64 or integer from Lua.
                        let threshold: f64 = t
                            .get::<Option<f64>>("threshold")
                            .ok()
                            .flatten()
                            .unwrap_or(0.0);
                        (limit, threshold)
                    }
                    None => (DEFAULT_SEARCH_LIMIT, 0.0),
                };

                let hits = api
                    .search(query, limit, threshold)
                    .await
                    .map_err(|e| mlua::Error::runtime(format!("search: {e}")))?;

                let table = lua.create_table()?;
                for (i, hit) in hits.into_iter().enumerate() {
                    let row = lua.create_table()?;
                    row.set("path", hit.path)?;
                    row.set("title", hit.title)?;
                    row.set("score", hit.score)?;
                    match hit.snippet {
                        Some(s) => row.set("snippet", s)?,
                        None => row.set("snippet", Value::Nil)?,
                    }
                    table.set(i + 1, row)?;
                }
                Ok(Value::Table(table))
            }
        })?;
    vault.set("search", search_fn)?;

    Ok(())
}

/// Register GraphView functions on the kiln module for graph traversal.
pub fn register_vault_module_with_graph(
    lua: &Lua,
    view: Arc<dyn GraphView>,
) -> Result<(), LuaError> {
    let globals = lua.globals();
    let cru: Table = globals.get("cru")?;
    let vault: Table = cru.get("kiln")?;

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

/// Register the kiln module with both NoteStore and GraphView
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
    use crate::test_support::TestLuaBuilder;

    #[test]
    fn test_register_kiln_module() {
        let lua = TestLuaBuilder::new().with_vault().build();

        let cru: Table = lua.globals().get("cru").expect("cru should exist");
        let kiln: Table = cru.get("kiln").expect("cru.kiln should exist");

        assert!(kiln.contains_key("list").unwrap());
        assert!(kiln.contains_key("get").unwrap());
        assert!(kiln.contains_key("search").unwrap());
        assert!(kiln.contains_key("outlinks").unwrap());
        assert!(kiln.contains_key("backlinks").unwrap());
        assert!(kiln.contains_key("neighbors").unwrap());
    }

    #[test]
    fn test_kiln_also_registered_as_crucible() {
        let lua = TestLuaBuilder::new().with_vault().build();

        let crucible: Table = lua
            .globals()
            .get("crucible")
            .expect("crucible should exist");
        let kiln: Table = crucible.get("kiln").expect("crucible.kiln should exist");

        assert!(kiln.contains_key("list").unwrap());
    }

    #[tokio::test]
    async fn test_kiln_list_stub_returns_empty() {
        let lua = TestLuaBuilder::new().with_vault().build();

        let result: Table = lua
            .load(r#"return cru.kiln.list()"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_kiln_get_stub_returns_nil() {
        let lua = TestLuaBuilder::new().with_vault().build();

        let result: Value = lua
            .load(r#"return cru.kiln.get("test.md")"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result, Value::Nil));
    }

    #[test]
    fn test_kiln_outlinks_stub_returns_empty() {
        let lua = TestLuaBuilder::new().with_vault().build();

        let result: Table = lua
            .load(r#"return cru.kiln.outlinks("test.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[test]
    fn test_kiln_backlinks_stub_returns_empty() {
        let lua = TestLuaBuilder::new().with_vault().build();

        let result: Table = lua
            .load(r#"return cru.kiln.backlinks("test.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[test]
    fn test_kiln_neighbors_stub_returns_empty() {
        let lua = TestLuaBuilder::new().with_vault().build();

        let result: Table = lua
            .load(r#"return cru.kiln.neighbors("test.md", 2)"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;
    use crate::test_support::TestLuaBuilder;
    use async_trait::async_trait;
    use crucible_core::events::{InternalSessionEvent, SessionEvent};
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
            let event = SessionEvent::internal(InternalSessionEvent::NoteCreated {
                path: path.into(),
                title: Some(title),
            });
            Ok(vec![event])
        }

        async fn get(&self, path: &str) -> StorageResult<Option<NoteRecord>> {
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
    async fn test_vault_list_returns_notes() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_notes(vec![
            sample_note("a.md", "Note A"),
            sample_note("b.md", "Note B"),
            sample_note("c.md", "Note C"),
        ]));
        let lua = TestLuaBuilder::new().with_vault_store(store).build();

        let result: Table = lua
            .load(r#"return cru.kiln.list()"#)
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
        let lua = TestLuaBuilder::new().with_vault_store(store).build();

        let result: Table = lua
            .load(r#"return cru.kiln.list(2)"#)
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
        let lua = TestLuaBuilder::new().with_vault_store(store).build();

        let result: Table = lua
            .load(r#"return cru.kiln.get("test.md")"#)
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
        let lua = TestLuaBuilder::new().with_vault_store(store).build();

        let result: Value = lua
            .load(r#"return cru.kiln.get("nonexistent.md")"#)
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
        let lua = TestLuaBuilder::new().with_vault_store(store).build();

        let result: Table = lua
            .load(
                r#"
                local note = cru.kiln.get("test.md")
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
        let lua = TestLuaBuilder::new().with_vault_store(store).build();

        let result: Table = lua
            .load(
                r#"
                local note = cru.kiln.get("test.md")
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
        let lua = TestLuaBuilder::new().with_vault_store(store).build();

        let result: Table = lua
            .load(
                r#"
                local note = cru.kiln.get("test.md")
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
    use crate::test_support::TestLuaBuilder;
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

    #[test]
    fn test_vault_outlinks_returns_paths() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());
        let lua = TestLuaBuilder::new().with_vault_graph(view).build();

        let result: Table = lua
            .load(r#"return cru.kiln.outlinks("test.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 2);
        assert_eq!(result.get::<String>(1).unwrap(), "linked/a.md");
        assert_eq!(result.get::<String>(2).unwrap(), "linked/b.md");
    }

    #[test]
    fn test_vault_outlinks_empty() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new().with_outlinks(vec![]));
        let lua = TestLuaBuilder::new().with_vault_graph(view).build();

        let result: Table = lua
            .load(r#"return cru.kiln.outlinks("orphan.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[test]
    fn test_vault_backlinks_returns_paths() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());
        let lua = TestLuaBuilder::new().with_vault_graph(view).build();

        let result: Table = lua
            .load(r#"return cru.kiln.backlinks("test.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 1);
        assert_eq!(result.get::<String>(1).unwrap(), "backlink/from-a.md");
    }

    #[test]
    fn test_vault_backlinks_empty() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new().with_backlinks(vec![]));
        let lua = TestLuaBuilder::new().with_vault_graph(view).build();

        let result: Table = lua
            .load(r#"return cru.kiln.backlinks("orphan.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[test]
    fn test_vault_neighbors_returns_paths() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());
        let lua = TestLuaBuilder::new().with_vault_graph(view).build();

        let result: Table = lua
            .load(r#"return cru.kiln.neighbors("test.md")"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 3);
    }

    #[test]
    fn test_vault_neighbors_with_depth() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new());
        let lua = TestLuaBuilder::new().with_vault_graph(view).build();

        let result: Table = lua
            .load(r#"return cru.kiln.neighbors("test.md", 3)"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 3);
    }

    #[test]
    fn test_vault_neighbors_empty() {
        let view: Arc<dyn GraphView> = Arc::new(MockGraphView::new().with_neighbors(vec![]));
        let lua = TestLuaBuilder::new().with_vault_graph(view).build();

        let result: Table = lua
            .load(r#"return cru.kiln.neighbors("isolated.md", 2)"#)
            .eval()
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }
}

#[cfg(test)]
mod api_tests {
    //! Tests for [`DaemonVaultApi`]-backed `cru.kiln.create_note` and
    //! `cru.kiln.search`. The daemon implementation lives in
    //! `crucible-daemon::vault_bridge`; here we drive both surfaces through a
    //! stub `DaemonVaultApi` to verify argument plumbing, error shape, and
    //! that `register_vault_module_with_api` wires the Lua-visible API.

    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;

    /// Records every call so individual tests can assert on argument shape.
    struct StubVaultApi {
        last_create: Mutex<Option<CreateCall>>,
        last_search: Mutex<Option<SearchCall>>,
        create_result: Mutex<Result<String, String>>,
        search_result: Mutex<Result<Vec<SearchHit>, String>>,
    }

    #[derive(Clone)]
    struct CreateCall {
        path: String,
        body: String,
        frontmatter: Option<serde_json::Value>,
        overwrite: bool,
    }

    #[derive(Clone)]
    struct SearchCall {
        query: String,
        limit: usize,
        threshold: f64,
    }

    impl StubVaultApi {
        fn new() -> Self {
            Self {
                last_create: Mutex::new(None),
                last_search: Mutex::new(None),
                create_result: Mutex::new(Ok("/abs/path/note.md".to_string())),
                search_result: Mutex::new(Ok(Vec::new())),
            }
        }

        fn with_create_result(self, r: Result<String, String>) -> Self {
            *self.create_result.lock().unwrap() = r;
            self
        }

        fn with_search_hits(self, hits: Vec<SearchHit>) -> Self {
            *self.search_result.lock().unwrap() = Ok(hits);
            self
        }
    }

    impl DaemonVaultApi for StubVaultApi {
        fn create_note(
            &self,
            relative_path: String,
            body: String,
            frontmatter: Option<serde_json::Value>,
            overwrite: bool,
        ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
            *self.last_create.lock().unwrap() = Some(CreateCall {
                path: relative_path,
                body,
                frontmatter,
                overwrite,
            });
            let result = self.create_result.lock().unwrap().clone();
            Box::pin(async move { result })
        }

        fn search(
            &self,
            query: String,
            limit: usize,
            threshold: f64,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<SearchHit>, String>> + Send>> {
            *self.last_search.lock().unwrap() = Some(SearchCall {
                query,
                limit,
                threshold,
            });
            let result = self.search_result.lock().unwrap().clone();
            Box::pin(async move { result })
        }
    }

    fn make_lua_with_api(api: Arc<dyn DaemonVaultApi>) -> Lua {
        let lua = Lua::new();
        register_vault_module(&lua).expect("stub vault");
        register_vault_module_with_api(&lua, api).expect("vault api");
        lua
    }

    // -------- create_note --------

    #[tokio::test]
    async fn create_note_passes_path_body_and_frontmatter_to_api() {
        let stub = Arc::new(StubVaultApi::new());
        let lua = make_lua_with_api(stub.clone() as Arc<dyn DaemonVaultApi>);

        let abs: String = lua
            .load(
                r##"
                return cru.kiln.create_note({
                    path = "Entities/Jane Doe.md",
                    body = "# Jane Doe\n\nWorks on [[Crucible]].",
                    frontmatter = {
                        type = "entity",
                        aliases = { "jane", "JD" },
                    },
                })
                "##,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(abs, "/abs/path/note.md");

        let call = stub.last_create.lock().unwrap().clone().expect("called");
        assert_eq!(call.path, "Entities/Jane Doe.md");
        assert!(call.body.contains("[[Crucible]]"));
        assert!(!call.overwrite);
        let fm = call.frontmatter.expect("frontmatter forwarded");
        assert_eq!(fm["type"], "entity");
        let aliases = fm["aliases"].as_array().unwrap();
        assert_eq!(aliases.len(), 2);
    }

    #[tokio::test]
    async fn create_note_overwrite_flag_propagates() {
        let stub = Arc::new(StubVaultApi::new());
        let lua = make_lua_with_api(stub.clone() as Arc<dyn DaemonVaultApi>);

        let _: String = lua
            .load(
                r#"
                return cru.kiln.create_note({
                    path = "a.md", body = "x", overwrite = true
                })
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        let call = stub.last_create.lock().unwrap().clone().unwrap();
        assert!(call.overwrite);
    }

    #[tokio::test]
    async fn create_note_surfaces_api_error_as_lua_error() {
        let stub = Arc::new(StubVaultApi::new().with_create_result(Err("file exists".to_string())));
        let lua = make_lua_with_api(stub as Arc<dyn DaemonVaultApi>);

        let err = lua
            .load(
                r#"
                return cru.kiln.create_note({ path = "a.md", body = "x" })
                "#,
            )
            .eval_async::<Value>()
            .await
            .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("file exists"), "got: {msg}");
    }

    #[tokio::test]
    async fn create_note_requires_path_and_body() {
        let stub = Arc::new(StubVaultApi::new());
        let lua = make_lua_with_api(stub as Arc<dyn DaemonVaultApi>);

        // missing path
        let err = lua
            .load(r#"return cru.kiln.create_note({ body = "x" })"#)
            .eval_async::<Value>()
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("'path'"));

        // missing body
        let err = lua
            .load(r#"return cru.kiln.create_note({ path = "a.md" })"#)
            .eval_async::<Value>()
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("'body'"));
    }

    #[tokio::test]
    async fn create_note_stub_errors_when_no_daemon_connected() {
        // Just register the bare stub (no upgrade). Calling create_note
        // should error rather than silently no-op.
        let lua = Lua::new();
        register_vault_module(&lua).unwrap();

        let err = lua
            .load(r#"return cru.kiln.create_note({ path = "a.md", body = "x" })"#)
            .eval_async::<Value>()
            .await
            .unwrap_err();
        assert!(
            format!("{err}").contains("no daemon connected"),
            "got: {err}"
        );
    }

    // -------- search --------

    #[tokio::test]
    async fn search_returns_empty_when_no_match() {
        let stub = Arc::new(StubVaultApi::new()); // default empty
        let lua = make_lua_with_api(stub as Arc<dyn DaemonVaultApi>);

        let table: Table = lua
            .load(r#"return cru.kiln.search("anything", { limit = 5 })"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(table.len().unwrap(), 0);
    }

    #[tokio::test]
    async fn search_returns_results_ordered_by_score_desc() {
        // The api is expected to return results pre-sorted desc by score;
        // we verify the Lua surface preserves that order.
        let hits = vec![
            SearchHit {
                path: "a.md".into(),
                title: "A".into(),
                score: 0.95,
                snippet: Some("snip a".into()),
            },
            SearchHit {
                path: "b.md".into(),
                title: "B".into(),
                score: 0.80,
                snippet: Some("snip b".into()),
            },
            SearchHit {
                path: "c.md".into(),
                title: "C".into(),
                score: 0.60,
                snippet: None,
            },
        ];
        let stub = Arc::new(StubVaultApi::new().with_search_hits(hits));
        let lua = make_lua_with_api(stub as Arc<dyn DaemonVaultApi>);

        let table: Table = lua
            .load(r#"return cru.kiln.search("q", { limit = 10 })"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(table.len().unwrap(), 3);
        let first: Table = table.get(1).unwrap();
        let second: Table = table.get(2).unwrap();
        let third: Table = table.get(3).unwrap();
        assert_eq!(first.get::<String>("path").unwrap(), "a.md");
        assert_eq!(first.get::<String>("title").unwrap(), "A");
        assert!((first.get::<f64>("score").unwrap() - 0.95).abs() < 1e-6);
        assert_eq!(first.get::<String>("snippet").unwrap(), "snip a");
        assert_eq!(second.get::<String>("path").unwrap(), "b.md");
        assert_eq!(third.get::<String>("path").unwrap(), "c.md");
        // snippet was None — should be nil in Lua
        let snip: Value = third.get("snippet").unwrap();
        assert!(matches!(snip, Value::Nil));
    }

    #[tokio::test]
    async fn search_forwards_limit_and_threshold_to_api() {
        let stub = Arc::new(StubVaultApi::new());
        let lua = make_lua_with_api(stub.clone() as Arc<dyn DaemonVaultApi>);

        let _: Table = lua
            .load(r#"return cru.kiln.search("q", { limit = 3, threshold = 0.6 })"#)
            .eval_async()
            .await
            .unwrap();

        let call = stub.last_search.lock().unwrap().clone().unwrap();
        assert_eq!(call.query, "q");
        assert_eq!(call.limit, 3);
        assert!((call.threshold - 0.6).abs() < 1e-6);
    }

    #[tokio::test]
    async fn search_uses_default_limit_when_opts_omitted() {
        let stub = Arc::new(StubVaultApi::new());
        let lua = make_lua_with_api(stub.clone() as Arc<dyn DaemonVaultApi>);

        let _: Table = lua
            .load(r#"return cru.kiln.search("q")"#)
            .eval_async()
            .await
            .unwrap();

        let call = stub.last_search.lock().unwrap().clone().unwrap();
        assert_eq!(call.limit, DEFAULT_SEARCH_LIMIT);
        assert_eq!(call.threshold, 0.0);
    }

    #[tokio::test]
    async fn search_returns_empty_table_when_called_from_runtime_without_api() {
        // Without an upgrade, the bare stub returns an empty table. This is
        // the documented "no daemon connected" behaviour for search; only
        // create_note returns an error in that state (it has nothing
        // sensible to return).
        let lua = Lua::new();
        register_vault_module(&lua).unwrap();

        let table: Table = lua
            .load(r#"return cru.kiln.search("q", { limit = 5 })"#)
            .eval_async()
            .await
            .unwrap();
        assert_eq!(table.len().unwrap(), 0);
    }

    #[tokio::test]
    async fn search_surfaces_api_error_as_lua_error() {
        let stub = Arc::new(StubVaultApi::new());
        *stub.search_result.lock().unwrap() = Err("embed failed".to_string());
        let lua = make_lua_with_api(stub as Arc<dyn DaemonVaultApi>);

        let err = lua
            .load(r#"return cru.kiln.search("q")"#)
            .eval_async::<Value>()
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("embed failed"), "got: {err}");
    }
}
