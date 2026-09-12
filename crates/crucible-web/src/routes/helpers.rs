//! Shared helpers for route handlers.
//!
//! Centralises note-to-JSON mapping, path validation, and content-size
//! constants that were previously duplicated across `search.rs` and `kiln.rs`.

use std::path::{Path, PathBuf};

use crate::WebError;

/// Response for model listings — the session-scoped `list_models` and the
/// session-less `list_all_models` return the same `{ models: [...] }` shape.
#[derive(Debug, serde::Serialize)]
pub(crate) struct ModelsResponse {
    pub(crate) models: Vec<String>,
}

// =========================================================================
// Note mapping
// =========================================================================

/// Tuple returned by [`crate::services::daemon::DaemonService::list_notes`].
pub(crate) type NoteListItem = crucible_daemon::rpc_client::NoteListRow;

/// Map a note list item to full metadata JSON.
///
/// Produces: `{ name, path, title, tags, updated_at }`.
pub(crate) fn note_to_metadata_json(row: NoteListItem) -> serde_json::Value {
    serde_json::json!({
        "name": row.name,
        "path": row.path,
        "title": row.title,
        "tags": row.tags,
        "updated_at": row.updated_at,
        // The note's own frontmatter: what a client needs to filter, sort or
        // group notes without asking a plugin to do it. Filtered at the
        // boundary (`NoteInfo::from`), so no daemon stamp is in here.
        "properties": row.properties,
    })
}

/// Map a note list item to a file-entry JSON.
///
/// Produces: `{ name, path, is_dir: false }`.
pub(crate) fn note_to_file_json(row: NoteListItem) -> serde_json::Value {
    serde_json::json!({
        "name": row.name,
        "path": row.path,
        "is_dir": false,
    })
}

// =========================================================================
// Path / name validation
// =========================================================================

/// Validate that a note *name* is free of traversal sequences.
///
/// Rejects names containing `..`, starting with `/` or `\`, or containing
/// null bytes.  Returns [`WebError::Chat`] on failure (preserving the
/// existing HTTP-400 behaviour of the search routes).
pub(crate) fn validate_note_name(name: &str) -> Result<(), WebError> {
    if name.contains("..") || name.starts_with('/') || name.starts_with('\\') || name.contains('\0')
    {
        return Err(WebError::Chat(
            "Invalid note name: path traversal not allowed".to_string(),
        ));
    }
    Ok(())
}

/// Reject paths containing traversal sequences (`..`), null bytes, or absolute paths.
///
/// Returns [`WebError::Validation`] on failure (preserving the existing
/// HTTP-422 behaviour of the kiln routes).
/// Reject `..` traversal sequences and NUL bytes, but ALLOW absolute paths.
///
/// The kiln file routes address files by absolute path (a note's own path) and
/// enforce kiln containment separately (`find_enclosing_kiln` +
/// `validate_file_within_kiln` / `validate_parent_within_kiln`, which
/// canonicalize and check `starts_with` the open kiln). Banning absolute paths
/// here would reject every real editor request while adding no security.
pub(crate) fn reject_path_traversal(path: &str) -> Result<(), WebError> {
    if path.contains("..") || path.contains('\0') {
        return Err(WebError::Validation(
            "Invalid path: traversal not allowed".to_string(),
        ));
    }
    Ok(())
}

/// Canonicalize the parent directory and verify it resides within the kiln.
pub(crate) fn validate_parent_within_kiln(file_path: &Path, kiln: &Path) -> Result<(), WebError> {
    let canonical_file_parent = file_path
        .parent()
        .ok_or_else(|| WebError::Validation("Path has no parent directory".to_string()))?;

    let canonical_parent = canonical_file_parent.canonicalize().map_err(|_| {
        WebError::Validation("Parent directory does not exist or is not accessible".to_string())
    })?;

    if !canonical_parent.starts_with(kiln) {
        return Err(WebError::Validation(
            "Path escapes kiln directory".to_string(),
        ));
    }

    Ok(())
}

/// Validate a write target within the kiln.
///
/// Checks the parent (via [`validate_parent_within_kiln`], which resolves any
/// ancestor symlinks) AND the final path component: if the target already exists
/// as a symlink, its fully-resolved destination must also stay within the kiln.
/// Without the second check, `fs::write` follows a pre-planted symlink (e.g.
/// `KILN/notes/evil.md -> ~/.bashrc`) and writes OUTSIDE the kiln even though the
/// parent directory is legitimate. `kiln` must be the canonical kiln root.
pub(crate) fn validate_write_target_within_kiln(
    file_path: &Path,
    kiln: &Path,
) -> Result<(), WebError> {
    validate_parent_within_kiln(file_path, kiln)?;

    // symlink_metadata does NOT follow the link, so this detects a symlinked
    // final component regardless of where it points.
    if let Ok(meta) = std::fs::symlink_metadata(file_path) {
        if meta.file_type().is_symlink() {
            let resolved = file_path.canonicalize().map_err(|_| {
                WebError::Validation("Symlinked path could not be resolved".to_string())
            })?;
            if !resolved.starts_with(kiln) {
                return Err(WebError::Validation(
                    "Path escapes kiln directory".to_string(),
                ));
            }
        }
    }

    Ok(())
}

/// Canonicalize a file path and verify it resides within the kiln.
pub(crate) fn validate_file_within_kiln(
    file_path: &Path,
    kiln: &Path,
    original_path: &str,
) -> Result<PathBuf, WebError> {
    let canonical_file = file_path
        .canonicalize()
        .map_err(|_| WebError::NotFound(format!("File not found: {original_path}")))?;

    if !canonical_file.starts_with(kiln) {
        return Err(WebError::Validation(
            "File path escapes kiln directory".to_string(),
        ));
    }

    Ok(canonical_file)
}

// =========================================================================
// Content limits
// =========================================================================

/// Maximum note/file content size (10 MB).
pub(crate) const MAX_CONTENT_SIZE: usize = 10 * 1024 * 1024;

// =========================================================================
// Concurrent writers
// =========================================================================

/// Refuse a whole-file write whose base no longer matches what is on disk.
///
/// THE one place this rule lives. Three routes overwrite a file whole — `PUT
/// /api/kiln/file`, `PUT /api/notes/{name}` and `PUT /api/canvas` — and each
/// of them could silently destroy another writer's work. Gating one leaves
/// the other two open, and three copies of the compare drift.
///
/// The check belongs HERE, next to the write, not in a client: a browser's
/// read-then-compare-then-PUT is three round trips with a window in the
/// middle, and it runs on the machine with the stale view of the disk.
/// `crucible-web/web/src/lib/offline/sync.ts` did exactly that before this
/// existed, because the alternative was no check at all.
///
/// `None` keeps the blind overwrite, deliberately: the desktop editor and
/// every existing caller save without a base, and making the field required
/// would break them all on one commit. A caller that sends a base gets the
/// guarantee; one that does not is no worse off than before.
///
/// A missing file compares as the empty hash, so a base sent for a note that
/// was deleted meanwhile is stale rather than silently recreating it.
pub(crate) async fn refuse_if_base_is_stale(
    path: &Path,
    base_hash: Option<&str>,
) -> Result<(), WebError> {
    let Some(base) = base_hash else {
        return Ok(());
    };
    let current = match tokio::fs::read_to_string(path).await {
        Ok(text) => crucible_core::note_edit::disk_hash(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(WebError::Io(e)),
    };
    if current == base {
        return Ok(());
    }
    Err(WebError::StaleBase {
        current_hash: current,
    })
}

#[cfg(test)]
mod stale_base_tests {
    use super::*;
    use tempfile::tempdir;

    /// The gate section 11 asked for and could not have, because until now
    /// there was no comparison in a route to break.
    #[tokio::test]
    async fn a_write_with_no_base_still_overwrites_blind() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("note.md");
        tokio::fs::write(&file, "on disk").await.unwrap();

        // Every existing caller saves without a base. They must keep working.
        refuse_if_base_is_stale(&file, None)
            .await
            .expect("no base means no check");
    }

    #[tokio::test]
    async fn a_write_whose_base_matches_the_disk_is_allowed() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("note.md");
        tokio::fs::write(&file, "on disk").await.unwrap();
        let base = crucible_core::note_edit::disk_hash("on disk");

        refuse_if_base_is_stale(&file, Some(&base))
            .await
            .expect("an unchanged file accepts its own hash");
    }

    #[tokio::test]
    async fn a_write_whose_base_moved_on_is_refused_with_the_current_hash() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("note.md");
        tokio::fs::write(&file, "another writer got here first")
            .await
            .unwrap();
        let stale = crucible_core::note_edit::disk_hash("what the caller read");

        let err = refuse_if_base_is_stale(&file, Some(&stale))
            .await
            .expect_err("a moved file must refuse the write");
        match err {
            WebError::StaleBase { current_hash } => assert_eq!(
                current_hash,
                crucible_core::note_edit::disk_hash("another writer got here first"),
                "the refusal carries what is on disk NOW, so the caller need not re-read"
            ),
            other => panic!("expected StaleBase, got {other:?}"),
        }
    }

    /// A base sent for a note that was deleted meanwhile is stale. Accepting
    /// it would silently recreate a note the user removed.
    #[tokio::test]
    async fn a_base_for_a_file_that_is_gone_is_stale() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("deleted.md");
        let base = crucible_core::note_edit::disk_hash("it used to say this");

        let err = refuse_if_base_is_stale(&missing, Some(&base))
            .await
            .expect_err("a deleted file must not be recreated by a stale write");
        assert!(matches!(err, WebError::StaleBase { current_hash } if current_hash.is_empty()));
    }

    /// Creating a NEW file sends no base, so it is not caught by the above.
    #[tokio::test]
    async fn creating_a_new_file_is_not_refused() {
        let dir = tempdir().unwrap();
        refuse_if_base_is_stale(&dir.path().join("new.md"), None)
            .await
            .expect("a new file has no base to be stale");
    }
}
