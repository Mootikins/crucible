//! markdown-it-rust AST conversion.
//!
//! Provides the `AstConverter` that backs [`super::basic_markdown_it`].
//! Enable with the `markdown-it-parser` feature flag.

#[cfg(feature = "markdown-it-parser")]
pub mod converter;
