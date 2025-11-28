//! Parser abstraction trait
//!
//! This module re-exports the MarkdownParser trait and types from crucible-parser.
//! Keeping it here maintains consistency with the traits module structure.

pub use crate::parser::error::{ParserError, ParserResult};
pub use crate::parser::traits::ParserRequirements;
pub use crate::parser::traits::{MarkdownParser, ParserCapabilities};
pub use crate::parser::types::ParsedNote;
