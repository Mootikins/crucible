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
    let header = crucible_core::note_frontmatter::split_yaml_frontmatter(content)
        .ok()?
        .header?;
    // A blank or comment-only header is an empty mapping, not `null`.
    match serde_yaml::from_str(header.yaml).ok()? {
        serde_json::Value::Null => Some(serde_json::json!({})),
        value => Some(value),
    }
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

    #[test]
    fn test_parse_frontmatter_bom_and_dot_fence() {
        let fm = parse_yaml_frontmatter("\u{feff}---\ntitle: T\n...\nbody").unwrap();
        assert_eq!(fm["title"], "T");
    }

    #[test]
    fn test_parse_frontmatter_empty_header_is_empty_object() {
        assert_eq!(
            parse_yaml_frontmatter("---\n---\nbody"),
            Some(serde_json::json!({}))
        );
    }

    #[test]
    fn test_parse_frontmatter_unclosed_or_invalid_is_none() {
        assert!(parse_yaml_frontmatter("---\ntitle: T\nbody").is_none());
        assert!(parse_yaml_frontmatter("---\ntitle: [\n---\nbody").is_none());
        assert!(parse_yaml_frontmatter("---\n- a\n---\nbody").is_none());
    }
}
