//! Utility functions for the Crucible Rune system
//!
//! This module provides various helper functions and utilities for
//! file operations, path sanitization, string manipulation, and more.

pub mod path_sanitizer;

use std::path::{Path, PathBuf};
use tracing::{debug, warn};
use std::hash::{Hash, Hasher};

/// Sanitize a tool name to ensure it's valid
pub fn sanitize_tool_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

/// Validate that a tool name follows the required format
pub fn validate_tool_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !name.starts_with('_')
        && !name.ends_with('_')
}

/// Extract tool name from file path
pub fn extract_tool_name_from_path(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(sanitize_tool_name)
}

/// Check if a file is a Rune tool file
pub fn is_rune_tool_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        matches!(ext, "rn" | "rune")
    } else {
        false
    }
}

/// Normalize a path for cross-platform compatibility
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            std::path::Component::Normal(seg) => {
                if let Some(seg_str) = seg.to_str() {
                    normalized.push(seg_str);
                }
            }
            std::path::Component::RootDir => {
                normalized.push(component);
            }
            std::path::Component::Prefix(prefix) => {
                normalized.push(prefix.as_os_str());
            }
            std::path::Component::CurDir => {
                // Skip current directory (.)
            }
            std::path::Component::ParentDir => {
                normalized.pop();
            }
        }
    }

    normalized
}

/// Create a relative path from base to target
pub fn make_relative_path(base: &Path, target: &Path) -> Option<PathBuf> {
    pathdiff::diff_paths(target, base)
}

/// Generate a unique ID for a tool based on its content
pub fn generate_tool_id(content: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Check if a string is valid JSON
pub fn is_valid_json(s: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(s).is_ok()
}

/// Format a duration in milliseconds to a human-readable string
pub fn format_duration_ms(duration_ms: u64) -> String {
    if duration_ms < 1000 {
        format!("{}ms", duration_ms)
    } else if duration_ms < 60_000 {
        format!("{:.1}s", duration_ms as f64 / 1000.0)
    } else if duration_ms < 3_600_000 {
        format!("{:.1}m", duration_ms as f64 / 60_000.0)
    } else {
        format!("{:.1}h", duration_ms as f64 / 3_600_000.0)
    }
}

/// Format a file size in bytes to a human-readable string
pub fn format_file_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// Truncate a string to a maximum length with ellipsis
pub fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Escape a string for safe display in logs
pub fn escape_log_string(s: &str) -> String {
    s.replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
        .replace('"', "\\\"")
}

/// Check if a path is safe (doesn't contain directory traversal)
pub fn is_safe_path(path: &Path, base_dir: &Path) -> bool {
    match path.canonicalize() {
        Ok(canonical_path) => {
            match base_dir.canonicalize() {
                Ok(base_canonical) => canonical_path.starts_with(base_canonical),
                Err(_) => {
                    warn!("Failed to canonicalize base directory: {:?}", base_dir);
                    false
                }
            }
        }
        Err(_) => {
            debug!("Failed to canonicalize path: {:?}", path);
            false
        }
    }
}

/// Create a safe filename from a string
pub fn create_safe_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            c if c.is_alphanumeric() => c,
            c if matches!(c, ' ' | '-' | '_' | '.') => c,
            _ => '_',
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

/// Get file modification time
pub fn get_file_modified_time(path: &Path) -> Result<chrono::DateTime<chrono::Utc>, std::io::Error> {
    let metadata = std::fs::metadata(path)?;
    let modified = metadata.modified()?;
    Ok(chrono::DateTime::from(modified))
}

/// Check if a file has been modified since a given time
pub fn is_file_modified_since(path: &Path, since: chrono::DateTime<chrono::Utc>) -> Result<bool, std::io::Error> {
    let modified_time = get_file_modified_time(path)?;
    Ok(modified_time > since)
}

/// Create a temporary directory with a given prefix
pub fn create_temp_dir(prefix: &str) -> Result<std::path::PathBuf, std::io::Error> {
    let temp_dir = std::env::temp_dir();
    let unique_name = format!("{}_{}", prefix, uuid::Uuid::new_v4());
    let dir_path = temp_dir.join(unique_name);
    std::fs::create_dir_all(&dir_path)?;
    Ok(dir_path)
}

/// Clean up a temporary directory
pub fn cleanup_temp_dir(dir_path: &Path) -> Result<(), std::io::Error> {
    if dir_path.exists() && dir_path.is_dir() {
        std::fs::remove_dir_all(dir_path)?;
    }
    Ok(())
}

/// Get the current working directory
pub fn current_dir() -> Result<std::path::PathBuf, std::io::Error> {
    std::env::current_dir()
}

/// Change the current working directory
pub fn set_current_dir<P: AsRef<Path>>(dir: P) -> Result<(), std::io::Error> {
    std::env::set_current_dir(dir)
}

/// Check if a directory exists and is accessible
pub fn is_accessible_directory(path: &Path) -> bool {
    path.exists() && path.is_dir() && !path.metadata().map(|m| m.permissions().readonly()).unwrap_or(true)
}

/// Get all files in a directory recursively
pub fn get_files_recursive<P: AsRef<Path>>(
    dir: P,
    extensions: &[&str],
) -> Result<Vec<std::path::PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    let dir_path = dir.as_ref();

    if !dir_path.exists() || !dir_path.is_dir() {
        return Ok(files);
    }

    let entries = std::fs::read_dir(dir_path)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Recursively search subdirectories
            match get_files_recursive(&path, extensions) {
                Ok(mut sub_files) => files.append(&mut sub_files),
                Err(e) => warn!("Error reading directory {:?}: {}", path, e),
            }
        } else if path.is_file() {
            // Check file extension
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if extensions.contains(&ext) {
                    files.push(path);
                }
            }
        }
    }

    Ok(files)
}

/// Read file content with size limit
pub fn read_file_with_limit<P: AsRef<Path>>(
    path: P,
    max_size: usize,
) -> Result<String, std::io::Error> {
    let path = path.as_ref();
    let metadata = std::fs::metadata(path)?;

    if metadata.len() as usize > max_size {
        return Err(std::io::Error::new(
            std::io::ErrorKind::FileTooLarge,
            format!("File size ({}) exceeds limit ({})", metadata.len(), max_size),
        ));
    }

    std::fs::read_to_string(path)
}

/// Write file content with atomic operation
pub fn write_file_atomic<P: AsRef<Path>, C: AsRef<[u8]>>(
    path: P,
    content: C,
) -> Result<(), std::io::Error> {
    let path = path.as_ref();
    let content = content.as_ref();

    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Write to temporary file first
    let temp_path = path.with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));
    std::fs::write(&temp_path, content)?;

    // Atomically rename to target path
    std::fs::rename(&temp_path, path)?;

    Ok(())
}

/// Create a backup of a file
pub fn backup_file<P: AsRef<Path>>(path: P) -> Result<std::path::PathBuf, std::io::Error> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("File not found: {:?}", path),
        ));
    }

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup_name = format!(
        "{}.{}.backup",
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file"),
        timestamp
    );

    let backup_path = path.with_file_name(backup_name);
    std::fs::copy(path, &backup_path)?;

    Ok(backup_path)
}

/// Calculate file hash for change detection
pub fn calculate_file_hash<P: AsRef<Path>>(path: P) -> Result<String, std::io::Error> {
    let path = path.as_ref();
    let content = std::fs::read_to_string(path)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    Ok(format!("{:x}", hasher.finish()))
}

/// Merge two JSON objects, with the second taking precedence
pub fn merge_json_objects(
    base: serde_json::Value,
    overlay: serde_json::Value,
) -> serde_json::Value {
    match (base, overlay) {
        (serde_json::Value::Object(mut base_map), serde_json::Value::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                if base_map.contains_key(&key) {
                    let base_value = base_map.remove(&key).unwrap();
                    base_map.insert(key, merge_json_objects(base_value, value));
                } else {
                    base_map.insert(key, value);
                }
            }
            serde_json::Value::Object(base_map)
        }
        (_, overlay) => overlay,
    }
}

/// Deep clone a JSON value
pub fn deep_clone_json(value: &serde_json::Value) -> serde_json::Value {
    serde_json::from_str(&serde_json::to_string(value).unwrap()).unwrap()
}

/// Check if two JSON values are equal (deep equality)
pub fn json_equals(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (a, b) {
        (serde_json::Value::Object(a_map), serde_json::Value::Object(b_map)) => {
            a_map.len() == b_map.len() &&
            a_map.iter().all(|(k, v)| b_map.get(k).map_or(false, |b_v| json_equals(v, b_v)))
        }
        (serde_json::Value::Array(a_arr), serde_json::Value::Array(b_arr)) => {
            a_arr.len() == b_arr.len() &&
            a_arr.iter().zip(b_arr.iter()).all(|(a_v, b_v)| json_equals(a_v, b_v))
        }
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_sanitize_tool_name() {
        assert_eq!(sanitize_tool_name("valid_name"), "valid_name");
        assert_eq!(sanitize_tool_name("invalid name!"), "invalid_name_");
        assert_eq!(sanitize_tool_name("123invalid"), "123invalid");
        assert_eq!(sanitize_tool_name(""), "");
        assert_eq!(sanitize_tool_name("___leading___trailing___"), "leading___trailing");
    }

    #[test]
    fn test_validate_tool_name() {
        assert!(validate_tool_name("valid_name"));
        assert!(validate_tool_name("valid123"));
        assert!(!validate_tool_name("invalid name!"));
        assert!(!validate_tool_name("_invalid"));
        assert!(!validate_tool_name("invalid_"));
        assert!(!validate_tool_name(""));
        assert!(!validate_tool_name(&"a".repeat(101)));
    }

    #[test]
    fn test_is_rune_tool_file() {
        assert!(is_rune_tool_file(Path::new("test.rn")));
        assert!(is_rune_tool_file(Path::new("test.rune")));
        assert!(!is_rune_tool_file(Path::new("test.rs")));
        assert!(!is_rune_tool_file(Path::new("test")));
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration_ms(500), "500ms");
        assert_eq!(format_duration_ms(1500), "1.5s");
        assert_eq!(format_duration_ms(90000), "1.5m");
        assert_eq!(format_duration_ms(4000000), "1.1h");
    }

    #[test]
    fn test_format_file_size() {
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(1536), "1.5 KB");
        assert_eq!(format_file_size(2097152), "2.0 MB");
        assert_eq!(format_file_size(1073741824), "1.0 GB");
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("short", 10), "short");
        assert_eq!(truncate_string("this is a very long string", 10), "this is...");
        assert_eq!(truncate_string("exactlen", 8), "exactlen");
    }

    #[test]
    fn test_escape_log_string() {
        assert_eq!(escape_log_string("line1\nline2"), "line1\\nline2");
        assert_eq!(escape_log_string("tab\there"), "tab\\there");
        assert_eq!(escape_log_string("quote\"here"), "quote\\\"here");
    }

    #[test]
    fn test_create_safe_filename() {
        assert_eq!(create_safe_filename("valid name"), "valid name");
        assert_eq!(create_safe_filename("invalid/name!"), "invalid_name_");
        assert_eq!(create_safe_filename("___leading___trailing___"), "leading___trailing");
    }

    #[test]
    fn test_json_operations() {
        let base = serde_json::json!({
            "a": 1,
            "b": {"x": 10}
        });

        let overlay = serde_json::json!({
            "b": {"y": 20},
            "c": 3
        });

        let merged = merge_json_objects(base, overlay);
        assert_eq!(merged["a"], 1);
        assert_eq!(merged["b"]["x"], 10);
        assert_eq!(merged["b"]["y"], 20);
        assert_eq!(merged["c"], 3);
    }

    #[test]
    fn test_json_equality() {
        let a = serde_json::json!({"a": 1, "b": [1, 2]});
        let b = serde_json::json!({"b": [1, 2], "a": 1});
        let c = serde_json::json!({"a": 1, "b": [1, 3]});

        assert!(json_equals(&a, &b));
        assert!(!json_equals(&a, &c));
    }

    #[test]
    fn test_file_operations() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.txt");

        // Test atomic write
        write_file_atomic(&file_path, "test content")?;
        assert_eq!(std::fs::read_to_string(&file_path)?, "test content");

        // Test backup
        let backup_path = backup_file(&file_path)?;
        assert!(backup_path.exists());
        assert_eq!(std::fs::read_to_string(&backup_path)?, "test content");

        // Test file hash
        let hash1 = calculate_file_hash(&file_path)?;
        write_file_atomic(&file_path, "modified content")?;
        let hash2 = calculate_file_hash(&file_path)?;
        assert_ne!(hash1, hash2);

        Ok(())
    }

    #[test]
    fn test_get_files_recursive() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;

        // Create test files
        std::fs::write(temp_dir.path().join("test1.rn"), "content1")?;
        std::fs::write(temp_dir.path().join("test2.rs"), "content2")?;
        std::fs::create_dir(temp_dir.path().join("subdir"))?;
        std::fs::write(temp_dir.path().join("subdir/test3.rn"), "content3")?;

        let files = get_files_recursive(temp_dir.path(), &["rn"])?;
        assert_eq!(files.len(), 2);

        Ok(())
    }
}