//! Shared helpers for route handlers.
//!
//! Centralises note-to-JSON mapping, note-name validation, and content-size
//! constants that were previously duplicated across `search.rs` and `kiln.rs`.
//! Path containment is not here: the daemon's `fs.read` and `fs.write` own it.

use crate::WebError;

/// Response for model listings — the session-scoped `list_models` and the
/// session-less `list_all_models` return the same `{ models: [...] }` shape.
// `Deserialize` is for the route tests, which read a reply back into the
// struct that wrote it. Nothing deserialises it on the wire.
#[derive(Debug, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct ModelsResponse {
    pub(crate) models: Vec<String>,
}

// =========================================================================
// Stream versioning (Task G6)
// =========================================================================

/// The stream protocol version this build speaks.
///
/// Daemon and browser upgrade from one repo, so skew is small — but a browser
/// one protocol ahead of a daemon must refuse to mis-parse, not guess. The
/// version travels TWICE on every stream: as the
/// `X-Crucible-Stream-Version` response header (for fetch-based clients and
/// the contract tests), and as the stream's first `stream_version` frame —
/// because `EventSource`, the browser's own transport, cannot read response
/// headers at all.
pub(crate) const STREAM_VERSION: u64 = 1;

/// The first frame of every versioned stream, mirroring the header.
pub(crate) fn stream_version_frame() -> axum::response::sse::Event {
    axum::response::sse::Event::default()
        .event("stream_version")
        .data(format!("{{\"version\":{STREAM_VERSION}}}"))
}

/// Wraps an SSE body with the version response header.
pub(crate) fn versioned<S>(
    stream: axum::response::sse::Sse<S>,
) -> (
    [(axum::http::HeaderName, String); 1],
    axum::response::sse::Sse<S>,
) {
    (
        [(
            axum::http::HeaderName::from_static("x-crucible-stream-version"),
            STREAM_VERSION.to_string(),
        )],
        stream,
    )
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

// =========================================================================
// Content limits
// =========================================================================

/// Maximum note/file content size (10 MB).
pub(crate) const MAX_CONTENT_SIZE: usize = 10 * 1024 * 1024;
