//! Unified MCP Server exposing all Crucible tools via stdio transport
//!
//! This module combines NoteTools, SearchTools, and KilnTools into a single
//! MCP server that agents can discover via the ACP protocol.
//!
//! ## Architecture
//!
//! The `CrucibleMcpServer` uses the single-router delegation pattern:
//! - Single `#[tool_router]` on CrucibleMcpServer provides unified MCP interface
//! - Tool methods delegate to organized business logic in NoteTools, SearchTools, KilnTools
//! - Maintains modular organization while providing single server endpoint
//!
//! This pattern allows:
//! - Clean MCP server interface for agents
//! - Organized business logic in separate modules
//! - Easy testing of individual tool categories
//! - Future composition of additional tool routers

use crate::{KilnTools, NoteTools, SearchTools};
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{model::CallToolResult, tool, tool_handler, tool_router, ServerHandler};
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
/// - **NoteTools** (6 tools): CRUD operations on notes
/// - **SearchTools** (3 tools): Semantic, text, and property search
/// - **KilnTools** (3 tools): Kiln metadata and statistics
#[derive(Clone)]
pub struct CrucibleMcpServer {
    note_tools: NoteTools,
    search_tools: SearchTools,
    kiln_tools: KilnTools,
    tool_router: ToolRouter<Self>,
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
        Self {
            note_tools: NoteTools::new(kiln_path.clone()),
            search_tools: SearchTools::new(kiln_path.clone(), knowledge_repo, embedding_provider),
            kiln_tools: KilnTools::new(kiln_path),
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
    pub fn list_tools(&self) -> Vec<rmcp::model::Tool> {
        self.tool_router.list_all()
    }

    /// Get the number of tools exposed by this server
    ///
    /// # Returns
    ///
    /// The count of available tools (expected: 12)
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
            instructions: Some("Crucible MCP server exposing 12 tools for knowledge management: 6 note operations, 3 search capabilities, and 3 kiln metadata functions.".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Arc;
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

        fn model_name(&self) -> &str {
            "mock-model"
        }

        fn dimensions(&self) -> usize {
            384
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
}
