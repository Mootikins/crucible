//! Pipeline Orchestration Layer
//!
//! This crate provides the main orchestrator for Crucible's note processing pipeline.
//!
//! ## Architecture
//!
//! The pipeline coordinates five phases:
//! 1. **Quick Filter**: Check file state (date modified + BLAKE3 hash) to skip unchanged files
//! 2. **Parse**: Transform markdown to AST using crucible-parser
//! 3. **Merkle Diff**: Build Merkle tree and compare with stored version to identify changed blocks
//! 4. **Enrich**: Generate embeddings and metadata for changed blocks using crucible-enrichment
//! 5. **Store**: Persist all changes using storage layer
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
//! - Coordinates all five phases in the right order
//! - Manages dependencies between phases
//! - Handles error recovery and rollback
//! - Provides single interface for UI layers (CLI, Desktop, MCP, etc.)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_pipeline::NotePipeline;
//!
//! // Create pipeline with dependencies
//! let pipeline = NotePipeline::new(
//!     change_detector,
//!     merkle_store,
//!     enrichment_service,
//!     storage,
//! );
//!
//! // Process a note
//! let result = pipeline.process(&path).await?;
//! ```

pub mod note_pipeline;

pub use note_pipeline::*;
