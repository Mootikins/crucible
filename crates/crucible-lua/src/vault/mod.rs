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
//! -- Dense block search over a NAMED kiln (async): the vector comes from
//! -- `cru.embed`, and each hit names a passage. The stub answers an empty
//! -- table; the daemon binds it through `register_kiln_repository_resolver`.
//! local hits = cru.kiln.search(kiln, cru.embed(kiln, "machine learning"), 5)
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
//!
//! -- The stored blocks of one note in a NAMED kiln, in span order, each
//! -- with its vector when it has one (async). A retrieval strategy reads
//! -- a hit's neighbours here. The stub answers an empty table; the daemon
//! -- binds it through `register_kiln_repository_resolver`.
//! local blocks = cru.kiln.blocks(kiln, "path/to/note.md")
//!
//! -- The graph of a NAMED kiln, through the same resolver (async): one
//! -- note record, every note record, and the resolved links of one note
//! -- in both directions. A strategy builds an adjacency from `notes` and
//! -- `links`.
//! local note = cru.kiln.note(kiln, "path/to/note.md")
//! local all = cru.kiln.notes(kiln, 500)
//! local links = cru.kiln.links(kiln, "path/to/note.md")
//! for _, target in ipairs(links.outlinks) do print(target) end
//! ```
//!
//! All three path-only graph functions read the daemon's resolved-link
//! index and are filtered by the same authority as `list`/`get`, so a
//! plugin bound to kiln A never sees a path from kiln B. The named reads go
//! through a repository the host already bound to the named kiln, and that
//! repository applies the same authority.

use crate::error::LuaError;
use crucible_core::storage::{
    scoped_backlinks, scoped_outlinks, visible_paths, NoteStore, Scope, StorageError, StorageResult,
};
use futures_util::future::BoxFuture;
use mlua::{Lua, LuaSerdeExt, Table, Value};
use std::collections::{HashMap, VecDeque};
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

/// Maps a kiln NAME to the repository that holds its blocks.
///
/// The same seam as [`KilnPathResolver`], one level up: the host owns the
/// name-to-repository map, and Lua names a kiln, never a directory or a
/// database. The error string reaches Lua as-is, so it must name the kiln.
///
/// The lookup is async: the daemon holds its open kilns behind an async lock
/// and opens a named kiln on first use.
pub type KilnRepositoryResolver = Arc<
    dyn Fn(
            &str,
        ) -> BoxFuture<
            'static,
            Result<Arc<dyn crucible_core::traits::KnowledgeRepository>, String>,
        > + Send
        + Sync,
>;

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

    /// One block hit of a dense search, as [`super::hit_to_lua`] builds it.
    /// The kiln is not on the row: the caller named it.
    pub const HIT: &str = "{ path: string, span_start: number, span_end: number, \
                            kind: string, score: number }";

    /// A dense block search over a NAMED kiln. The vector comes from
    /// `cru.embed`, so its dimension is the caller's to get right. The stub
    /// answers an empty table.
    pub fn search() -> String {
        format!("(kiln: string, vector: {{ number }}, limit: number) -> {{ {HIT} }}")
    }

    /// The named graph reads: the kiln first, never a path to it.
    pub fn note() -> String {
        format!("(kiln: string, path: string) -> {NOTE}?")
    }

    pub fn notes() -> String {
        format!("(kiln: string, limit: number?) -> {{ {NOTE} }}")
    }

    /// Resolved paths in both directions. Dangling targets are absent.
    pub const NOTE_LINKS: &str = "{ outlinks: { string }, backlinks: { string } }";

    pub fn links() -> String {
        format!("(kiln: string, path: string) -> {NOTE_LINKS}")
    }

    /// The three graph functions all answer with note paths.
    pub const OUTLINKS: &str = "(path: string) -> { string }";
    pub const BACKLINKS: &str = "(path: string) -> { string }";

    /// `depth` counts hops and defaults to 1 — direct links only.
    pub const NEIGHBORS: &str = "(path: string, depth: number?) -> { string }";

    /// Every neighbour within `depth`, each with the hop count at which the
    /// walk first reached it.
    ///
    /// `NEIGHBORS` answers "within N hops" and says nothing about WHICH hop,
    /// so a caller that wants rings calls it once per depth and pays a full
    /// scan each time. The walk already computes the number; this returns it.
    pub const NEIGHBORS_WITH_HOPS: &str =
        "(path: string, depth: number?) -> { { path: string, hops: number } }";

    /// One stored block, as [`super::block_record_to_lua`] builds it. The
    /// vector is absent when the block fell under the word floor.
    pub const BLOCK: &str =
        "{ span_start: number, span_end: number, kind: string, vector: { number }? }";

    /// The kiln is named, not implied: a rerank stage sees hits from every
    /// attached kiln, and the host maps the name to a repository.
    pub fn blocks() -> String {
        format!("(kiln: string, path: string) -> {{ {BLOCK} }}")
    }

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

/// The `cru.kiln` members the host binds through a resolver, which a storage
/// upgrade must carry over rather than replace with the stubs.
const HOST_BOUND: &[&str] = &["blocks", "note", "notes", "links", "search", "path"];

/// Register the named reads — `cru.kiln.blocks`, `note`, `notes`, `links`
/// and `search` — against a repository resolver.
///
/// Registration replaces the empty stubs `register_vault_module` installs.
/// Every function takes the kiln NAME first; the resolver maps it to a
/// repository the host already bound to that kiln. `note`, `notes` and
/// `links` go through that repository's read scope; `blocks` and `search`
/// read the kiln's block table, which holds only that kiln's rows.
pub fn register_kiln_repository_resolver(
    lua: &Lua,
    resolver: KilnRepositoryResolver,
) -> Result<(), LuaError> {
    let cru: Table = lua.globals().get("cru")?;
    let vault: Table = cru.get("kiln")?;
    let mut kiln = crate::host_registry::Ns::over(lua, "cru.kiln", vault);

    let r = Arc::clone(&resolver);
    kiln.async_func(
        "blocks",
        &decl::blocks(),
        move |lua, (name, path): (String, String)| {
            let r = Arc::clone(&r);
            async move {
                let repo = r(&name).await.map_err(mlua::Error::runtime)?;
                let blocks = repo.blocks_for_note(&path).await.map_err(repo_error)?;
                let table = lua.create_table()?;
                for (i, block) in blocks.iter().enumerate() {
                    table.set(i + 1, block_record_to_lua(&lua, block)?)?;
                }
                Ok(Value::Table(table))
            }
        },
    )?;

    let r = Arc::clone(&resolver);
    kiln.async_func(
        "note",
        &decl::note(),
        move |lua, (name, path): (String, String)| {
            let r = Arc::clone(&r);
            async move {
                let repo = r(&name).await.map_err(mlua::Error::runtime)?;
                match repo.get_note_by_path(&path).await.map_err(repo_error)? {
                    Some(record) => note_record_to_lua(&lua, &record),
                    None => Ok(Value::Nil),
                }
            }
        },
    )?;

    let r = Arc::clone(&resolver);
    kiln.async_func(
        "notes",
        &decl::notes(),
        move |lua, (name, limit): (String, Option<usize>)| {
            let r = Arc::clone(&r);
            async move {
                let repo = r(&name).await.map_err(mlua::Error::runtime)?;
                let records = repo.list_note_records().await.map_err(repo_error)?;
                let table = lua.create_table()?;
                let limit = limit.unwrap_or(records.len());
                for (i, record) in records.iter().take(limit).enumerate() {
                    table.set(i + 1, note_record_to_lua(&lua, record)?)?;
                }
                Ok(Value::Table(table))
            }
        },
    )?;

    let r = Arc::clone(&resolver);
    kiln.async_func(
        "links",
        &decl::links(),
        move |lua, (name, path): (String, String)| {
            let r = Arc::clone(&r);
            async move {
                let repo = r(&name).await.map_err(mlua::Error::runtime)?;
                let links = repo.links_for_note(&path).await.map_err(repo_error)?;
                let table = lua.create_table()?;
                table.set("outlinks", string_vec_to_lua_table(&lua, &links.outlinks)?)?;
                table.set(
                    "backlinks",
                    string_vec_to_lua_table(&lua, &links.backlinks)?,
                )?;
                Ok(Value::Table(table))
            }
        },
    )?;

    kiln.async_func(
        "search",
        &decl::search(),
        move |lua, (name, vector, limit): (String, Vec<f32>, usize)| {
            let r = Arc::clone(&resolver);
            async move {
                let repo = r(&name).await.map_err(mlua::Error::runtime)?;
                let hits = repo
                    .search_blocks(vector, limit)
                    .await
                    .map_err(repo_error)?;
                let table = lua.create_table()?;
                // A hit without a block names a whole note, which this row
                // shape cannot carry. The block store answers only block hits,
                // so the filter is a type guard, not a policy.
                for (i, hit) in hits.iter().filter(|h| h.block.is_some()).enumerate() {
                    table.set(i + 1, hit_to_lua(&lua, hit)?)?;
                }
                Ok(Value::Table(table))
            }
        },
    )?;
    Ok(())
}

fn repo_error(e: crucible_core::CrucibleError) -> mlua::Error {
    mlua::Error::runtime(format!("Kiln error: {e}"))
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

    // The named reads are async like their resolver-backed replacements, for
    // the reason the graph stubs below give.
    kiln.async_func(
        "search",
        &decl::search(),
        |lua, (_kiln, _vector, _limit): (String, Vec<f32>, usize)| async move {
            let table = lua.create_table()?;
            Ok(Value::Table(table))
        },
    )?;

    kiln.async_func(
        "note",
        &decl::note(),
        |_, (_kiln, _path): (String, String)| async move { Ok(Value::Nil) },
    )?;

    kiln.async_func(
        "notes",
        &decl::notes(),
        |lua, (_kiln, _limit): (String, Option<usize>)| async move {
            let table = lua.create_table()?;
            Ok(Value::Table(table))
        },
    )?;

    kiln.async_func(
        "links",
        &decl::links(),
        |lua, (_kiln, _path): (String, String)| async move {
            let table = lua.create_table()?;
            table.set("outlinks", lua.create_table()?)?;
            table.set("backlinks", lua.create_table()?)?;
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

    // The stub carries the declaration the Luau checker reads, so a function
    // registered only on the store-backed path is a type error in every
    // plugin that calls it. `every_shipped_plugin_typechecks` caught exactly
    // that when `neighbors_with_hops` landed here late.
    kiln.async_func(
        "neighbors_with_hops",
        decl::NEIGHBORS_WITH_HOPS,
        |lua, (_path, _depth): (String, Option<usize>)| async move {
            let table = lua.create_table()?;
            Ok(Value::Table(table))
        },
    )?;

    // Async like its resolver-backed replacement, for the same reason as the
    // graph stubs above.
    kiln.async_func(
        "blocks",
        &decl::blocks(),
        |lua, (_kiln, _path): (String, String)| async move {
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
    // The host binds the named reads and `cru.kiln.path` at boot through
    // resolvers this crate cannot rebuild, and `register_vault_module`
    // publishes a fresh table. Carry those bindings over: without this,
    // every kiln open put the stubs back, and a plugin read empty blocks
    // after the first `cru process`.
    let globals = lua.globals();
    let host_bound: Vec<(&str, Value)> = match globals
        .get::<Table>("cru")
        .and_then(|cru| cru.get::<Table>("kiln"))
    {
        Ok(previous) => HOST_BOUND
            .iter()
            .copied()
            .filter_map(|name| previous.get::<Value>(name).ok().map(|v| (name, v)))
            .filter(|(_, v)| v.is_function())
            .collect(),
        Err(_) => Vec::new(),
    };

    register_vault_module(lua)?;

    let cru: Table = globals.get("cru")?;
    let vault: Table = cru.get("kiln")?;
    for (name, value) in host_bound {
        vault.set(name, value)?;
    }
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

    // `search` is not bound here: it names a kiln, and the host binds it
    // through `register_kiln_repository_resolver`, carried over above.

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

    let store_for_hops = Arc::clone(&store);
    let authority_for_hops = authority.clone();
    kiln.async_func(
        "neighbors",
        decl::NEIGHBORS,
        move |lua, (path, depth): (String, Option<usize>)| {
            let s = Arc::clone(&store);
            let auth = authority.clone();
            async move {
                let reached = scoped_neighbors(s.as_ref(), &auth, &path, depth.unwrap_or(1))
                    .await
                    .map_err(kiln_error)?;
                // Paths only, SORTED BY PATH. This signature is what plugins
                // already call, so the hop counts go through
                // `neighbors_with_hops` beside it rather than changing this
                // return shape — and the order is part of that shape.
                //
                // `scoped_neighbors` sorts hop-major now, for the rings its
                // other caller draws. Re-sorting here is what keeps this
                // answer byte-identical to the one it gave before.
                let mut paths: Vec<String> = reached.into_iter().map(|(p, _)| p).collect();
                paths.sort();
                string_vec_to_lua_table(&lua, &paths)
            }
        },
    )?;

    kiln.async_func(
        "neighbors_with_hops",
        decl::NEIGHBORS_WITH_HOPS,
        move |lua, (path, depth): (String, Option<usize>)| {
            let s = Arc::clone(&store_for_hops);
            let auth = authority_for_hops.clone();
            async move {
                let reached = scoped_neighbors(s.as_ref(), &auth, &path, depth.unwrap_or(1))
                    .await
                    .map_err(kiln_error)?;
                let out = lua.create_table()?;
                for (index, (p, hops)) in reached.into_iter().enumerate() {
                    let row = lua.create_table()?;
                    row.set("path", p)?;
                    row.set("hops", hops)?;
                    out.set(index + 1, row)?;
                }
                Ok(Value::Table(out))
            }
        },
    )?;

    Ok(())
}

// ============================================================================
// Graph traversal over NoteStore
// ============================================================================

// `visible_paths`, `scoped_outlinks` and `scoped_backlinks` live in
// `crucible_core::storage::scoped_links`, shared with the knowledge
// repository that answers the named reads.

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
) -> StorageResult<Vec<(String, usize)>> {
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

    // The walk always knew the hop count; it used to discard it here. A caller
    // that wants rings otherwise calls this once per depth, and each call
    // re-reads the whole note list and the whole link table.
    //
    // BFS reaches a node first at its shortest hop count, and a node is
    // enqueued at most once, so the recorded number is the distance.
    let mut reached: HashMap<String, usize> = HashMap::from([(path.to_string(), 0)]);
    let mut queue: VecDeque<(String, usize)> = VecDeque::from([(path.to_string(), 0)]);
    while let Some((current, hops)) = queue.pop_front() {
        if hops >= depth {
            continue;
        }
        for neighbor in adjacency.get(&current).into_iter().flatten() {
            if !reached.contains_key(neighbor) {
                reached.insert(neighbor.clone(), hops + 1);
                queue.push_back((neighbor.clone(), hops + 1));
            }
        }
    }

    reached.remove(path);
    let mut pairs: Vec<(String, usize)> = reached.into_iter().collect();
    // Sorted by hop, then by path: the ring order a caller draws, and stable
    // so plugin output stays reproducible.
    pairs.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    Ok(pairs)
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

/// One block as Lua sees it: the span, the kind, and the vector when the
/// block has one. The text and the hash stay behind — a strategy scores
/// vectors, and the snippet already rides on the hit.
fn block_record_to_lua(
    lua: &Lua,
    block: &crucible_core::storage::BlockRecord,
) -> Result<Value, mlua::Error> {
    let table = lua.create_table()?;
    table.set("span_start", block.span_start)?;
    table.set("span_end", block.span_end)?;
    table.set("kind", block.kind.as_str())?;
    if let Some(vector) = &block.embedding {
        table.set("vector", vector.as_slice())?;
    }
    Ok(Value::Table(table))
}

/// One block hit as Lua sees it. The snippet stays behind: a strategy that
/// wants the text reads `cru.kiln.blocks`, and the hit already names the
/// span.
fn hit_to_lua(lua: &Lua, hit: &crucible_core::types::SearchResult) -> Result<Value, mlua::Error> {
    let table = lua.create_table()?;
    table.set("path", hit.document_id.0.as_str())?;
    if let Some(block) = &hit.block {
        table.set("span_start", block.span_start)?;
        table.set("span_end", block.span_end)?;
        table.set("kind", block.kind.as_str())?;
    }
    table.set("score", hit.score)?;
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
