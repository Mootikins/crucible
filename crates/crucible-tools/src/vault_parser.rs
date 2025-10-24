//! Vault Parser - Phase 1A TDD Implementation
//!
//! This module provides functionality to parse markdown files and extract frontmatter.
//! Implemented to make the failing tests pass with minimal functionality.

use crate::vault_types::{VaultFile, VaultError, VaultResult};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Parser for markdown files with YAML frontmatter
#[derive(Debug, Clone)]
pub struct VaultParser {
    /// Whether to validate frontmatter strictly
    strict_mode: bool,
}

impl VaultParser {
    /// Create a new vault parser
    pub fn new() -> Self {
        Self {
            strict_mode: false,
        }
    }

    /// Create a parser with strict frontmatter validation
    pub fn strict() -> Self {
        Self {
            strict_mode: true,
        }
    }

    /// Parse a markdown file and extract metadata
    pub async fn parse_file(&self, file_path: &str) -> VaultResult<VaultFile> {
        let file_path = file_path.to_string();

        // Check if file exists first
        if !std::path::Path::new(&file_path).exists() {
            return Err(VaultError::FileNotFound(file_path));
        }

        // Read file content in blocking task
        let content = tokio::task::spawn_blocking({
            let file_path = file_path.clone();
            move || {
                fs::read_to_string(&file_path)
            }
        }).await.map_err(|e| VaultError::HashError(format!("Task join error: {}", e)))??;

        // Parse the content
        self.parse_content(file_path, content).await
    }

    /// Parse markdown content and extract frontmatter
    pub async fn parse_content(&self, file_path: String, content: String) -> VaultResult<VaultFile> {
        let (frontmatter, markdown_content) = self.extract_frontmatter(&content)?;

        // Parse frontmatter into structured data
        let parsed_frontmatter = self.parse_frontmatter_yaml(&frontmatter)?;

        // Create metadata
        let mut metadata = crate::vault_types::FileMetadata::new();

        // Normalize frontmatter (tags, dates, etc.)
        let normalized_frontmatter = self.normalize_frontmatter(&parsed_frontmatter);
        metadata.frontmatter = normalized_frontmatter;
        metadata.size = content.len() as u64;

        // Extract title from content or filename
        metadata.title = self.extract_title(&markdown_content, &file_path);

        // Extract and parse dates from frontmatter
        metadata.created = self.extract_created_date(&metadata.frontmatter);

        // Get file modification time
        if let Ok(metadata_fs) = fs::metadata(&file_path) {
            if let Ok(modified) = metadata_fs.modified() {
                metadata.modified = chrono::DateTime::from(modified);
            }
        }

        // Calculate hash
        let hash = self.calculate_content_hash(&content);

        // Create vault file
        let path = std::path::PathBuf::from(file_path);
        let vault_file = VaultFile {
            path,
            metadata,
            content: markdown_content,
            hash,
        };

        Ok(vault_file)
    }

    /// Extract frontmatter and content from markdown
    fn extract_frontmatter(&self, content: &str) -> VaultResult<(String, String)> {
        // Check if content starts with frontmatter delimiter
        if !content.starts_with("---") {
            // No frontmatter, return empty frontmatter and full content
            return Ok((String::new(), content.trim().to_string()));
        }

        // Find the end of frontmatter
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() < 2 {
            return Err(VaultError::FrontmatterParseError(
                "Frontmatter is incomplete".to_string()
            ));
        }

        let mut frontmatter_lines = Vec::new();
        let mut content_lines = Vec::new();
        let mut in_frontmatter = false;
        let mut frontmatter_found = false;

        for (i, line) in lines.iter().enumerate() {
            if *line == "---" {
                if !in_frontmatter && i == 0 {
                    // Start of frontmatter
                    in_frontmatter = true;
                    continue;
                } else if in_frontmatter {
                    // End of frontmatter
                    in_frontmatter = false;
                    frontmatter_found = true;
                    continue;
                }
            }

            if in_frontmatter {
                frontmatter_lines.push(*line);
            } else if frontmatter_found || i > 0 {
                content_lines.push(*line);
            }
        }

        if !frontmatter_found {
            // No proper frontmatter found
            return Ok((String::new(), content.trim().to_string()));
        }

        let frontmatter = frontmatter_lines.join("\n");
        let content = content_lines.join("\n").trim().to_string();

        Ok((frontmatter, content))
    }

    /// Parse YAML frontmatter into structured data
    fn parse_frontmatter_yaml(&self, frontmatter: &str) -> VaultResult<HashMap<String, Value>> {
        if frontmatter.trim().is_empty() {
            return Ok(HashMap::new());
        }

        let parsed: serde_yaml::Value = serde_yaml::from_str(frontmatter)
            .map_err(|e| VaultError::FrontmatterParseError(
                format!("YAML parsing failed: {}", e)
            ))?;

        // Convert YAML value to JSON value
        let json_value: Value = serde_yaml_to_json(&parsed);

        // Convert to HashMap
        if let Value::Object(map) = json_value {
            Ok(map.into_iter().collect())
        } else {
            Err(VaultError::FrontmatterParseError(
                "Frontmatter must be a mapping/object".to_string()
            ))
        }
    }

    /// Extract title from content or filename
    fn extract_title(&self, content: &str, file_path: &str) -> Option<String> {
        // First try to extract from first heading
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                // Remove leading # and whitespace
                let title = trimmed.trim_start_matches('#').trim();
                if !title.is_empty() {
                    return Some(title.to_string());
                }
            }
        }

        // Fallback to filename
        std::path::Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    }

    /// Calculate hash of content for change detection
    fn calculate_content_hash(&self, content: &str) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Normalize frontmatter values (tags, dates, etc.)
    fn normalize_frontmatter(&self, frontmatter: &std::collections::HashMap<String, Value>) -> std::collections::HashMap<String, Value> {
        let mut normalized = frontmatter.clone();

        // Normalize tags array
        if let Some(tags_value) = normalized.get_mut("tags") {
            if let Some(tags_array) = tags_value.as_array_mut() {
                let normalized_tags: Vec<Value> = tags_array.iter()
                    .filter_map(|tag| tag.as_str())
                    .map(|tag| self.normalize_tag(tag))
                    .filter(|tag| !tag.is_empty())
                    .map(|tag| Value::String(tag))
                    .collect();
                *tags_value = Value::Array(normalized_tags);
            }
        }

        normalized
    }

    /// Normalize a single tag
    fn normalize_tag(&self, tag: &str) -> String {
        tag.trim()
            .to_lowercase()
            .replace(' ', "-")
            .replace('_', "-")
            .replace("--", "-")
            .trim_matches('-')
            .to_string()
    }

    /// Extract created date from frontmatter
    fn extract_created_date(&self, frontmatter: &std::collections::HashMap<String, Value>) -> Option<chrono::DateTime<chrono::Utc>> {
        // Try different date field names
        let date_fields = ["created", "date", "published", "posted"];

        for field in &date_fields {
            if let Some(date_value) = frontmatter.get(*field) {
                if let Some(date_str) = date_value.as_str() {
                    if let Ok(parsed_date) = self.parse_date_string(date_str) {
                        return Some(parsed_date);
                    }
                }
            }
        }

        None
    }

    /// Parse date string in various formats
    fn parse_date_string(&self, date_str: &str) -> Result<chrono::DateTime<chrono::Utc>, Box<dyn std::error::Error>> {
        // Try ISO 8601 format first
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_str) {
            return Ok(dt.with_timezone(&chrono::Utc));
        }

        // Try YYYY-MM-DD format
        if let Ok(dt) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            return Ok(dt.and_hms_opt(0, 0, 0).unwrap().and_utc());
        }

        // Try other common formats
        let formats = [
            "%Y-%m-%d %H:%M:%S",
            "%Y/%m/%d",
            "%m/%d/%Y",
            "%d-%m-%Y",
            "%B %d, %Y",
            "%b %d, %Y",
        ];

        for format in &formats {
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(date_str, format) {
                return Ok(dt.and_utc());
            }
            if let Ok(date) = chrono::NaiveDate::parse_from_str(date_str, format) {
                return Ok(date.and_hms_opt(0, 0, 0).unwrap().and_utc());
            }
        }

        Err(format!("Unable to parse date: {}", date_str).into())
    }
}

/// Convert YAML Value to JSON Value
fn serde_yaml_to_json(yaml_value: &serde_yaml::Value) -> Value {
    match yaml_value {
        serde_yaml::Value::Null => Value::Null,
        serde_yaml::Value::Bool(b) => Value::Bool(*b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(serde_json::Number::from(i))
            } else if let Some(u) = n.as_u64() {
                Value::Number(serde_json::Number::from(u))
            } else if let Some(f) = n.as_f64() {
                Value::Number(serde_json::Number::from_f64(f).unwrap_or_else(|| serde_json::Number::from(0)))
            } else {
                Value::Null
            }
        }
        serde_yaml::Value::String(s) => Value::String(s.clone()),
        serde_yaml::Value::Sequence(seq) => {
            Value::Array(seq.iter().map(serde_yaml_to_json).collect())
        }
        serde_yaml::Value::Mapping(map) => {
            let mut json_map = serde_json::Map::new();
            for (key, value) in map {
                if let serde_yaml::Value::String(key_str) = key {
                    json_map.insert(key_str.clone(), serde_yaml_to_json(value));
                }
            }
            Value::Object(json_map)
        }
        serde_yaml::Value::Tagged(_) => {
            // Handle tagged values by converting to null (or you could implement proper handling)
            Value::Null
        }
    }
}

impl Default for VaultParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[tokio::test]
    async fn test_parser_creates_successfully() {
        let parser = VaultParser::new();
        assert!(!parser.strict_mode);

        let strict_parser = VaultParser::strict();
        assert!(strict_parser.strict_mode);
    }

    #[tokio::test]
    async fn test_parse_content_without_frontmatter() {
        let parser = VaultParser::new();
        let content = "# Simple Note\n\nThis is a note without frontmatter.";

        let result = parser.parse_content("test.md".to_string(), content.to_string()).await.unwrap();

        assert_eq!(result.get_title(), "Simple Note");
        assert!(result.metadata.frontmatter.is_empty());
        assert_eq!(result.content, "# Simple Note\n\nThis is a note without frontmatter.");
    }

    #[tokio::test]
    async fn test_parse_content_with_frontmatter() {
        let parser = VaultParser::new();
        let content = r#"---
type: meta
tags: [test, example]
created: 2025-01-01
---
# Test Note

This is a note with frontmatter."#;

        let result = parser.parse_content("test.md".to_string(), content.to_string()).await.unwrap();

        assert_eq!(result.get_title(), "Test Note");
        assert_eq!(result.get_type(), Some("meta".to_string()));
        assert_eq!(result.get_tags(), vec!["test", "example"]);
        assert!(result.metadata.frontmatter.contains_key("created"));
        assert_eq!(result.content, "# Test Note\n\nThis is a note with frontmatter.");
    }

    #[tokio::test]
    async fn test_parse_file_not_found() {
        let parser = VaultParser::new();
        let result = parser.parse_file("/nonexistent/file.md").await;

        assert!(result.is_err());
        match result.unwrap_err() {
            VaultError::FileNotFound(_) => {}, // Expected
            other => panic!("Expected FileNotFound, got: {:?}", other),
        }
    }
}