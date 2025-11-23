//! Unified MCP Server exposing all Crucible tools via stdio transport
//!
//! This module combines NoteTools, SearchTools, and KilnTools into a single
//! MCP server that agents can discover via the ACP protocol.
//!
//! ## Architecture
//!
//! The `CrucibleMcpServer` aggregates multiple tool routers and delegates
//! tool calls to the appropriate handler. This pattern allows:
//! - Modular tool organization (notes, search, kiln)
//! - Future MCP server composition (adding external MCP servers)
//! - Clean separation of concerns

use rmcp::{
    model::*,
    ServerHandler, ServiceExt,
    transport::stdio,
    handler::server::RequestContext,
    RoleServer,
    McpError,
};
use crate::{NoteTools, SearchTools, KilnTools};
use std::sync::Arc;
use crucible_core::traits::KnowledgeRepository;
use crucible_core::enrichment::EmbeddingProvider;

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
            search_tools: SearchTools::new(
                kiln_path.clone(),
                knowledge_repo,
                embedding_provider,
            ),
            kiln_tools: KilnTools::new(kiln_path),
        }
    }

    /// Start serving via stdio transport
    ///
    /// This method starts the MCP server and blocks until shutdown.
    /// Used by the CLI's `mcp-server` subcommand.
    ///
    /// # Errors
    ///
    /// Returns an error if the server fails to start or encounters
    /// a fatal error during operation.
    pub async fn serve_stdio(self) -> Result<(), Box<dyn std::error::Error>> {
        let service = self.serve(stdio()).await?;
        service.waiting().await?;
        Ok(())
    }
}

impl ServerHandler for CrucibleMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "Crucible knowledge management system. \
                 Use these tools to create, read, update, delete, and search notes \
                 in your personal knowledge base (kiln). \
                 \n\nAvailable tools: create_note, read_note, update_note, delete_note, \
                 list_notes, read_metadata, semantic_search, text_search, property_search, \
                 get_kiln_info."
                    .to_string()
            ),
        }
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParam>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        // Collect tools from all sub-routers
        let mut all_tools = Vec::new();

        // Get tools from note_tools (6 tools)
        match self.note_tools.list_tools(request.clone(), context.clone()).await {
            Ok(result) => all_tools.extend(result.tools),
            Err(e) => tracing::warn!("Failed to list note tools: {:?}", e),
        }

        // Get tools from search_tools (3 tools)
        match self.search_tools.list_tools(request.clone(), context.clone()).await {
            Ok(result) => all_tools.extend(result.tools),
            Err(e) => tracing::warn!("Failed to list search tools: {:?}", e),
        }

        // Get tools from kiln_tools (3 tools)
        match self.kiln_tools.list_tools(request, context).await {
            Ok(result) => all_tools.extend(result.tools),
            Err(e) => tracing::warn!("Failed to list kiln tools: {:?}", e),
        }

        Ok(ListToolsResult {
            tools: all_tools,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let tool_name = &request.name;

        // Route to appropriate tool handler based on tool name
        // Note tools (6 tools)
        if matches!(
            tool_name.as_ref(),
            "create_note" | "read_note" | "read_metadata"
            | "update_note" | "delete_note" | "list_notes"
        ) {
            return self.note_tools.call_tool(request, context).await;
        }

        // Search tools (3 tools)
        if matches!(
            tool_name.as_ref(),
            "semantic_search" | "text_search" | "property_search"
        ) {
            return self.search_tools.call_tool(request, context).await;
        }

        // Kiln tools (3 tools)
        if matches!(
            tool_name.as_ref(),
            "get_kiln_info" | "list_recent_notes" | "calculate_kiln_stats"
        ) {
            return self.kiln_tools.call_tool(request, context).await;
        }

        // Tool not found
        Err(McpError::method_not_found::<CallToolRequestMethod>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::sync::Arc;
    use crucible_core::enrichment::mock::MockEmbeddingProvider;
    use crucible_core::mock::MockKnowledgeRepo;

    #[test]
    fn test_server_creation() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepo::new()) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider::new()) as Arc<dyn EmbeddingProvider>;

        let _server = CrucibleMcpServer::new(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        // Server should create successfully
    }

    #[test]
    fn test_server_info() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepo::new()) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider::new()) as Arc<dyn EmbeddingProvider>;

        let server = CrucibleMcpServer::new(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );
        let info = server.get_info();

        assert_eq!(info.protocol_version, ProtocolVersion::V_2024_11_05);
        assert!(info.capabilities.tools.is_some());
        assert!(info.instructions.is_some());
        assert!(info.instructions.unwrap().contains("Crucible"));
    }

    #[tokio::test]
    async fn test_tool_routing() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepo::new()) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider::new()) as Arc<dyn EmbeddingProvider>;

        let server = CrucibleMcpServer::new(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        // Create a mock request context
        use rmcp::handler::server::{Peer, PeerMeta};
        let peer = Peer::new(PeerMeta::default());
        let context = RequestContext::new(peer);

        // Verify tools are available via the server handler
        let result = server.list_tools(None, context).await;
        assert!(result.is_ok());

        let tools = result.unwrap().tools;

        // Should have at least 10 tools (may have more with property_search variants)
        assert!(
            tools.len() >= 10,
            "Expected at least 10 tools, got {}",
            tools.len()
        );

        // Check specific tools exist
        let tool_names: Vec<&str> = tools.iter()
            .map(|t| t.name.as_ref())
            .collect();

        // Note tools (6)
        assert!(tool_names.contains(&"create_note"), "Missing create_note tool");
        assert!(tool_names.contains(&"read_note"), "Missing read_note tool");
        assert!(tool_names.contains(&"update_note"), "Missing update_note tool");
        assert!(tool_names.contains(&"delete_note"), "Missing delete_note tool");
        assert!(tool_names.contains(&"list_notes"), "Missing list_notes tool");
        assert!(tool_names.contains(&"read_metadata"), "Missing read_metadata tool");

        // Search tools (3)
        assert!(tool_names.contains(&"semantic_search"), "Missing semantic_search tool");
        assert!(tool_names.contains(&"text_search"), "Missing text_search tool");
        assert!(tool_names.contains(&"property_search"), "Missing property_search tool");

        // Kiln tools (at least get_kiln_info)
        assert!(tool_names.contains(&"get_kiln_info"), "Missing get_kiln_info tool");
    }
}
