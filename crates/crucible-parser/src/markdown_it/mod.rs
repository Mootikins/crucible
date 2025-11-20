//! markdown-it-rust based parser implementation
//!
//! This module provides an alternative parser implementation using markdown-it-rust,
//! which offers a more extensible plugin architecture for custom syntax.
//!
//! Enable with the `markdown-it-parser` feature flag.

#[cfg(feature = "markdown-it-parser")]
pub mod converter;
#[cfg(feature = "markdown-it-parser")]
pub mod parser;
#[cfg(feature = "markdown-it-parser")]
pub mod plugins;

#[cfg(feature = "markdown-it-parser")]
pub use parser::MarkdownItParser;
