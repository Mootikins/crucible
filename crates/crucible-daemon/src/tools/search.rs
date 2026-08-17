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
        // No name: this constructor is handed a directory and nothing else
        // (`cru mcp` against a path, the kiln-only server). Naming it after the
        // directory is exactly the disclosure `SearchResult::kiln` exists to
        // prevent, so hits from this source go out unattributed. Session-scoped
        // servers replace this source via `with_search_sources`, which carries
        // the registry names.
        let search_sources = vec![KilnSearchSource {
            kiln_path: std::path::PathBuf::from(&kiln_path),
            kiln_name: None,
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
            Some(self.scope.anchor()),
        )
        .await
        .mcp_err_ctx("Note search failed")?;

        // The kiln is named, never located: this object is a tool result, i.e.
        // a message the model reads. `"kiln_path": <absolute directory>` used to
        // be here, one per hit. The key is omitted rather than emptied when the
        // source had no registry name — a `""` kiln is a kiln the model would
        // believe in.
        let all_results: Vec<serde_json::Value> = note_results
            .into_iter()
            .map(|r| {
                let mut entry = serde_json::Map::new();
                entry.insert("type".to_string(), serde_json::json!("note"));
                entry.insert("id".to_string(), serde_json::json!(r.document_id));
                entry.insert("score".to_string(), serde_json::json!(r.score));
                entry.insert("snippet".to_string(), serde_json::json!(r.snippet));
                entry.insert("highlights".to_string(), serde_json::json!(r.highlights));
                if let Some(name) = r.kiln.as_ref() {
                    entry.insert("kiln".to_string(), serde_json::json!(name.as_str()));
                }
                serde_json::Value::Object(entry)
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
            // Name what the caller named, never what it resolved to. Echoing
            // `search_path` here handed the model the kiln's absolute
            // directory for free: name a folder that cannot exist and read the
            // root out of the refusal. The caller supplied `folder`; the kiln
            // root it is resolved against is precisely what it did not.
            return Err(rmcp::ErrorData::invalid_params(
                format!(
                    "Search path does not exist: {}",
                    folder.as_deref().unwrap_or(".")
                ),
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
mod tests;
