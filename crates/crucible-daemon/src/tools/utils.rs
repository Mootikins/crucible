//! Shared utility functions for crucible-daemon
//!
//! This module contains helper functions used across multiple tool modules.
//!
//! Path validation used to live here as `validate_path_within_kiln` — a shared
//! *check* the note, search and kiln tools each remembered to call. It is gone,
//! replaced by [`super::fs_scope::FsScope`], a shared *capability* they cannot
//! decline to call: see that module for why the distinction is the whole fix.

#![allow(clippy::missing_errors_doc)]

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
/// use crucible_daemon::tools::utils::parse_yaml_frontmatter;
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
