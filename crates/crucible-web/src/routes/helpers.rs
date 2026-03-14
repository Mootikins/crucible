//! Shared helpers for route handlers.
//!
//! Centralises note-to-JSON mapping, path validation, and content-size
//! constants that were previously duplicated across `search.rs` and `kiln.rs`.

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

/// Reject paths containing traversal sequences (`..`) or null bytes.
///
/// Returns [`WebError::Validation`] on failure (preserving the existing
/// HTTP-422 behaviour of the kiln routes).
pub(crate) fn validate_no_traversal(path: &str) -> Result<(), WebError> {
    if path.contains("..") || path.contains('\0') {
        return Err(WebError::Validation(
            "Invalid path: traversal not allowed".to_string(),
        ));
    }
    Ok(())
}

// =========================================================================
// Content limits
// =========================================================================

/// Maximum note/file content size (10 MB).
pub(crate) const MAX_CONTENT_SIZE: usize = 10 * 1024 * 1024;
