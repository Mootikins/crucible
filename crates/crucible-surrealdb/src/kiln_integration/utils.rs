//! Kiln integration utilities
//!
//! ID normalization, path resolution, and helper functions.

use crate::utils::resolve_and_normalize_path;
use std::path::Path;

/// Normalize document ID to entities: format
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

/// Generate chunk namespace from normalized document ID
pub(crate) fn chunk_namespace(normalized_doc_id: &str) -> String {
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
pub(crate) fn escape_record_id(value: &str) -> String {
    value.replace('\'', "\\'")
}

/// Extract chunk body from chunk ID
pub(crate) fn chunk_record_body(chunk_id: &str) -> &str {
    chunk_id.strip_prefix("embeddings:").unwrap_or(chunk_id)
}

/// Extract record body (table:id -> id)
pub(crate) fn record_body(reference: &str) -> &str {
    if let Some((prefix, rest)) = reference.split_once(':') {
        if prefix == "entities" || prefix == "notes" {
            return rest;
        }
    }
    reference
}

/// Resolve relative path from absolute path and kiln root
pub(crate) fn resolve_relative_path(path: &Path, kiln_root: &Path) -> String {
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

/// Generate a note ID from path and kiln root
pub fn generate_document_id(document_path: &Path, kiln_root: &Path) -> String {
    let relative = resolve_relative_path(document_path, kiln_root);
    let normalized = relative
        .trim_start_matches(std::path::MAIN_SEPARATOR)
        .replace('\\', "/")
        .replace(':', "_");
    format!("entities:note:{}", normalized)
}
