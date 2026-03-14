//! Shared helpers for route handlers.
//!
//! Centralises note-to-JSON mapping, path validation, and content-size
//! constants that were previously duplicated across `search.rs` and `kiln.rs`.

use std::path::{Path, PathBuf};

use crate::WebError;

// =========================================================================
// Note mapping
// =========================================================================

/// Tuple returned by [`crate::services::daemon::DaemonService::list_notes`].
pub(crate) type NoteListItem = (String, String, Option<String>, Vec<String>, Option<String>);

/// Map a note list item to full metadata JSON.
///
/// Produces: `{ name, path, title, tags, updated_at }`.
pub(crate) fn note_to_metadata_json(
    (name, path, title, tags, updated_at): NoteListItem,
) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "path": path,
        "title": title,
        "tags": tags,
        "updated_at": updated_at,
    })
}

/// Map a note list item to a file-entry JSON.
///
/// Produces: `{ name, path, is_dir: false }`.
pub(crate) fn note_to_file_json(
    (name, path, _title, _tags, _updated_at): NoteListItem,
) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "path": path,
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
pub(crate) fn validate_no_traversal(path: &str) -> Result<(), WebError> {
    // Reject absolute paths — callers must use kiln-relative or daemon-resolved paths.
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(WebError::Validation(
            "Invalid path: absolute paths not allowed".to_string(),
        ));
    }
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
