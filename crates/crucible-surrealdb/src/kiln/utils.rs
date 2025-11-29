//! Utility functions shared across kiln modules

use crate::utils::resolve_and_normalize_path;
use std::path::Path;

/// Retry configuration for transaction conflicts
pub const MAX_RETRIES: u32 = 5;
pub const INITIAL_BACKOFF_MS: u64 = 10;

/// Normalize a document ID to the entities: format
pub fn normalize_document_id(doc_id: &str) -> String {
    if doc_id.starts_with("entities:") {
        return doc_id.to_string();
    }

    if doc_id.starts_with("note:") {
        return format!("entities:{}", doc_id);
    }

    if let Some(stripped) = doc_id.strip_prefix("notes:") {
        if stripped.starts_with("note:") {
            return format!("entities:{}", stripped);
        }
        return format!("entities:note:{}", stripped);
    }

    format!("entities:{}", doc_id)
}

/// Resolve path relative to kiln root
pub fn resolve_relative_path(path: &Path, kiln_root: &Path) -> String {
    let normalized = resolve_and_normalize_path(path, kiln_root);

    // If normalization resulted in an empty string, use the filename
    if normalized.is_empty() {
        return path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("note")
            .to_string();
    }
    normalized
}

/// Get namespace from normalized document ID for chunk storage
pub fn chunk_namespace(normalized_doc_id: &str) -> String {
    let body = record_body(normalized_doc_id);
    let trimmed = body.trim_start_matches("note:");
    trimmed
        .trim_start_matches(std::path::MAIN_SEPARATOR)
        .replace(['\\', '/', ':'], "_")
}

/// Escape a record ID for safe use in SurrealDB queries with angle brackets
///
/// When using angle bracket syntax (⟨...⟩), SurrealDB allows special characters
/// like colons and slashes. We only need to escape single quotes to prevent
/// breaking out of the angle bracket delimiters.
pub fn escape_record_id(value: &str) -> String {
    value.replace('\'', "\\'")
}

/// Extract the body portion of a chunk ID (after "embeddings:")
pub fn chunk_record_body(chunk_id: &str) -> &str {
    chunk_id.strip_prefix("embeddings:").unwrap_or(chunk_id)
}

/// Extract the body portion of a record ID (after "prefix:")
pub fn record_body(reference: &str) -> &str {
    // Find the first colon and return everything after it
    reference
        .find(':')
        .map(|idx| &reference[idx + 1..])
        .unwrap_or(reference)
}

/// Check if an error is a retryable transaction conflict
pub fn is_retryable_error(error_msg: &str) -> bool {
    error_msg.contains("read or write conflict")
        || error_msg.contains("transaction can be retried")
}

/// Generate a document ID from a path
pub fn generate_document_id(path: &std::path::Path, kiln_root: &std::path::Path) -> String {
    let relative = resolve_relative_path(path, kiln_root);
    format!("entities:note:{}", relative)
}
