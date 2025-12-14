//! Search operations tools
//!
//! This module provides semantic, text, and property search tools.

use crucible_core::{enrichment::EmbeddingProvider, traits::KnowledgeRepository};
use grep::regex::RegexMatcher;
use grep::searcher::{sinks::UTF8, Searcher};
use ignore::WalkBuilder;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{model::CallToolResult, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use walkdir::WalkDir;

/// Default value for limit parameter
fn default_limit() -> usize {
    10
}

/// Default value for case_insensitive
fn default_true() -> bool {
    true
}

#[derive(Clone)]
#[allow(missing_docs)]
pub struct SearchTools {
    kiln_path: String,
    knowledge_repo: Arc<dyn KnowledgeRepository>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
}

/// Parameters for semantic search
#[derive(Deserialize, JsonSchema)]
pub struct SemanticSearchParams {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

/// Parameters for text search
#[derive(Deserialize, JsonSchema)]
pub struct TextSearchParams {
    query: String,
    #[serde(default)]
    folder: Option<String>,
    #[serde(default = "default_true")]
    case_insensitive: bool,
    #[serde(default = "default_limit")]
    limit: usize,
}

/// Parameters for property search
#[derive(Deserialize, JsonSchema)]
pub struct PropertySearchParams {
    properties: serde_json::Value,
    #[serde(default = "default_limit")]
    limit: usize,
}

impl SearchTools {
    #[allow(missing_docs)]
    pub fn new(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        Self {
            kiln_path,
            knowledge_repo,
            embedding_provider,
        }
    }
}

#[tool_router]
impl SearchTools {
    #[tool(description = "Search notes using semantic similarity")]
    pub async fn semantic_search(
        &self,
        params: Parameters<SemanticSearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let query = params.query;
        let limit = params.limit;

        let embedding = self.embedding_provider.embed(&query).await.map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Failed to generate embedding: {e}"), None)
        })?;

        let search_results = self
            .knowledge_repo
            .search_vectors(embedding)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(format!("Search failed: {e}"), None))?;

        let results_json: Vec<serde_json::Value> = search_results
            .into_iter()
            .take(limit)
            .map(|r| {
                serde_json::json!({
                    "id": r.document_id,
                    "score": r.score,
                    "snippet": r.snippet,
                    "highlights": r.highlights
                })
            })
            .collect();

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::json!({
                "results": results_json,
                "query": query,
                "limit": limit
            }),
        )?]))
    }

    #[tool(description = "Fast full-text search across notes")]
    pub async fn text_search(
        &self,
        params: Parameters<TextSearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let query = params.query.clone();
        let folder = params.folder.clone();
        let case_insensitive = params.case_insensitive;
        let limit = params.limit;

        // Security: Validate folder to prevent traversal attacks
        let search_path = validate_folder_within_kiln(
            &self.kiln_path,
            folder.as_deref(),
        )?;

        if !search_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("Search path does not exist: {}", search_path.display()),
                None,
            ));
        }

        // Build regex matcher
        let matcher = if case_insensitive {
            RegexMatcher::new_line_matcher(&format!("(?i){}", regex::escape(&query)))
        } else {
            RegexMatcher::new_line_matcher(&regex::escape(&query))
        }
        .map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Failed to create matcher: {e}"), None)
        })?;

        let mut matches = Vec::new();
        let mut searcher = Searcher::new();
        let mut stopped_early = false;

        // Walk files
        for entry in WalkBuilder::new(&search_path)
            .standard_filters(true)
            .build()
            .filter_map(Result::ok)
        {
            if !entry.file_type().map_or(false, |ft| ft.is_file()) {
                continue;
            }

            let path = entry.path();
            if path.extension().map_or(true, |ext| ext != "md") {
                continue;
            }

            // Search this file
            let relative_path = path
                .strip_prefix(&self.kiln_path)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            let mut file_matches = Vec::new();
            let mut line_num = 0u64;

            let result = searcher.search_path(
                &matcher,
                path,
                UTF8(|lnum, line| {
                    line_num = lnum;
                    file_matches.push(serde_json::json!({
                        "path": relative_path.clone(),
                        "line_number": lnum,
                        "line_content": line.trim_end(),
                    }));

                    // Stop if we've hit the limit
                    if matches.len() + file_matches.len() >= limit {
                        Ok(false)
                    } else {
                        Ok(true)
                    }
                }),
            );

            if result.is_ok() {
                matches.extend(file_matches);
                if matches.len() >= limit {
                    stopped_early = true;
                    break;
                }
            }
        }

        let count = matches.len();
        matches.truncate(limit);

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::json!({
                "query": query,
                "matches": matches,
                "count": count,
                "truncated": stopped_early
            }),
        )?]))
    }

    #[tool(description = "Search notes by frontmatter properties (includes tags)")]
    pub async fn property_search(
        &self,
        params: Parameters<PropertySearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let search_props = params
            .properties
            .as_object()
            .ok_or_else(|| rmcp::ErrorData::invalid_params("properties must be an object", None))?;
        let limit = params.limit;

        let mut matches = Vec::new();

        // Walk all markdown files
        for entry in WalkDir::new(&self.kiln_path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
        {
            // Read file and parse frontmatter
            let content = match std::fs::read_to_string(entry.path()) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let frontmatter = match parse_yaml_frontmatter(&content) {
                Some(fm) => fm,
                None => continue,
            };

            // Check if all search properties match
            let matches_all = search_props.iter().all(|(key, search_value)| {
                frontmatter.get(key).map_or(false, |prop_value| {
                    // Handle array values as OR logic
                    if let Some(search_array) = search_value.as_array() {
                        // Property value must match any of the search values
                        if let Some(prop_array) = prop_value.as_array() {
                            // Array intersection
                            search_array.iter().any(|sv| prop_array.contains(sv))
                        } else {
                            // Single value must match any search value
                            search_array.contains(prop_value)
                        }
                    } else {
                        // Exact match
                        prop_value == search_value
                    }
                })
            });

            if matches_all {
                let relative_path = entry
                    .path()
                    .strip_prefix(&self.kiln_path)
                    .unwrap_or(entry.path())
                    .to_string_lossy()
                    .to_string();

                // Get basic stats
                let word_count = content.split_whitespace().count();

                matches.push(serde_json::json!({
                    "path": relative_path,
                    "frontmatter": frontmatter,
                    "word_count": word_count,
                }));

                if matches.len() >= limit {
                    break;
                }
            }
        }

        let count = matches.len();

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::json!({
                "properties": params.properties,
                "matches": matches,
                "count": count,
            }),
        )?]))
    }
}

// Use shared utilities for frontmatter parsing and path validation
use crate::utils::{parse_yaml_frontmatter, validate_folder_within_kiln};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // Mock implementations for testing
    struct MockKnowledgeRepository;
    struct MockEmbeddingProvider;

    #[async_trait::async_trait]
    impl crucible_core::traits::KnowledgeRepository for MockKnowledgeRepository {
        async fn get_note_by_name(
            &self,
            _name: &str,
        ) -> crucible_core::Result<Option<crucible_core::parser::ParsedNote>> {
            Ok(None)
        }

        async fn list_notes(
            &self,
            _path: Option<&str>,
        ) -> crucible_core::Result<Vec<crucible_core::traits::knowledge::NoteInfo>> {
            Ok(vec![])
        }

        async fn search_vectors(
            &self,
            _vector: Vec<f32>,
        ) -> crucible_core::Result<Vec<crucible_core::types::SearchResult>> {
            Ok(vec![])
        }
    }

    #[async_trait::async_trait]
    impl crucible_core::enrichment::EmbeddingProvider for MockEmbeddingProvider {
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.1; 384]) // Mock 384-dimensional embedding
        }

        async fn embed_batch(&self, _texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(vec![vec![0.1; 384]; _texts.len()])
        }

        fn model_name(&self) -> &str {
            "mock-model"
        }

        fn dimensions(&self) -> usize {
            384
        }
    }

    fn create_search_tools(kiln_path: String) -> SearchTools {
        let knowledge_repo = Arc::new(MockKnowledgeRepository);
        let embedding_provider = Arc::new(MockEmbeddingProvider);
        SearchTools::new(kiln_path, knowledge_repo, embedding_provider)
    }

    // ===== text_search tests =====

    #[tokio::test]
    async fn test_text_search_basic() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create test file
        fs::write(
            temp_dir.path().join("test.md"),
            "# Test Note\n\nThis contains TODO items.\n\nAnother line.",
        )
        .unwrap();

        let search_tools = create_search_tools(kiln_path);

        let params = Parameters(TextSearchParams {
            query: "TODO".to_string(),
            folder: None,
            case_insensitive: true,
            limit: 10,
        });

        let result = search_tools.text_search(params).await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["query"], "TODO");
                assert_eq!(parsed["count"], 1);

                let matches = parsed["matches"].as_array().unwrap();
                assert_eq!(matches.len(), 1);
                assert!(matches[0]["line_content"]
                    .as_str()
                    .unwrap()
                    .contains("TODO"));
            }
        }
    }

    #[tokio::test]
    async fn test_text_search_case_sensitive() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        fs::write(
            temp_dir.path().join("test.md"),
            "TODO in uppercase\ntodo in lowercase",
        )
        .unwrap();

        let search_tools = create_search_tools(kiln_path);

        // Case insensitive - should find both
        let params = Parameters(TextSearchParams {
            query: "todo".to_string(),
            folder: None,
            case_insensitive: true,
            limit: 10,
        });

        let result = search_tools.text_search(params).await.unwrap();
        if let Some(content) = result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["count"], 2);
            }
        }

        // Case sensitive - should find only one
        let params = Parameters(TextSearchParams {
            query: "todo".to_string(),
            folder: None,
            case_insensitive: false,
            limit: 10,
        });

        let result = search_tools.text_search(params).await.unwrap();
        if let Some(content) = result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["count"], 1);
            }
        }
    }

    #[tokio::test]
    async fn test_text_search_with_folder() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create subfolder with file
        fs::create_dir(temp_dir.path().join("subfolder")).unwrap();
        fs::write(
            temp_dir.path().join("subfolder/test.md"),
            "Match in subfolder",
        )
        .unwrap();

        fs::write(temp_dir.path().join("root.md"), "Match in root").unwrap();

        let search_tools = create_search_tools(kiln_path);

        // Search only in subfolder
        let params = Parameters(TextSearchParams {
            query: "Match".to_string(),
            folder: Some("subfolder".to_string()),
            case_insensitive: true,
            limit: 10,
        });

        let result = search_tools.text_search(params).await.unwrap();
        if let Some(content) = result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["count"], 1);
                let matches = parsed["matches"].as_array().unwrap();
                assert!(matches[0]["path"].as_str().unwrap().contains("subfolder"));
            }
        }
    }

    #[tokio::test]
    async fn test_text_search_limit() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create file with multiple matches
        fs::write(
            temp_dir.path().join("test.md"),
            "match\nmatch\nmatch\nmatch\nmatch",
        )
        .unwrap();

        let search_tools = create_search_tools(kiln_path);

        let params = Parameters(TextSearchParams {
            query: "match".to_string(),
            folder: None,
            case_insensitive: true,
            limit: 3,
        });

        let result = search_tools.text_search(params).await.unwrap();
        if let Some(content) = result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                let matches = parsed["matches"].as_array().unwrap();
                assert_eq!(matches.len(), 3); // Limited to 3
                assert_eq!(parsed["truncated"], true);
            }
        }
    }

    // ===== property_search tests =====

    #[tokio::test]
    async fn test_property_search_single_property() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create note with frontmatter
        fs::write(
            temp_dir.path().join("draft.md"),
            "---\nstatus: draft\n---\n\nContent",
        )
        .unwrap();

        fs::write(
            temp_dir.path().join("published.md"),
            "---\nstatus: published\n---\n\nContent",
        )
        .unwrap();

        let search_tools = create_search_tools(kiln_path);

        let params = Parameters(PropertySearchParams {
            properties: serde_json::json!({"status": "draft"}),
            limit: 10,
        });

        let result = search_tools.property_search(params).await.unwrap();
        if let Some(content) = result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["count"], 1);
                let matches = parsed["matches"].as_array().unwrap();
                assert!(matches[0]["path"].as_str().unwrap().contains("draft"));
            }
        }
    }

    #[tokio::test]
    async fn test_property_search_multiple_properties_and() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        fs::write(
            temp_dir.path().join("match.md"),
            "---\nstatus: draft\npriority: high\n---\n\nContent",
        )
        .unwrap();

        fs::write(
            temp_dir.path().join("nomatch.md"),
            "---\nstatus: draft\npriority: low\n---\n\nContent",
        )
        .unwrap();

        let search_tools = create_search_tools(kiln_path);

        let params = Parameters(PropertySearchParams {
            properties: serde_json::json!({"status": "draft", "priority": "high"}),
            limit: 10,
        });

        let result = search_tools.property_search(params).await.unwrap();
        if let Some(content) = result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["count"], 1);
            }
        }
    }

    #[tokio::test]
    async fn test_property_search_tags_or_logic() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        fs::write(
            temp_dir.path().join("urgent.md"),
            "---\ntags: [urgent, work]\n---\n\nContent",
        )
        .unwrap();

        fs::write(
            temp_dir.path().join("important.md"),
            "---\ntags: [important, personal]\n---\n\nContent",
        )
        .unwrap();

        let search_tools = create_search_tools(kiln_path);

        // Search for notes with either "urgent" OR "important" tags
        let params = Parameters(PropertySearchParams {
            properties: serde_json::json!({"tags": ["urgent", "important"]}),
            limit: 10,
        });

        let result = search_tools.property_search(params).await.unwrap();
        if let Some(content) = result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["count"], 2); // Both should match
            }
        }
    }

    #[tokio::test]
    async fn test_property_search_no_frontmatter() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        fs::write(
            temp_dir.path().join("no-fm.md"),
            "Just content, no frontmatter",
        )
        .unwrap();

        let search_tools = create_search_tools(kiln_path);

        let params = Parameters(PropertySearchParams {
            properties: serde_json::json!({"status": "draft"}),
            limit: 10,
        });

        let result = search_tools.property_search(params).await.unwrap();
        if let Some(content) = result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["count"], 0); // No matches
            }
        }
    }

    // ===== Helper function tests =====

    #[test]
    fn test_parse_yaml_frontmatter() {
        let content = "---\ntitle: Test\ntags: [one, two]\n---\n\nContent here";
        let fm = parse_yaml_frontmatter(content).unwrap();

        assert_eq!(fm["title"], "Test");
        assert_eq!(fm["tags"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_parse_yaml_frontmatter_none() {
        let content = "No frontmatter here";
        let fm = parse_yaml_frontmatter(content);

        assert!(fm.is_none());
    }

    // ===== Security Tests for Path Traversal =====

    #[tokio::test]
    async fn test_text_search_folder_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let search_tools = create_search_tools(kiln_path);

        let result = search_tools
            .text_search(Parameters(TextSearchParams {
                query: "test".to_string(),
                folder: Some("../../../etc".to_string()),
                case_insensitive: true,
                limit: 10,
            }))
            .await;

        assert!(
            result.is_err(),
            "Should reject path traversal in folder parameter"
        );
        if let Err(e) = result {
            assert!(
                e.message.contains("Path traversal"),
                "Error should mention path traversal"
            );
        }
    }

    #[tokio::test]
    async fn test_text_search_absolute_folder() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let search_tools = create_search_tools(kiln_path);

        let result = search_tools
            .text_search(Parameters(TextSearchParams {
                query: "test".to_string(),
                folder: Some("/etc".to_string()),
                case_insensitive: true,
                limit: 10,
            }))
            .await;

        assert!(result.is_err(), "Should reject absolute path in folder");
        if let Err(e) = result {
            assert!(
                e.message.contains("Absolute paths are not allowed"),
                "Error should mention absolute paths"
            );
        }
    }
}
