//! Composable query translation pipeline for Crucible.
//!
//! This crate provides a modular query translation system that supports
//! multiple source syntaxes (SQL/PGQ, jaq, SQL sugar) and target backends
//! (SurrealDB, with DuckDB planned).
//!
//! # Architecture
//!
//! ```text
//! Source Syntax → QuerySyntaxRegistry → GraphIR → TransformChain → QueryRenderer → Output
//!      ↓              ↓                    ↓            ↓                ↓
//!   SQL/PGQ      (priority-based)      (shared)   (optional)     (capability-based)
//!   jaq-style    first match wins       types     validation      SurrealQL, DuckDB
//!   SQL sugar
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use crucible_query::{
//!     QueryPipelineBuilder,
//!     syntax::{QuerySyntaxRegistryBuilder, SqlSugarSyntax, JaqSyntax, PgqSyntax},
//!     transform::ValidateTransform,
//!     render::SurrealRenderer,
//! };
//!
//! // Build the default Crucible pipeline
//! let syntax_registry = QuerySyntaxRegistryBuilder::new()
//!     .with_syntax(PgqSyntax)       // Priority 50
//!     .with_syntax(SqlSugarSyntax)  // Priority 40
//!     .with_syntax(JaqSyntax)       // Priority 30
//!     .build();
//!
//! let pipeline = QueryPipelineBuilder::new()
//!     .syntax_registry(syntax_registry)
//!     .transform(ValidateTransform)
//!     .renderer(SurrealRenderer::default())
//!     .build();
//!
//! // Execute queries in any supported syntax
//! let result = pipeline.execute("SELECT outlinks FROM 'Index'")?;
//! let result = pipeline.execute("outlinks(\"Index\")")?;
//! let result = pipeline.execute("MATCH (a {title:'Index'})-[:wikilink]->(b)")?;
//! ```

pub mod error;
pub mod ir;
pub mod pipeline;
pub mod render;
pub mod syntax;
pub mod transform;

// Re-export main types at crate root for convenience
pub use error::{ParseError, PipelineError, RenderError, TransformError};
pub use ir::GraphIR;
pub use pipeline::{QueryPipeline, QueryPipelineBuilder};
pub use render::{QueryRenderer, RenderedQuery};
pub use syntax::{QuerySyntax, QuerySyntaxRegistry, QuerySyntaxRegistryBuilder};
pub use transform::QueryTransform;
