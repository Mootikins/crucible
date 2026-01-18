//! Unified attribute discovery for Rune scripts
//!
//! This module provides a consistent pattern for discovering Rune functions
//! annotated with attributes like `#[tool(...)]`, `#[handler(...)]`, etc.
//!
//! ## Usage
//!
//! ```rust,ignore
//! // Implement FromAttributes for your type
//! impl FromAttributes for RuneTool {
//!     fn attribute_name() -> &'static str { "tool" }
//!     fn from_attrs(attrs: &str, fn_name: &str, path: &Path) -> Result<Self, Error> { ... }
//! }
//!
//! // Discover all items from files
//! let discovery = AttributeDiscovery::new();
//! let tools: Vec<RuneTool> = discovery.discover_all(&paths)?;
//! ```

use crate::RuneError;
use crucible_core::discovery::DiscoveryPaths;
use glob::glob;
use regex::Regex;
use std::path::Path;
use tracing::{debug, warn};

/// Trait for types that can be discovered from Rune script attributes
///
/// Implement this trait to enable automatic discovery of your type
/// from Rune scripts annotated with a specific attribute.
pub trait FromAttributes: Sized {
    /// The attribute name to search for (e.g., "tool", "handler", "param")
    fn attribute_name() -> &'static str;

    /// Alternate attribute names for backwards compatibility (e.g., "hook" for "handler")
    fn alternate_names() -> &'static [&'static str] {
        &[]
    }

    /// Parse an instance from attribute content and function metadata
    ///
    /// # Arguments
    /// * `attrs` - The content inside the attribute parentheses (e.g., `desc = "..."`)
    /// * `fn_name` - The name of the annotated function
    /// * `path` - The path to the source file
    /// * `docs` - Doc comment lines (if any)
    fn from_attrs(attrs: &str, fn_name: &str, path: &Path, docs: &str) -> Result<Self, RuneError>;
}

/// Discovers items from Rune scripts using attribute annotations
pub struct AttributeDiscovery {
    /// File extensions to search for
    extensions: Vec<String>,
    /// Whether to search subdirectories
    recursive: bool,
}

impl Default for AttributeDiscovery {
    fn default() -> Self {
        Self {
            extensions: vec!["rn".to_string(), "rune".to_string()],
            recursive: true,
        }
    }
}

impl AttributeDiscovery {
    /// Create a new discovery instance with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set file extensions to search
    pub fn with_extensions(mut self, extensions: Vec<String>) -> Self {
        self.extensions = extensions;
        self
    }

    /// Set whether to search subdirectories
    pub fn with_recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    /// Discover all items of type T from the given discovery paths
    pub fn discover_all<T: FromAttributes>(
        &self,
        paths: &DiscoveryPaths,
    ) -> Result<Vec<T>, RuneError> {
        let mut items = Vec::new();

        for dir in paths.existing_paths() {
            let discovered = self.discover_in_directory::<T>(&dir)?;
            debug!(
                "Discovered {} {} items in {}",
                discovered.len(),
                T::attribute_name(),
                dir.display()
            );
            items.extend(discovered);
        }

        Ok(items)
    }

    /// Discover items in a single directory
    pub fn discover_in_directory<T: FromAttributes>(
        &self,
        dir: &Path,
    ) -> Result<Vec<T>, RuneError> {
        let mut items = Vec::new();

        for ext in &self.extensions {
            let pattern = if self.recursive {
                format!("{}/**/*.{}", dir.display(), ext)
            } else {
                format!("{}/*.{}", dir.display(), ext)
            };

            for entry in glob(&pattern).map_err(|e| RuneError::Discovery(e.to_string()))? {
                match entry {
                    Ok(path) => {
                        debug!(
                            "Scanning {} for #{} attributes",
                            path.display(),
                            T::attribute_name()
                        );
                        match self.parse_from_file::<T>(&path) {
                            Ok(file_items) => {
                                debug!(
                                    "Found {} {} in {}",
                                    file_items.len(),
                                    T::attribute_name(),
                                    path.display()
                                );
                                items.extend(file_items);
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to parse {} from {}: {}",
                                    T::attribute_name(),
                                    path.display(),
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Glob error: {}", e);
                    }
                }
            }
        }

        Ok(items)
    }

    /// Parse items from a single Rune file
    pub fn parse_from_file<T: FromAttributes>(&self, path: &Path) -> Result<Vec<T>, RuneError> {
        let content = std::fs::read_to_string(path).map_err(|e| RuneError::Io(e.to_string()))?;
        self.parse_from_source::<T>(&content, path)
    }

    /// Parse items from source code
    pub fn parse_from_source<T: FromAttributes>(
        &self,
        content: &str,
        path: &Path,
    ) -> Result<Vec<T>, RuneError> {
        let mut items = Vec::new();

        // Collect all attribute names (primary + alternates)
        let mut attr_names = vec![T::attribute_name()];
        attr_names.extend(T::alternate_names());

        for attr_name in attr_names {
            // Build regex for this attribute type
            // Matches: optional doc comments, #[attr(...)], pub [async] fn name(...)
            let pattern = format!(
                r"(?ms)(?P<docs>(?:///[^\n]*\n)*)?\s*#\[{}\((?P<attrs>[^)]*)\)\]\s*pub\s+(?:async\s+)?fn\s+(?P<fn_name>\w+)\s*\(",
                regex::escape(attr_name)
            );

            let re = Regex::new(&pattern)
                .map_err(|e| RuneError::Discovery(format!("Invalid regex: {}", e)))?;

            for cap in re.captures_iter(content) {
                let fn_name = cap.name("fn_name").map(|m| m.as_str()).unwrap_or("main");
                let attrs = cap.name("attrs").map(|m| m.as_str()).unwrap_or("");
                let docs = cap.name("docs").map(|m| m.as_str()).unwrap_or("");

                match T::from_attrs(attrs, fn_name, path, docs) {
                    Ok(item) => items.push(item),
                    Err(e) => {
                        warn!(
                            "Failed to parse #{} for function '{}' in {}: {}",
                            attr_name,
                            fn_name,
                            path.display(),
                            e
                        );
                    }
                }
            }
        }

        Ok(items)
    }
}

/// Helper functions for parsing common attribute patterns
pub mod attr_parsers {
    use regex::Regex;

    /// Extract a string attribute like `key = "value"`
    pub fn extract_string(attrs: &str, key: &str) -> Option<String> {
        let pattern = format!(r#"{}[\s]*=[\s]*"([^"]*)""#, key);
        let re = Regex::new(&pattern).ok()?;
        re.captures(attrs)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
    }

    /// Extract a boolean attribute like `key = true` or bare `key`
    pub fn extract_bool(attrs: &str, key: &str) -> Option<bool> {
        // Check for `key = true` or `key = false`
        let pattern = format!(r#"{}[\s]*=[\s]*(true|false)"#, key);
        if let Ok(re) = Regex::new(&pattern) {
            if let Some(cap) = re.captures(attrs) {
                if let Some(m) = cap.get(1) {
                    return Some(m.as_str() == "true");
                }
            }
        }

        // Check for bare `key` (implies true)
        let bare_pattern = format!(r"\b{}\b", key);
        if let Ok(re) = Regex::new(&bare_pattern) {
            if re.is_match(attrs)
                && !attrs.contains(&format!("{} =", key))
                && !attrs.contains(&format!("{}=", key))
            {
                return Some(true);
            }
        }

        None
    }

    /// Extract an integer attribute like `key = 42`
    pub fn extract_int(attrs: &str, key: &str) -> Option<i64> {
        let pattern = format!(r#"{}[\s]*=[\s]*(-?\d+)"#, key);
        let re = Regex::new(&pattern).ok()?;
        re.captures(attrs)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse().ok())
    }

    /// Extract an array attribute like `tags = ["a", "b"]`
    pub fn extract_string_array(attrs: &str, key: &str) -> Option<Vec<String>> {
        let pattern = format!(r#"{}[\s]*=[\s]*\[([^\]]*)\]"#, key);
        let re = Regex::new(&pattern).ok()?;

        re.captures(attrs).and_then(|c| c.get(1)).map(|m| {
            let inner = m.as_str();
            // Parse quoted strings from array
            let string_re = Regex::new(r#""([^"]*)""#).unwrap();
            string_re
                .captures_iter(inner)
                .filter_map(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .collect()
        })
    }

    /// Extract description from doc comments (/// lines)
    pub fn extract_doc_description(docs: &str) -> Option<String> {
        let lines: Vec<&str> = docs
            .lines()
            .map(|l| l.trim().trim_start_matches("///").trim())
            .filter(|l| !l.is_empty())
            .collect();

        if lines.is_empty() {
            None
        } else {
            Some(lines.join(" "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::attr_parsers::*;
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // Simple test type for attribute discovery
    #[derive(Debug, Clone, PartialEq)]
    struct TestItem {
        name: String,
        description: String,
        path: PathBuf,
        priority: i64,
        tags: Vec<String>,
    }

    impl FromAttributes for TestItem {
        fn attribute_name() -> &'static str {
            "test_item"
        }

        fn from_attrs(
            attrs: &str,
            fn_name: &str,
            path: &Path,
            docs: &str,
        ) -> Result<Self, RuneError> {
            let description = extract_string(attrs, "desc")
                .or_else(|| extract_doc_description(docs))
                .unwrap_or_else(|| format!("Test item: {}", fn_name));

            let priority = extract_int(attrs, "priority").unwrap_or(100);
            let tags = extract_string_array(attrs, "tags").unwrap_or_default();

            Ok(TestItem {
                name: fn_name.to_string(),
                description,
                path: path.to_path_buf(),
                priority,
                tags,
            })
        }
    }

    #[test]
    fn test_extract_string() {
        assert_eq!(
            extract_string(r#"desc = "hello world""#, "desc"),
            Some("hello world".to_string())
        );
        assert_eq!(
            extract_string(r#"desc="no spaces""#, "desc"),
            Some("no spaces".to_string())
        );
        assert_eq!(extract_string(r#"other = "value""#, "desc"), None);
    }

    #[test]
    fn test_extract_bool() {
        assert_eq!(extract_bool("enabled = true", "enabled"), Some(true));
        assert_eq!(extract_bool("enabled = false", "enabled"), Some(false));
        assert_eq!(extract_bool("enabled", "enabled"), Some(true));
        assert_eq!(extract_bool("other = true", "enabled"), None);
    }

    #[test]
    fn test_extract_int() {
        assert_eq!(extract_int("priority = 50", "priority"), Some(50));
        assert_eq!(extract_int("priority = -10", "priority"), Some(-10));
        assert_eq!(extract_int("other = 50", "priority"), None);
    }

    #[test]
    fn test_extract_string_array() {
        let tags = extract_string_array(r#"tags = ["a", "b", "c"]"#, "tags");
        assert_eq!(
            tags,
            Some(vec!["a".to_string(), "b".to_string(), "c".to_string()])
        );

        let empty = extract_string_array(r#"tags = []"#, "tags");
        assert_eq!(empty, Some(vec![]));
    }

    #[test]
    fn test_extract_doc_description() {
        let docs = "/// This is a description\n/// with multiple lines";
        assert_eq!(
            extract_doc_description(docs),
            Some("This is a description with multiple lines".to_string())
        );

        assert_eq!(extract_doc_description(""), None);
    }

    #[test]
    fn test_parse_from_source() {
        let content = r#"
/// A test function
#[test_item(desc = "Override description", priority = 50, tags = ["a", "b"])]
pub fn my_test_fn() {}

#[test_item()]
pub fn another_fn() {}
"#;

        let discovery = AttributeDiscovery::new();
        let items: Vec<TestItem> = discovery
            .parse_from_source(content, Path::new("test.rn"))
            .unwrap();

        assert_eq!(items.len(), 2);

        assert_eq!(items[0].name, "my_test_fn");
        assert_eq!(items[0].description, "Override description");
        assert_eq!(items[0].priority, 50);
        assert_eq!(items[0].tags, vec!["a", "b"]);

        assert_eq!(items[1].name, "another_fn");
        assert_eq!(items[1].priority, 100); // default
    }

    #[test]
    fn test_parse_async_function() {
        let content = r#"
#[test_item(desc = "Async function")]
pub async fn async_fn() {}
"#;

        let discovery = AttributeDiscovery::new();
        let items: Vec<TestItem> = discovery
            .parse_from_source(content, Path::new("test.rn"))
            .unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "async_fn");
    }

    #[test]
    fn test_doc_fallback() {
        let content = r#"
/// Doc comment description
#[test_item()]
pub fn doc_fn() {}
"#;

        let discovery = AttributeDiscovery::new();
        let items: Vec<TestItem> = discovery
            .parse_from_source(content, Path::new("test.rn"))
            .unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].description, "Doc comment description");
    }

    #[test]
    fn test_discover_in_directory() {
        let temp = TempDir::new().unwrap();

        let script1 = r#"
#[test_item(desc = "First")]
pub fn first() {}
"#;
        fs::write(temp.path().join("script1.rn"), script1).unwrap();

        let script2 = r#"
#[test_item(desc = "Second")]
pub fn second() {}

#[test_item(desc = "Third")]
pub fn third() {}
"#;
        fs::write(temp.path().join("script2.rn"), script2).unwrap();

        let discovery = AttributeDiscovery::new();
        let items: Vec<TestItem> = discovery.discover_in_directory(temp.path()).unwrap();

        assert_eq!(items.len(), 3);
    }

    #[test]
    fn test_discover_all_with_paths() {
        let temp = TempDir::new().unwrap();

        let script = r#"
#[test_item(desc = "Found")]
pub fn found() {}
"#;
        fs::write(temp.path().join("script.rn"), script).unwrap();

        let paths = DiscoveryPaths::empty("test").with_path(temp.path().to_path_buf());
        let discovery = AttributeDiscovery::new();
        let items: Vec<TestItem> = discovery.discover_all(&paths).unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "found");
    }
}
