//! Unified MCP Server exposing all Crucible tools via stdio transport
//!
//! This module combines NoteTools, SearchTools, and KilnTools into a single
//! MCP server that agents can discover via the ACP protocol.

use rmcp::{
    model::*,
    tool_handler, ServerHandler, ServiceExt,
    transport::stdio,
};
use crate::{NoteTools, SearchTools, KilnTools};
use std::sync::Arc;
use crucible_core::traits::KnowledgeRepository;
use crucible_llm::EmbeddingProvider;

/// Unified MCP server exposing all Crucible tools
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
                 in your personal knowledge base (kiln)."
                    .to_string()
            ),
        }
    }

    async fn list_tools(&self, _request: ListToolsRequest) -> Result<ListToolsResponse, ErrorData> {
        // Collect tools from all routers
        let mut all_tools = Vec::new();

        // Get tools from note_tools
        if let Ok(note_result) = self.note_tools.list_tools(_request.clone()).await {
            all_tools.extend(note_result.tools);
        }

        // Get tools from search_tools
        if let Ok(search_result) = self.search_tools.list_tools(_request.clone()).await {
            all_tools.extend(search_result.tools);
        }

        // Get tools from kiln_tools
        if let Ok(kiln_result) = self.kiln_tools.list_tools(_request).await {
            all_tools.extend(kiln_result.tools);
        }

        Ok(ListToolsResponse {
            tools: all_tools,
            next_cursor: None,
        })
    }

    async fn call_tool(&self, request: CallToolRequest) -> Result<CallToolResult, ErrorData> {
        let tool_name = request.params.name.as_str();

        // Route to appropriate tool handler based on tool name
        // Note tools
        if matches!(tool_name, "create_note" | "read_note" | "read_metadata" | "update_note" | "delete_note" | "list_notes") {
            return self.note_tools.call_tool(request).await;
        }

        // Search tools
        if matches!(tool_name, "semantic_search" | "text_search" | "property_search") {
            return self.search_tools.call_tool(request).await;
        }

        // Kiln tools
        if matches!(tool_name, "get_kiln_info" | "list_recent_notes" | "calculate_kiln_stats") {
            return self.kiln_tools.call_tool(request).await;
        }

        // Tool not found
        Err(ErrorData {
            code: ErrorCode(-32601),
            message: format!("Unknown tool: {}", tool_name).into(),
            data: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::sync::Arc;

    // Mock implementations for testing
    use crucible_core::traits::NoteMetadata;
    use crucible_core::ParsedNote;
    use anyhow::Error;
    use std::pin::Pin;
    use std::future::Future;

    struct MockKnowledgeRepo;
    impl KnowledgeRepository for MockKnowledgeRepo {
        fn search_vectors<'life0, 'async_trait>(
            &'life0 self,
            _vector: Vec<f32>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<crucible_core::types::SearchResult>, Error>> + Send + 'async_trait>>
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async { Ok(vec![]) })
        }

        fn get_note_by_name<'life0, 'life1, 'async_trait>(
            &'life0 self,
            _name: &'life1 str,
        ) -> Pin<Box<dyn Future<Output = Result<Option<ParsedNote>, Error>> + Send + 'async_trait>>
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async { Ok(None) })
        }

        fn list_notes<'life0, 'life1, 'async_trait>(
            &'life0 self,
            _folder: Option<&'life1 str>,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<NoteMetadata>, Error>> + Send + 'async_trait>>
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async { Ok(vec![]) })
        }
    }

    struct MockEmbeddingProvider;
    impl EmbeddingProvider for MockEmbeddingProvider {
        fn embed<'life0, 'life1, 'async_trait>(
            &'life0 self,
            _text: &'life1 str,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<f32>, Box<dyn std::error::Error + Send + Sync>>> + Send + 'async_trait>>
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async { Ok(vec![0.1; 384]) })
        }
    }

    #[test]
    fn test_server_creation() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepo) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let server = CrucibleMcpServer::new(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        // Should create successfully
        assert!(server.note_tools.kiln_path == temp.path().to_str().unwrap());
    }

    #[test]
    fn test_server_info() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepo) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

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
        let knowledge_repo = Arc::new(MockKnowledgeRepo) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let server = CrucibleMcpServer::new(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        // Verify tools are available via the server handler
        let tools = server.list_tools(ListToolsRequest {}).await.unwrap();

        // Should have 10 tools (6 note + 3 search + 1 kiln is actually 3 kiln)
        // Note: create_note, read_note, read_metadata, update_note, delete_note, list_notes (6)
        // Search: semantic_search, text_search, property_search (3)
        // Kiln: get_kiln_info, list_recent_notes, calculate_kiln_stats (3)
        assert!(tools.tools.len() >= 10, "Expected at least 10 tools, got {}", tools.tools.len());

        // Check specific tools exist
        let tool_names: Vec<&str> = tools.tools.iter()
            .map(|t| t.name.as_str())
            .collect();

        // Note tools
        assert!(tool_names.contains(&"create_note"), "Missing create_note tool");
        assert!(tool_names.contains(&"read_note"), "Missing read_note tool");
        assert!(tool_names.contains(&"update_note"), "Missing update_note tool");
        assert!(tool_names.contains(&"delete_note"), "Missing delete_note tool");
        assert!(tool_names.contains(&"list_notes"), "Missing list_notes tool");
        assert!(tool_names.contains(&"read_metadata"), "Missing read_metadata tool");

        // Search tools
        assert!(tool_names.contains(&"semantic_search"), "Missing semantic_search tool");
        assert!(tool_names.contains(&"text_search"), "Missing text_search tool");
        assert!(tool_names.contains(&"property_search"), "Missing property_search tool");

        // Kiln tools
        assert!(tool_names.contains(&"get_kiln_info"), "Missing get_kiln_info tool");
    }
}
