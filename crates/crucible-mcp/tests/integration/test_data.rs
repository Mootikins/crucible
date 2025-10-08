//! Test data fixtures and builders for integration tests

use serde_json::json;
use std::collections::HashMap;

/// Builder for creating test file data
pub struct TestFileBuilder {
    pub path: String,
    pub name: String,
    pub folder: String,
    pub extension: String,
    pub size: u64,
    pub created: i64,
    pub modified: i64,
    pub tags: Vec<String>,
    pub properties: HashMap<String, serde_json::Value>,
    pub content: Option<String>,
}

impl TestFileBuilder {
    pub fn new(path: &str) -> Self {
        let parts: Vec<&str> = path.rsplitn(2, '/').collect();
        let name = parts[0].to_string();
        let folder = if parts.len() > 1 { parts[1].to_string() } else { String::new() };
        let extension = name.split('.').last().unwrap_or("md").to_string();

        Self {
            path: path.to_string(),
            name,
            folder,
            extension,
            size: 1024,
            created: 1704067200000, // 2024-01-01
            modified: 1704153600000, // 2024-01-02
            tags: vec![],
            properties: HashMap::new(),
            content: None,
        }
    }

    pub fn with_tags(mut self, tags: Vec<&str>) -> Self {
        self.tags = tags.into_iter().map(String::from).collect();
        self
    }

    pub fn with_property(mut self, key: &str, value: serde_json::Value) -> Self {
        self.properties.insert(key.to_string(), value);
        self
    }

    pub fn with_content(mut self, content: &str) -> Self {
        self.content = Some(content.to_string());
        self
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size = size;
        self
    }

    pub fn build_file_info(&self) -> serde_json::Value {
        json!({
            "path": self.path,
            "name": self.name,
            "folder": self.folder,
            "extension": self.extension,
            "size": self.size,
            "created": self.created,
            "modified": self.modified
        })
    }

    pub fn build_metadata(&self) -> serde_json::Value {
        json!({
            "path": self.path,
            "properties": self.properties,
            "tags": self.tags,
            "folder": self.folder,
            "links": [],
            "backlinks": [],
            "stats": {
                "size": self.size,
                "created": self.created,
                "modified": self.modified,
                "wordCount": 150
            }
        })
    }

    pub fn build_content(&self) -> String {
        self.content.clone().unwrap_or_else(|| {
            format!(
                "---
{}---

# {}

Sample content for testing.",
                self.build_frontmatter(),
                self.name.trim_end_matches(".md")
            )
        })
    }

    fn build_frontmatter(&self) -> String {
        let mut fm = String::new();
        
        for (key, value) in &self.properties {
            fm.push_str(&format!("{}: {}
", key, value));
        }
        
        if !self.tags.is_empty() {
            fm.push_str(&format!("tags: [{}]
", self.tags.join(", ")));
        }
        
        fm
    }
}

/// Test fixtures
pub struct TestFixtures;

impl TestFixtures {
    /// Create a sample vault with diverse files for testing
    pub fn sample_vault() -> Vec<TestFileBuilder> {
        vec![
            TestFileBuilder::new("projects/project-alpha.md")
                .with_tags(vec!["project", "active"])
                .with_property("status", json!("in-progress"))
                .with_property("priority", json!("high"))
                .with_content("# Project Alpha

AI research project focusing on neural networks."),
            
            TestFileBuilder::new("projects/project-beta.md")
                .with_tags(vec!["project", "completed"])
                .with_property("status", json!("completed"))
                .with_property("priority", json!("medium")),
            
            TestFileBuilder::new("daily/2024-01-01.md")
                .with_tags(vec!["daily", "journal"])
                .with_property("mood", json!("productive")),
            
            TestFileBuilder::new("notes/machine-learning.md")
                .with_tags(vec!["ai", "research"])
                .with_property("topic", json!("ml"))
                .with_content("# Machine Learning

Deep learning and neural networks overview."),
            
            TestFileBuilder::new("notes/productivity.md")
                .with_tags(vec!["productivity", "self-improvement"])
                .with_property("topic", json!("productivity")),
        ]
    }

    /// Create files for search testing
    pub fn search_test_files() -> Vec<TestFileBuilder> {
        vec![
            TestFileBuilder::new("search/ai-research.md")
                .with_tags(vec!["ai", "research"])
                .with_property("category", json!("research"))
                .with_content("Artificial intelligence and machine learning research."),
            
            TestFileBuilder::new("search/productivity-tips.md")
                .with_tags(vec!["productivity"])
                .with_property("category", json!("tips"))
                .with_content("Tips for improving daily productivity."),
        ]
    }

    /// Sample embedding settings
    pub fn embedding_settings() -> serde_json::Value {
        json!({
            "provider": "ollama",
            "apiUrl": "http://localhost:11434",
            "apiKey": null,
            "model": "nomic-embed-text"
        })
    }

    /// Sample embedding models list
    pub fn embedding_models() -> Vec<String> {
        vec![
            "nomic-embed-text".to_string(),
            "mxbai-embed-large".to_string(),
            "all-minilm".to_string(),
        ]
    }

    /// Error responses for testing
    pub fn error_response(message: &str) -> serde_json::Value {
        json!({
            "error": message
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_builder() {
        let file = TestFileBuilder::new("test/file.md")
            .with_tags(vec!["test"])
            .with_property("key", json!("value"));
        
        assert_eq!(file.path, "test/file.md");
        assert_eq!(file.folder, "test");
        assert_eq!(file.name, "file.md");
        assert_eq!(file.extension, "md");
        assert_eq!(file.tags.len(), 1);
    }

    #[test]
    fn test_sample_vault() {
        let vault = TestFixtures::sample_vault();
        assert_eq!(vault.len(), 5);
    }
}
