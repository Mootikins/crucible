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
//! -- Search notes (async). BOTH bodies answer with an empty table today:
//! -- the stub and the store-backed one build a table and never fill it, so
//! -- the scored `{ path, title, score, snippet }` rows this once advertised
//! -- do not exist. The declaration says `{ any }` for that reason.
//! local results = cru.kiln.search("machine learning", {limit = 5, threshold = 0.6})
//!
//! -- There is no `cru.kiln.create_note`. This block used to document one,
//! -- with a frontmatter table and an `overwrite` flag; nothing in `crates/`
//! -- has ever registered it. A plugin author following the example got a
//! -- nil-index error. Writing a note is the `create_note` TOOL, through the
//! -- agent surface.
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
use crucible_core::storage::{NoteStore, Scope, StorageError, StorageResult};
use mlua::{Lua, LuaSerdeExt, Table, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// Maps a kiln NAME to its root directory.
///
/// The host injects it — the daemon passes a registry-backed closure — so
/// this crate never learns where kilns live. The error string reaches Lua
/// as-is: it must name the kiln, never a directory.
///
/// The kiln is the seam that owns name-to-path resolution. `cru.kiln.path`
/// is the one Lua surface that crosses it.
pub type KilnPathResolver = Arc<dyn Fn(&str) -> Result<PathBuf, String> + Send + Sync>;

/// The message a VM with no resolver answers `cru.kiln.path` with.
const NO_RESOLVER: &str = "cru.kiln.path is not available in this runtime";

/// The Luau type of every `cru.kiln.*` function, in one place.
///
/// Each name is registered TWICE — once as a stub by
/// [`register_vault_module`], once store-backed by
/// [`register_vault_module_with_store_scoped`] — and both registrations read
/// these, so the two bodies of one name cannot describe different functions.
/// [`crate::host_registry::Ns`] holds each one to its closure's Rust types.
mod decl {
    /// The note record [`super::note_record_to_lua`] builds, field for field.
    /// Nothing else reaches Lua from a `NoteRecord`.
    pub const NOTE: &str = "{ path: string, title: string, content_hash: string, \
                             tags: { string }, links_to: { string }, \
                             properties: { [string]: any }, updated_at: string, \
                             has_embedding: boolean }";

    pub fn list() -> String {
        format!("(limit: number?) -> {{ {NOTE} }}")
    }

    /// `nil` when the kiln holds no note at `path`, and when no store is
    /// bound at all.
    pub fn get() -> String {
        format!("(path: string) -> {NOTE}?")
    }

    /// The element type is `any`, not a result record: the closure builds an
    /// EMPTY table and never fills it, in both registrations. See the comment
    /// in `register_vault_module_with_store_scoped`.
    pub const SEARCH: &str =
        "(query: string, options: { limit: number?, threshold: number? }?) -> { any }";

    /// The three graph functions all answer with note paths.
    pub const OUTLINKS: &str = "(path: string) -> { string }";
    pub const BACKLINKS: &str = "(path: string) -> { string }";

    /// `depth` counts hops and defaults to 1 — direct links only.
    pub const NEIGHBORS: &str = "(path: string, depth: number?) -> { string }";

    /// The name is REQUIRED and the relative part is not: `kiln_path` raises
    /// without a name and joins the relative part when it gets one. Declaring
    /// it `(name: string?)` rejected `cru.kiln.path(kiln, ".crucible/proposals")`,
    /// which is what the `reflection` plugin really calls.
    ///
    /// It RAISES rather than answering nil — both when no resolver is bound
    /// and when a name does not resolve — so the return carries no `?`.
    pub const PATH: &str = "(name: string, relative: string?) -> string";
}

/// Join `relative` onto `root`, refusing anything that is not a plain
/// relative component.
///
/// This is a bug lint, not a boundary: a plugin builds the path from parts it
/// already knows, and `..` there is a mistake to report, not an attack to
/// contain.
fn join_relative(root: &Path, name: &str, relative: &str) -> Result<PathBuf, LuaError> {
    let mut joined = root.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => joined.push(part),
            _ => {
                return Err(LuaError::Runtime(format!(
                    "cru.kiln.path('{name}', '{relative}'): the relative part must be \
                     plain components — no '..', '.' or absolute part"
                )))
            }
        }
    }
    Ok(joined)
}

/// Resolve one kiln name to its canonical root, then join `relative`.
fn kiln_path(
    resolver: &KilnPathResolver,
    name: &str,
    relative: Option<&str>,
) -> Result<PathBuf, LuaError> {
    if name.is_empty() {
        return Err(LuaError::Runtime(
            "cru.kiln.path needs a kiln name".to_string(),
        ));
    }
    let root = resolver(name).map_err(LuaError::Runtime)?;
    let canonical = std::fs::canonicalize(&root)
        .map_err(|e| LuaError::Runtime(format!("kiln '{name}' is not reachable: {e}")))?;
    join_relative(&canonical, name, relative.unwrap_or(""))
}

/// Register `cru.kiln.path(name, relative?)` against a resolver.
///
/// Registration replaces the raising stub `register_vault_module` installs,
/// so a host upgrades once it can map a name to a directory.
pub fn register_kiln_path_resolver(lua: &Lua, resolver: KilnPathResolver) -> Result<(), LuaError> {
    let cru: Table = lua.globals().get("cru")?;
    let vault: Table = cru.get("kiln")?;
    let mut kiln = crate::host_registry::Ns::over(lua, "cru.kiln", vault);
    kiln.func(
        "path",
        decl::PATH,
        move |_lua, (name, relative): (String, Option<String>)| {
            let path = kiln_path(&resolver, &name, relative.as_deref())?;
            Ok(path.to_string_lossy().into_owned())
        },
    )?;
    Ok(())
}

/// Register the kiln module with a Lua state
///
/// This creates the `cru.kiln` namespace with stub functions.
/// Use `register_vault_module_with_store` to add database-backed functionality.
pub fn register_vault_module(lua: &Lua) -> Result<(), LuaError> {
    let mut kiln = crate::host_registry::Ns::new(lua, "cru.kiln")?;

    kiln.async_func(
        "list",
        &decl::list(),
        |lua, _limit: Option<usize>| async move {
            let table = lua.create_table()?;
            Ok(Value::Table(table))
        },
    )?;

    kiln.async_func("get", &decl::get(), |_, _path: String| async move {
        Ok(Value::Nil)
    })?;

    kiln.async_func(
        "search",
        decl::SEARCH,
        |lua, (_query, _opts): (String, Option<Table>)| async move {
            let table = lua.create_table()?;
            Ok(Value::Table(table))
        },
    )?;

    // The graph stubs are async like their store-backed replacements, not
    // because they await anything but so a script written against the stub
    // keeps working after `register_vault_module_with_store_scoped` swaps
    // them: a sync-to-async swap would turn every call site into a
    // "yield from outside a coroutine" error the moment a kiln opened.
    kiln.async_func(
        "outlinks",
        decl::OUTLINKS,
        |lua, _path: String| async move {
            let table = lua.create_table()?;
            Ok(Value::Table(table))
        },
    )?;

    kiln.async_func(
        "backlinks",
        decl::BACKLINKS,
        |lua, _path: String| async move {
            let table = lua.create_table()?;
            Ok(Value::Table(table))
        },
    )?;

    kiln.async_func(
        "neighbors",
        decl::NEIGHBORS,
        |lua, (_path, _depth): (String, Option<usize>)| async move {
            let table = lua.create_table()?;
            Ok(Value::Table(table))
        },
    )?;

    // `cru.kiln.path` exists on every VM so a plugin sees one answer, not a
    // nil call, when the host cannot resolve names.
    kiln.func(
        "path",
        decl::PATH,
        |_lua, (_name, _relative): (String, Option<String>)| {
            Err::<String, _>(LuaError::Runtime(NO_RESOLVER.to_string()).into())
        },
    )?;

    kiln.publish()?;

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
    let mut kiln = crate::host_registry::Ns::over(lua, "cru.kiln", vault);

    let s = Arc::clone(&store);
    let auth = authority.clone();
    kiln.async_func("list", &decl::list(), move |lua, limit: Option<usize>| {
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

    let s = Arc::clone(&store);
    let auth = authority.clone();
    kiln.async_func("get", &decl::get(), move |lua, path: String| {
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

    // `search` is intentionally not implemented here — the bare stub from
    // `register_vault_module` (which returns an empty table) is the
    // production-shipping behaviour. Implementing semantic search would
    // require an embedding provider, which lives daemon-side. `decl::SEARCH`
    // says `{ any }` for that reason: no result record is ever built.

    let s = Arc::clone(&store);
    let auth = authority.clone();
    kiln.async_func("outlinks", decl::OUTLINKS, move |lua, path: String| {
        let s = Arc::clone(&s);
        let auth = auth.clone();
        async move {
            let paths = scoped_outlinks(s.as_ref(), &auth, &path)
                .await
                .map_err(kiln_error)?;
            string_vec_to_lua_table(&lua, &paths)
        }
    })?;

    let s = Arc::clone(&store);
    let auth = authority.clone();
    kiln.async_func("backlinks", decl::BACKLINKS, move |lua, path: String| {
        let s = Arc::clone(&s);
        let auth = auth.clone();
        async move {
            let paths = scoped_backlinks(s.as_ref(), &auth, &path)
                .await
                .map_err(kiln_error)?;
            string_vec_to_lua_table(&lua, &paths)
        }
    })?;

    kiln.async_func(
        "neighbors",
        decl::NEIGHBORS,
        move |lua, (path, depth): (String, Option<usize>)| {
            let s = Arc::clone(&store);
            let auth = authority.clone();
            async move {
                let paths = scoped_neighbors(s.as_ref(), &auth, &path, depth.unwrap_or(1))
                    .await
                    .map_err(kiln_error)?;
                string_vec_to_lua_table(&lua, &paths)
            }
        },
    )?;

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
