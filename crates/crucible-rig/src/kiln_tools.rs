//! Rig-compatible kiln tools
//!
//! This module provides Rig `Tool` trait implementations for kiln operations.
//! These enable internal agents to access semantic search, notes, and other
//! knowledge base features.
//!
//! ## Available Tools
//!
//! - `SemanticSearchTool` - Search notes using embedding similarity
//! - `ReadNoteTool` - Read note content
//! - `ListNotesTool` - List notes in a directory
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rig::kiln_tools::KilnContext;
//!
//! let ctx = KilnContext::new(kiln_path, knowledge_repo, embedding_provider);
//! let tools = ctx.all_tools();
//! // Add tools to agent builder
//! ```

use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

/// Error type for kiln tool operations
#[derive(Debug, Error)]
pub enum KilnToolError {
    /// Search operation failed
    #[error("Search error: {0}")]
    Search(String),

    /// Note operation failed
    #[error("Note error: {0}")]
    Note(String),

    /// Embedding generation failed
    #[error("Embedding error: {0}")]
    Embedding(String),
}

/// Shared kiln context for tools
#[derive(Clone)]
pub struct KilnContext {
    kiln_path: PathBuf,
    knowledge_repo: Arc<dyn KnowledgeRepository>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
}

impl KilnContext {
    /// Create a new kiln context
    pub fn new(
        kiln_path: impl Into<PathBuf>,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        Self {
            kiln_path: kiln_path.into(),
            knowledge_repo,
            embedding_provider,
        }
    }

    /// Get kiln path
    pub fn kiln_path(&self) -> &PathBuf {
        &self.kiln_path
    }

    /// Get all kiln tools as a vector for agent building
    pub fn all_tools(&self) -> Vec<Box<dyn rig::tool::ToolDyn>> {
        vec![
            Box::new(SemanticSearchTool::new(self.clone())),
            Box::new(ReadNoteTool::new(self.clone())),
            Box::new(ListNotesTool::new(self.clone())),
        ]
    }
}

// =============================================================================
// SemanticSearchTool
// =============================================================================

/// Arguments for semantic search
#[derive(Debug, Deserialize)]
pub struct SemanticSearchArgs {
    /// The search query
    query: String,
    /// Maximum number of results (default: 10)
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    10
}

/// Tool output for semantic search
#[derive(Debug, Serialize)]
pub struct SemanticSearchOutput {
    results: Vec<SearchResultItem>,
    query: String,
    count: usize,
}

#[derive(Debug, Serialize)]
struct SearchResultItem {
    document_id: String,
    score: f64,
    snippet: Option<String>,
}

/// Semantic search tool using embeddings
pub struct SemanticSearchTool {
    ctx: KilnContext,
}

impl SemanticSearchTool {
    /// Create a new semantic search tool
    pub fn new(ctx: KilnContext) -> Self {
        Self { ctx }
    }
}

impl Tool for SemanticSearchTool {
    const NAME: &'static str = "semantic_search";

    type Args = SemanticSearchArgs;
    type Output = SemanticSearchOutput;
    type Error = KilnToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Search notes using semantic similarity. Returns notes ranked by relevance to the query.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 10)"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // Generate embedding for query
        let embedding = self
            .ctx
            .embedding_provider
            .embed(&args.query)
            .await
            .map_err(|e| KilnToolError::Embedding(e.to_string()))?;

        // Search using knowledge repository
        let results = self
            .ctx
            .knowledge_repo
            .search_vectors(embedding)
            .await
            .map_err(|e| KilnToolError::Search(e.to_string()))?;

        // Convert and limit results
        let search_results: Vec<SearchResultItem> = results
            .into_iter()
            .take(args.limit)
            .map(|r| SearchResultItem {
                document_id: r.document_id.to_string(),
                score: r.score,
                snippet: r.snippet,
            })
            .collect();

        let count = search_results.len();

        Ok(SemanticSearchOutput {
            results: search_results,
            query: args.query,
            count,
        })
    }
}

// =============================================================================
// ReadNoteTool
// =============================================================================

/// Arguments for reading a note
#[derive(Debug, Deserialize)]
pub struct ReadNoteArgs {
    /// Note name or path (relative to kiln root)
    name: String,
}

/// Tool output for reading a note
#[derive(Debug, Serialize)]
pub struct ReadNoteOutput {
    name: String,
    content: String,
    path: String,
}

/// Read note content from the kiln
pub struct ReadNoteTool {
    ctx: KilnContext,
}

impl ReadNoteTool {
    /// Create a new read note tool
    pub fn new(ctx: KilnContext) -> Self {
        Self { ctx }
    }
}

impl Tool for ReadNoteTool {
    const NAME: &'static str = "read_note";

    type Args = ReadNoteArgs;
    type Output = ReadNoteOutput;
    type Error = KilnToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Read the content of a note from the knowledge base.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Note name or path (relative to kiln root, without .md extension)"
                    }
                },
                "required": ["name"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // Get note from repository
        let note = self
            .ctx
            .knowledge_repo
            .get_note_by_name(&args.name)
            .await
            .map_err(|e| KilnToolError::Note(e.to_string()))?
            .ok_or_else(|| KilnToolError::Note(format!("Note not found: {}", args.name)))?;

        // Extract name from path
        let name = note
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Use plain_text from content (parsed/stripped markdown)
        let content = note.content.plain_text.clone();

        Ok(ReadNoteOutput {
            name,
            content,
            path: note.path.to_string_lossy().to_string(),
        })
    }
}

// =============================================================================
// ListNotesTool
// =============================================================================

/// Arguments for listing notes
#[derive(Debug, Deserialize)]
pub struct ListNotesArgs {
    /// Optional folder path (relative to kiln root)
    #[serde(default)]
    path: Option<String>,
}

/// Tool output for listing notes
#[derive(Debug, Serialize)]
pub struct ListNotesOutput {
    notes: Vec<NoteInfo>,
    path: Option<String>,
    count: usize,
}

#[derive(Debug, Serialize)]
struct NoteInfo {
    name: String,
    path: String,
}

/// List notes in a directory
pub struct ListNotesTool {
    ctx: KilnContext,
}

impl ListNotesTool {
    /// Create a new list notes tool
    pub fn new(ctx: KilnContext) -> Self {
        Self { ctx }
    }
}

impl Tool for ListNotesTool {
    const NAME: &'static str = "list_notes";

    type Args = ListNotesArgs;
    type Output = ListNotesOutput;
    type Error = KilnToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "List notes in the knowledge base, optionally within a specific folder."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Optional folder path (relative to kiln root)"
                    }
                }
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let notes = self
            .ctx
            .knowledge_repo
            .list_notes(args.path.as_deref())
            .await
            .map_err(|e| KilnToolError::Note(e.to_string()))?;

        let note_infos: Vec<NoteInfo> = notes
            .into_iter()
            .map(|n| NoteInfo {
                name: n.name,
                path: n.path,
            })
            .collect();

        let count = note_infos.len();

        Ok(ListNotesOutput {
            notes: note_infos,
            path: args.path,
            count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_limit() {
        assert_eq!(default_limit(), 10);
    }

    #[test]
    fn test_tool_names() {
        assert_eq!(SemanticSearchTool::NAME, "semantic_search");
        assert_eq!(ReadNoteTool::NAME, "read_note");
        assert_eq!(ListNotesTool::NAME, "list_notes");
    }
}
