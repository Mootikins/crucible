//! Parser error types
//!
//! This module re-exports error types from crucible-core (canonical source).
//! See crucible-core::parser::error for the actual definitions.

// Re-export all error types from canonical source
pub use crucible_core::parser::error::{
    ErrorSeverity, ParseError, ParseErrorType, ParserError, ParserResult,
};
