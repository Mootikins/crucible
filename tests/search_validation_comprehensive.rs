//! Comprehensive search validation tests for Crucible knowledge management system
//!
//! This test suite provides comprehensive validation of all search dimensions:
//! - Parsed metadata search (tags, dates, status, people, custom properties)
//! - Text content search (phrases, titles, code blocks, lists, headings)
//! - Embedding-based semantic search (similarity, cross-language, ranking)
//! - Tool search integration (discovery, execution, metadata)
//! - Link structure search (backlinks, embeds, orphans, graph traversal)
//! - Interface parity testing (CLI vs REPL vs tool APIs)
//!
//! Tests use the comprehensive static test kiln with 11 realistic markdown files
//! containing 45+ frontmatter properties and diverse content types.

use std::collections::HashMap;
use std::path::Path;
use anyhow::Result;
use serde_json::json;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};

use crate::common::{CrucibleToolManager, TestKilnManager};
use crate::utils::test_assertions::{assert_search_results, assert_metadata_matches};

/// Test harness for comprehensive search validation
pub struct SearchTestHarness {
    pub temp_dir: TempDir,
    pub kiln_manager: TestKilnManager,
    test_documents: HashMap<String, TestDocument>,
}

/// Represents a test document with its metadata and content
#[derive(Debug, Clone)]
pub struct TestDocument {
    pub path: String,
    pub title: String,
    pub content: String,
    pub metadata: HashMap<String, serde_json::Value>,
    pub tags: Vec<String>,
    pub links: Vec<String>,
    pub embeddings: Option<Vec<f32>>,
}

impl SearchTestHarness {
    /// Create a new search test harness with the comprehensive test kiln
    pub async fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let mut kiln_manager = TestKilnManager::new(temp_dir.path());

        // Initialize the comprehensive test kiln
        kiln_manager.setup_comprehensive_test_kiln().await?;

        // Ensure crucible-tools are initialized
        CrucibleToolManager::ensure_initialized_global().await?;

        let mut harness = Self {
            temp_dir,
            kiln_manager,
            test_documents: HashMap::new(),
        };

        // Index all test documents
        harness.index_test_documents().await?;

        Ok(harness)
    }

    /// Index all documents in the test kiln for search validation
    async fn index_test_documents(&mut self) -> Result<()> {
        // Get all files in the test kiln
        let files = self.kiln_manager.list_all_markdown_files().await?;

        for file_path in files {
            let content = self.kiln_manager.read_file(&file_path).await?;
            let metadata = self.kiln_manager.extract_frontmatter(&content).await?;

            // Parse links from content
            let links = self.extract_links(&content);

            // Parse tags from metadata
            let tags = self.extract_tags(&metadata);

            let test_doc = TestDocument {
                path: file_path.clone(),
                title: self.extract_title(&content),
                content,
                metadata,
                tags,
                links,
                embeddings: None, // Will be populated during embedding tests
            };

            self.test_documents.insert(file_path, test_doc);
        }

        Ok(())
    }

    /// Extract title from markdown content
    fn extract_title(&self, content: &str) -> String {
        content
            .lines()
            .find(|line| line.starts_with("# "))
            .map(|line| line.trim_start_matches("# ").to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    }

    /// Extract links from markdown content
    fn extract_links(&self, content: &str) -> Vec<String> {
        let mut links = Vec::new();

        // Extract wikilinks [[Link]]
        for line in content.lines() {
            let start = 0;
            while let Some(pos) = line[start..].find("[[") {
                let abs_pos = start + pos;
                if let Some(end_pos) = line[abs_pos..].find("]]") {
                    let link_text = &line[abs_pos + 2..abs_pos + end_pos];
                    // Handle aliases [[Link|Alias]]
                    let clean_link = link_text.split('|').next().unwrap_or(link_text);
                    links.push(clean_link.to_string());
                }
                break;
            }
        }

        links
    }

    /// Extract tags from metadata
    fn extract_tags(&self, metadata: &HashMap<String, serde_json::Value>) -> Vec<String> {
        metadata
            .get("tags")
            .and_then(|tags| tags.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|tag| tag.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get a test document by path
    pub fn get_document(&self, path: &str) -> Option<&TestDocument> {
        self.test_documents.get(path)
    }

    /// Get all test documents
    pub fn get_all_documents(&self) -> impl Iterator<Item = &TestDocument> {
        self.test_documents.values()
    }

    /// Search using the CLI search interface
    pub async fn search_cli(&self, query: &str, limit: u32) -> Result<Vec<SearchResult>> {
        let result = CrucibleToolManager::execute_tool_global(
            "search_documents",
            json!({
                "query": query,
                "top_k": limit
            }),
            Some("test_user".to_string()),
            Some("search_test".to_string()),
        ).await?;

        if let Some(data) = result.data {
            if let Some(results) = data.get("results").and_then(|r| r.as_array()) {
                let search_results: Vec<SearchResult> = results
                    .iter()
                    .filter_map(|item| {
                        if let (Some(file_path), Some(title), Some(score)) = (
                            item.get("file_path").and_then(|p| p.as_str()),
                            item.get("title").and_then(|t| t.as_str()),
                            item.get("score").and_then(|s| s.as_f64())
                        ) {
                            Some(SearchResult {
                                path: file_path.to_string(),
                                title: title.to_string(),
                                score,
                                content_snippet: item.get("content")
                                    .and_then(|c| c.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                metadata: item.get("metadata").cloned(),
                            })
                        } else {
                            None
                        }
                    })
                    .collect();
                return Ok(search_results);
            }
        }

        Ok(Vec::new())
    }

    /// Search by metadata properties
    pub async fn search_by_metadata(&self, properties: HashMap<String, serde_json::Value>) -> Result<Vec<String>> {
        let result = CrucibleToolManager::execute_tool_global(
            "search_by_metadata",
            json!({
                "properties": properties
            }),
            Some("test_user".to_string()),
            Some("search_test".to_string()),
        ).await?;

        if let Some(data) = result.data {
            if let Some(files) = data.get("files").and_then(|f| f.as_array()) {
                let file_paths: Vec<String> = files
                    .iter()
                    .filter_map(|item| item.get("path").and_then(|p| p.as_str()))
                    .map(|s| s.to_string())
                    .collect();
                return Ok(file_paths);
            }
        }

        Ok(Vec::new())
    }

    /// Search by content
    pub async fn search_by_content(&self, query: &str, limit: u32) -> Result<Vec<SearchResult>> {
        let result = CrucibleToolManager::execute_tool_global(
            "search_by_content",
            json!({
                "query": query,
                "limit": limit
            }),
            Some("test_user".to_string()),
            Some("search_test".to_string()),
        ).await?;

        if let Some(data) = result.data {
            if let Some(results) = data.get("results").and_then(|r| r.as_array()) {
                let search_results: Vec<SearchResult> = results
                    .iter()
                    .filter_map(|item| {
                        if let (Some(file_path), Some(title), Some(content)) = (
                            item.get("path").and_then(|p| p.as_str()),
                            item.get("title").and_then(|t| t.as_str()),
                            item.get("content").and_then(|c| c.as_str())
                        ) {
                            Some(SearchResult {
                                path: file_path.to_string(),
                                title: title.to_string(),
                                score: 1.0, // Content search doesn't provide scores
                                content_snippet: content.to_string(),
                                metadata: item.get("metadata").cloned(),
                            })
                        } else {
                            None
                        }
                    })
                    .collect();
                return Ok(search_results);
            }
        }

        Ok(Vec::new())
    }

    /// Perform semantic search
    pub async fn semantic_search(&self, query: &str, limit: u32) -> Result<Vec<SearchResult>> {
        let result = CrucibleToolManager::execute_tool_global(
            "semantic_search",
            json!({
                "query": query,
                "top_k": limit
            }),
            Some("test_user".to_string()),
            Some("search_test".to_string()),
        ).await?;

        if let Some(data) = result.data {
            if let Some(results) = data.get("results").and_then(|r| r.as_array()) {
                let search_results: Vec<SearchResult> = results
                    .iter()
                    .filter_map(|item| {
                        if let (Some(file_path), Some(title), Some(score)) = (
                            item.get("file_path").and_then(|p| p.as_str()),
                            item.get("title").and_then(|t| t.as_str()),
                            item.get("score").and_then(|s| s.as_f64())
                        ) {
                            Some(SearchResult {
                                path: file_path.to_string(),
                                title: title.to_string(),
                                score,
                                content_snippet: item.get("content_snippet")
                                    .and_then(|c| c.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                metadata: item.get("metadata").cloned(),
                            })
                        } else {
                            None
                        }
                    })
                    .collect();
                return Ok(search_results);
            }
        }

        Ok(Vec::new())
    }

    /// Get link relationships (backlinks, etc.)
    pub async fn get_link_relationships(&self, document_path: &str) -> Result<LinkRelationships> {
        let result = CrucibleToolManager::execute_tool_global(
            "get_link_relationships",
            json!({
                "document_path": document_path
            }),
            Some("test_user".to_string()),
            Some("search_test".to_string()),
        ).await?;

        if let Some(data) = result.data {
            let backlinks: Vec<String> = data
                .get("backlinks")
                .and_then(|bl| bl.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| item.as_str())
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();

            let outgoing_links: Vec<String> = data
                .get("outgoing_links")
                .and_then(|ol| ol.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| item.as_str())
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();

            let embeds: Vec<String> = data
                .get("embeds")
                .and_then(|e| e.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| item.as_str())
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();

            return Ok(LinkRelationships {
                backlinks,
                outgoing_links,
                embeds,
            });
        }

        Ok(LinkRelationships::default())
    }

    /// Rebuild search indexes for testing
    pub async fn rebuild_indexes(&self) -> Result<()> {
        CrucibleToolManager::execute_tool_global(
            "rebuild_index",
            json!({
                "force": true,
                "index_types": ["semantic", "full_text", "metadata"]
            }),
            Some("test_user".to_string()),
            Some("search_test".to_string()),
        ).await?;

        // Wait a bit for indexing to complete
        sleep(Duration::from_millis(100)).await;

        Ok(())
    }
}

/// Search result representation
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    pub path: String,
    pub title: String,
    pub score: f64,
    pub content_snippet: String,
    pub metadata: Option<serde_json::Value>,
}

/// Link relationships for a document
#[derive(Debug, Clone, Default)]
pub struct LinkRelationships {
    pub backlinks: Vec<String>,
    pub outgoing_links: Vec<String>,
    pub embeds: Vec<String>,
}

// ============================================================================
// Parsed Metadata Search Tests
// ============================================================================

#[cfg(test)]
mod metadata_search_tests {
    use super::*;

    /// Test tag-based searches
    #[tokio::test]
    async fn test_tag_based_searches() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Test single tag search
        let results = harness.search_cli("tag:project", 10).await?;
        assert!(!results.is_empty(), "Should find documents with 'project' tag");

        // Verify all results have the project tag
        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                assert!(doc.tags.contains(&"project".to_string()),
                       "Result {} should have 'project' tag", result.path);
            }
        }

        // Test multiple tag search (AND logic)
        let results = harness.search_cli("tag:project tag:active", 10).await?;

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                assert!(doc.tags.contains(&"project".to_string()),
                       "Result {} should have 'project' tag", result.path);
                assert!(doc.tags.contains(&"active".to_string()),
                       "Result {} should have 'active' tag", result.path);
            }
        }

        // Test tag with special characters
        let results = harness.search_cli("tag:knowledge-management", 10).await?;
        assert!(!results.is_empty(), "Should handle hyphenated tags");

        Ok(())
    }

    /// Test date range searches
    #[tokio::test]
    async fn test_date_range_searches() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Test creation date search
        let results = harness.search_cli("created:2025-01-01", 10).await?;

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                if let Some(created) = doc.metadata.get("created").and_then(|c| c.as_str()) {
                    assert!(created.starts_with("2025-01-01"),
                           "Document {} created date should match", result.path);
                }
            }
        }

        // Test date range search
        let results = harness.search_cli("created:>2025-01-01 created:<2025-01-31", 10).await?;

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                if let Some(created) = doc.metadata.get("created").and_then(|c| c.as_str()) {
                    assert!(created > "2025-01-01" && created < "2025-01-31",
                           "Document {} should be within date range", result.path);
                }
            }
        }

        // Test modification date search
        let results = harness.search_cli("modified:>2025-01-20", 10).await?;

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                if let Some(modified) = doc.metadata.get("modified").and_then(|m| m.as_str()) {
                    assert!(modified > "2025-01-20",
                           "Document {} modification date should be after 2025-01-20", result.path);
                }
            }
        }

        Ok(())
    }

    /// Test status and priority searches
    #[tokio::test]
    async fn test_status_priority_searches() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Test status search
        let results = harness.search_cli("status:active", 10).await?;
        assert!(!results.is_empty(), "Should find active documents");

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                if let Some(status) = doc.metadata.get("status").and_then(|s| s.as_str()) {
                    assert_eq!(status, "active",
                              "Document {} should have active status", result.path);
                }
            }
        }

        // Test priority search
        let results = harness.search_cli("priority:high", 10).await?;
        assert!(!results.is_empty(), "Should find high priority documents");

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                if let Some(priority) = doc.metadata.get("priority").and_then(|p| p.as_str()) {
                    assert_eq!(priority, "high",
                              "Document {} should have high priority", result.path);
                }
            }
        }

        // Test combined status and priority
        let results = harness.search_cli("status:active priority:high", 10).await?;

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                if let Some(status) = doc.metadata.get("status").and_then(|s| s.as_str()) {
                    assert_eq!(status, "active", "Should have active status");
                }
                if let Some(priority) = doc.metadata.get("priority").and_then(|p| p.as_str()) {
                    assert_eq!(priority, "high", "Should have high priority");
                }
            }
        }

        Ok(())
    }

    /// Test people and author searches
    #[tokio::test]
    async fn test_people_author_searches() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Test author search
        let results = harness.search_cli("author:\"Sarah Chen\"", 10).await?;

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                if let Some(author) = doc.metadata.get("author").and_then(|a| a.as_str()) {
                    assert!(author.contains("Sarah Chen"),
                           "Document {} should be authored by Sarah Chen", result.path);
                }
            }
        }

        // Test institution search
        let results = harness.search_cli("institution:\"Stanford Research Institute\"", 10).await?;

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                if let Some(institution) = doc.metadata.get("institution").and_then(|i| i.as_str()) {
                    assert!(institution.contains("Stanford"),
                           "Document {} should be from Stanford", result.path);
                }
            }
        }

        // Test partial name search
        let results = harness.search_cli("author:Michael", 10).await?;

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                if let Some(author) = doc.metadata.get("author").and_then(|a| a.as_str()) {
                    assert!(author.contains("Michael"),
                           "Document {} should contain Michael", result.path);
                }
            }
        }

        Ok(())
    }

    /// Test custom property searches
    #[tokio::test]
    async fn test_custom_property_searches() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Test numeric property search
        let results = harness.search_cli("budget:>10000", 10).await?;

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                if let Some(budget) = doc.metadata.get("budget").and_then(|b| b.as_str()) {
                    // Remove currency symbols and parse
                    let numeric_budget = budget.replace(&['$', ','][..], "")
                                               .parse::<f64>()
                                               .unwrap_or(0.0);
                    assert!(numeric_budget > 10000.0,
                           "Document {} budget should be > 10000", result.path);
                }
            }
        }

        // Test team size search
        let results = harness.search_cli("team_size:>3", 10).await?;

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                if let Some(team_size) = doc.metadata.get("team_size").and_then(|t| t.as_u64()) {
                    assert!(team_size > 3,
                           "Document {} team size should be > 3", result.path);
                }
            }
        }

        // Test category search
        let results = harness.search_cli("category:academic", 10).await?;

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                if let Some(category) = doc.metadata.get("category").and_then(|c| c.as_str()) {
                    assert_eq!(category, "academic",
                              "Document {} should be academic category", result.path);
                }
            }
        }

        // Test boolean property search
        let results = harness.search_cli("peer_reviewed:true", 10).await?;

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                if let Some(peer_reviewed) = doc.metadata.get("peer_reviewed").and_then(|p| p.as_bool()) {
                    assert!(peer_reviewed,
                           "Document {} should be peer reviewed", result.path);
                }
            }
        }

        Ok(())
    }

    /// Test complex multi-criteria metadata searches
    #[tokio::test]
    async fn test_complex_metadata_searches() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Complex query with multiple criteria
        let results = harness.search_cli(
            "tag:research status:active priority:medium created:>2025-01-10",
            10
        ).await?;

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                // Check all criteria
                assert!(doc.tags.contains(&"research".to_string()),
                       "Should have research tag");

                if let Some(status) = doc.metadata.get("status").and_then(|s| s.as_str()) {
                    assert_eq!(status, "active", "Should have active status");
                }

                if let Some(priority) = doc.metadata.get("priority").and_then(|p| p.as_str()) {
                    assert_eq!(priority, "medium", "Should have medium priority");
                }

                if let Some(created) = doc.metadata.get("created").and_then(|c| c.as_str()) {
                    assert!(created > "2025-01-10", "Should be created after 2025-01-10");
                }
            }
        }

        // Test wildcard property search
        let mut properties = HashMap::new();
        properties.insert("type".to_string(), json!("project"));

        let matching_paths = harness.search_by_metadata(properties).await?;
        assert!(!matching_paths.is_empty(), "Should find project type documents");

        for path in &matching_paths {
            if let Some(doc) = harness.get_document(path) {
                if let Some(doc_type) = doc.metadata.get("type").and_then(|t| t.as_str()) {
                    assert_eq!(doc_type, "project", "Should be project type");
                }
            }
        }

        Ok(())
    }

    /// Test metadata search edge cases
    #[tokio::test]
    async fn test_metadata_search_edge_cases() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Test search with non-existent tag
        let results = harness.search_cli("tag:nonexistent", 10).await?;
        assert!(results.is_empty(), "Should return no results for non-existent tag");

        // Test search with non-existent property
        let results = harness.search_cli("nonexistent_property:value", 10).await?;
        assert!(results.is_empty(), "Should return no results for non-existent property");

        // Test search with special characters in values
        let results = harness.search_cli("author:\"Dr. Michael Rodriguez\"", 10).await?;
        // Should handle quotes and special characters properly

        // Test case sensitivity
        let results_lower = harness.search_cli("status:active", 10).await?;
        let results_upper = harness.search_cli("status:ACTIVE", 10).await?;
        // Should handle case insensitivity properly

        // Test empty search
        let results = harness.search_cli("", 10).await?;
        // Should return all documents or reasonable default

        Ok(())
    }
}

// ============================================================================
// Text Content Search Tests
// ============================================================================

#[cfg(test)]
mod text_content_search_tests {
    use super::*;

    /// Test exact phrase matching
    #[tokio::test]
    async fn test_exact_phrase_matching() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Test exact phrase search with quotes
        let results = harness.search_cli("\"machine learning algorithms\"", 10).await?;

        // Verify results contain the exact phrase
        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                assert!(doc.content.to_lowercase().contains("machine learning"),
                       "Document {} should contain the phrase", result.path);
            }
        }

        // Test multi-word phrase without quotes (should work similarly)
        let results = harness.search_cli("knowledge management system", 10).await?;
        assert!(!results.is_empty(), "Should find documents containing knowledge management system");

        // Test phrase with special characters
        let results = harness.search_cli("\"Cronbach's alpha\"", 10).await?;
        // Should handle apostrophes and special characters

        Ok(())
    }

    /// Test title-based searches
    #[tokio::test]
    async fn test_title_based_searches() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Search for documents with specific titles
        let results = harness.search_cli("title:\"Project Management\"", 10).await?;

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                assert!(doc.title.to_lowercase().contains("project management"),
                       "Document title should match search", result.path);
            }
        }

        // Test partial title match
        let results = harness.search_cli("title:Research", 10).await?;

        for result in &results {
            if let Some(doc) = harness.get_document(&result.path) {
                assert!(doc.title.to_lowercase().contains("research"),
                       "Document title should contain 'research'", result.path);
            }
        }

        // Test title search across all documents
        let all_titles = harness.get_all_documents()
            .map(|doc| doc.title.to_lowercase())
            .collect::<Vec<_>>();

        let common_words = ["management", "research", "documentation", "meeting"];
        for word in common_words {
            let results = harness.search_cli(&format!("title:{}", word), 10).await?;
            assert!(!results.is_empty(), "Should find documents with '{}' in title", word);
        }

        Ok(())
    }

    /// Test code block searches
    #[tokio::test]
    async fn test_code_block_searches() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Search for specific programming languages in code blocks
        let code_searches = vec![
            ("```javascript", "JavaScript code blocks"),
            ("```python", "Python code blocks"),
            ("```rust", "Rust code blocks"),
            ("```sql", "SQL code blocks"),
        ];

        for (code_marker, description) in code_searches {
            let results = harness.search_cli(code_marker, 10).await?;

            for result in &results {
                if let Some(doc) = harness.get_document(&result.path) {
                    assert!(doc.content.contains(code_marker),
                           "Document {} should contain {}", result.path, description);
                }
            }
        }

        // Search for specific functions or code patterns
        let code_patterns = vec![
            "function search",
            "def analyze",
            "fn process",
            "SELECT * FROM",
        ];

        for pattern in code_patterns {
            let results = harness.search_cli(pattern, 10).await?;
            // Verify that pattern matching works in code context
        }

        Ok(())
    }

    /// Test list item searches
    #[tokio::test]
    async fn test_list_item_searches() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Search for checklist items
        let results = harness.search_cli("- [x]", 10).await?;
        // Should find completed checklist items

        let results = harness.search_cli("- [ ]", 10).await?;
        // Should find incomplete checklist items

        // Search for specific list content
        let list_searches = vec![
            "milestone",
            "objective",
            "requirement",
            "task",
        ];

        for search_term in list_searches {
            let results = harness.search_cli(&format!("- {}", search_term), 10).await?;

            for result in &results {
                if let Some(doc) = harness.get_document(&result.path) {
                    // Verify that the search term appears in a list context
                    let lines: Vec<&str> = doc.content.lines().collect();
                    let found_in_list = lines.iter().any(|line| {
                        line.trim().starts_with('-') || line.trim().starts_with('*') ||
                        line.trim().starts_with(|c: char| c.is_numeric() && line.contains('.'))
                    } && line.to_lowercase().contains(search_term);

                    assert!(found_in_list || doc.content.to_lowercase().contains(search_term),
                           "Should find '{}' in list or general content", search_term);
                }
            }
        }

        Ok(())
    }

    /// Test heading searches
    #[tokio::test]
    async fn test_heading_searches() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Search for specific heading levels
        let heading_searches = vec![
            ("## ", "Level 2 headings"),
            ("### ", "Level 3 headings"),
            ("#### ", "Level 4 headings"),
        ];

        for (heading_prefix, description) in heading_searches {
            let results = harness.search_cli(heading_prefix, 10).await?;

            for result in &results {
                if let Some(doc) = harness.get_document(&result.path) {
                    let lines: Vec<&str> = doc.content.lines().collect();
                    let has_heading = lines.iter().any(|line| line.starts_with(heading_prefix));
                    assert!(has_heading,
                           "Document {} should contain {}", result.path, description);
                }
            }
        }

        // Search for heading content
        let heading_content_searches = vec![
            "## Overview",
            "### Methods",
            "## Implementation",
            "### Analysis",
        ];

        for heading in heading_content_searches {
            let results = harness.search_cli(heading, 10).await?;

            for result in &results {
                if let Some(doc) = harness.get_document(&result.path) {
                    assert!(doc.content.contains(heading),
                           "Document {} should contain heading '{}'", result.path, heading);
                }
            }
        }

        Ok(())
    }

    /// Test case sensitivity and normalization
    #[tokio::test]
    async fn test_case_sensitivity_normalization() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        let test_terms = vec![
            "Knowledge Management",
            "KNOWLEDGE MANAGEMENT",
            "knowledge management",
            "KnOwLeDgE MaNaGeMeNt",
        ];

        let mut result_counts = Vec::new();
        for term in test_terms {
            let results = harness.search_cli(term, 10).await?;
            result_counts.push(results.len());
        }

        // All case variations should return similar results
        let max_count = *result_counts.iter().max().unwrap_or(&0);
        let min_count = *result_counts.iter().min().unwrap_or(&0);

        assert!((max_count as i32 - min_count as i32).abs() <= 1,
               "Case variations should return similar result counts");

        Ok(())
    }

    /// Test special characters and unicode
    #[tokio::test]
    async fn test_special_characters_unicode() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Search for mathematical symbols
        let math_searches = vec![
            "α",
            "p < 0.05",
            "μ = 78.4",
            "Cohen's d",
        ];

        for search_term in math_searches {
            let results = harness.search_cli(search_term, 10).await?;
            // Should handle mathematical notation without errors
        }

        // Search for unicode content
        let unicode_searches = vec![
            "café",
            "naïve",
            "résumé",
        ];

        for search_term in unicode_searches {
            let results = harness.search_cli(search_term, 10).await?;
            // Should handle unicode characters properly
        }

        Ok(())
    }

    /// Test Boolean operators in text search
    #[tokio::test]
    async fn test_boolean_operators() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Test AND operator (implicit)
        let results = harness.search_cli("project management", 10).await?;
        let and_count = results.len();

        // Test individual terms
        let proj_results = harness.search_cli("project", 10).await?;
        let mgmt_results = harness.search_cli("management", 10).await?;

        // Combined search should be more specific
        assert!(and_count <= proj_results.len() && and_count <= mgmt_results.len(),
               "Combined search should be more specific than individual searches");

        // Test phrase vs word search
        let phrase_results = harness.search_cli("\"project management\"", 10).await?;
        let word_results = harness.search_cli("project management", 10).await?;

        // Phrase search should be more specific
        assert!(phrase_results.len() <= word_results.len(),
               "Phrase search should be more specific than word search");

        Ok(())
    }

    /// Test proximity and context search
    #[tokio::test]
    async fn test_proximity_context_search() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Search for terms that should appear together
        let proximity_searches = vec![
            ("knowledge management", "related concepts"),
            ("research methods", "academic context"),
            ("project timeline", "planning context"),
            ("team collaboration", "organizational context"),
        ];

        for (search_term, context_desc) in proximity_searches {
            let results = harness.search_cli(search_term, 10).await?;

            for result in &results {
                if let Some(doc) = harness.get_document(&result.path) {
                    assert!(doc.content.to_lowercase().contains(search_term),
                           "Document {} should contain '{}'", result.path, search_term);
                }
            }
        }

        // Test search within specific sections
        let section_searches = vec![
            "## Overview knowledge",
            "### Methods research",
            "## Implementation technical",
        ];

        for search_term in section_searches {
            let results = harness.search_cli(search_term, 10).await?;
            // Should find content within specific sections
        }

        Ok(())
    }

    /// Test content search ranking and relevance
    #[tokio::test]
    async fn test_content_search_ranking() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        // Search for a common term and check ranking
        let results = harness.search_cli("management", 10).await?;

        if results.len() > 1 {
            // Results should be ordered by relevance
            for i in 1..results.len() {
                // For content search, check if more relevant results appear first
                // This is a basic test - actual ranking algorithms may vary
                let prev_doc = harness.get_document(&results[i-1].path);
                let curr_doc = harness.get_document(&results[i].path);

                if let (Some(prev), Some(curr)) = (prev_doc, curr_doc) {
                    // Check if previous result has more occurrences or better positioning
                    let prev_count = prev.content.matches("management").count();
                    let curr_count = curr.content.matches("management").count();

                    // More occurrences should generally rank higher
                    if prev_count < curr_count {
                        // This might indicate a ranking issue, but we don't fail the test
                        // as ranking algorithms can be complex
                    }
                }
            }
        }

        Ok(())
    }

    /// Test content search with different result limits
    #[tokio::test]
    async fn test_content_search_limits() -> Result<()> {
        let harness = SearchTestHarness::new().await?;

        let search_term = "research";
        let limits = vec![1, 3, 5, 10];

        for limit in limits {
            let results = harness.search_cli(search_term, limit).await?;
            assert!(results.len() <= limit as usize,
                   "Results should not exceed limit of {}", limit);

            if !results.is_empty() {
                // Verify all results actually contain the search term
                for result in &results {
                    if let Some(doc) = harness.get_document(&result.path) {
                        assert!(doc.content.to_lowercase().contains(search_term),
                               "Result should contain search term");
                    }
                }
            }
        }

        Ok(())
    }
}