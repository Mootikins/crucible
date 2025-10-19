//! Markdown parsing infrastructure for Crucible
//!
//! This module provides the core parsing traits and types for extracting structured
//! data from markdown files in the vault.

pub mod types;
pub mod traits;
pub mod error;
pub mod adapter;
pub mod pulldown;

pub use types::{
    ParsedDocument, Frontmatter, FrontmatterFormat, Wikilink, Tag,
    DocumentContent, Heading, CodeBlock,
};
pub use traits::{MarkdownParser, ParserCapabilities};
pub use error::{ParserError, ParserResult};
pub use adapter::SurrealDBAdapter;
pub use pulldown::PulldownParser;
