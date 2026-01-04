//! Search operations tools
//!
//! This module provides semantic, text, and property search tools.
//!
//! # NoteStore Integration
//!
//! `SearchTools` can optionally use a `NoteStore` for property searches. When a
//! `NoteStore` is provided, `property_search` uses the indexed metadata instead of
//! walking the filesystem. This provides:
//!
//! - Faster queries on large kilns
//! - Consistent data from the indexed store
//! - Support for complex filters via `NoteStore::search`
//!
//! If no `NoteStore` is provided, property search falls back to filesystem scanning.

#![allow(clippy::doc_markdown, clippy::manual_let_else, missing_docs)]

use crucible_core::storage::NoteStore;
use crucible_core::{enrichment::EmbeddingProvider, traits::KnowledgeRepository};
use crucible_skills::storage::SkillStore;
use grep::regex::RegexMatcher;
use grep::searcher::{sinks::UTF8, Searcher};
use ignore::WalkBuilder;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{model::CallToolResult, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;
use walkdir::WalkDir;

/// Default value for limit parameter
fn default_limit() -> usize {
    10
}

/// Default value for `case_insensitive`
fn default_true() -> bool {
    true
}

/// Custom schema for JSON object (used for required `serde_json::Value` fields).
/// `serde_json::Value` produces an empty schema that llama.cpp can't handle.
fn json_object_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let mut map = serde_json::Map::new();
    map.insert("type".to_owned(), serde_json::json!("object"));
    map.into()
}

#[derive(Clone)]
#[allow(missing_docs)]
pub struct SearchTools {
    kiln_path: String,
    knowledge_repo: Arc<dyn KnowledgeRepository>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
    skill_store: Option<Arc<SkillStore>>,
    /// Optional NoteStore for indexed property searches
    note_store: Option<Arc<dyn NoteStore>>,
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
    /// Optional folder to search within (relative to kiln root)
    folder: Option<String>,
    #[serde(default = "default_true")]
    case_insensitive: bool,
    #[serde(default = "default_limit")]
    limit: usize,
}

/// Parameters for property search
#[derive(Deserialize, JsonSchema)]
pub struct PropertySearchParams {
    /// Key-value pairs to search for in frontmatter properties
    #[schemars(schema_with = "json_object_schema")]
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
            skill_store: None,
            note_store: None,
        }
    }

    /// Create SearchTools with skill search support
    #[allow(missing_docs)]
    pub fn with_skill_store(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        skill_store: Arc<SkillStore>,
    ) -> Self {
        Self {
            kiln_path,
            knowledge_repo,
            embedding_provider,
            skill_store: Some(skill_store),
            note_store: None,
        }
    }

    /// Create SearchTools with NoteStore for optimized property searches
    ///
    /// When a NoteStore is provided, `property_search` uses the indexed metadata
    /// instead of walking the filesystem, providing faster queries on large kilns.
    pub fn with_note_store(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        note_store: Arc<dyn NoteStore>,
    ) -> Self {
        Self {
            kiln_path,
            knowledge_repo,
            embedding_provider,
            skill_store: None,
            note_store: Some(note_store),
        }
    }

    /// Create SearchTools with both SkillStore and NoteStore
    pub fn with_stores(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        skill_store: Arc<SkillStore>,
        note_store: Arc<dyn NoteStore>,
    ) -> Self {
        Self {
            kiln_path,
            knowledge_repo,
            embedding_provider,
            skill_store: Some(skill_store),
            note_store: Some(note_store),
        }
    }
}

#[tool_router]
impl SearchTools {
    #[tool(description = "Search notes and skills using semantic similarity")]
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

        // Search notes
        let note_results = self
            .knowledge_repo
            .search_vectors(embedding.clone())
            .await
            .map_err(|e| {
                rmcp::ErrorData::internal_error(format!("Note search failed: {e}"), None)
            })?;

        let mut all_results: Vec<serde_json::Value> = note_results
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "type": "note",
                    "id": r.document_id,
                    "score": r.score,
                    "snippet": r.snippet,
                    "highlights": r.highlights
                })
            })
            .collect();

        // Search skills if skill_store is available
        if let Some(ref skill_store) = self.skill_store {
            match skill_store.search_by_embedding(&embedding, limit).await {
                Ok(skill_results) => {
                    for skill_result in skill_results {
                        all_results.push(serde_json::json!({
                            "type": "skill",
                            "name": skill_result.name,
                            "description": skill_result.description,
                            "scope": skill_result.scope,
                            "score": skill_result.relevance
                        }));
                    }
                }
                Err(e) => {
                    tracing::warn!("Skill search failed: {}", e);
                    // Continue without skill results
                }
            }
        }

        // Sort all results by score descending
        all_results.sort_by(|a, b| {
            let score_a = a["score"].as_f64().unwrap_or(0.0);
            let score_b = b["score"].as_f64().unwrap_or(0.0);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Take only the top results
        all_results.truncate(limit);

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::json!({
                "results": all_results,
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
        let search_path = validate_folder_within_kiln(&self.kiln_path, folder.as_deref())?;

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
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }

            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "md") {
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

        // Use NoteStore if available for faster indexed access
        if let Some(ref note_store) = self.note_store {
            return self
                .property_search_via_store(note_store, search_props, limit, &params.properties)
                .await;
        }

        // Fall back to filesystem-based search
        self.property_search_via_filesystem(search_props, limit, &params.properties)
            .await
    }

    /// Property search using NoteStore index
    async fn property_search_via_store(
        &self,
        note_store: &Arc<dyn NoteStore>,
        search_props: &serde_json::Map<String, serde_json::Value>,
        limit: usize,
        original_properties: &serde_json::Value,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // Get all notes from the store
        let all_notes = note_store.list().await.map_err(|e| {
            rmcp::ErrorData::internal_error(format!("Failed to list notes from store: {e}"), None)
        })?;

        let mut matches = Vec::new();

        for note in all_notes {
            // Check if all search properties match against the note's properties
            let matches_all = search_props.iter().all(|(key, search_value)| {
                // Special handling for tags - check the tags field directly
                if key == "tags" {
                    return match_tags_property(&note.tags, search_value);
                }

                // Check in properties map
                note.properties.get(key).is_some_and(|prop_value| {
                    property_matches(prop_value, search_value)
                })
            });

            if matches_all {
                // Convert NoteRecord properties to JSON for consistent response format
                let frontmatter: serde_json::Value = serde_json::json!({
                    "title": note.title,
                    "tags": note.tags,
                });

                // Merge with other properties
                let mut frontmatter_obj = frontmatter.as_object().cloned().unwrap_or_default();
                for (k, v) in &note.properties {
                    frontmatter_obj.insert(k.clone(), v.clone());
                }

                matches.push(serde_json::json!({
                    "path": note.path,
                    "frontmatter": frontmatter_obj,
                    "source": "index",
                }));

                if matches.len() >= limit {
                    break;
                }
            }
        }

        let count = matches.len();

        Ok(CallToolResult::success(vec![rmcp::model::Content::json(
            serde_json::json!({
                "properties": original_properties,
                "matches": matches,
                "count": count,
            }),
        )?]))
    }

    /// Property search using filesystem scanning (fallback)
    async fn property_search_via_filesystem(
        &self,
        search_props: &serde_json::Map<String, serde_json::Value>,
        limit: usize,
        original_properties: &serde_json::Value,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut matches = Vec::new();

        // Walk all markdown files
        for entry in WalkDir::new(&self.kiln_path)
            .follow_links(false)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
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
                frontmatter.get(key).is_some_and(|prop_value| {
                    property_matches(prop_value, search_value)
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
                "properties": original_properties,
                "matches": matches,
                "count": count,
            }),
        )?]))
    }
}

/// Check if a property value matches a search value
fn property_matches(prop_value: &serde_json::Value, search_value: &serde_json::Value) -> bool {
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
}

/// Check if tags match a search value (special handling for NoteRecord.tags)
fn match_tags_property(tags: &[String], search_value: &serde_json::Value) -> bool {
    if let Some(search_array) = search_value.as_array() {
        // OR logic: any search tag matches any note tag
        search_array.iter().any(|sv| {
            if let Some(s) = sv.as_str() {
                tags.contains(&s.to_string())
            } else {
                false
            }
        })
    } else if let Some(search_str) = search_value.as_str() {
        // Single tag match
        tags.contains(&search_str.to_string())
    } else {
        false
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

        fn model_name(&self) -> &'static str {
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

    /// Check a schema for common llama.cpp incompatible patterns.
    ///
    /// Known issues:
    /// - `"default": null` - caused by `#[serde(default)]` on `Option<T>` (redundant, just remove it)
    /// - `"additionalProperties": true` - from `serde_json::Value` without custom schema
    ///
    /// Use `#[schemars(schema_with = "...")]` for `serde_json::Value` fields.
    /// Don't use `#[serde(default)]` on `Option<T>` - serde already treats missing Options as None.
    fn check_schema_compatible(json: &str) -> Result<(), String> {
        if json.contains(r#""default":null"#) || json.contains(r#""default": null"#) {
            return Err(
                "Schema contains 'default: null' - remove #[serde(default)] from Option<T> fields"
                    .into(),
            );
        }
        if json.contains(r#""additionalProperties":true"#)
            || json.contains(r#""additionalProperties": true"#)
        {
            return Err("Schema contains 'additionalProperties: true' - use #[schemars(schema_with = \"...\")] for serde_json::Value fields".into());
        }
        Ok(())
    }

    /// Validates all tool parameter schemas are compatible with llama.cpp's GBNF converter.
    ///
    /// Common mistakes this catches:
    /// - Using `#[serde(default)]` on `Option<T>` fields (generates `"default": null`)
    /// - Using bare `serde_json::Value` without `#[schemars(schema_with = "...")]`
    #[test]
    fn test_tool_schemas_llama_cpp_compatible() {
        use crate::notes::{
            CreateNoteParams, DeleteNoteParams, ListNotesParams, ReadMetadataParams,
            ReadNoteParams, UpdateNoteParams,
        };

        let schemas: &[(&str, String)] = &[
            (
                "TextSearchParams",
                serde_json::to_string(&schemars::schema_for!(TextSearchParams)).unwrap(),
            ),
            (
                "SemanticSearchParams",
                serde_json::to_string(&schemars::schema_for!(SemanticSearchParams)).unwrap(),
            ),
            (
                "PropertySearchParams",
                serde_json::to_string(&schemars::schema_for!(PropertySearchParams)).unwrap(),
            ),
            (
                "CreateNoteParams",
                serde_json::to_string(&schemars::schema_for!(CreateNoteParams)).unwrap(),
            ),
            (
                "ReadNoteParams",
                serde_json::to_string(&schemars::schema_for!(ReadNoteParams)).unwrap(),
            ),
            (
                "ReadMetadataParams",
                serde_json::to_string(&schemars::schema_for!(ReadMetadataParams)).unwrap(),
            ),
            (
                "UpdateNoteParams",
                serde_json::to_string(&schemars::schema_for!(UpdateNoteParams)).unwrap(),
            ),
            (
                "DeleteNoteParams",
                serde_json::to_string(&schemars::schema_for!(DeleteNoteParams)).unwrap(),
            ),
            (
                "ListNotesParams",
                serde_json::to_string(&schemars::schema_for!(ListNotesParams)).unwrap(),
            ),
        ];

        let mut errors = Vec::new();
        for (name, json) in schemas {
            if let Err(e) = check_schema_compatible(json) {
                errors.push(format!("{name}: {e}"));
            }
        }

        assert!(
            errors.is_empty(),
            "Schema compatibility issues:\n{}",
            errors.join("\n")
        );
    }
}

// ===== NoteStore Integration Tests =====
// These tests verify the property_search NoteStore code path works correctly

#[cfg(test)]
mod note_store_tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::{Filter, NoteRecord, StorageResult};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tempfile::TempDir;

    // Mock implementations for testing
    struct MockKnowledgeRepository;
    struct MockEmbeddingProvider;

    #[async_trait]
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

    #[async_trait]
    impl crucible_core::enrichment::EmbeddingProvider for MockEmbeddingProvider {
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.1; 384])
        }

        async fn embed_batch(&self, _texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(vec![vec![0.1; 384]; _texts.len()])
        }

        fn model_name(&self) -> &'static str {
            "mock-model"
        }

        fn dimensions(&self) -> usize {
            384
        }
    }

    /// Mock NoteStore for testing the NoteStore integration path
    struct MockNoteStore {
        notes: Mutex<HashMap<String, NoteRecord>>,
    }

    impl MockNoteStore {
        fn new() -> Self {
            Self {
                notes: Mutex::new(HashMap::new()),
            }
        }

        fn add_note(&self, record: NoteRecord) {
            let mut notes = self.notes.lock().unwrap();
            notes.insert(record.path.clone(), record);
        }
    }

    #[async_trait]
    impl NoteStore for MockNoteStore {
        async fn upsert(&self, note: NoteRecord) -> StorageResult<()> {
            self.add_note(note);
            Ok(())
        }

        async fn get(&self, path: &str) -> StorageResult<Option<NoteRecord>> {
            let notes = self.notes.lock().unwrap();
            Ok(notes.get(path).cloned())
        }

        async fn delete(&self, path: &str) -> StorageResult<()> {
            let mut notes = self.notes.lock().unwrap();
            notes.remove(path);
            Ok(())
        }

        async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
            let notes = self.notes.lock().unwrap();
            Ok(notes.values().cloned().collect())
        }

        async fn get_by_hash(&self, _hash: &BlockHash) -> StorageResult<Option<NoteRecord>> {
            Ok(None)
        }

        async fn search(
            &self,
            _embedding: &[f32],
            _k: usize,
            _filter: Option<Filter>,
        ) -> StorageResult<Vec<crucible_core::storage::note_store::SearchResult>> {
            Ok(vec![])
        }
    }

    fn create_search_tools_with_store(
        kiln_path: String,
        note_store: Arc<dyn NoteStore>,
    ) -> SearchTools {
        let knowledge_repo = Arc::new(MockKnowledgeRepository);
        let embedding_provider = Arc::new(MockEmbeddingProvider);
        SearchTools::with_note_store(kiln_path, knowledge_repo, embedding_provider, note_store)
    }

    #[tokio::test]
    async fn test_property_search_uses_note_store() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create a mock NoteStore with notes having properties
        let mock_store = Arc::new(MockNoteStore::new());

        let mut props1 = HashMap::new();
        props1.insert("status".to_string(), serde_json::json!("draft"));

        mock_store.add_note(NoteRecord {
            path: "draft-note.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: "Draft Note".to_string(),
            tags: vec!["work".to_string()],
            links_to: vec![],
            properties: props1,
            updated_at: Utc::now(),
        });

        let mut props2 = HashMap::new();
        props2.insert("status".to_string(), serde_json::json!("published"));

        mock_store.add_note(NoteRecord {
            path: "published-note.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: "Published Note".to_string(),
            tags: vec!["blog".to_string()],
            links_to: vec![],
            properties: props2,
            updated_at: Utc::now(),
        });

        let search_tools = create_search_tools_with_store(kiln_path, mock_store);

        // Search for draft notes
        let result = search_tools
            .property_search(Parameters(PropertySearchParams {
                properties: serde_json::json!({"status": "draft"}),
                limit: 10,
            }))
            .await;

        assert!(result.is_ok(), "property_search should succeed");

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

                assert_eq!(parsed["count"], 1);
                let matches = parsed["matches"].as_array().unwrap();
                assert_eq!(matches.len(), 1);

                // Verify source is from index
                assert_eq!(matches[0]["source"], "index");
                assert!(matches[0]["path"].as_str().unwrap().contains("draft"));
            }
        }
    }

    #[tokio::test]
    async fn test_property_search_by_tags_uses_note_store() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create a mock NoteStore with tagged notes
        let mock_store = Arc::new(MockNoteStore::new());

        mock_store.add_note(NoteRecord {
            path: "rust-note.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: "Rust Note".to_string(),
            tags: vec!["rust".to_string(), "programming".to_string()],
            links_to: vec![],
            properties: HashMap::new(),
            updated_at: Utc::now(),
        });

        mock_store.add_note(NoteRecord {
            path: "python-note.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: "Python Note".to_string(),
            tags: vec!["python".to_string(), "programming".to_string()],
            links_to: vec![],
            properties: HashMap::new(),
            updated_at: Utc::now(),
        });

        mock_store.add_note(NoteRecord {
            path: "cooking-note.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: "Cooking Note".to_string(),
            tags: vec!["cooking".to_string(), "recipes".to_string()],
            links_to: vec![],
            properties: HashMap::new(),
            updated_at: Utc::now(),
        });

        let search_tools = create_search_tools_with_store(kiln_path, mock_store);

        // Search for notes with "rust" or "python" tags (OR logic)
        let result = search_tools
            .property_search(Parameters(PropertySearchParams {
                properties: serde_json::json!({"tags": ["rust", "python"]}),
                limit: 10,
            }))
            .await;

        assert!(result.is_ok(), "property_search should succeed");

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

                // Should match both rust and python notes (OR logic)
                assert_eq!(parsed["count"], 2);
            }
        }
    }

    #[tokio::test]
    async fn test_property_search_single_tag() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let mock_store = Arc::new(MockNoteStore::new());

        mock_store.add_note(NoteRecord {
            path: "tagged.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: "Tagged Note".to_string(),
            tags: vec!["important".to_string()],
            links_to: vec![],
            properties: HashMap::new(),
            updated_at: Utc::now(),
        });

        mock_store.add_note(NoteRecord {
            path: "untagged.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: "Untagged Note".to_string(),
            tags: vec![],
            links_to: vec![],
            properties: HashMap::new(),
            updated_at: Utc::now(),
        });

        let search_tools = create_search_tools_with_store(kiln_path, mock_store);

        // Search for notes with "important" tag (single string)
        let result = search_tools
            .property_search(Parameters(PropertySearchParams {
                properties: serde_json::json!({"tags": "important"}),
                limit: 10,
            }))
            .await;

        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();
                assert_eq!(parsed["count"], 1);
            }
        }
    }

    #[tokio::test]
    async fn test_property_search_multiple_properties_and_logic() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let mock_store = Arc::new(MockNoteStore::new());

        let mut props1 = HashMap::new();
        props1.insert("status".to_string(), serde_json::json!("draft"));
        props1.insert("priority".to_string(), serde_json::json!("high"));

        mock_store.add_note(NoteRecord {
            path: "high-priority-draft.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: "High Priority Draft".to_string(),
            tags: vec![],
            links_to: vec![],
            properties: props1,
            updated_at: Utc::now(),
        });

        let mut props2 = HashMap::new();
        props2.insert("status".to_string(), serde_json::json!("draft"));
        props2.insert("priority".to_string(), serde_json::json!("low"));

        mock_store.add_note(NoteRecord {
            path: "low-priority-draft.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: "Low Priority Draft".to_string(),
            tags: vec![],
            links_to: vec![],
            properties: props2,
            updated_at: Utc::now(),
        });

        let search_tools = create_search_tools_with_store(kiln_path, mock_store);

        // Search for draft AND high priority (AND logic between properties)
        let result = search_tools
            .property_search(Parameters(PropertySearchParams {
                properties: serde_json::json!({"status": "draft", "priority": "high"}),
                limit: 10,
            }))
            .await;

        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

                // Should only match the high priority draft
                assert_eq!(parsed["count"], 1);
                let matches = parsed["matches"].as_array().unwrap();
                assert!(matches[0]["path"]
                    .as_str()
                    .unwrap()
                    .contains("high-priority"));
            }
        }
    }

    #[tokio::test]
    async fn test_property_search_respects_limit() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let mock_store = Arc::new(MockNoteStore::new());

        // Add multiple notes with same status
        for i in 0..5 {
            let mut props = HashMap::new();
            props.insert("status".to_string(), serde_json::json!("draft"));

            mock_store.add_note(NoteRecord {
                path: format!("note{i}.md"),
                content_hash: BlockHash::zero(),
                embedding: None,
                title: format!("Note {i}"),
                tags: vec![],
                links_to: vec![],
                properties: props,
                updated_at: Utc::now(),
            });
        }

        let search_tools = create_search_tools_with_store(kiln_path, mock_store);

        // Search with limit of 3
        let result = search_tools
            .property_search(Parameters(PropertySearchParams {
                properties: serde_json::json!({"status": "draft"}),
                limit: 3,
            }))
            .await;

        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(raw_text) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

                // Should respect the limit
                assert_eq!(parsed["count"], 3);
            }
        }
    }
}
