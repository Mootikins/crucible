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
//! -- Get outgoing links from a note (async) - resolved note paths
//! local links = cru.kiln.outlinks("path/to/note.md")
//!
//! -- Get incoming links to a note (async) - notes linking here
//! local backlinks = cru.kiln.backlinks("path/to/note.md")
//!
//! -- Get neighbors within depth (async) - undirected walk, depth 1 = direct
//! local nearby = cru.kiln.neighbors("path/to/note.md", 2)
//! ```
//!
//! All three graph functions read the daemon's resolved-link index and are
//! filtered by the same authority as `list`/`get`, so a plugin bound to
//! kiln A never sees a path from kiln B.

use crate::error::LuaError;
use crate::lua_util::register_in_namespaces;
use crucible_core::storage::{NoteStore, Scope, StorageError, StorageResult};
use mlua::{Lua, LuaSerdeExt, Table, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

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

    // The graph stubs are async like their store-backed replacements, not
    // because they await anything but so a script written against the stub
    // keeps working after `register_vault_module_with_store_scoped` swaps
    // them: a sync-to-async swap would turn every call site into a
    // "yield from outside a coroutine" error the moment a kiln opened.
    let outlinks_stub = lua.create_async_function(|lua, _path: String| async move {
        let table = lua.create_table()?;
        Ok(Value::Table(table))
    })?;
    vault.set("outlinks", outlinks_stub)?;

    let backlinks_stub = lua.create_async_function(|lua, _path: String| async move {
        let table = lua.create_table()?;
        Ok(Value::Table(table))
    })?;
    vault.set("backlinks", backlinks_stub)?;

    let neighbors_stub =
        lua.create_async_function(|lua, (_path, _depth): (String, Option<usize>)| async move {
            let table = lua.create_table()?;
            Ok(Value::Table(table))
        })?;
    vault.set("neighbors", neighbors_stub)?;

    register_in_namespaces(lua, "kiln", vault)?;

    Ok(())
}

/// Register the kiln module with NoteStore for database-backed queries.
///
/// Test/convenience wrapper: passes an unbound workspace authority, which
/// matches no stamped scope, so only notes with no `scope:` property at all
/// are visible through it. Production daemon callers must use the `_scoped`
/// variant with the kiln workspace path.
pub fn register_vault_module_with_store(
    lua: &Lua,
    store: Arc<dyn NoteStore>,
) -> Result<(), LuaError> {
    register_vault_module_with_store_scoped(
        lua,
        store,
        Scope::workspace_unchecked(std::path::PathBuf::new()),
    )
}

/// Scoped variant: every `cru.kiln.list` and `cru.kiln.get` call is
/// filtered by the given authority. The daemon wires this with
/// `Scope::Workspace { path: kiln_path }` so a plugin running in kiln A
/// cannot read kiln B's notes — even with adversarial path arguments.
pub fn register_vault_module_with_store_scoped(
    lua: &Lua,
    store: Arc<dyn NoteStore>,
    authority: Scope,
) -> Result<(), LuaError> {
    register_vault_module(lua)?;

    let globals = lua.globals();
    let cru: Table = globals.get("cru")?;
    let vault: Table = cru.get("kiln")?;

    let s = Arc::clone(&store);
    let auth = authority.clone();
    let list_fn = lua.create_async_function(move |lua, limit: Option<usize>| {
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
                Err(e) => Err(kiln_error(e)),
            }
        }
    })?;
    vault.set("list", list_fn)?;

    let s = Arc::clone(&store);
    let auth = authority.clone();
    let get_fn = lua.create_async_function(move |lua, path: String| {
        let s = Arc::clone(&s);
        let auth = auth.clone();
        async move {
            match s.get(&path, &auth).await {
                Ok(Some(record)) => note_record_to_lua(&lua, &record),
                Ok(None) => Ok(Value::Nil),
                Err(e) => Err(kiln_error(e)),
            }
        }
    })?;
    vault.set("get", get_fn)?;

    // `search` is intentionally not implemented here — the bare stub from
    // `register_vault_module` (which returns an empty table) is the
    // production-shipping behaviour. Implementing semantic search would
    // require an embedding provider, which lives daemon-side.

    let s = Arc::clone(&store);
    let auth = authority.clone();
    let outlinks_fn = lua.create_async_function(move |lua, path: String| {
        let s = Arc::clone(&s);
        let auth = auth.clone();
        async move {
            let paths = scoped_outlinks(s.as_ref(), &auth, &path)
                .await
                .map_err(kiln_error)?;
            string_vec_to_lua_table(&lua, &paths)
        }
    })?;
    vault.set("outlinks", outlinks_fn)?;

    let s = Arc::clone(&store);
    let auth = authority.clone();
    let backlinks_fn = lua.create_async_function(move |lua, path: String| {
        let s = Arc::clone(&s);
        let auth = auth.clone();
        async move {
            let paths = scoped_backlinks(s.as_ref(), &auth, &path)
                .await
                .map_err(kiln_error)?;
            string_vec_to_lua_table(&lua, &paths)
        }
    })?;
    vault.set("backlinks", backlinks_fn)?;

    let neighbors_fn =
        lua.create_async_function(move |lua, (path, depth): (String, Option<usize>)| {
            let s = Arc::clone(&store);
            let auth = authority.clone();
            async move {
                let paths = scoped_neighbors(s.as_ref(), &auth, &path, depth.unwrap_or(1))
                    .await
                    .map_err(kiln_error)?;
                string_vec_to_lua_table(&lua, &paths)
            }
        })?;
    vault.set("neighbors", neighbors_fn)?;

    Ok(())
}

// ============================================================================
// Graph traversal over NoteStore
// ============================================================================

/// The note paths `authority` is allowed to read.
///
/// Every path the three graph functions emit is gated through this set.
/// `NoteStore`'s link methods (`backlinks`, `graph_links`) take no authority
/// — they are raw projections of the daemon's resolved-link index, and the
/// trait leaves scope enforcement to the layer above it, unlike `list`/`get`
/// which filter in SQL. So the filter has to be applied *here*: without it
/// `cru.kiln.backlinks()` would hand a plugin bound to kiln A exactly the
/// paths `cru.kiln.list()` is careful to hide from it.
///
/// One scoped `list` rather than a `get` per candidate: `neighbors` needs the
/// whole visible set anyway, and one query beats a round trip per hop.
async fn visible_paths(store: &dyn NoteStore, authority: &Scope) -> StorageResult<HashSet<String>> {
    Ok(store
        .list(authority)
        .await?
        .into_iter()
        .map(|record| record.path)
        .collect())
}

/// Resolved outgoing links of `path`, filtered to what `authority` can read.
///
/// Reads `graph_links()` rather than `NoteRecord::links_to`, which the note
/// pipeline fills with *raw* wikilink targets (`"async"`, `"Async"`) and not
/// note paths. Raw targets do not join with what `backlinks()` returns, so
/// using them would stop `outlinks` and `backlinks` being inverses and a
/// caller could not walk the graph one hop at a time. Unresolved (dangling)
/// edges are dropped for the same reason: they name no note, so they can
/// neither be traversed further nor scope-checked. The `kiln.graph` RPC
/// remains the surface that reports dangling edges.
async fn scoped_outlinks(
    store: &dyn NoteStore,
    authority: &Scope,
    path: &str,
) -> StorageResult<Vec<String>> {
    let visible = visible_paths(store, authority).await?;
    if !visible.contains(path) {
        return Ok(Vec::new());
    }

    Ok(sorted_unique(
        store
            .graph_links()
            .await?
            .into_iter()
            .filter(|link| link.resolved && link.source == path && visible.contains(&link.target))
            .map(|link| link.target),
    ))
}

/// Notes whose links resolve to `path`, filtered to what `authority` can read.
async fn scoped_backlinks(
    store: &dyn NoteStore,
    authority: &Scope,
    path: &str,
) -> StorageResult<Vec<String>> {
    let visible = visible_paths(store, authority).await?;
    if !visible.contains(path) {
        return Ok(Vec::new());
    }

    Ok(sorted_unique(
        store
            .backlinks(path)
            .await?
            .into_iter()
            .filter(|source| visible.contains(source)),
    ))
}

/// Notes within `depth` hops of `path`, filtered to what `authority` can read.
///
/// `NoteStore` has no neighbor query, so this ports the BFS from
/// `GraphView::neighbors` (`crucible-core/src/storage/graph.rs`) rather than
/// delegating to something that only looks equivalent. The ported semantics,
/// each of which a caller can observe:
///
/// - `depth == 0` returns nothing; `depth == 1` is direct links only.
/// - The walk is **undirected** — every hop follows outlinks and backlinks
///   alike, so a note that merely links *to* the start counts as a neighbor.
/// - The start note is marked visited up front and removed at the end, so it
///   never appears in the result, not via a cycle and not via a self-link.
/// - A node is enqueued at most once, which is what makes cycles terminate.
///
/// One intentional difference: results are sorted. `GraphView` returned them
/// in `HashSet` order, i.e. unspecified, so no caller could have depended on
/// it, and a stable order makes plugin output reproducible.
async fn scoped_neighbors(
    store: &dyn NoteStore,
    authority: &Scope,
    path: &str,
    depth: usize,
) -> StorageResult<Vec<String>> {
    if depth == 0 {
        return Ok(Vec::new());
    }

    let visible = visible_paths(store, authority).await?;
    if !visible.contains(path) {
        return Ok(Vec::new());
    }

    // Undirected adjacency over the resolved, in-scope subgraph. Building it
    // once is what keeps the walk to a single `graph_links` read.
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    for link in store.graph_links().await? {
        if !link.resolved || !visible.contains(&link.source) || !visible.contains(&link.target) {
            continue;
        }
        adjacency
            .entry(link.source.clone())
            .or_default()
            .push(link.target.clone());
        adjacency.entry(link.target).or_default().push(link.source);
    }

    let mut visited: HashSet<String> = HashSet::from([path.to_string()]);
    let mut queue: VecDeque<(String, usize)> = VecDeque::from([(path.to_string(), 0)]);
    while let Some((current, hops)) = queue.pop_front() {
        if hops >= depth {
            continue;
        }
        for neighbor in adjacency.get(&current).into_iter().flatten() {
            if visited.insert(neighbor.clone()) {
                queue.push_back((neighbor.clone(), hops + 1));
            }
        }
    }

    visited.remove(path);
    Ok(sorted_unique(visited))
}

/// Deterministic order for a Lua-visible array of paths.
fn sorted_unique(paths: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut paths: Vec<String> = paths.into_iter().collect();
    paths.sort();
    paths.dedup();
    paths
}

fn kiln_error(e: StorageError) -> mlua::Error {
    mlua::Error::runtime(format!("Kiln error: {e}"))
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
mod tests;
