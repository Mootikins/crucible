//! Unified MCP Server exposing all Crucible tools via stdio transport
//!
//! This module combines `NoteTools`, `SearchTools`, and `KilnTools` into a single
//! MCP server that agents can discover via the ACP protocol.
//!
//! ## Architecture
//!
//! The `CrucibleMcpServer` uses the single-router delegation pattern:
//! - Single `#[tool_router]` on `CrucibleMcpServer` provides unified MCP interface
//! - Tool methods delegate to organized business logic in `NoteTools`, `SearchTools`, `KilnTools`
//! - Maintains modular organization while providing single server endpoint
//!
//! This pattern allows:
//! - Clean MCP server interface for agents
//! - Organized business logic in separate modules
//! - Easy testing of individual tool categories
//! - Future composition of additional tool routers
//!
//! ## `NoteStore` Integration
//!
//! When a `NoteStore` is provided via `with_note_store()`, the tools use indexed
//! metadata for faster operations:
//! - `read_metadata` uses the index instead of parsing from filesystem
//! - `list_notes` uses the index for directory listing
//! - `property_search` uses the index for property filtering

#![allow(missing_docs)]

use crate::{KilnTools, NoteTools, SearchTools};
use crucible_core::background::{BackgroundSpawner, JobStatus, SubagentBlockingConfig};
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::storage::NoteStore;
use crucible_core::traits::KnowledgeRepository;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{model::CallToolResult, tool, tool_handler, tool_router, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// Re-export parameter types from individual modules
use crate::notes::{
    CreateNoteParams, DeleteNoteParams, ListNotesParams, ReadMetadataParams, ReadNoteParams,
    UpdateNoteParams,
};
use crate::search::{PropertySearchParams, SemanticSearchParams, TextSearchParams};

/// Unified MCP server exposing all Crucible tools
///
/// This server aggregates tools from three categories:
/// - **`NoteTools`** (6 tools): CRUD operations on notes
/// - **`SearchTools`** (3 tools): Semantic, text, and property search
/// - **`KilnTools`** (3 tools): Kiln metadata and statistics
#[derive(Clone)]
pub struct CrucibleMcpServer {
    note_tools: NoteTools,
    search_tools: SearchTools,
    kiln_tools: KilnTools,
    delegation_context: Option<DelegationContext>,
    tool_router: ToolRouter<Self>,
}

#[derive(Clone)]
pub struct DelegationContext {
    pub background_spawner: Arc<dyn BackgroundSpawner>,
    pub session_id: String,
    pub targets: Vec<String>,
    pub enabled: bool,
    pub depth: u32,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DelegateSessionParams {
    pub prompt: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub background: Option<bool>,
}

impl CrucibleMcpServer {
    /// Create a new MCP server for a kiln
    ///
    /// # Arguments
    ///
    /// * `kiln_path` - Path to the kiln directory
    /// * `knowledge_repo` - Repository for semantic search
    /// * `embedding_provider` - Provider for generating embeddings
    pub fn new(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        Self::new_with_delegation(kiln_path, knowledge_repo, embedding_provider, None)
    }

    pub fn new_with_delegation(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        delegation_context: Option<DelegationContext>,
    ) -> Self {
        Self {
            note_tools: NoteTools::new(kiln_path.clone()),
            search_tools: SearchTools::new(kiln_path.clone(), knowledge_repo, embedding_provider),
            kiln_tools: KilnTools::new(kiln_path),
            delegation_context,
            tool_router: Self::tool_router(),
        }
    }

    /// Create a new MCP server with `NoteStore` for optimized operations
    ///
    /// When a `NoteStore` is provided, the following operations use indexed metadata:
    /// - `read_metadata` - Uses index instead of parsing from filesystem
    /// - `list_notes` - Uses index for directory listing
    /// - `property_search` - Uses index for property filtering
    ///
    /// # Arguments
    ///
    /// * `kiln_path` - Path to the kiln directory
    /// * `knowledge_repo` - Repository for semantic search
    /// * `embedding_provider` - Provider for generating embeddings
    /// * `note_store` - `NoteStore` for indexed metadata access
    pub fn with_note_store(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        note_store: Arc<dyn NoteStore>,
    ) -> Self {
        Self {
            note_tools: NoteTools::with_note_store(kiln_path.clone(), note_store.clone()),
            search_tools: SearchTools::with_note_store(
                kiln_path.clone(),
                knowledge_repo,
                embedding_provider,
                note_store,
            ),
            kiln_tools: KilnTools::new(kiln_path),
            delegation_context: None,
            tool_router: Self::tool_router(),
        }
    }

    /// List all available tools with their metadata
    ///
    /// This is useful for testing and debugging to verify tool exposure.
    ///
    /// # Returns
    ///
    /// A vector of tool definitions including name, description, and input schema
    #[must_use]
    pub fn list_tools(&self) -> Vec<rmcp::model::Tool> {
        self.tool_router.list_all()
    }

    /// Get the number of tools exposed by this server
    ///
    /// # Returns
    ///
    /// The count of available tools
    #[must_use]
    pub fn tool_count(&self) -> usize {
        self.tool_router.list_all().len()
    }
}

// ===== MCP Server Implementation =====
// Single router with delegation to organized tool modules

#[tool_router]
impl CrucibleMcpServer {
    // ===== Note Tools (6) =====

    #[tool(description = "Create a new note in the kiln")]
    pub async fn create_note(
        &self,
        params: Parameters<CreateNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.note_tools.create_note(params).await
    }

    #[tool(description = "Read note content with optional line range")]
    pub async fn read_note(
        &self,
        params: Parameters<ReadNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.note_tools.read_note(params).await
    }

    #[tool(description = "Read note metadata without loading full content")]
    pub async fn read_metadata(
        &self,
        params: Parameters<ReadMetadataParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.note_tools.read_metadata(params).await
    }

    #[tool(description = "Update an existing note")]
    pub async fn update_note(
        &self,
        params: Parameters<UpdateNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.note_tools.update_note(params).await
    }

    #[tool(description = "Delete a note from the kiln")]
    pub async fn delete_note(
        &self,
        params: Parameters<DeleteNoteParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.note_tools.delete_note(params).await
    }

    #[tool(description = "List notes in a directory")]
    pub async fn list_notes(
        &self,
        params: Parameters<ListNotesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.note_tools.list_notes(params).await
    }

    // ===== Search Tools (3) =====

    #[tool(description = "Search notes using semantic similarity")]
    pub async fn semantic_search(
        &self,
        params: Parameters<SemanticSearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.search_tools.semantic_search(params).await
    }

    #[tool(description = "Fast full-text search across notes")]
    pub async fn text_search(
        &self,
        params: Parameters<TextSearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.search_tools.text_search(params).await
    }

    #[tool(description = "Search notes by frontmatter properties (includes tags)")]
    pub async fn property_search(
        &self,
        params: Parameters<PropertySearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        self.search_tools.property_search(params).await
    }

    // ===== Kiln Tools (3) =====

    #[tool(description = "Get comprehensive kiln information including root path and statistics")]
    pub async fn get_kiln_info(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        self.kiln_tools.get_kiln_info().await
    }

    #[tool(description = "Get kiln roots information")]
    pub async fn get_kiln_roots(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        self.kiln_tools.get_kiln_roots().await
    }

    #[tool(description = "Get kiln statistics")]
    pub async fn get_kiln_stats(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        self.kiln_tools.get_kiln_stats().await
    }

    #[tool(description = "Delegate work to a child agent session")]
    pub async fn delegate_session(
        &self,
        params: Parameters<DelegateSessionParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;

        let delegation = self.delegation_context.as_ref().ok_or_else(|| {
            rmcp::ErrorData::internal_error(
                "delegate_session unavailable: no daemon delegation context",
                None,
            )
        })?;

        if !delegation.enabled {
            return Err(rmcp::ErrorData::invalid_params(
                "delegate_session is disabled for this session",
                None,
            ));
        }

        if let Some(target) = params.target.as_ref() {
            if !delegation.targets.is_empty() && !delegation.targets.contains(target) {
                return Err(rmcp::ErrorData::invalid_params(
                    format!(
                        "target '{target}' is not allowed. Available targets: {}",
                        delegation.targets.join(", ")
                    ),
                    None,
                ));
            }
        }

        let child_depth = delegation.depth.saturating_add(1);
        let mut context_parts = vec![format!("Delegation depth: {child_depth}")];
        if let Some(description) = params.description.as_ref().filter(|d| !d.is_empty()) {
            context_parts.push(format!("Description: {description}"));
        }
        if let Some(target) = params.target.as_ref() {
            context_parts.push(format!("Target agent: {target}"));
        }
        let context = Some(context_parts.join("\n"));

        if params.background.unwrap_or(false) {
            let job_id = delegation
                .background_spawner
                .spawn_subagent(&delegation.session_id, params.prompt, context)
                .await
                .map_err(|e| {
                    rmcp::ErrorData::internal_error(
                        format!("Failed to spawn delegated session: {e}"),
                        None,
                    )
                })?;

            let content = rmcp::model::Content::json(serde_json::json!({
                "delegation_id": job_id,
                "status": "spawned",
            }))?;
            return Ok(CallToolResult::success(vec![content]));
        }

        let result = delegation
            .background_spawner
            .spawn_subagent_blocking(
                &delegation.session_id,
                params.prompt,
                context,
                SubagentBlockingConfig::default(),
                None,
            )
            .await
            .map_err(|e| {
                rmcp::ErrorData::internal_error(
                    format!("Failed to run delegated session: {e}"),
                    None,
                )
            })?;

        let content = if result.info.status == JobStatus::Completed {
            serde_json::json!({
                "delegation_id": format!("deleg-{}", result.info.id),
                "status": "completed",
                "result": result.output.unwrap_or_default(),
            })
        } else {
            serde_json::json!({
                "delegation_id": format!("deleg-{}", result.info.id),
                "status": "failed",
                "error": result.error.unwrap_or_else(|| format!("Delegated session ended with status {}", result.info.status)),
            })
        };

        let content = rmcp::model::Content::json(content)?;
        Ok(CallToolResult::success(vec![content]))
    }
}

// ===== ServerHandler Implementation =====
// Automatically implements call_tool and list_tools using the tool_router field

#[tool_handler]
impl ServerHandler for CrucibleMcpServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::default(),
            capabilities: rmcp::model::ServerCapabilities {
                tools: Some(rmcp::model::ToolsCapability {
                    list_changed: None,
                }),
                ..Default::default()
            },
            server_info: rmcp::model::Implementation {
                name: "crucible-mcp-server".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: Some("Crucible MCP Server".into()),
                icons: None,
                website_url: None,
            },
            instructions: Some("Crucible MCP server exposing 13 tools for knowledge management: 6 note operations, 3 search capabilities, 3 kiln metadata functions, and session delegation.".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crucible_core::background::{JobError, JobInfo, JobKind, JobResult};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
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

        fn provider_name(&self) -> &str {
            "mock"
        }

        async fn list_models(&self) -> anyhow::Result<Vec<String>> {
            Ok(vec!["mock-model".to_string()])
        }
    }

    struct MockBackgroundSpawner {
        spawn_calls: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl BackgroundSpawner for MockBackgroundSpawner {
        async fn spawn_bash(
            &self,
            _session_id: &str,
            _command: String,
            _workdir: Option<std::path::PathBuf>,
            _timeout: Option<std::time::Duration>,
        ) -> Result<String, JobError> {
            Err(JobError::SpawnFailed("not implemented in test".to_string()))
        }

        async fn spawn_subagent(
            &self,
            _session_id: &str,
            _prompt: String,
            _context: Option<String>,
        ) -> Result<String, JobError> {
            self.spawn_calls.fetch_add(1, Ordering::SeqCst);
            Ok("job-test".to_string())
        }

        async fn spawn_subagent_blocking(
            &self,
            session_id: &str,
            _prompt: String,
            _context: Option<String>,
            _config: SubagentBlockingConfig,
            _cancel_rx: Option<tokio::sync::oneshot::Receiver<()>>,
        ) -> Result<JobResult, JobError> {
            let mut info = JobInfo::new(
                session_id.to_string(),
                JobKind::Subagent {
                    prompt: "test".to_string(),
                    context: None,
                },
            );
            info.mark_completed();
            Ok(JobResult::success(info, "done".to_string()))
        }

        fn list_jobs(&self, _session_id: &str) -> Vec<JobInfo> {
            vec![]
        }

        fn get_job_result(&self, _job_id: &String) -> Option<JobResult> {
            None
        }

        async fn cancel_job(&self, _job_id: &String) -> bool {
            false
        }
    }

    #[test]
    fn test_server_creation() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let _server = CrucibleMcpServer::new(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        // Server should create successfully
    }

    #[test]
    fn test_tool_router_creation() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let _server = CrucibleMcpServer::new(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        // This should compile and not panic - the tool_router macro generates the router
        let _router = CrucibleMcpServer::tool_router();
    }

    #[tokio::test]
    async fn test_delegate_session_without_context_returns_graceful_error() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;
        let server = CrucibleMcpServer::new(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        let result = server
            .delegate_session(Parameters(DelegateSessionParams {
                prompt: "test".to_string(),
                description: Some("desc".to_string()),
                target: None,
                background: Some(true),
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("no daemon delegation context"));
    }

    #[tokio::test]
    async fn test_delegate_session_spawns_background_subagent() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;
        let spawner = Arc::new(MockBackgroundSpawner {
            spawn_calls: AtomicUsize::new(0),
        });

        let server = CrucibleMcpServer::new_with_delegation(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
            Some(DelegationContext {
                background_spawner: spawner.clone(),
                session_id: "chat-parent".to_string(),
                targets: vec!["opencode".to_string()],
                enabled: true,
                depth: 0,
            }),
        );

        let result = server
            .delegate_session(Parameters(DelegateSessionParams {
                prompt: "do work".to_string(),
                description: Some("desc".to_string()),
                target: Some("opencode".to_string()),
                background: Some(true),
            }))
            .await;

        assert!(result.is_ok());
        assert_eq!(spawner.spawn_calls.load(Ordering::SeqCst), 1);
    }
}
