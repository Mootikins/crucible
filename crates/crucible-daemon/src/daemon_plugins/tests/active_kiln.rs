//! `cru.kiln.active` — the one thing a plugin is told about the kiln it is
//! running against.
//!
//! Split out of `tests/mod.rs` to stay under the 1000-line module budget
//! enforced by `no_new_oversized_modules`.

use super::*;
use crate::storage::sqlite::{SqliteConfig, SqliteNoteStore, SqlitePool};
use crucible_core::parser::BlockHash;
use crucible_core::storage::NoteRecord;
use std::path::Path;

/// Path a `Scope::workspace_unchecked` authority is derived from in these
/// tests. Never touched on disk — `_unchecked` skips canonicalize.
const KILN: &str = "/kiln";

fn note(path: &str, links: &[&str]) -> NoteRecord {
    NoteRecord::new(path, BlockHash::zero())
        .with_title(path.trim_end_matches(".md"))
        .with_links(links.iter().map(|s| (*s).to_string()).collect())
}

/// `a.md -> b.md -> c.md`, inserted target-first so every wikilink resolves on
/// write rather than relying on the re-resolution pass.
async fn chain_store() -> Arc<dyn NoteStore> {
    let pool = SqlitePool::new(SqliteConfig::memory()).expect("pool");
    let store = SqliteNoteStore::new(pool);
    for record in [
        note("c.md", &[]),
        note("b.md", &["c.md"]),
        note("a.md", &["b.md"]),
    ] {
        store.upsert(record).await.expect("upsert");
    }
    Arc::new(store)
}

/// A plugin learns which kiln is active by NAME, and cannot learn where it
/// is. `cru.kiln.active_path` used to hand every loaded plugin the kiln's
/// absolute directory for a kiln it never named.
#[tokio::test]
async fn the_active_kiln_reaches_plugins_by_name_and_never_by_path() {
    let loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .upgrade_with_storage(
            chain_store().await,
            Path::new(KILN),
            Some(&crucible_core::config::KilnName::parse("work-notes").unwrap()),
        )
        .expect("upgrade with storage");
    let lua = loader.plugin_lua();

    let active: String = lua
        .load(r#"return tostring(cru.kiln.active)"#)
        .eval_async()
        .await
        .expect("eval");
    assert_eq!(active, "work-notes");

    let path: String = lua
        .load(r#"return tostring(cru.kiln.active_path)"#)
        .eval_async()
        .await
        .expect("eval");
    assert_eq!(path, "nil", "the kiln directory is gone, not renamed");
}

/// An unregistered kiln names nothing — and CLEARS whatever the previous
/// open named, so a handler is never told about a kiln it has moved off.
/// `nil`, not `""`: an empty string is truthy in Lua.
#[tokio::test]
async fn an_unnamed_kiln_clears_the_active_name_rather_than_emptying_it() {
    let loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .upgrade_with_storage(
            chain_store().await,
            Path::new(KILN),
            Some(&crucible_core::config::KilnName::parse("work-notes").unwrap()),
        )
        .expect("first upgrade");
    loader
        .upgrade_with_storage(chain_store().await, Path::new("/elsewhere"), None)
        .expect("second upgrade");

    let active: String = loader
        .plugin_lua()
        .load(r#"return type(cru.kiln.active) .. ":" .. tostring(cru.kiln.active)"#)
        .eval_async()
        .await
        .expect("eval");
    assert_eq!(active, "nil:nil");
}
