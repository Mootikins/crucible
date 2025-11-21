//! Parser trait definitions
//!
//! This module re-exports parser traits from crucible-core (canonical source).
//! See crucible-core::parser::traits for the actual definitions.

// Re-export all trait types from canonical source
pub use crucible_core::parser::traits::{
    MarkdownParser, ParserCapabilities, ParserCapabilitiesExt, ParserRequirements,
};
