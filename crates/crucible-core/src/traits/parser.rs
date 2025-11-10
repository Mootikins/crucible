//! Parser abstraction trait
//!
//! This module re-exports the MarkdownParser trait from the parser module.
//! Keeping it here maintains consistency with the traits module structure.

pub use crate::parser::error::{ParserError, ParserResult};
pub use crate::parser::traits::{MarkdownParser, ParserCapabilities, ParserRequirements};
pub use crucible_parser::types::ParsedDocument;
