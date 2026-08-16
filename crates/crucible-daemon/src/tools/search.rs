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

use super::containment::RootSet;
use super::fs_scope::FsScope;
use super::grep_engine::{grep_search, GrepSearchError, WalkScope};
use super::helpers::{json_success, McpResultExt};
use crate::multi_kiln_search::KilnSearchSource;
use crucible_core::serde_helpers::default_true;
use crucible_core::storage::NoteStore;
use crucible_core::{enrichment::EmbeddingProvider, traits::KnowledgeRepository};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{model::CallToolResult, tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;

/// Default value for limit parameter
fn default_limit() -> usize {
    10
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
    /// The kiln as a capability rather than a path — see [`super::fs_scope`].
    /// `grep_notes` and the `property_search` fallback both WALK, so the rule
    /// has to apply to what they yield, not only to the folder a caller names.
    scope: FsScope,
    embedding_provider: Arc<dyn EmbeddingProvider>,
    /// Optional NoteStore for indexed property searches
    note_store: Option<Arc<dyn NoteStore>>,
    /// Kilns `semantic_search` fans out across: the primary plus any
    /// session-connected kilns (same sources precognition uses — one
    /// builder, one filter policy). Trust gating happens at attach time.
    search_sources: Vec<KilnSearchSource>,
}

/// Parameters for semantic search
#[derive(Deserialize, JsonSchema)]
pub struct SemanticSearchParams {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

/// Parameters for grep-style note search (`grep_notes`)
#[derive(Deserialize, JsonSchema)]
pub struct GrepNotesParams {
    query: String,
    /// Optional folder to search within (relative to kiln root)
    folder: Option<String>,
    /// Treat `query` as a regular expression (Rust regex syntax) instead of a
    /// literal string
    #[serde(default)]
    regex: bool,
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
        let search_sources = vec![KilnSearchSource {
            kiln_path: std::path::PathBuf::from(&kiln_path),
            knowledge_repo,
        }];
        Self {
            scope: FsScope::kiln(kiln_path, RootSet::Ambient),
            embedding_provider,
            note_store: None,
            search_sources,
        }
    }

    /// Contain these tools to the session's roots — see
    /// [`super::notes::NoteTools::with_containment`].
    #[must_use]
    pub(crate) fn with_containment(mut self, containment: RootSet) -> Self {
        self.scope = self.scope.with_containment(containment);
        self
    }

    /// The kiln root, for storage-authority derivation and relative reporting.
    fn kiln_path(&self) -> String {
        self.scope.anchor().to_string_lossy().into_owned()
    }

    pub fn with_note_store(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        note_store: Arc<dyn NoteStore>,
    ) -> Self {
        let mut tools = Self::new(kiln_path, knowledge_repo, embedding_provider);
        tools.note_store = Some(note_store);
        tools
    }

    /// Replace the search fan-out set with the session's kilns, as built by
    /// `AgentManager::collect_kiln_search_sources`. An empty set keeps the
    /// single source built from `kiln_path`.
    pub fn with_search_sources(mut self, sources: Vec<KilnSearchSource>) -> Self {
        if !sources.is_empty() {
            self.search_sources = sources;
        }
        self
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

        let embedding = self
            .embedding_provider
            .embed(&query)
            .await
            .mcp_err_ctx("Failed to generate embedding")?;

        // Fan out across the session's kilns through the same engine
        // precognition uses (dedup + merge-sort + kiln labeling). Trust
        // filtering is None here: every kiln passes the trust gate at attach
        // time.
        let note_results = crate::multi_kiln_search::search_across_kilns(
            &self.search_sources,
            embedding,
            limit,
            None,
            self.scope.anchor(),
        )
        .await
        .mcp_err_ctx("Note search failed")?;

        let all_results: Vec<serde_json::Value> = note_results
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "type": "note",
                    "id": r.document_id,
                    "score": r.score,
                    "snippet": r.snippet,
                    "highlights": r.highlights,
                    "kiln_path": r.kiln_path,
                })
            })
            .collect();

        json_success(serde_json::json!({
            "results": all_results,
            "query": query,
            "limit": limit
        }))
    }

    #[tool(
        description = "Grep-style text search over notes (ripgrep engine): literal substring by default, set regex=true for regex patterns (Rust regex syntax). Returns file/line matches with match offsets, in file order — no ranking or stemming. For meaning-based discovery use semantic_search."
    )]
    pub async fn grep_notes(
        &self,
        params: Parameters<GrepNotesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let query = params.query.clone();
        // LLMs sometimes send the literal string "null" instead of omitting the field
        let folder = params.folder.filter(|f| !f.is_empty() && f != "null");
        let case_insensitive = params.case_insensitive;
        let limit = params.limit;

        let search_path = self.scope.resolve_folder(folder.as_deref())?;

        if !search_path.exists() {
            return Err(rmcp::ErrorData::invalid_params(
                format!("Search path does not exist: {}", search_path.display()),
                None,
            ));
        }

        // Reuse the shared grep engine, restricting to notes (`*.md`) and
        // reporting paths relative to the kiln root even when a subfolder is
        // walked.
        let admits = |path: &std::path::Path| self.scope.admits(path);
        let (hits, truncated) = grep_search(
            WalkScope::contained(search_path.as_path(), &admits),
            self.scope.anchor(),
            &query,
            params.regex,
            Some("*.md"),
            limit,
            case_insensitive,
        )
        .map_err(|e| match e {
            // A bad user pattern is the caller's mistake, not an internal
            // failure — name the syntax problem so it can be corrected.
            GrepSearchError::InvalidRegex(_) => {
                rmcp::ErrorData::invalid_params(e.to_string(), None)
            }
            GrepSearchError::Other(err) => {
                rmcp::ErrorData::internal_error(format!("Text search failed: {err:#}"), None)
            }
        })?;

        let matches: Vec<serde_json::Value> = hits
            .into_iter()
            .map(|h| {
                serde_json::json!({
                    "path": h.rel_path,
                    "line_number": h.line,
                    "line_content": h.text,
                    "match_start": h.match_start,
                    "match_end": h.match_end,
                })
            })
            .collect();

        let count = matches.len();

        json_success(serde_json::json!({
            "query": query,
            "matches": matches,
            "count": count,
            "truncated": truncated
        }))
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
        // Get all notes from the store — workspace authority derived from
        // the kiln this MCP server is bound to.
        let kiln = self.kiln_path();
        let authority = crucible_core::storage::Scope::workspace(&kiln)
            .unwrap_or_else(|_| crucible_core::storage::Scope::workspace_unchecked(&kiln));
        let all_notes = note_store
            .list(&authority)
            .await
            .mcp_err_ctx("Failed to list notes from store")?;

        let mut matches = Vec::new();

        for note in all_notes {
            // Check if all search properties match against the note's properties
            let matches_all = search_props.iter().all(|(key, search_value)| {
                // Special handling for tags - check the tags field directly
                if key == "tags" {
                    return match_tags_property(&note.tags, search_value);
                }

                // Check in properties map
                note.properties
                    .get(key)
                    .is_some_and(|prop_value| property_matches(prop_value, search_value))
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

        json_success(serde_json::json!({
            "properties": original_properties,
            "matches": matches,
            "count": count,
        }))
    }

    /// Property search using filesystem scanning (fallback)
    ///
    /// This function is async for API consistency with the `NoteStore` path,
    /// even though filesystem operations are synchronous.
    #[allow(clippy::unused_async)]
    async fn property_search_via_filesystem(
        &self,
        search_props: &serde_json::Map<String, serde_json::Value>,
        limit: usize,
        original_properties: &serde_json::Value,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut matches = Vec::new();

        // Through the scope, so the walk cannot rake in a denied subtree under
        // an allowed kiln. This fallback had no filter whatsoever — not even
        // the hidden-directory skip the other walkers carry.
        let root = self.scope.resolve("")?;
        for entry in self
            .scope
            .walk_files(&root)
            .filter(|e| crucible_core::kiln::is_note_file(e.path()))
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
                frontmatter
                    .get(key)
                    .is_some_and(|prop_value| property_matches(prop_value, search_value))
            });

            if matches_all {
                let relative_path = self
                    .scope
                    .relativize(entry.path())
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

        json_success(serde_json::json!({
            "properties": original_properties,
            "matches": matches,
            "count": count,
        }))
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

// Use shared utilities for frontmatter parsing
use super::utils::parse_yaml_frontmatter;

/// Extract the JSON payload from a tool result, panicking (rather than silently
/// skipping assertions) if the result has no text content or invalid JSON.
#[cfg(test)]
fn parse_tool_json(result: &CallToolResult) -> serde_json::Value {
    let content = result
        .content
        .first()
        .expect("tool result should contain content");
    let raw_text = content
        .as_text()
        .expect("tool result content should be text");
    serde_json::from_str(&raw_text.text).expect("tool result text should be valid JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    use crate::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};

    fn create_search_tools(kiln_path: String) -> SearchTools {
        let knowledge_repo = Arc::new(MockKnowledgeRepository);
        let embedding_provider = Arc::new(MockEmbeddingProvider);
        SearchTools::new(kiln_path, knowledge_repo, embedding_provider)
    }

    // ===== grep_notes tests =====

    #[tokio::test]
    async fn test_grep_notes_basic() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create test file
        fs::write(
            temp_dir.path().join("test.md"),
            "# Test Note\n\nThis contains TODO items.\n\nAnother line.",
        )
        .unwrap();

        let search_tools = create_search_tools(kiln_path);

        let params = Parameters(GrepNotesParams {
            query: "TODO".to_string(),
            folder: None,
            regex: false,
            case_insensitive: true,
            limit: 10,
        });

        let result = search_tools.grep_notes(params).await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        let parsed = parse_tool_json(&call_result);
        assert_eq!(parsed["query"], "TODO");
        assert_eq!(parsed["count"], 1);

        let matches = parsed["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert!(matches[0]["line_content"]
            .as_str()
            .unwrap()
            .contains("TODO"));
    }

    #[tokio::test]
    async fn test_grep_notes_case_sensitive() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        fs::write(
            temp_dir.path().join("test.md"),
            "TODO in uppercase\ntodo in lowercase",
        )
        .unwrap();

        let search_tools = create_search_tools(kiln_path);

        // Case insensitive - should find both
        let params = Parameters(GrepNotesParams {
            query: "todo".to_string(),
            folder: None,
            regex: false,
            case_insensitive: true,
            limit: 10,
        });

        let result = search_tools.grep_notes(params).await.unwrap();
        let parsed = parse_tool_json(&result);
        assert_eq!(parsed["count"], 2);

        // Case sensitive - should find only one
        let params = Parameters(GrepNotesParams {
            query: "todo".to_string(),
            folder: None,
            regex: false,
            case_insensitive: false,
            limit: 10,
        });

        let result = search_tools.grep_notes(params).await.unwrap();
        let parsed = parse_tool_json(&result);
        assert_eq!(parsed["count"], 1);
    }

    #[tokio::test]
    async fn test_grep_notes_with_folder() {
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
        let params = Parameters(GrepNotesParams {
            query: "Match".to_string(),
            folder: Some("subfolder".to_string()),
            regex: false,
            case_insensitive: true,
            limit: 10,
        });

        let result = search_tools.grep_notes(params).await.unwrap();
        let parsed = parse_tool_json(&result);
        assert_eq!(parsed["count"], 1);
        let matches = parsed["matches"].as_array().unwrap();
        assert!(matches[0]["path"].as_str().unwrap().contains("subfolder"));
    }

    #[tokio::test]
    async fn test_grep_notes_limit() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create file with multiple matches
        fs::write(
            temp_dir.path().join("test.md"),
            "match\nmatch\nmatch\nmatch\nmatch",
        )
        .unwrap();

        let search_tools = create_search_tools(kiln_path);

        let params = Parameters(GrepNotesParams {
            query: "match".to_string(),
            folder: None,
            regex: false,
            case_insensitive: true,
            limit: 3,
        });

        let result = search_tools.grep_notes(params).await.unwrap();
        let parsed = parse_tool_json(&result);
        let matches = parsed["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 3); // Limited to 3
        assert_eq!(parsed["truncated"], true);
    }

    // ===== regex-mode tests =====

    #[tokio::test]
    async fn regex_mode_matches_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        fs::write(
            temp_dir.path().join("test.md"),
            "foo123bar\nfoobar\nno match\n",
        )
        .unwrap();

        let search_tools = create_search_tools(kiln_path);

        let params = Parameters(GrepNotesParams {
            query: r"foo[0-9]+bar".to_string(),
            folder: None,
            regex: true,
            case_insensitive: false,
            limit: 10,
        });

        let result = search_tools.grep_notes(params).await.unwrap();
        let parsed = parse_tool_json(&result);
        assert_eq!(parsed["count"], 1, "regex should match only foo123bar");
        let matches = parsed["matches"].as_array().unwrap();
        assert_eq!(matches[0]["line_content"], "foo123bar");
    }

    #[tokio::test]
    async fn literal_mode_does_not_interpret_metachars() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // "foo.bar" as a regex would match "fooXbar"; as a literal it must not.
        fs::write(temp_dir.path().join("test.md"), "fooXbar\nfoo.bar\n").unwrap();

        let search_tools = create_search_tools(kiln_path);

        let params = Parameters(GrepNotesParams {
            query: "foo.bar".to_string(),
            folder: None,
            regex: false,
            case_insensitive: false,
            limit: 10,
        });

        let result = search_tools.grep_notes(params).await.unwrap();
        let parsed = parse_tool_json(&result);
        assert_eq!(parsed["count"], 1, "literal dot must not match fooXbar");
        let matches = parsed["matches"].as_array().unwrap();
        assert_eq!(matches[0]["line_content"], "foo.bar");
    }

    #[tokio::test]
    async fn invalid_regex_returns_syntax_error() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        fs::write(temp_dir.path().join("test.md"), "content\n").unwrap();

        let search_tools = create_search_tools(kiln_path);

        let params = Parameters(GrepNotesParams {
            query: "foo(".to_string(),
            folder: None,
            regex: true,
            case_insensitive: false,
            limit: 10,
        });

        let err = search_tools
            .grep_notes(params)
            .await
            .expect_err("unbalanced paren should be rejected");
        assert!(
            err.message.contains("Invalid regex"),
            "error should identify the regex as invalid: {}",
            err.message
        );
        assert!(
            err.message.contains("unclosed group") || err.message.contains("error"),
            "error should name the syntax problem: {}",
            err.message
        );
    }

    #[tokio::test]
    async fn grep_notes_reports_match_offsets() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        fs::write(temp_dir.path().join("test.md"), "a needle here\n").unwrap();

        let search_tools = create_search_tools(kiln_path);

        let params = Parameters(GrepNotesParams {
            query: "needle".to_string(),
            folder: None,
            regex: false,
            case_insensitive: false,
            limit: 10,
        });

        let result = search_tools.grep_notes(params).await.unwrap();
        let parsed = parse_tool_json(&result);
        let matches = parsed["matches"].as_array().unwrap();
        assert_eq!(matches[0]["match_start"], 2);
        assert_eq!(matches[0]["match_end"], 8);
    }

    #[tokio::test]
    async fn regex_mode_respects_case_sensitivity() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        fs::write(
            temp_dir.path().join("test.md"),
            "TODO5 upper\ntodo7 lower\n",
        )
        .unwrap();

        let search_tools = create_search_tools(kiln_path.clone());

        // Insensitive: both lines match the pattern.
        let params = Parameters(GrepNotesParams {
            query: r"todo[0-9]".to_string(),
            folder: None,
            regex: true,
            case_insensitive: true,
            limit: 10,
        });
        let parsed = parse_tool_json(&search_tools.grep_notes(params).await.unwrap());
        assert_eq!(parsed["count"], 2);

        // Sensitive: only the lowercase line matches.
        let params = Parameters(GrepNotesParams {
            query: r"todo[0-9]".to_string(),
            folder: None,
            regex: true,
            case_insensitive: false,
            limit: 10,
        });
        let parsed = parse_tool_json(&search_tools.grep_notes(params).await.unwrap());
        assert_eq!(parsed["count"], 1);
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
        let parsed = parse_tool_json(&result);
        assert_eq!(parsed["count"], 1);
        let matches = parsed["matches"].as_array().unwrap();
        assert!(matches[0]["path"].as_str().unwrap().contains("draft"));
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
        let parsed = parse_tool_json(&result);
        assert_eq!(parsed["count"], 1);
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
        let parsed = parse_tool_json(&result);
        assert_eq!(parsed["count"], 2); // Both should match
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
        let parsed = parse_tool_json(&result);
        assert_eq!(parsed["count"], 0); // No matches
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
    async fn test_grep_notes_folder_traversal() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let search_tools = create_search_tools(kiln_path);

        let result = search_tools
            .grep_notes(Parameters(GrepNotesParams {
                query: "test".to_string(),
                folder: Some("../../../etc".to_string()),
                regex: false,
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
    async fn test_grep_notes_absolute_folder() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();
        let search_tools = create_search_tools(kiln_path);

        let result = search_tools
            .grep_notes(Parameters(GrepNotesParams {
                query: "test".to_string(),
                folder: Some("/etc".to_string()),
                regex: false,
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
        if (json.contains(r#""type":"object""#) || json.contains(r#""type": "object""#))
            && !json.contains(r#""properties""#)
        {
            return Err("Schema has type=object but missing properties key".into());
        }
        if json.contains(r#""$schema""#) {
            return Err("Schema contains $schema meta field".into());
        }
        if json.contains(r#""title""#) {
            return Err("Schema contains title meta field".into());
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
        use crate::provider::tool_bridge::sanitize_tool_schema;
        use crate::tools::mcp_server::{
            CancelJobParams, DelegateSessionParams, GetJobResultParams, ListJobsParams,
        };
        use crate::tools::notes::{
            CreateNoteParams, DeleteNoteParams, ListNotesParams, ReadMetadataParams,
            ReadNoteParams, UpdateNoteParams,
        };

        let sanitize = |raw: schemars::Schema| -> String {
            let mut v: serde_json::Value = serde_json::to_value(&raw).unwrap();
            sanitize_tool_schema(&mut v);
            serde_json::to_string(&v).unwrap()
        };

        let schemas: &[(&str, String)] = &[
            (
                "GrepNotesParams",
                sanitize(schemars::schema_for!(GrepNotesParams)),
            ),
            (
                "SemanticSearchParams",
                sanitize(schemars::schema_for!(SemanticSearchParams)),
            ),
            (
                "PropertySearchParams",
                sanitize(schemars::schema_for!(PropertySearchParams)),
            ),
            (
                "CreateNoteParams",
                sanitize(schemars::schema_for!(CreateNoteParams)),
            ),
            (
                "ReadNoteParams",
                sanitize(schemars::schema_for!(ReadNoteParams)),
            ),
            (
                "ReadMetadataParams",
                sanitize(schemars::schema_for!(ReadMetadataParams)),
            ),
            (
                "UpdateNoteParams",
                sanitize(schemars::schema_for!(UpdateNoteParams)),
            ),
            (
                "DeleteNoteParams",
                sanitize(schemars::schema_for!(DeleteNoteParams)),
            ),
            (
                "ListNotesParams",
                sanitize(schemars::schema_for!(ListNotesParams)),
            ),
            (
                "ListJobsParams",
                sanitize(schemars::schema_for!(ListJobsParams)),
            ),
            (
                "GetJobResultParams",
                sanitize(schemars::schema_for!(GetJobResultParams)),
            ),
            (
                "CancelJobParams",
                sanitize(schemars::schema_for!(CancelJobParams)),
            ),
            (
                "DelegateSessionParams",
                sanitize(schemars::schema_for!(DelegateSessionParams)),
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

    #[test]
    fn test_check_schema_catches_missing_properties() {
        // Raw schema with type=object but no properties (like ListJobsParams before sanitization)
        let raw = r#"{"type":"object"}"#;
        assert!(
            check_schema_compatible(raw).is_err(),
            "Should reject schema with type=object but missing properties"
        );

        // Schema with $schema field
        let with_schema_field = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{}}"#;
        assert!(
            check_schema_compatible(with_schema_field).is_err(),
            "Should reject schema with $schema field"
        );

        // Schema with title field
        let with_title = r#"{"title":"Foo","type":"object","properties":{}}"#;
        assert!(
            check_schema_compatible(with_title).is_err(),
            "Should reject schema with title field"
        );

        // Valid schema should pass
        let valid = r#"{"type":"object","properties":{}}"#;
        assert!(
            check_schema_compatible(valid).is_ok(),
            "Should accept valid schema"
        );
    }
}

// ===== NoteStore Integration Tests =====
// These tests verify the property_search NoteStore code path works correctly

#[cfg(test)]
mod note_store_tests {
    use super::*;
    use async_trait::async_trait;
    use crucible_core::events::{InternalSessionEvent, NoteChangeType, SessionEvent};
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::{Filter, NoteRecord, StorageResult};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tempfile::TempDir;

    use crate::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};

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

    fn note(
        path: impl Into<String>,
        title: impl Into<String>,
        tags: Vec<String>,
        properties: HashMap<String, serde_json::Value>,
    ) -> NoteRecord {
        NoteRecord::new(path, BlockHash::zero())
            .with_title(title)
            .with_tags(tags)
            .with_properties(properties)
    }

    #[async_trait]
    impl NoteStore for MockNoteStore {
        async fn upsert(&self, note: NoteRecord) -> StorageResult<Vec<SessionEvent>> {
            self.add_note(note.clone());
            let existed = self.notes.lock().unwrap().contains_key(&note.path);
            let event = if existed {
                SessionEvent::internal(InternalSessionEvent::NoteModified {
                    path: note.path.into(),
                    change_type: NoteChangeType::Content,
                })
            } else {
                SessionEvent::internal(InternalSessionEvent::NoteCreated {
                    path: note.path.into(),
                    title: Some(note.title),
                })
            };
            Ok(vec![event])
        }

        async fn get(
            &self,
            path: &str,
            _authority: &crucible_core::storage::Scope,
        ) -> StorageResult<Option<NoteRecord>> {
            let notes = self.notes.lock().unwrap();
            Ok(notes.get(path).cloned())
        }

        async fn delete(&self, path: &str) -> StorageResult<SessionEvent> {
            let mut notes = self.notes.lock().unwrap();
            let existed = notes.remove(path).is_some();
            Ok(SessionEvent::internal(InternalSessionEvent::NoteDeleted {
                path: path.into(),
                existed,
            }))
        }

        async fn list(
            &self,
            _authority: &crucible_core::storage::Scope,
        ) -> StorageResult<Vec<NoteRecord>> {
            let notes = self.notes.lock().unwrap();
            Ok(notes.values().cloned().collect())
        }

        async fn get_by_hash(
            &self,
            _hash: &BlockHash,
            _authority: &crucible_core::storage::Scope,
        ) -> StorageResult<Option<NoteRecord>> {
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

        mock_store.add_note(note(
            "draft-note.md".to_string(),
            "Draft Note".to_string(),
            vec!["work".to_string()],
            props1,
        ));

        let mut props2 = HashMap::new();
        props2.insert("status".to_string(), serde_json::json!("published"));

        mock_store.add_note(note(
            "published-note.md".to_string(),
            "Published Note".to_string(),
            vec!["blog".to_string()],
            props2,
        ));

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
        let parsed = parse_tool_json(&call_result);

        assert_eq!(parsed["count"], 1);
        let matches = parsed["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);

        // Verify source is from index
        assert_eq!(matches[0]["source"], "index");
        assert!(matches[0]["path"].as_str().unwrap().contains("draft"));
    }

    #[tokio::test]
    async fn test_property_search_by_tags_uses_note_store() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create a mock NoteStore with tagged notes
        let mock_store = Arc::new(MockNoteStore::new());

        mock_store.add_note(note(
            "rust-note.md".to_string(),
            "Rust Note".to_string(),
            vec!["rust".to_string(), "programming".to_string()],
            HashMap::new(),
        ));

        mock_store.add_note(note(
            "python-note.md".to_string(),
            "Python Note".to_string(),
            vec!["python".to_string(), "programming".to_string()],
            HashMap::new(),
        ));

        mock_store.add_note(note(
            "cooking-note.md".to_string(),
            "Cooking Note".to_string(),
            vec!["cooking".to_string(), "recipes".to_string()],
            HashMap::new(),
        ));

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
        let parsed = parse_tool_json(&call_result);

        // Should match both rust and python notes (OR logic)
        assert_eq!(parsed["count"], 2);
    }

    #[tokio::test]
    async fn test_property_search_single_tag() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let mock_store = Arc::new(MockNoteStore::new());

        mock_store.add_note(note(
            "tagged.md".to_string(),
            "Tagged Note".to_string(),
            vec!["important".to_string()],
            HashMap::new(),
        ));

        mock_store.add_note(note(
            "untagged.md".to_string(),
            "Untagged Note".to_string(),
            vec![],
            HashMap::new(),
        ));

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
        let parsed = parse_tool_json(&call_result);
        assert_eq!(parsed["count"], 1);
    }

    #[tokio::test]
    async fn test_property_search_multiple_properties_and_logic() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let mock_store = Arc::new(MockNoteStore::new());

        let mut props1 = HashMap::new();
        props1.insert("status".to_string(), serde_json::json!("draft"));
        props1.insert("priority".to_string(), serde_json::json!("high"));

        mock_store.add_note(note(
            "high-priority-draft.md".to_string(),
            "High Priority Draft".to_string(),
            vec![],
            props1,
        ));

        let mut props2 = HashMap::new();
        props2.insert("status".to_string(), serde_json::json!("draft"));
        props2.insert("priority".to_string(), serde_json::json!("low"));

        mock_store.add_note(note(
            "low-priority-draft.md".to_string(),
            "Low Priority Draft".to_string(),
            vec![],
            props2,
        ));

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
        let parsed = parse_tool_json(&call_result);

        // Should only match the high priority draft
        assert_eq!(parsed["count"], 1);
        let matches = parsed["matches"].as_array().unwrap();
        assert!(matches[0]["path"]
            .as_str()
            .unwrap()
            .contains("high-priority"));
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

            mock_store.add_note(note(
                format!("note{i}.md"),
                format!("Note {i}"),
                vec![],
                props,
            ));
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
        let parsed = parse_tool_json(&call_result);

        // Should respect the limit
        assert_eq!(parsed["count"], 3);
    }
}
