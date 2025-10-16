// Path sanitization utilities for privacy protection
//
// This module provides utilities to sanitize file paths in tool output to prevent
// exposing absolute paths and sensitive directory information.

use std::path::{Path, PathBuf};

/// Sanitize a file path for display in tool output
///
/// This function converts absolute paths to relative paths and removes sensitive
/// information from the path structure to protect user privacy.
///
/// # Arguments
/// * `path` - The path to sanitize
/// * `vault_root` - Optional vault root path for relativization
///
/// # Examples
/// ```
/// use crucible_mcp::utils::path_sanitizer::sanitize_path;
/// use std::path::Path;
///
/// let absolute_path = Path::new("/home/user/Documents/vault/notes/test.md");
/// let vault_root = Some(Path::new("/home/user/Documents/vault"));
/// let sanitized = sanitize_path(absolute_path, vault_root);
/// assert_eq!(sanitized, "notes/test.md");
/// ```
pub fn sanitize_path(path: &Path, vault_root: Option<&Path>) -> String {
    // First, try to make it relative to the vault root
    let relative_path = if let Some(root) = vault_root {
        path.strip_prefix(root)
            .unwrap_or_else(|_| path.strip_prefix("/").unwrap_or(path))
    } else {
        path.strip_prefix("/").unwrap_or(path)
    };

    // Convert to string and normalize
    let path_str = relative_path.to_string_lossy();

    // Remove any remaining sensitive patterns
    let sanitized = path_str
        // Replace common user directories with generic equivalents
        .replace("/home/", "~")
        .replace("/Users/", "~")
        // Replace common system directories
        .replace("/tmp/", "[temp]/")
        .replace("/var/tmp/", "[temp]/")
        // Remove consecutive slashes
        .replace("//", "/");

    let sanitized = sanitized
        // Remove leading ./ if present
        .strip_prefix("./")
        .unwrap_or(&sanitized);

    sanitized.to_string()
}

/// Sanitize multiple paths in a collection
pub fn sanitize_paths(paths: &[impl AsRef<Path>], vault_root: Option<&Path>) -> Vec<String> {
    paths.iter()
        .map(|p| sanitize_path(p.as_ref(), vault_root))
        .collect()
}

/// Sanitize error messages that may contain file paths
///
/// This function searches for patterns that look like file paths in error messages
/// and sanitizes them to protect privacy.
pub fn sanitize_error_message(error_msg: &str, vault_root: Option<&Path>) -> String {
    let mut sanitized = error_msg.to_string();

    // Replace absolute path patterns
    if let Some(home) = std::env::var("HOME").ok() {
        sanitized = sanitized.replace(&home, "~");
    }

    // Common system paths
    sanitized = sanitized
        .replace("/home/", "~/")
        .replace("/Users/", "~/")
        .replace("/tmp/", "[temp]/")
        .replace("/var/tmp/", "[temp]/");

    // If we have a vault root, replace it with "[vault]"
    if let Some(root) = vault_root {
        if let Some(root_str) = root.to_str() {
            sanitized = sanitized.replace(root_str, "[vault]");
        }
    }

    sanitized
}

/// Get a safe representation of a path for logging
///
/// This provides a path representation that's safe to include in logs and error messages
/// while still being useful for debugging.
pub fn safe_path_for_logging(path: &Path, vault_root: Option<&Path>) -> String {
    sanitize_path(path, vault_root)
}

/// Check if a path looks like an absolute path
pub fn is_absolute_path(path: &Path) -> bool {
    path.has_root() && !path.starts_with("./") && !path.starts_with("../")
}

/// Make a path relative if possible
pub fn make_relative_if_possible(path: &Path, base: Option<&Path>) -> PathBuf {
    if let Some(base_path) = base {
        path.strip_prefix(base_path)
            .unwrap_or_else(|_| path.strip_prefix("/").unwrap_or(path))
            .to_path_buf()
    } else {
        path.strip_prefix("/")
            .unwrap_or(path)
            .to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_sanitize_absolute_path() {
        let path = Path::new("/home/user/Documents/vault/notes/test.md");
        let vault_root = Path::new("/home/user/Documents/vault");
        let sanitized = sanitize_path(path, Some(vault_root));
        assert_eq!(sanitized, "notes/test.md");
    }

    #[test]
    fn test_sanitize_without_vault_root() {
        let path = Path::new("/home/user/Documents/notes/test.md");
        let sanitized = sanitize_path(path, None);
        assert_eq!(sanitized, "user/Documents/notes/test.md");
    }

    #[test]
    fn test_sanitize_relative_path() {
        let path = Path::new("./notes/test.md");
        let sanitized = sanitize_path(path, None);
        assert_eq!(sanitized, "notes/test.md");
    }

    #[test]
    fn test_sanitize_error_message() {
        let error = "Failed to read file /home/user/Documents/vault/notes/test.md: Permission denied";
        let vault_root = Path::new("/home/user/Documents/vault");
        let sanitized = sanitize_error_message(error, Some(vault_root));
        assert!(sanitized.contains("[vault]/notes/test.md"));
    }

    #[test]
    fn test_is_absolute_path() {
        assert!(is_absolute_path(Path::new("/home/user/file.md")));
        assert!(!is_absolute_path(Path::new("./file.md")));
        assert!(!is_absolute_path(Path::new("../file.md")));
        assert!(!is_absolute_path(Path::new("file.md")));
    }

    #[test]
    fn test_make_relative_if_possible() {
        let path = Path::new("/home/user/Documents/vault/notes/test.md");
        let base = Path::new("/home/user/Documents/vault");
        let relative = make_relative_if_possible(path, Some(base));
        assert_eq!(relative, Path::new("notes/test.md"));
    }

    #[test]
    fn test_sanitize_paths_collection() {
        let paths = vec![
            Path::new("/home/user/vault/notes/one.md"),
            Path::new("/home/user/vault/notes/two.md"),
        ];
        let vault_root = Path::new("/home/user/vault");
        let sanitized = sanitize_paths(&paths, Some(vault_root));
        assert_eq!(sanitized, vec!["notes/one.md", "notes/two.md"]);
    }
}