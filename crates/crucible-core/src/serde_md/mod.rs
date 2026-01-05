//! Serde Serializer for Markdown output
//!
//! This module provides a custom serde Serializer that outputs Markdown
//! instead of JSON. Types that derive `Serialize` can be rendered to
//! Markdown using the same API as `serde_json`.
//!
//! # Example
//!
//! ```ignore
//! use crucible_core::serde_md;
//!
//! #[derive(serde::Serialize)]
//! struct Message { role: String, content: String }
//!
//! let msg = Message { role: "user".into(), content: "Hello!".into() };
//! let md = serde_md::to_string(&msg).unwrap();
//! ```
//!
//! # Extensibility
//!
//! Types can implement `MarkdownRenderable` for custom rendering based on
//! struct/variant name. The Serializer checks for registered renderers
//! before falling back to generic key-value formatting.

mod error;
mod serializer;

pub use error::{Error, Result};
pub use serializer::{to_string, to_string_pretty, Serializer};

/// Trait for types that provide custom markdown rendering
///
/// Implement this to override the default struct → key-value rendering.
/// The renderer receives collected fields and produces markdown output.
pub trait MarkdownRenderer: Send + Sync {
    /// The type/variant name this renderer handles
    fn handles(&self) -> &'static str;

    /// Render fields to markdown
    fn render(&self, fields: &std::collections::BTreeMap<&str, String>) -> Result<String>;
}

/// Registry of custom renderers for specific types
#[derive(Default)]
pub struct RendererRegistry {
    renderers: Vec<Box<dyn MarkdownRenderer>>,
}

impl RendererRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, renderer: Box<dyn MarkdownRenderer>) {
        self.renderers.push(renderer);
    }

    pub fn find(&self, type_name: &str) -> Option<&dyn MarkdownRenderer> {
        self.renderers
            .iter()
            .find(|r| r.handles() == type_name)
            .map(|r| r.as_ref())
    }
}
