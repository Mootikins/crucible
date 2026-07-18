//! markdown-it-rust AST conversion + custom syntax plugins.
//!
//! Provides the `AstConverter` and wikilink/tag/callout/latex plugins that back
//! [`super::basic_markdown_it`]. Enable with the `markdown-it-parser` feature flag.

#[cfg(feature = "markdown-it-parser")]
pub mod converter;
#[cfg(feature = "markdown-it-parser")]
pub mod plugins;
