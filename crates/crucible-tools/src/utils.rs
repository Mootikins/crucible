//! Shared utility functions for crucible-tools
//!
//! This module contains helper functions used across multiple tool modules.

use std::path::{Path, PathBuf};

/// Parse YAML frontmatter from markdown content
///
/// Returns the parsed frontmatter as a JSON value, or None if no valid
/// frontmatter is found.
///
/// # Arguments
///
/// * `content` - The markdown content to parse
///
/// # Example
///
/// ```rust
/// use crucible_tools::utils::parse_yaml_frontmatter;
///
/// let content = "---\ntitle: My Note\ntags: [rust, code]\n---\n\n# Content";
/// let frontmatter = parse_yaml_frontmatter(content);
/// assert!(frontmatter.is_some());
/// ```
#[must_use]
pub fn parse_yaml_frontmatter(content: &str) -> Option<serde_json::Value> {
    // Check if starts with ---
    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        return None;
    }

    // Find closing ---
    let rest = &content[4..]; // Skip opening ---\n
    let end_pos = rest.find("\n---\n").or_else(|| rest.find("\r\n---\r\n"))?;

    let yaml_str = &rest[..end_pos];

    // Parse YAML to serde_json::Value
    serde_yaml::from_str(yaml_str).ok()
}

/// Validates that a user-provided path is within the kiln directory
///
/// This function prevents path traversal attacks by ensuring that:
/// 1. The path is not absolute
/// 2. The path does not contain ".." components
/// 3. The resolved path starts with the kiln directory
///
/// # Security
///
/// This is a critical security function that prevents:
/// - Path traversal attacks (e.g., "../../../etc/passwd")
/// - Absolute path escapes (e.g., "/etc/passwd")
/// - Symlink escapes (by canonicalizing paths)
///
/// # Arguments
///
/// * `kiln_path` - The trusted base directory (must be absolute)
/// * `user_path` - The user-provided path to validate
///
/// # Returns
///
/// Returns the canonicalized full path if valid, or an `ErrorData` if the path
/// is invalid or attempts to escape the kiln directory.
///
/// # Example
///
/// ```rust,no_run
/// use crucible_tools::utils::validate_path_within_kiln;
///
/// let kiln = "/home/user/kiln";
/// let valid = validate_path_within_kiln(kiln, "notes/test.md");
/// assert!(valid.is_ok());
///
/// let invalid = validate_path_within_kiln(kiln, "../etc/passwd");
/// assert!(invalid.is_err());
/// ```
pub fn validate_path_within_kiln(
    kiln_path: &str,
    user_path: &str,
) -> Result<PathBuf, rmcp::ErrorData> {
    // 1. Reject absolute paths
    let user_path_obj = Path::new(user_path);
    if user_path_obj.is_absolute() {
        return Err(rmcp::ErrorData::invalid_params(
            format!("Absolute paths are not allowed for security reasons: {user_path}"),
            None,
        ));
    }

    // 2. Reject paths with ".." components (OWASP: Input Validation)
    for component in user_path_obj.components() {
        if let std::path::Component::ParentDir = component {
            return Err(rmcp::ErrorData::invalid_params(
                format!("Path traversal is not allowed for security reasons: {user_path}"),
                None,
            ));
        }
    }

    // 3. Join with kiln path and canonicalize
    let kiln_path_obj = Path::new(kiln_path);
    let full_path = kiln_path_obj.join(user_path);

    // Canonicalize the kiln path to handle symlinks in the base directory
    let canonical_kiln = kiln_path_obj.canonicalize().map_err(|e| {
        rmcp::ErrorData::internal_error(format!("Failed to canonicalize kiln path: {e}"), None)
    })?;

    // For the full path, we need to handle the case where it doesn't exist yet
    // (e.g., creating a new file). We'll canonicalize the parent and then append
    // the final component.
    let validated_path = if full_path.exists() {
        // If it exists, canonicalize it fully to prevent symlink escapes
        let canonical_full = full_path.canonicalize().map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Failed to canonicalize path: {e}"), None)
        })?;

        // Verify the canonicalized path is still within kiln
        if !canonical_full.starts_with(&canonical_kiln) {
            return Err(rmcp::ErrorData::invalid_params(
                format!("Path escapes kiln directory (symlink attack?): {user_path}"),
                None,
            ));
        }

        canonical_full
    } else {
        // For non-existent paths, validate the parent exists and is within kiln
        let parent = full_path.parent().unwrap_or(kiln_path_obj);

        // If parent doesn't exist, we need to validate up to the first existing ancestor
        let mut current = parent;
        while !current.exists() && current != kiln_path_obj {
            current = current.parent().unwrap_or(kiln_path_obj);
        }

        // Canonicalize the existing ancestor
        let canonical_parent = current.canonicalize().map_err(|e| {
            rmcp::ErrorData::internal_error(
                format!("Failed to canonicalize parent path: {e}"),
                None,
            )
        })?;

        // Verify the parent is within kiln
        if !canonical_parent.starts_with(&canonical_kiln) {
            return Err(rmcp::ErrorData::invalid_params(
                format!("Path escapes kiln directory: {user_path}"),
                None,
            ));
        }

        // Return the non-canonicalized full path since it doesn't exist yet
        full_path
    };

    Ok(validated_path)
}

/// Validates an optional folder parameter for search operations
///
/// This is a convenience wrapper around `validate_path_within_kiln` for
/// handling optional folder parameters.
///
/// # Security
///
/// Uses the same security validations as `validate_path_within_kiln`.
///
/// # Arguments
///
/// * `kiln_path` - The trusted base directory (must be absolute)
/// * `folder` - Optional user-provided folder to validate
///
/// # Returns
///
/// Returns the validated path (`kiln_path` if folder is None, or validated folder path)
///
/// # Example
///
/// ```rust,no_run
/// use crucible_tools::utils::validate_folder_within_kiln;
///
/// let kiln = "/home/user/kiln";
/// let result = validate_folder_within_kiln(kiln, Some("projects"));
/// assert!(result.is_ok());
///
/// let result = validate_folder_within_kiln(kiln, None);
/// assert!(result.is_ok());
/// ```
pub fn validate_folder_within_kiln(
    kiln_path: &str,
    folder: Option<&str>,
) -> Result<PathBuf, rmcp::ErrorData> {
    match folder {
        Some(f) => validate_path_within_kiln(kiln_path, f),
        None => Ok(PathBuf::from(kiln_path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_parse_frontmatter_basic() {
        let content = "---\ntitle: Test\n---\n\n# Content";
        let result = parse_yaml_frontmatter(content);
        assert!(result.is_some());
        let fm = result.unwrap();
        assert_eq!(fm.get("title").unwrap().as_str().unwrap(), "Test");
    }

    #[test]
    fn test_parse_frontmatter_no_frontmatter() {
        let content = "# Just a heading\n\nSome content";
        let result = parse_yaml_frontmatter(content);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_frontmatter_windows_line_endings() {
        let content = "---\r\ntitle: Test\r\n---\r\n\r\n# Content";
        let result = parse_yaml_frontmatter(content);
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_frontmatter_with_tags() {
        let content = "---\ntitle: Note\ntags:\n  - rust\n  - code\n---\n\n# Content";
        let result = parse_yaml_frontmatter(content);
        assert!(result.is_some());
        let fm = result.unwrap();
        assert!(fm.get("tags").unwrap().is_array());
    }

    // ===== Security Tests for Path Validation =====

    #[test]
    fn test_validate_path_rejects_parent_directory_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Test various forms of parent directory traversal
        let attacks = vec![
            "../etc/passwd",
            "../../etc/passwd",
            "../../../etc/passwd",
            "notes/../../../etc/passwd",
            "notes/../../etc/passwd",
        ];

        for attack in attacks {
            let result = validate_path_within_kiln(&kiln_path, attack);
            assert!(
                result.is_err(),
                "Should reject path traversal attack: {attack}"
            );
            if let Err(e) = result {
                assert!(
                    e.message.contains("Path traversal"),
                    "Error should mention path traversal for: {attack}"
                );
            }
        }
    }

    #[test]
    fn test_validate_path_rejects_absolute_paths() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Test various absolute paths
        let attacks = vec!["/etc/passwd", "/root/.ssh/id_rsa", "/var/log/syslog"];

        for attack in attacks {
            let result = validate_path_within_kiln(&kiln_path, attack);
            assert!(
                result.is_err(),
                "Should reject absolute path attack: {attack}"
            );
            if let Err(e) = result {
                assert!(
                    e.message.contains("Absolute paths are not allowed"),
                    "Error should mention absolute paths for: {attack}"
                );
            }
        }
    }

    #[test]
    fn test_validate_path_allows_valid_nested_paths() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create nested directory structure
        std::fs::create_dir_all(temp_dir.path().join("projects/rust")).unwrap();

        // Test valid nested paths
        let valid_paths = vec![
            "notes.md",
            "projects/todo.md",
            "projects/rust/main.rs",
            "deeply/nested/path/file.md",
        ];

        for path in valid_paths {
            let result = validate_path_within_kiln(&kiln_path, path);
            assert!(result.is_ok(), "Should accept valid path: {path}");
        }
    }

    #[test]
    fn test_validate_path_accepts_nonexistent_files() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Test that we can validate paths for files that don't exist yet
        // (needed for create operations)
        let result = validate_path_within_kiln(&kiln_path, "new_note.md");
        assert!(
            result.is_ok(),
            "Should accept non-existent file for creation"
        );

        let result = validate_path_within_kiln(&kiln_path, "new_folder/new_note.md");
        assert!(
            result.is_ok(),
            "Should accept non-existent nested file for creation"
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_validate_path_blocks_symlink_escape() {
        use std::os::unix::fs::symlink;

        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create a directory outside the kiln
        let outside_dir = TempDir::new().unwrap();
        std::fs::write(outside_dir.path().join("secret.txt"), "secret data").unwrap();

        // Create a symlink inside kiln that points outside
        let symlink_path = temp_dir.path().join("evil_link");
        symlink(outside_dir.path(), &symlink_path).unwrap();

        // Try to access file through symlink
        let result = validate_path_within_kiln(&kiln_path, "evil_link/secret.txt");
        assert!(
            result.is_err(),
            "Should reject symlink escape to outside kiln"
        );

        if let Err(e) = result {
            assert!(
                e.message.contains("escapes kiln directory"),
                "Error should mention escape attempt"
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn test_validate_path_allows_internal_symlinks() {
        use std::os::unix::fs::symlink;

        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create a file inside kiln
        std::fs::write(temp_dir.path().join("real_file.md"), "content").unwrap();

        // Create a symlink inside kiln that points to another file inside kiln
        let symlink_path = temp_dir.path().join("link_to_file.md");
        symlink(temp_dir.path().join("real_file.md"), &symlink_path).unwrap();

        // This should be allowed since both files are within kiln
        let result = validate_path_within_kiln(&kiln_path, "link_to_file.md");
        assert!(result.is_ok(), "Should allow internal symlink within kiln");
    }

    #[test]
    fn test_validate_folder_with_none() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let result = validate_folder_within_kiln(&kiln_path, None);
        assert!(result.is_ok(), "Should accept None folder");
        assert_eq!(
            result.unwrap(),
            PathBuf::from(&kiln_path),
            "Should return kiln path for None"
        );
    }

    #[test]
    fn test_validate_folder_with_valid_folder() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create a valid subfolder
        std::fs::create_dir(temp_dir.path().join("projects")).unwrap();

        let result = validate_folder_within_kiln(&kiln_path, Some("projects"));
        assert!(result.is_ok(), "Should accept valid folder");
    }

    #[test]
    fn test_validate_folder_rejects_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let result = validate_folder_within_kiln(&kiln_path, Some("../etc"));
        assert!(result.is_err(), "Should reject folder with path traversal");
    }

    #[test]
    fn test_validate_path_empty_string() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Empty string should be treated as current directory (kiln root)
        let result = validate_path_within_kiln(&kiln_path, "");
        assert!(result.is_ok(), "Should accept empty string as kiln root");
    }

    #[test]
    fn test_validate_path_dot_current_directory() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // "." should be treated as current directory (kiln root)
        let result = validate_path_within_kiln(&kiln_path, ".");
        assert!(result.is_ok(), "Should accept '.' as kiln root");
    }

    #[test]
    fn test_validate_path_rejects_dot_dot() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // ".." should be rejected
        let result = validate_path_within_kiln(&kiln_path, "..");
        assert!(result.is_err(), "Should reject '..' path");
    }

    #[test]
    fn test_validate_path_unicode_filenames() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Test Unicode filenames (should be allowed)
        let unicode_paths = vec!["日本語.md", "émoji-📝.md", "русский/файл.md"];

        for path in unicode_paths {
            let result = validate_path_within_kiln(&kiln_path, path);
            assert!(result.is_ok(), "Should accept Unicode path: {path}");
        }
    }

    #[test]
    fn test_validate_path_special_characters() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Test special but valid characters in filenames
        let special_paths = vec![
            "file-with-dashes.md",
            "file_with_underscores.md",
            "file.multiple.dots.md",
            "file (with parens).md",
            "file [with brackets].md",
        ];

        for path in special_paths {
            let result = validate_path_within_kiln(&kiln_path, path);
            assert!(
                result.is_ok(),
                "Should accept valid special characters: {path}"
            );
        }
    }
}
