//! Pipeline Orchestration Layer
//!
//! This crate provides the main orchestrator for Crucible's note processing pipeline.
//!
//! ## Architecture
//!
//! The pipeline coordinates four phases:
//! 1. **Quick Filter**: Check file state (date modified + BLAKE3 hash) to skip unchanged files
//! 2. **Parse**: Transform markdown to AST using crucible-parser
//! 3. **Enrich**: Generate embeddings and metadata using crucible-enrichment
//! 4. **Store**: Persist all changes using storage layer
//!
//! ## Clear Separation of Concerns
//!
//! Infrastructure crates (DO NOT orchestrate):
//! - `crucible-parser`: Just parses markdown to AST
//! - `crucible-enrichment`: Just provides enrichment services
//! - `crucible-llm`: Just provides embedding generation
//! - `crucible-surrealdb`: Just provides storage operations
//!
//! This crate (crucible-pipeline):
//! - Coordinates all four phases in the right order
//! - Manages dependencies between phases
//! - Handles error recovery and rollback
//! - Provides single interface for UI layers (CLI, Desktop, MCP, etc.)

pub mod note_pipeline;

// Re-export pipeline types
pub use note_pipeline::{NotePipeline, NotePipelineConfig, ParserBackend};

// Re-export core types for convenience (defined in crucible-core)
pub use crucible_core::processing::{NotePipelineOrchestrator, PipelineMetrics, ProcessingResult};
