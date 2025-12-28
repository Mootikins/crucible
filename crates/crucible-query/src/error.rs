//! Error types for the query translation pipeline.

use thiserror::Error;

/// Errors from parsing query syntax
#[derive(Debug, Error)]
pub enum ParseError {
    /// No registered syntax could parse the input
    #[error("no matching syntax for query: {input}")]
    NoMatchingSyntax {
        input: String,
        tried: Vec<&'static str>,
    },

    /// SQL/PGQ parsing error with detailed location info
    #[error("PGQ parse error:\n{errors}")]
    Pgq { errors: String },

    /// jaq-style parsing error
    #[error("jaq parse error: {message}")]
    Jaq { message: String },

    /// SQL alias parsing error
    #[error("SQL parse error: {message}")]
    SqlAlias { message: String },

    /// Invalid query structure
    #[error("invalid query: {message}")]
    Invalid { message: String },
}

/// Errors from IR transformation
#[derive(Debug, Error)]
pub enum TransformError {
    /// Validation failed
    #[error("validation error: {message}")]
    Validation { message: String },

    /// Filter translation failed
    #[error("unsupported filter pattern: {pattern}")]
    UnsupportedFilter { pattern: String },
}

/// Errors from rendering to target
#[derive(Debug, Error)]
pub enum RenderError {
    /// Unsupported pattern for this renderer
    #[error("unsupported pattern for renderer")]
    UnsupportedPattern,

    /// Missing required field
    #[error("missing required field: {field}")]
    MissingField { field: String },
}

/// Pipeline-level errors
#[derive(Debug, Error)]
pub enum PipelineError {
    /// Parse phase failed
    #[error("parse error: {0}")]
    Parse(#[from] ParseError),

    /// Transform phase failed
    #[error("transform error: {0}")]
    Transform(#[from] TransformError),

    /// Render phase failed
    #[error("render error: {0}")]
    Render(#[from] RenderError),
}
