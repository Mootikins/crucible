//! Resolved links of one note, filtered to what an authority can read.
//!
//! `NoteStore`'s link methods (`backlinks`, `graph_links`) take no
//! authority: they are raw projections of the resolved-link index, and the
//! trait leaves scope enforcement to the layer above it, unlike `list` and
//! `get`, which filter in SQL. So the filter is applied here, once, for
//! every reader — the Lua `cru.kiln.*` graph functions and the knowledge
//! repository alike. Without it a plugin bound to kiln A would read exactly
//! the paths `list` hides from it.

use crate::storage::{NoteStore, Scope, StorageResult};
use std::collections::HashSet;

/// The note paths `authority` is allowed to read.
///
/// One scoped `list` rather than a `get` per candidate: a graph walk needs
/// the whole visible set, and one query beats a round trip per hop.
pub async fn visible_paths(
    store: &dyn NoteStore,
    authority: &Scope,
) -> StorageResult<HashSet<String>> {
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
/// pipeline fills with RAW wikilink targets (`"async"`, `"Async"`) and not
/// note paths. Raw targets do not join with what `backlinks()` returns, so
/// `outlinks` and `backlinks` would stop being inverses. Dangling edges are
/// dropped for the same reason: they name no note, so a caller can neither
/// follow them nor scope-check them. The `kiln.graph` RPC reports them.
pub async fn scoped_outlinks(
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
pub async fn scoped_backlinks(
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

/// Deterministic order for an array of paths a caller will show or compare.
pub fn sorted_unique(paths: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut paths: Vec<String> = paths.into_iter().collect();
    paths.sort();
    paths.dedup();
    paths
}
