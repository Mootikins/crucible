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

use super::helpers::{make_server_info, McpResultExt};
use super::{KilnTools, NoteTools, SearchTools};
use crucible_config::{DataClassification, TrustLevel};
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
use super::notes::{
    CreateNoteParams, DeleteNoteParams, ListNotesParams, ReadMetadataParams, ReadNoteParams,
    UpdateNoteParams,
};
use super::search::{PropertySearchParams, SemanticSearchParams, TextSearchParams};

/// Unified MCP server exposing all Crucible tools
///
/// This server aggregates tools from three categories:
/// - **`NoteTools`** (6 tools): CRUD operations on notes
/// - **`SearchTools`** (3 tools): Semantic, text, and property search
/// - **`KilnTools`** (1 tool): Kiln metadata and statistics
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
    pub data_classification: DataClassification,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DelegateSessionParams {
    /// The task or question for the delegated agent to work on
    pub prompt: String,
    /// Brief human-readable description of what this delegation does
    #[serde(default)]
    pub description: Option<String>,
    /// Target agent name to delegate to (e.g., "cursor", "opencode"). Omit to use the same agent type.
    #[serde(default)]
    pub target: Option<String>,
    /// If true, return immediately with a delegation ID. If false (default), wait for the result.
    #[serde(default)]
    pub background: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ListJobsParams {}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetJobResultParams {
    /// The job ID to retrieve the result for
    pub job_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CancelJobParams {
    /// The job ID to cancel
    pub job_id: String,
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
                note_store.clone(),
            ),
            kiln_tools: KilnTools::with_note_store(kiln_path, note_store),
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
        let mut tools = self.tool_router.list_all();

        if let Some(delegation_context) = &self.delegation_context {
            if !delegation_context.targets.is_empty() {
                if let Some(delegate_tool) = tools.iter_mut().find(|t| t.name == "delegate_session")
                {
                    let targets_str = delegation_context.targets.join(", ");
                    let new_desc = format!(
                        "Delegate a task to another AI agent. Available delegation targets: {targets_str}. The target agent receives the prompt, executes the task, and returns the result."
                    );
                    delegate_tool.description = Some(new_desc.into());
                }
            }
        }

        tools
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

    #[tool(
        description = "Delegate a task to another AI agent (e.g., cursor, opencode). The target agent receives the prompt, executes the task, and returns the result. Use this when asked to hand off work to a specific agent."
    )]
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

        let child_trust_level = TrustLevel::Cloud;
        if !child_trust_level.satisfies(delegation.data_classification) {
            return Err(rmcp::ErrorData::invalid_params(
                format!(
                    "Delegated agent's trust level '{}' is insufficient for kiln data classification '{}'. Requires '{}' trust.",
                    child_trust_level,
                    delegation.data_classification,
                    delegation.data_classification.required_trust_level(),
                ),
                None,
            ));
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

    // ===== Job Tools (3) =====

    #[tool(
        description = "List all background jobs (running and completed) for the current session"
    )]
    pub async fn list_jobs(
        &self,
        _params: Parameters<ListJobsParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let delegation = self.delegation_context.as_ref().ok_or_else(|| {
            rmcp::ErrorData::internal_error(
                "list_jobs unavailable: no daemon delegation context",
                None,
            )
        })?;

        let jobs = delegation
            .background_spawner
            .list_jobs(&delegation.session_id);
        let content = rmcp::model::Content::json(
            serde_json::to_value(&jobs).mcp_err_ctx("Failed to serialize jobs")?,
        )?;
        Ok(CallToolResult::success(vec![content]))
    }

    #[tool(description = "Get the result of a specific background job by ID")]
    pub async fn get_job_result(
        &self,
        params: Parameters<GetJobResultParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let delegation = self.delegation_context.as_ref().ok_or_else(|| {
            rmcp::ErrorData::internal_error(
                "get_job_result unavailable: no daemon delegation context",
                None,
            )
        })?;

        match delegation.background_spawner.get_job_result(&params.job_id) {
            Some(result) => {
                let content =
                    rmcp::model::Content::json(serde_json::to_value(&result).map_err(|e| {
                        rmcp::ErrorData::internal_error(
                            format!("Failed to serialize job result: {e}"),
                            None,
                        )
                    })?)?;
                Ok(CallToolResult::success(vec![content]))
            }
            None => Err(rmcp::ErrorData::invalid_params(
                format!("Job not found: {}", params.job_id),
                None,
            )),
        }
    }

    #[tool(description = "Cancel a running background job by ID")]
    pub async fn cancel_job(
        &self,
        params: Parameters<CancelJobParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let delegation = self.delegation_context.as_ref().ok_or_else(|| {
            rmcp::ErrorData::internal_error(
                "cancel_job unavailable: no daemon delegation context",
                None,
            )
        })?;

        let cancelled = delegation
            .background_spawner
            .cancel_job(&params.job_id)
            .await;
        let content = rmcp::model::Content::json(serde_json::json!({
            "job_id": params.job_id,
            "cancelled": cancelled,
        }))?;
        Ok(CallToolResult::success(vec![content]))
    }
}

// ===== ServerHandler Implementation ====
// Automatically implements call_tool and list_tools using the tool_router field

#[tool_handler]
impl ServerHandler for CrucibleMcpServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        make_server_info(
            "Crucible knowledge management server with 14 tools. \
            Notes: create_note, read_note, update_note, delete_note, list_notes, \
            read_metadata. \
            Search: semantic_search, text_search, property_search. \
            Kiln: get_kiln_info. \
            Delegation: delegate_session \
            \u{2014} hand off tasks to other agents when asked to delegate. \
            Jobs: list_jobs, get_job_result, cancel_job \
            \u{2014} manage background jobs.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crucible_core::background::{JobError, JobInfo, JobKind, JobResult};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tempfile::TempDir;

    use crate::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};

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
                data_classification: DataClassification::Public,
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

    #[test]
    fn test_delegate_session_description_includes_target_hints() {
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
                targets: vec!["my-custom-agent".to_string(), "another-agent".to_string()],
                enabled: true,
                depth: 0,
                data_classification: DataClassification::Public,
            }),
        );

        let tools = server.list_tools();
        let delegate_tool = tools
            .iter()
            .find(|t| t.name == "delegate_session")
            .expect("delegate_session tool should exist");

        let desc = delegate_tool
            .description
            .as_ref()
            .map(|d| d.as_ref())
            .unwrap_or("");
        assert!(
            desc.contains("my-custom-agent"),
            "Description should contain 'my-custom-agent' target. Got: {}",
            desc
        );
        assert!(
            desc.contains("another-agent"),
            "Description should contain 'another-agent' target. Got: {}",
            desc
        );
    }

    #[test]
    fn test_delegate_session_description_generic_when_no_targets() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let server = CrucibleMcpServer::new(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        let tools = server.list_tools();
        let delegate_tool = tools
            .iter()
            .find(|t| t.name == "delegate_session")
            .expect("delegate_session tool should exist");

        let desc = delegate_tool
            .description
            .as_ref()
            .map(|d| d.as_ref())
            .unwrap_or("");
        assert!(
            !desc.contains("Available targets:"),
            "Description should not have 'Available targets:' when no context. Got: {}",
            desc
        );
    }

    #[test]
    fn test_delegate_session_description_generic_when_empty_targets() {
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
                targets: vec![],
                enabled: true,
                depth: 0,
                data_classification: DataClassification::Public,
            }),
        );

        let tools = server.list_tools();
        let delegate_tool = tools
            .iter()
            .find(|t| t.name == "delegate_session")
            .expect("delegate_session tool should exist");

        let desc = delegate_tool
            .description
            .as_ref()
            .map(|d| d.as_ref())
            .unwrap_or("");
        assert!(
            !desc.contains("Available targets:"),
            "Description should not have 'Available targets:' when targets empty. Got: {}",
            desc
        );
    }

    struct MockJobBackgroundSpawner;

    #[async_trait::async_trait]
    impl BackgroundSpawner for MockJobBackgroundSpawner {
        async fn spawn_bash(
            &self,
            _session_id: &str,
            _command: String,
            _workdir: Option<std::path::PathBuf>,
            _timeout: Option<std::time::Duration>,
        ) -> Result<String, JobError> {
            Err(JobError::SpawnFailed("not implemented".to_string()))
        }

        async fn spawn_subagent(
            &self,
            _session_id: &str,
            _prompt: String,
            _context: Option<String>,
        ) -> Result<String, JobError> {
            Err(JobError::SpawnFailed("not implemented".to_string()))
        }

        fn list_jobs(&self, session_id: &str) -> Vec<JobInfo> {
            let mut info = JobInfo::new(
                session_id.to_string(),
                JobKind::Subagent {
                    prompt: "test task".to_string(),
                    context: None,
                },
            );
            info.id = "job-test-123".to_string();
            vec![info]
        }

        fn get_job_result(&self, job_id: &String) -> Option<JobResult> {
            if job_id == "job-test-123" {
                let mut info = JobInfo::new(
                    "test-session".to_string(),
                    JobKind::Subagent {
                        prompt: "test".to_string(),
                        context: None,
                    },
                );
                info.id = "job-test-123".to_string();
                info.mark_completed();
                Some(JobResult::success(info, "completed output".to_string()))
            } else {
                None
            }
        }

        async fn cancel_job(&self, job_id: &String) -> bool {
            job_id == "job-test-123"
        }
    }

    fn make_server_without_delegation() -> CrucibleMcpServer {
        let temp = TempDir::new().unwrap();
        CrucibleMcpServer::new(
            temp.path().to_str().unwrap().to_string(),
            Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>,
            Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>,
        )
    }

    fn make_server_with_job_spawner() -> CrucibleMcpServer {
        let temp = TempDir::new().unwrap();
        CrucibleMcpServer::new_with_delegation(
            temp.path().to_str().unwrap().to_string(),
            Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>,
            Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>,
            Some(DelegationContext {
                background_spawner: Arc::new(MockJobBackgroundSpawner),
                session_id: "test-session".to_string(),
                targets: vec![],
                enabled: true,
                depth: 0,
                data_classification: DataClassification::Public,
            }),
        )
    }

    fn make_server_with_delegation_classification(
        data_classification: DataClassification,
    ) -> (CrucibleMcpServer, Arc<MockBackgroundSpawner>) {
        let temp = TempDir::new().unwrap();
        let spawner = Arc::new(MockBackgroundSpawner {
            spawn_calls: AtomicUsize::new(0),
        });
        let server = CrucibleMcpServer::new_with_delegation(
            temp.path().to_str().unwrap().to_string(),
            Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>,
            Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>,
            Some(DelegationContext {
                background_spawner: spawner.clone(),
                session_id: "chat-parent".to_string(),
                targets: vec![],
                enabled: true,
                depth: 0,
                data_classification,
            }),
        );

        (server, spawner)
    }

    fn make_server_with_delegation_disabled(
        data_classification: DataClassification,
    ) -> CrucibleMcpServer {
        let temp = TempDir::new().unwrap();
        CrucibleMcpServer::new_with_delegation(
            temp.path().to_str().unwrap().to_string(),
            Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>,
            Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>,
            Some(DelegationContext {
                background_spawner: Arc::new(MockBackgroundSpawner {
                    spawn_calls: AtomicUsize::new(0),
                }),
                session_id: "chat-parent".to_string(),
                targets: vec![],
                enabled: false,
                depth: 0,
                data_classification,
            }),
        )
    }

    #[tokio::test]
    async fn test_delegation_allowed_for_internal_kiln() {
        let (server, spawner) =
            make_server_with_delegation_classification(DataClassification::Internal);

        let result = server
            .delegate_session(Parameters(DelegateSessionParams {
                prompt: "do work".to_string(),
                description: Some("desc".to_string()),
                target: None,
                background: Some(true),
            }))
            .await;

        assert!(result.is_ok());
        assert_eq!(spawner.spawn_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_delegation_blocked_for_confidential_kiln() {
        let (server, spawner) =
            make_server_with_delegation_classification(DataClassification::Confidential);

        let result = server
            .delegate_session(Parameters(DelegateSessionParams {
                prompt: "do work".to_string(),
                description: Some("desc".to_string()),
                target: None,
                background: Some(true),
            }))
            .await;

        assert!(result.is_err());
        assert_eq!(spawner.spawn_calls.load(Ordering::SeqCst), 0);

        let err = result.unwrap_err();
        assert!(err.message.contains("insufficient"));
        assert!(err.message.contains("cloud"));
        assert!(err.message.contains("confidential"));
        assert!(err.message.contains("local"));
    }

    #[tokio::test]
    async fn test_delegation_allowed_for_public_kiln() {
        let (server, spawner) =
            make_server_with_delegation_classification(DataClassification::Public);

        let result = server
            .delegate_session(Parameters(DelegateSessionParams {
                prompt: "do work".to_string(),
                description: Some("desc".to_string()),
                target: None,
                background: Some(true),
            }))
            .await;

        assert!(result.is_ok());
        assert_eq!(spawner.spawn_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_delegation_disabled_fires_before_trust_check() {
        // enabled=false + Confidential: should get "disabled" error, not trust error
        let server = make_server_with_delegation_disabled(DataClassification::Confidential);
        let result = server
            .delegate_session(Parameters(DelegateSessionParams {
                prompt: "do work".to_string(),
                description: None,
                target: None,
                background: Some(true),
            }))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("disabled"),
            "Expected 'disabled' error but got: {}",
            err.message
        );
        assert!(
            !err.message.contains("insufficient"),
            "Should not get trust error, got: {}",
            err.message
        );
    }

    #[tokio::test]
    async fn test_delegation_disabled_with_public_kiln() {
        // enabled=false + Public: should still get "disabled" error
        let server = make_server_with_delegation_disabled(DataClassification::Public);
        let result = server
            .delegate_session(Parameters(DelegateSessionParams {
                prompt: "do work".to_string(),
                description: None,
                target: None,
                background: Some(true),
            }))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("disabled"),
            "Expected 'disabled' error but got: {}",
            err.message
        );
    }

    #[tokio::test]
    async fn test_list_jobs_without_context_returns_error() {
        let server = make_server_without_delegation();
        let result = server.list_jobs(Parameters(ListJobsParams {})).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("no daemon delegation context"));
    }

    #[tokio::test]
    async fn test_list_jobs_returns_jobs_for_session() {
        let server = make_server_with_job_spawner();
        let result = server.list_jobs(Parameters(ListJobsParams {})).await;

        assert!(result.is_ok());
        let call_result = result.unwrap();
        assert!(!call_result.content.is_empty());
    }

    #[tokio::test]
    async fn test_get_job_result_without_context_returns_error() {
        let server = make_server_without_delegation();
        let result = server
            .get_job_result(Parameters(GetJobResultParams {
                job_id: "job-test-123".to_string(),
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("no daemon delegation context"));
    }

    #[tokio::test]
    async fn test_get_job_result_returns_result_for_known_job() {
        let server = make_server_with_job_spawner();
        let result = server
            .get_job_result(Parameters(GetJobResultParams {
                job_id: "job-test-123".to_string(),
            }))
            .await;

        assert!(result.is_ok());
        let call_result = result.unwrap();
        assert!(!call_result.content.is_empty());
    }

    #[tokio::test]
    async fn test_get_job_result_unknown_job_returns_error() {
        let server = make_server_with_job_spawner();
        let result = server
            .get_job_result(Parameters(GetJobResultParams {
                job_id: "nonexistent-job".to_string(),
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Job not found"));
    }

    #[tokio::test]
    async fn test_cancel_job_without_context_returns_error() {
        let server = make_server_without_delegation();
        let result = server
            .cancel_job(Parameters(CancelJobParams {
                job_id: "job-test-123".to_string(),
            }))
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("no daemon delegation context"));
    }

    #[tokio::test]
    async fn test_cancel_job_returns_cancelled_status() {
        let server = make_server_with_job_spawner();
        let result = server
            .cancel_job(Parameters(CancelJobParams {
                job_id: "job-test-123".to_string(),
            }))
            .await;

        assert!(result.is_ok());
        let call_result = result.unwrap();
        assert!(!call_result.content.is_empty());
    }

    #[test]
    fn test_job_tools_appear_in_tool_router() {
        let server = make_server_with_job_spawner();
        let tools = server.list_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

        assert!(
            tool_names.contains(&"list_jobs"),
            "list_jobs should be in tool list: {:?}",
            tool_names
        );
        assert!(
            tool_names.contains(&"get_job_result"),
            "get_job_result should be in tool list: {:?}",
            tool_names
        );
        assert!(
            tool_names.contains(&"cancel_job"),
            "cancel_job should be in tool list: {:?}",
            tool_names
        );
    }
}
