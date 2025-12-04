//! Kiln integration utilities
//!
//! ID normalization, path resolution, and helper functions.

use crate::utils::resolve_and_normalize_path;
use std::path::{Component, Path, PathBuf};

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

/// Clean and normalize a relative path
pub(crate) fn clean_relative_path(path: &Path) -> Option<PathBuf> {
    let mut stack: Vec<PathBuf> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if stack.pop().is_none() {
                    return None;
                }
            }
            Component::Normal(part) => stack.push(PathBuf::from(part)),
            Component::Prefix(_) | Component::RootDir => return None,
        }
    }

    let mut normalized = PathBuf::new();
    for part in stack {
        normalized.push(part);
    }

    Some(normalized)
}

/// Convert record reference to string
pub(crate) fn record_ref_to_string(value: &serde_json::Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }

    if let Some(obj) = value.as_object() {
        if let Some(thing) = obj.get("thing").and_then(|v| v.as_str()) {
            return Some(thing.to_string());
        }
        let table = obj.get("tb")?.as_str()?;
        let id = obj.get("id")?.as_str()?;
        return Some(format!("{}:{}", table, id));
    }

    None
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

/// Parse timestamp from multiple candidate values
pub(crate) fn parse_timestamp(
    primary: Option<&serde_json::Value>,
    fallback_one: Option<&serde_json::Value>,
    fallback_two: Option<&serde_json::Value>,
) -> chrono::DateTime<chrono::Utc> {
    let candidates = [
        primary.and_then(|v| v.as_str()),
        fallback_one.and_then(|v| v.as_str()),
        fallback_two.and_then(|v| v.as_str()),
    ];

    for candidate in candidates {
        if let Some(ts) = candidate {
            if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(ts) {
                return parsed.with_timezone(&chrono::Utc);
            }
        }
    }

    chrono::Utc::now()
}
