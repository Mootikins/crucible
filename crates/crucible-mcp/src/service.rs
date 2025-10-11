// crates/crucible-mcp/src/service.rs
//
// rmcp-based MCP Server Service Layer
//
// This module provides the official rmcp-based implementation of the Crucible MCP server.
// It wraps existing tool implementations from tools::mod with rmcp's #[tool] macro.

use rmcp::{ErrorData as McpError, model::*, tool, tool_router, tool_handler, handler::server::{wrapper::Parameters, ServerHandler, tool::ToolRouter}};
use crate::database::EmbeddingDatabase;
use crate::embeddings::EmbeddingProvider;
use std::sync::Arc;

/// Crucible MCP Service using rmcp SDK
///
/// This service exposes all 13 Crucible MCP tools via the rmcp protocol.
/// It delegates to existing tool implementations in the tools module.
#[derive(Clone)]
pub struct CrucibleMcpService {
    database: Arc<EmbeddingDatabase>,
    provider: Arc<dyn EmbeddingProvider>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl CrucibleMcpService {
    /// Create a new Crucible MCP service instance
    pub fn new(database: EmbeddingDatabase, provider: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            database: Arc::new(database),
            provider,
            tool_router: Self::tool_router(),
        }
    }

    /// Search notes by frontmatter properties
    #[tool(description = "[READ] Find notes by YAML properties")]
    async fn search_by_properties(
        &self,
        Parameters(params): Parameters<crate::types::SearchByPropertiesParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: Some(params.properties),
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };
        let result = crate::tools::search_by_properties(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Search notes by tags
    #[tool(description = "[READ] Find notes by tags")]
    async fn search_by_tags(
        &self,
        Parameters(params): Parameters<crate::types::SearchByTagsParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: Some(params.tags),
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };
        let result = crate::tools::search_by_tags(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Search notes in a specific folder
    #[tool(description = "[READ] List notes in folder (recursive)")]
    async fn search_by_folder(
        &self,
        Parameters(params): Parameters<crate::types::SearchByFolderParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: None,
            path: Some(params.path),
            recursive: Some(params.recursive),
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };
        let result = crate::tools::search_by_folder(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Search notes by filename pattern
    #[tool(description = "[READ] Find notes matching filename")]
    async fn search_by_filename(
        &self,
        Parameters(params): Parameters<crate::types::SearchByFilenameParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: Some(params.pattern),
            query: None,
            top_k: None,
            force: None,
        };
        let result = crate::tools::search_by_filename(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Full-text search in note contents
    #[tool(description = "[READ] Full-text search in note contents")]
    async fn search_by_content(
        &self,
        Parameters(params): Parameters<crate::types::SearchByContentParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: Some(params.query),
            top_k: None,
            force: None,
        };
        let result = crate::tools::search_by_content(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Semantic search using embeddings
    #[tool(description = "[READ] Semantic search (needs index_vault first)")]
    async fn semantic_search(
        &self,
        Parameters(params): Parameters<crate::types::SemanticSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: Some(params.query),
            top_k: Some(params.top_k),
            force: None,
        };
        let result = crate::tools::semantic_search(&self.database, &self.provider, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Generate embeddings for all vault notes
    #[tool(description = "[INDEX] Generate embeddings for notes (slow)")]
    async fn index_vault(
        &self,
        Parameters(params): Parameters<crate::types::IndexVaultParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: None,
            path: Some(params.path),
            recursive: None,
            pattern: Some(params.pattern),
            query: None,
            top_k: None,
            force: Some(params.force),
        };
        let result = crate::tools::index_vault(&self.database, &self.provider, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Get metadata for a specific note
    #[tool(description = "[READ] Get note metadata and frontmatter")]
    async fn get_note_metadata(
        &self,
        Parameters(params): Parameters<crate::types::GetNoteMetadataParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: None,
            path: Some(params.path),
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };
        let result = crate::tools::get_note_metadata(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Update frontmatter properties of a note
    #[tool(description = "[WRITE] Update note frontmatter properties")]
    async fn update_note_properties(
        &self,
        Parameters(params): Parameters<crate::types::UpdateNotePropertiesParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: Some(params.properties),
            tags: None,
            path: Some(params.path),
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };
        let result = crate::tools::update_note_properties(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Index a Crucible document for search
    #[tool(description = "[INDEX] Index document for search")]
    async fn index_document(
        &self,
        Parameters(params): Parameters<crate::types::IndexDocumentParams>,
    ) -> Result<CallToolResult, McpError> {
        // For now, pass the document as-is through ToolCallArgs
        // TODO: Define proper document type and pass directly to tool
        let properties = serde_json::from_value(params.document)
            .map_err(|e| McpError::invalid_params(format!("Invalid document format: {}", e), None))?;
        let args = crate::types::ToolCallArgs {
            properties: Some(properties),
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };
        let result = crate::tools::index_document(&self.database, &self.provider, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Search indexed Crucible documents
    #[tool(description = "[READ] Search indexed documents")]
    async fn search_documents(
        &self,
        Parameters(params): Parameters<crate::types::SearchDocumentsParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: Some(params.query),
            top_k: Some(params.top_k),
            force: None,
        };
        let result = crate::tools::search_documents(&self.database, &self.provider, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Get statistics about indexed documents
    #[tool(description = "[READ] Get indexing statistics")]
    async fn get_document_stats(
        &self,
        Parameters(_params): Parameters<crate::types::GetDocumentStatsParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };
        let result = crate::tools::get_document_stats(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Update properties of a Crucible document
    #[tool(description = "[WRITE] Update document properties")]
    async fn update_document_properties(
        &self,
        Parameters(params): Parameters<crate::types::UpdateDocumentPropertiesParams>,
    ) -> Result<CallToolResult, McpError> {
        // Pass document_id via path and properties via properties
        let args = crate::types::ToolCallArgs {
            properties: Some(params.properties),
            tags: None,
            path: Some(params.document_id),
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };
        let result = crate::tools::update_document_properties(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Convert ToolCallResult to rmcp's CallToolResult
    ///
    /// CRITICAL: This method handles errors by returning successful tool results
    /// with isError=true. This is required for Claude Desktop compatibility.
    ///
    /// rmcp errors (Err returns) should only be used for protocol-level failures,
    /// not tool execution failures. Tool failures are returned as successful
    /// tool responses with error information in the content.
    fn convert_result(&self, result: crate::types::ToolCallResult) -> Result<CallToolResult, McpError> {
        if result.success {
            // Success case: return data as formatted JSON
            let content = if let Some(data) = result.data {
                serde_json::to_string_pretty(&data)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?
            } else {
                "Success".to_string()
            };
            Ok(CallToolResult::success(vec![Content::text(content)]))
        } else {
            // Error case: return as tool error (not protocol error)
            // This is critical for Claude Desktop - errors must be wrapped as tool results
            let error_message = result.error.unwrap_or_else(|| "Unknown error".to_string());

            // Include any partial data in the error response
            let error_content = if let Some(data) = result.data {
                format!("Error: {}\n\nPartial data:\n{}",
                    error_message,
                    serde_json::to_string_pretty(&data).unwrap_or_default())
            } else {
                error_message
            };

            Ok(CallToolResult::error(vec![Content::text(error_content)]))
        }
    }
}

// Implement ServerHandler to enable Service<RoleServer> trait for CrucibleMcpService
#[tool_handler]
impl ServerHandler for CrucibleMcpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "crucible-mcp".to_string(),
                version: "0.1.0".to_string(),
                title: Some("Crucible MCP Server".to_string()),
                icons: None,
                website_url: None,
            },
            instructions: Some("Crucible MCP server providing semantic search and document indexing for Obsidian vaults".to_string()),
        }
    }
}

// The #[tool_router] macro generates the Service<RoleServer> implementation automatically

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use async_trait::async_trait;
    use crate::embeddings::{EmbeddingResponse, EmbeddingResult};

    // Mock embedding provider for testing
    struct TestEmbeddingProvider;

    #[async_trait]
    impl EmbeddingProvider for TestEmbeddingProvider {
        async fn embed(&self, _text: &str) -> EmbeddingResult<EmbeddingResponse> {
            Ok(EmbeddingResponse::new(vec![0.1; 384], "test-model".to_string()))
        }

        async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
            Ok(texts.iter().map(|_| EmbeddingResponse::new(vec![0.1; 384], "test-model".to_string())).collect())
        }

        fn model_name(&self) -> &str {
            "test-model"
        }

        fn dimensions(&self) -> usize {
            384
        }

        fn provider_name(&self) -> &str {
            "TestProvider"
        }

        async fn health_check(&self) -> EmbeddingResult<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn test_service_creation() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap();

        let provider = Arc::new(TestEmbeddingProvider);

        let _service = CrucibleMcpService::new(db, provider);
        // If we get here, service was created successfully
    }
}
