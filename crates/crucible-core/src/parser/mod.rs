//! Markdown parsing infrastructure for Crucible
//!
//! This module provides the core parsing traits and types for extracting structured
//! data from markdown files in the vault.

pub mod adapter;
pub mod error;
pub mod pulldown;
pub mod traits;
pub mod types;

pub use adapter::SurrealDBAdapter;
pub use error::{ParserError, ParserResult};
pub use pulldown::PulldownParser;
pub use traits::{MarkdownParser, ParserCapabilities};
pub use types::{
    CodeBlock, DocumentContent, Frontmatter, FrontmatterFormat, Heading, ListBlock, ListItem,
    ListType, Paragraph, ParsedDocument, Tag, TaskStatus, Wikilink,
};
