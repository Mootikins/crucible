//! Search and indexing tools
//!
//! This module provides advanced search capabilities including semantic search,
//! full-text search, pattern matching, and index maintenance operations.

use crate::system_tools::{schemas, Tool};
use crate::types::*;
use crucible_services::types::tool::{ToolDefinition, ToolExecutionContext, ToolExecutionResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Duration;
use tracing::info;

/// Search documents with semantic similarity
pub struct SearchDocumentsTool;

impl SearchDocumentsTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SearchDocumentsTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SearchDocumentsTool {
    fn definition(&self) -> &ToolDefinition {
        lazy_static::lazy_static! {
            static ref DEFINITION: ToolDefinition = ToolDefinition {
                name: "search_documents".to_string(),
                description: "Search documents using semantic similarity".to_string(),
                category: Some("Search".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query text"
                        },
                        "top_k": {
                            "type": "integer",
                            "description": "Number of results to return",
                            "default": 10,
                            "minimum": 1,
                            "maximum": 100
                        },
                        "filters": {
                            "type": "object",
                            "description": "Optional filters to apply",
                            "properties": {
                                "tags": {
                                    "type": "array",
                                    "items": {"type": "string"}
                                },
                                "folder": {"type": "string"},
                                "date_range": {
                                    "type": "object",
                                    "properties": {
                                        "start": {"type": "string"},
                                        "end": {"type": "string"}
                                    }
                                }
                            }
                        }
                    },
                    "required": ["query"]
                }),
                version: Some("1.0.0".to_string()),
            };
        }
        &DEFINITION
    }

    async fn execute(
        &self,
        params: Value,
        _context: &ToolExecutionContext,
    ) -> Result<ToolExecutionResult> {
        let query = match params.get("query").and_then(|q| q.as_str()) {
            Some(query) => query,
            None => {
                return Ok(ToolExecutionResult {
                    success: false,
                    result: None,
                    error: Some("Missing query".to_string()),
                    execution_time: Duration::from_millis(0),
                    tool_name: "search_documents".to_string(),
                    context: _context.clone(),
                });
            }
        };

        let top_k = params
            .get("top_k")
            .and_then(|k| k.as_u64())
            .unwrap_or(10) as u32;

        let filters = params.get("filters");

        info!("Searching documents: {} (top_k: {}, filters: {:?})", query, top_k, filters);

        // Mock implementation with realistic results
        let documents = vec![
            json!({
                "file_path": "docs/ai-research/transformers.md",
                "title": "Transformer Architecture",
                "content": "The transformer architecture revolutionized natural language processing...",
                "score": 0.95,
                "metadata": {
                    "tags": ["ai", "nlp", "transformers"],
                    "folder": "docs/ai-research",
                    "created_at": "2024-01-15T10:30:00Z"
                }
            }),
            json!({
                "file_path": "projects/ml-pipeline/notes.md",
                "title": "ML Pipeline Implementation",
                "content": "Implementation details for our machine learning pipeline using transformers...",
                "score": 0.87,
                "metadata": {
                    "tags": ["ml", "pipeline", "implementation"],
                    "folder": "projects/ml-pipeline",
                    "created_at": "2024-01-20T14:22:00Z"
                }
            }),
        ];

        Ok(ToolExecutionResult {
            success: true,
            result: Some(json!({
                "documents": documents,
                "query": query,
                "total_results": documents.len()
            })),
            error: None,
            execution_time: Duration::from_millis(150),
            tool_name: "search_documents".to_string(),
            context: _context.clone(),
        })
    }
}

/// Rebuild search indexes
pub struct RebuildIndexTool;

impl RebuildIndexTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RebuildIndexTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for RebuildIndexTool {
    fn definition(&self) -> &ToolDefinition {
        lazy_static::lazy_static! {
            static ref DEFINITION: ToolDefinition = ToolDefinition {
                name: "rebuild_index".to_string(),
                description: "Rebuild search indexes for all documents".to_string(),
                category: Some("Search".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "force": {
                            "type": "boolean",
                            "description": "Force rebuild even if index exists",
                            "default": false
                        },
                        "index_types": {
                            "type": "array",
                            "items": {
                                "type": "string",
                                "enum": ["semantic", "full_text", "metadata"]
                            },
                            "description": "Types of indexes to rebuild",
                            "default": ["semantic", "full_text", "metadata"]
                        }
                    }
                }),
                version: Some("1.0.0".to_string()),
            };
        }
        &DEFINITION
    }

    async fn execute(
        &self,
        params: Value,
        _context: &ToolExecutionContext,
    ) -> Result<ToolExecutionResult> {
        let force = params.get("force").and_then(|f| f.as_bool()).unwrap_or(false);
        let index_types = params
            .get("index_types")
            .and_then(|types| types.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec!["semantic".to_string(), "full_text".to_string(), "metadata".to_string()]);

        info!("Rebuilding indexes: {:?} (force: {})", index_types, force);

        // Mock implementation
        let documents_processed = 1250;
        let rebuilt_indexes = index_types.clone();

        Ok(ToolExecutionResult {
            success: true,
            result: Some(json!({
                "rebuilt_indexes": rebuilt_indexes,
                "documents_processed": documents_processed,
                "execution_time_ms": 5432
            })),
            error: None,
            execution_time: Duration::from_millis(5432),
            tool_name: "rebuild_index".to_string(),
            context: _context.clone(),
        })
    }
}

/// Get search index statistics
pub struct GetIndexStatsTool;

impl GetIndexStatsTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GetIndexStatsTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GetIndexStatsTool {
    fn definition(&self) -> &ToolDefinition {
        lazy_static::lazy_static! {
            static ref DEFINITION: ToolDefinition = ToolDefinition {
                name: "get_index_stats".to_string(),
                description: "Get statistics about search indexes".to_string(),
                category: Some("Search".to_string()),
                input_schema: json!({"type": "object"}),
                version: Some("1.0.0".to_string()),
            };
        }
        &DEFINITION
    }

    async fn execute(
        &self,
        _params: Value,
        _context: &ToolExecutionContext,
    ) -> Result<ToolExecutionResult> {
        info!("Getting index statistics");

        // Mock implementation with realistic index statistics
        let indexes = vec![
            json!({
                "name": "semantic_index",
                "type": "vector",
                "size_bytes": 52428800, // 50MB
                "documents": 1250,
                "last_updated": "2024-01-20T15:30:00Z",
                "status": "ready"
            }),
            json!({
                "name": "full_text_index",
                "type": "inverted",
                "size_bytes": 15728640, // 15MB
                "documents": 1250,
                "last_updated": "2024-01-20T15:30:00Z",
                "status": "ready"
            }),
            json!({
                "name": "metadata_index",
                "type": "document",
                "size_bytes": 2097152, // 2MB
                "documents": 1250,
                "last_updated": "2024-01-20T15:30:00Z",
                "status": "ready"
            }),
        ];

        let total_documents = 1250;
        let total_size_bytes: u64 = indexes
            .iter()
            .filter_map(|idx| idx["size_bytes"].as_u64())
            .sum();

        Ok(ToolExecutionResult {
            success: true,
            result: Some(json!({
                "indexes": indexes,
                "total_documents": total_documents,
                "total_size_bytes": total_size_bytes
            })),
            error: None,
            execution_time: Duration::from_millis(50),
            tool_name: "get_index_stats".to_string(),
            context: _context.clone(),
        })
    }
}

/// Optimize search indexes
pub struct OptimizeIndexTool;

impl OptimizeIndexTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for OptimizeIndexTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for OptimizeIndexTool {
    fn definition(&self) -> &ToolDefinition {
        lazy_static::lazy_static! {
            static ref DEFINITION: ToolDefinition = ToolDefinition {
                name: "optimize_index".to_string(),
                description: "Optimize search indexes for better performance".to_string(),
                category: Some("Search".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "index_names": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Specific indexes to optimize (empty for all)"
                        },
                        "rebuild_threshold": {
                            "type": "number",
                            "description": "Fragmentation threshold to trigger rebuild",
                            "default": 0.3,
                            "minimum": 0.0,
                            "maximum": 1.0
                        }
                    }
                }),
                version: Some("1.0.0".to_string()),
            };
        }
        &DEFINITION
    }

    async fn execute(
        &self,
        params: Value,
        _context: &ToolExecutionContext,
    ) -> Result<ToolExecutionResult> {
        let index_names = params
            .get("index_names")
            .and_then(|names| names.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            });
        let rebuild_threshold = params
            .get("rebuild_threshold")
            .and_then(|t| t.as_f64())
            .unwrap_or(0.3);

        info!("Optimizing indexes: {:?} (threshold: {})", index_names, rebuild_threshold);

        // Mock implementation
        let optimized_indexes = vec!["semantic_index".to_string(), "full_text_index".to_string()];
        let rebuilt_indexes = vec!["metadata_index".to_string()];
        let space_saved_bytes = 1048576; // 1MB
        let performance_improvement = "15% faster search".to_string();

        Ok(ToolExecutionResult {
            success: true,
            result: Some(json!({
                "optimized_indexes": optimized_indexes,
                "rebuilt_indexes": rebuilt_indexes,
                "space_saved_bytes": space_saved_bytes,
                "performance_improvement": performance_improvement
            })),
            error: None,
            execution_time: Duration::from_millis(2500),
            tool_name: "optimize_index".to_string(),
            context: _context.clone(),
        })
    }
}

/// Advanced search with multiple criteria
pub struct AdvancedSearchTool;

impl AdvancedSearchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for AdvancedSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for AdvancedSearchTool {
    fn definition(&self) -> &ToolDefinition {
        lazy_static::lazy_static! {
            static ref DEFINITION: ToolDefinition = ToolDefinition {
                name: "advanced_search".to_string(),
                description: "Advanced search with multiple criteria and ranking".to_string(),
                category: Some("Search".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "object",
                            "properties": {
                                "text": {"type": "string"},
                                "semantic": {"type": "boolean", "default": true},
                                "tags": {"type": "array", "items": {"type": "string"}},
                                "properties": {"type": "object"},
                                "folder": {"type": "string"},
                                "date_range": {
                                    "type": "object",
                                    "properties": {
                                        "start": {"type": "string"},
                                        "end": {"type": "string"}
                                    }
                                }
                            }
                        },
                        "ranking": {
                            "type": "object",
                            "properties": {
                                "method": {
                                    "type": "string",
                                    "enum": ["relevance", "date", "custom"],
                                    "default": "relevance"
                                },
                                "weights": {
                                    "type": "object",
                                    "properties": {
                                        "semantic": {"type": "number", "default": 0.5},
                                        "text_match": {"type": "number", "default": 0.3},
                                        "recency": {"type": "number", "default": 0.2}
                                    }
                                }
                            }
                        },
                        "limit": {
                            "type": "integer",
                            "default": 20,
                            "minimum": 1,
                            "maximum": 100
                        }
                    },
                    "required": ["query"]
                }),
                version: Some("1.0.0".to_string()),
            };
        }
        &DEFINITION
    }

    async fn execute(
        &self,
        params: Value,
        _context: &ToolExecutionContext,
    ) -> Result<ToolExecutionResult> {
        let query = params.get("query").ok_or_else(|| {
            anyhow::anyhow!("Missing query parameter")
        })?;

        let ranking = params.get("ranking");
        let limit = params
            .get("limit")
            .and_then(|l| l.as_u64())
            .unwrap_or(20) as u32;

        info!("Advanced search: {:?} (ranking: {:?}, limit: {})", query, ranking, limit);

        // Mock implementation with sophisticated results
        let results = vec![
            json!({
                "file_path": "research/ai/transformers.md",
                "title": "Transformer Architecture Deep Dive",
                "content_snippet": "The transformer architecture, introduced in 'Attention Is All You Need', revolutionized NLP...",
                "score": 0.94,
                "match_details": {
                    "semantic_score": 0.92,
                    "text_matches": ["transformer", "architecture", "attention"],
                    "recency_boost": 0.02
                },
                "metadata": {
                    "tags": ["ai", "nlp", "transformers"],
                    "word_count": 3240,
                    "created_at": "2024-01-15T10:30:00Z"
                }
            }),
            json!({
                "file_path": "projects/ml/bert-implementation.md",
                "title": "BERT Implementation Notes",
                "content_snippet": "Implementation details for BERT model fine-tuning on our custom dataset...",
                "score": 0.87,
                "match_details": {
                    "semantic_score": 0.85,
                    "text_matches": ["bert", "implementation", "fine-tuning"],
                    "recency_boost": 0.02
                },
                "metadata": {
                    "tags": ["ml", "bert", "implementation"],
                    "word_count": 1876,
                    "created_at": "2024-01-18T16:45:00Z"
                }
            }),
        ];

        Ok(ToolExecutionResult {
            success: true,
            result: Some(json!({
                "results": results,
                "total_found": results.len(),
                "search_time_ms": 156
            })),
            error: None,
            execution_time: Duration::from_millis(156),
            tool_name: "advanced_search".to_string(),
            context: _context.clone(),
        })
    }
}

/// Create a search tool by name
pub fn create_tool(name: &str) -> Box<dyn Tool> {
    match name {
        "search_documents" => Box::new(SearchDocumentsTool::new()),
        "rebuild_index" => Box::new(RebuildIndexTool::new()),
        "get_index_stats" => Box::new(GetIndexStatsTool::new()),
        "optimize_index" => Box::new(OptimizeIndexTool::new()),
        "advanced_search" => Box::new(AdvancedSearchTool::new()),
        _ => panic!("Unknown search tool: {}", name),
    }
}

/// Register all search tools with the tool manager
pub fn register_search_tools(manager: &mut crate::system_tools::ToolManager) {
    manager.register_tool(SearchDocumentsTool::new());
    manager.register_tool(RebuildIndexTool::new());
    manager.register_tool(GetIndexStatsTool::new());
    manager.register_tool(OptimizeIndexTool::new());
    manager.register_tool(AdvancedSearchTool::new());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system_tools::ToolManager;

    #[tokio::test]
    async fn test_search_documents_tool() {
        let tool = SearchDocumentsTool::new();
        let context = ToolExecutionContext::default();

        let params = json!({
            "query": "machine learning transformers",
            "top_k": 5,
            "filters": {
                "tags": ["ai", "research"],
                "folder": "docs"
            }
        });

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
        assert!(result.result.is_some());
    }

    #[tokio::test]
    async fn test_advanced_search_tool() {
        let tool = AdvancedSearchTool::new();
        let context = ToolExecutionContext::default();

        let params = json!({
            "query": {
                "text": "transformer attention",
                "semantic": true,
                "tags": ["ai"],
                "date_range": {
                    "start": "2024-01-01",
                    "end": "2024-01-31"
                }
            },
            "ranking": {
                "method": "relevance",
                "weights": {
                    "semantic": 0.6,
                    "text_match": 0.3,
                    "recency": 0.1
                }
            },
            "limit": 10
        });

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_register_search_tools() {
        let mut manager = ToolManager::new();
        register_search_tools(&manager);

        let search_tools = manager.list_tools_by_category(&ToolCategory::Search);
        assert!(!search_tools.is_empty());
        assert!(search_tools.iter().any(|t| t.name == "search_documents"));
        assert!(search_tools.iter().any(|t| t.name == "advanced_search"));
        assert!(search_tools.iter().any(|t| t.name == "rebuild_index"));
    }
}