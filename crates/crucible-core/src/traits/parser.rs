//! Parser abstraction trait
//!
//! This module re-exports the MarkdownParser trait and types from crucible-parser.
//! Keeping it here maintains consistency with the traits module structure.

pub use crucible_parser::error::{ParserError, ParserResult};
pub use crucible_parser::traits::{MarkdownParser, ParserCapabilities};
pub use crucible_parser::types::ParsedNote;
pub use crate::parser::traits::ParserRequirements;
