//! Discovery of Rune tools from filesystem
//!
//! Supports two formats:
//!
//! ## Single-tool file format (legacy)
//! One tool per file, metadata in doc comments:
//! ```rune
//! //! My tool description
//! //! @entry run
//! //! @param input string The input text
//! pub fn run(input) { ... }
//! ```
//!
//! ## Multi-tool file format (recommended)
//! Multiple tools per file using `#[tool(...)]` attributes:
//! ```rune
//! /// Create a new note
//! #[tool(desc = "Creates a note with title and content")]
//! #[param(name = "title", type = "string", desc = "Note title")]
//! #[param(name = "content", type = "string", desc = "Note content")]
//! pub fn create_note(title, content) { ... }
//!
//! /// Search notes
//! #[tool(desc = "Search notes by query")]
//! #[param(name = "query", type = "string", desc = "Search query")]
//! pub fn search_notes(query) { ... }
//! ```

use crate::types::{RuneDiscoveryConfig, RuneTool};
use crate::RuneError;
use glob::glob;
use regex::Regex;
use serde_json::{json, Value};
use std::path::Path;
use tracing::{debug, info, warn};

/// Discover Rune tools from configured directories
pub struct ToolDiscovery {
    config: RuneDiscoveryConfig,
}

impl ToolDiscovery {
    /// Create a new discovery instance
    pub fn new(config: RuneDiscoveryConfig) -> Self {
        Self { config }
    }

    /// Discover all tools from configured directories
    pub fn discover_all(&self) -> Result<Vec<RuneTool>, RuneError> {
        let mut tools = Vec::new();

        for dir in &self.config.tool_directories {
            if !dir.exists() {
                debug!("Rune directory does not exist: {}", dir.display());
                continue;
            }

            let discovered = self.discover_in_directory(dir)?;
            info!(
                "Discovered {} Rune tools in {}",
                discovered.len(),
                dir.display()
            );
            tools.extend(discovered);
        }

        Ok(tools)
    }

    /// Discover tools in a single directory
    pub fn discover_in_directory(&self, dir: &Path) -> Result<Vec<RuneTool>, RuneError> {
        let mut tools = Vec::new();

        for ext in &self.config.extensions {
            let pattern = if self.config.recursive {
                format!("{}/**/*.{}", dir.display(), ext)
            } else {
                format!("{}/*.{}", dir.display(), ext)
            };

            for entry in glob(&pattern).map_err(|e| RuneError::Discovery(e.to_string()))? {
                match entry {
                    Ok(path) => {
                        debug!("Found Rune file: {}", path.display());
                        match self.parse_tools_from_file(&path) {
                            Ok(file_tools) => {
                                debug!("Found {} tools in {}", file_tools.len(), path.display());
                                tools.extend(file_tools);
                            }
                            Err(e) => {
                                warn!("Failed to parse Rune tools from {}: {}", path.display(), e);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Glob error: {}", e);
                    }
                }
            }
        }

        Ok(tools)
    }

    /// Parse tools from a Rune file (supports both single-tool and multi-tool formats)
    fn parse_tools_from_file(&self, path: &Path) -> Result<Vec<RuneTool>, RuneError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| RuneError::Io(e.to_string()))?;

        // Check if file uses multi-tool format (has #[tool(...)] attributes)
        if content.contains("#[tool(") {
            self.parse_multi_tool_file(&content, path)
        } else {
            // Fall back to single-tool format
            let tool = self.parse_single_tool_file(&content, path)?;
            Ok(vec![tool])
        }
    }

    /// Parse a file with multiple `#[tool(...)]` annotated functions
    fn parse_multi_tool_file(&self, content: &str, path: &Path) -> Result<Vec<RuneTool>, RuneError> {
        let mut tools = Vec::new();

        // Regex to find #[tool(...)] blocks followed by function definitions
        // This parses:
        // - Optional doc comments (/// ...)
        // - #[tool(desc = "...", ...)]
        // - Zero or more #[param(...)]
        // - pub fn name(...)
        let tool_pattern = Regex::new(
            r"(?ms)(?P<docs>(?:///[^\n]*\n)*)?\s*#\[tool\((?P<tool_attrs>[^)]*)\)\]\s*(?P<params>(?:#\[param\([^)]*\)\]\s*)*)pub\s+(?:async\s+)?fn\s+(?P<fn_name>\w+)\s*\("
        ).map_err(|e| RuneError::Discovery(format!("Invalid regex: {}", e)))?;

        for cap in tool_pattern.captures_iter(content) {
            let fn_name = cap.name("fn_name").map(|m| m.as_str()).unwrap_or("main");
            let tool_attrs = cap.name("tool_attrs").map(|m| m.as_str()).unwrap_or("");
            let params_str = cap.name("params").map(|m| m.as_str()).unwrap_or("");
            let docs = cap.name("docs").map(|m| m.as_str()).unwrap_or("");

            // Parse tool attributes
            let tool_meta = self.parse_tool_attributes(tool_attrs);

            // Parse parameter attributes
            let params = self.parse_param_attributes(params_str);

            // Extract description from docs if not in attributes
            let description = tool_meta
                .description
                .or_else(|| self.extract_doc_description(docs))
                .unwrap_or_else(|| format!("Rune tool: {}", fn_name));

            // Build the tool
            let mut tool = RuneTool::new(fn_name, path.to_path_buf())
                .with_description(&description)
                .with_entry_point(fn_name);

            // Apply optional attributes
            if let Some(version) = tool_meta.version {
                tool = tool.with_version(version);
            }
            if !tool_meta.tags.is_empty() {
                tool = tool.with_tags(tool_meta.tags);
            }

            // Build input schema from params
            if !params.is_empty() {
                let schema = self.build_schema_from_params(&params);
                tool = tool.with_schema(schema);
            }

            tools.push(tool);
        }

        if tools.is_empty() {
            warn!(
                "File {} has #[tool(...)] markers but no valid tools found",
                path.display()
            );
        }

        Ok(tools)
    }

    /// Parse tool attributes from `#[tool(desc = "...", tags = ["a", "b"])]`
    fn parse_tool_attributes(&self, attrs: &str) -> ToolMetadata {
        let mut meta = ToolMetadata::default();

        // Parse desc/description
        if let Some(desc) = self.extract_string_attr(attrs, "desc")
            .or_else(|| self.extract_string_attr(attrs, "description"))
        {
            meta.description = Some(desc);
        }

        // Parse version
        if let Some(version) = self.extract_string_attr(attrs, "version") {
            meta.version = Some(version);
        }

        // Parse tags (array format)
        if let Some(tags_str) = self.extract_array_attr(attrs, "tags") {
            meta.tags = tags_str;
        }

        // Parse category
        if let Some(category) = self.extract_string_attr(attrs, "category") {
            meta.tags.insert(0, category);
        }

        meta
    }

    /// Parse parameter attributes from multiple `#[param(...)]` blocks
    fn parse_param_attributes(&self, params_str: &str) -> Vec<ParamInfo> {
        let mut params = Vec::new();

        // Match each #[param(...)] block
        let param_pattern = Regex::new(r"#\[param\(([^)]*)\)\]").unwrap();

        for cap in param_pattern.captures_iter(params_str) {
            if let Some(attrs) = cap.get(1) {
                let attrs_str = attrs.as_str();

                let name = self.extract_string_attr(attrs_str, "name").unwrap_or_default();
                let type_hint = self.extract_string_attr(attrs_str, "type")
                    .or_else(|| self.extract_string_attr(attrs_str, "ty"));
                let description = self.extract_string_attr(attrs_str, "desc")
                    .or_else(|| self.extract_string_attr(attrs_str, "description"));
                let required = self.extract_bool_attr(attrs_str, "required").unwrap_or(true);

                if !name.is_empty() {
                    params.push(ParamInfo {
                        name,
                        type_hint,
                        description,
                        required,
                    });
                }
            }
        }

        params
    }

    /// Extract a string attribute like `key = "value"`
    fn extract_string_attr(&self, attrs: &str, key: &str) -> Option<String> {
        let pattern = format!(r#"{}[\s]*=[\s]*"([^"]*)""#, key);
        let re = Regex::new(&pattern).ok()?;
        re.captures(attrs).and_then(|c| c.get(1)).map(|m| m.as_str().to_string())
    }

    /// Extract a boolean attribute like `key = true` or just `key`
    fn extract_bool_attr(&self, attrs: &str, key: &str) -> Option<bool> {
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
            if re.is_match(attrs) && !attrs.contains(&format!("{} =", key)) {
                return Some(true);
            }
        }

        None
    }

    /// Extract an array attribute like `tags = ["a", "b"]`
    fn extract_array_attr(&self, attrs: &str, key: &str) -> Option<Vec<String>> {
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

    /// Extract description from doc comments
    fn extract_doc_description(&self, docs: &str) -> Option<String> {
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

    /// Build JSON schema from parameter info
    fn build_schema_from_params(&self, params: &[ParamInfo]) -> Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in params {
            let mut prop = serde_json::Map::new();

            // Map type hint to JSON Schema type
            let json_type = match param.type_hint.as_deref() {
                Some("string") | Some("str") | Some("String") => "string",
                Some("number") | Some("int") | Some("integer") | Some("i32") | Some("i64") => "integer",
                Some("float") | Some("f32") | Some("f64") => "number",
                Some("bool") | Some("boolean") => "boolean",
                Some("array") | Some("list") | Some("Vec") => "array",
                Some("object") | Some("map") | Some("Object") => "object",
                _ => "string", // Default to string
            };

            prop.insert("type".to_string(), Value::String(json_type.to_string()));

            if let Some(desc) = &param.description {
                prop.insert("description".to_string(), Value::String(desc.clone()));
            }

            properties.insert(param.name.clone(), Value::Object(prop));

            if param.required {
                required.push(Value::String(param.name.clone()));
            }
        }

        json!({
            "type": "object",
            "properties": properties,
            "required": required
        })
    }

    /// Parse a single-tool file (legacy format)
    fn parse_single_tool_file(&self, content: &str, path: &Path) -> Result<RuneTool, RuneError> {
        // Extract tool name from filename (without extension)
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| RuneError::Discovery("Invalid filename".to_string()))?
            .to_string();

        // Parse metadata from doc comments
        let metadata = self.parse_legacy_metadata(content);

        let mut tool = RuneTool::new(&name, path.to_path_buf());

        // Apply parsed metadata
        if let Some(desc) = metadata.description {
            tool = tool.with_description(desc);
        }
        if let Some(entry) = metadata.entry_point {
            tool = tool.with_entry_point(entry);
        }
        if let Some(version) = metadata.version {
            tool = tool.with_version(version);
        }
        if !metadata.tags.is_empty() {
            tool = tool.with_tags(metadata.tags);
        }
        if let Some(schema) = metadata.input_schema {
            tool = tool.with_schema(schema);
        }

        Ok(tool)
    }

    /// Parse metadata from legacy Rune file doc comments
    fn parse_legacy_metadata(&self, content: &str) -> ToolMetadata {
        let mut metadata = ToolMetadata::default();
        let mut params: Vec<ParamInfo> = Vec::new();

        // Look for doc comments at the top of the file
        for line in content.lines() {
            let line = line.trim();

            if line.starts_with("//!") {
                let comment = line.trim_start_matches("//!").trim();

                // First non-directive doc comment is the description
                if !comment.starts_with('@') && metadata.description.is_none() && !comment.is_empty()
                {
                    metadata.description = Some(comment.to_string());
                } else if let Some(rest) = comment.strip_prefix("@entry ") {
                    metadata.entry_point = Some(rest.trim().to_string());
                } else if let Some(rest) = comment.strip_prefix("@version ") {
                    metadata.version = Some(rest.trim().to_string());
                } else if let Some(rest) = comment.strip_prefix("@tags ") {
                    metadata.tags = rest.split(',').map(|s| s.trim().to_string()).collect();
                } else if let Some(rest) = comment.strip_prefix("@param ") {
                    // Parse parameter for schema
                    let parts: Vec<&str> = rest.splitn(3, ' ').collect();
                    if !parts.is_empty() {
                        params.push(ParamInfo {
                            name: parts[0].to_string(),
                            type_hint: parts.get(1).map(|s| s.to_string()),
                            description: parts.get(2).map(|s| s.to_string()),
                            required: true,
                        });
                    }
                }
            } else if !line.starts_with("//") && !line.is_empty() {
                // Stop parsing at first non-comment line
                break;
            }
        }

        // Build input schema from params
        if !params.is_empty() {
            metadata.input_schema = Some(self.build_schema_from_params(&params));
        }

        metadata
    }
}

/// Parsed metadata from Rune file
#[derive(Default)]
struct ToolMetadata {
    description: Option<String>,
    entry_point: Option<String>,
    version: Option<String>,
    tags: Vec<String>,
    input_schema: Option<Value>,
}

/// Parameter information
struct ParamInfo {
    name: String,
    type_hint: Option<String>,
    description: Option<String>,
    required: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_discovery_empty_dir() {
        let temp = TempDir::new().unwrap();
        let config = RuneDiscoveryConfig {
            tool_directories: vec![temp.path().to_path_buf()],
            extensions: vec!["rn".to_string()],
            recursive: true,
        };
        let discovery = ToolDiscovery::new(config);
        let tools = discovery.discover_all().unwrap();
        assert!(tools.is_empty());
    }

    #[test]
    fn test_discovery_finds_rn_file() {
        let temp = TempDir::new().unwrap();
        let tool_file = temp.path().join("hello.rn");
        fs::write(&tool_file, "//! Greet someone\npub fn main() {}").unwrap();

        let config = RuneDiscoveryConfig {
            tool_directories: vec![temp.path().to_path_buf()],
            extensions: vec!["rn".to_string()],
            recursive: true,
        };
        let discovery = ToolDiscovery::new(config);
        let tools = discovery.discover_all().unwrap();

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "hello");
        assert_eq!(tools[0].description, "Greet someone");
    }

    #[test]
    fn test_parse_metadata_with_directives() {
        let content = r#"//! My awesome tool
//! @entry run
//! @version 1.0.0
//! @tags ai, transform
//! @param input string The input text
//! @param count number How many times

pub fn run(input, count) {
    // ...
}
"#;
        let discovery = ToolDiscovery::new(RuneDiscoveryConfig::default());
        let metadata = discovery.parse_legacy_metadata(content);

        assert_eq!(metadata.description, Some("My awesome tool".to_string()));
        assert_eq!(metadata.entry_point, Some("run".to_string()));
        assert_eq!(metadata.version, Some("1.0.0".to_string()));
        assert_eq!(metadata.tags, vec!["ai", "transform"]);
        assert!(metadata.input_schema.is_some());
    }

    #[test]
    fn test_multi_tool_file() {
        let temp = TempDir::new().unwrap();
        let tool_file = temp.path().join("notes.rn");
        fs::write(
            &tool_file,
            r#"
/// Create a new note
#[tool(desc = "Creates a note with title and content")]
#[param(name = "title", type = "string", desc = "Note title")]
#[param(name = "content", type = "string", desc = "Note content")]
pub fn create_note(title, content) {
    // Create note logic
}

/// Search notes by query
#[tool(desc = "Search notes by query", tags = ["search", "notes"])]
#[param(name = "query", type = "string", desc = "Search query")]
#[param(name = "limit", type = "integer", desc = "Max results", required = false)]
pub fn search_notes(query, limit) {
    // Search logic
}
"#,
        )
        .unwrap();

        let config = RuneDiscoveryConfig {
            tool_directories: vec![temp.path().to_path_buf()],
            extensions: vec!["rn".to_string()],
            recursive: true,
        };
        let discovery = ToolDiscovery::new(config);
        let tools = discovery.discover_all().unwrap();

        assert_eq!(tools.len(), 2);

        // Check create_note tool
        let create_tool = tools.iter().find(|t| t.name == "create_note").unwrap();
        assert_eq!(
            create_tool.description,
            "Creates a note with title and content"
        );
        assert_eq!(create_tool.entry_point, "create_note");

        // Check search_notes tool
        let search_tool = tools.iter().find(|t| t.name == "search_notes").unwrap();
        assert_eq!(search_tool.description, "Search notes by query");
        assert!(search_tool.tags.contains(&"search".to_string()));
        assert!(search_tool.tags.contains(&"notes".to_string()));
    }

    #[test]
    fn test_mixed_doc_and_attr_description() {
        let content = r#"
/// This is from doc comment
#[tool(desc = "This is from attribute")]
pub fn my_tool() {}
"#;
        let discovery = ToolDiscovery::new(RuneDiscoveryConfig::default());
        let tools = discovery
            .parse_multi_tool_file(content, Path::new("test.rn"))
            .unwrap();

        assert_eq!(tools.len(), 1);
        // Attribute description takes precedence
        assert_eq!(tools[0].description, "This is from attribute");
    }

    #[test]
    fn test_doc_only_description() {
        let content = r#"
/// A helpful tool description
#[tool()]
pub fn my_tool() {}
"#;
        let discovery = ToolDiscovery::new(RuneDiscoveryConfig::default());
        let tools = discovery
            .parse_multi_tool_file(content, Path::new("test.rn"))
            .unwrap();

        assert_eq!(tools.len(), 1);
        // Falls back to doc comment
        assert_eq!(tools[0].description, "A helpful tool description");
    }

    #[test]
    fn test_async_function_discovery() {
        let content = r#"
#[tool(desc = "Async tool")]
pub async fn async_tool() {}
"#;
        let discovery = ToolDiscovery::new(RuneDiscoveryConfig::default());
        let tools = discovery
            .parse_multi_tool_file(content, Path::new("test.rn"))
            .unwrap();

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "async_tool");
    }

    #[test]
    fn test_extract_string_attr() {
        let discovery = ToolDiscovery::new(RuneDiscoveryConfig::default());

        assert_eq!(
            discovery.extract_string_attr(r#"desc = "hello world""#, "desc"),
            Some("hello world".to_string())
        );
        assert_eq!(
            discovery.extract_string_attr(r#"desc="no spaces""#, "desc"),
            Some("no spaces".to_string())
        );
        assert_eq!(
            discovery.extract_string_attr(r#"other = "value""#, "desc"),
            None
        );
    }

    #[test]
    fn test_extract_array_attr() {
        let discovery = ToolDiscovery::new(RuneDiscoveryConfig::default());

        let tags = discovery.extract_array_attr(r#"tags = ["a", "b", "c"]"#, "tags");
        assert_eq!(tags, Some(vec!["a".to_string(), "b".to_string(), "c".to_string()]));

        let empty = discovery.extract_array_attr(r#"tags = []"#, "tags");
        assert_eq!(empty, Some(vec![]));
    }
}
