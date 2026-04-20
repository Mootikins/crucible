//! Note enrichment.
//!
//! Generates embeddings and metadata for parsed notes. The `Enricher` is the
//! single concrete type; wrap it in an `Arc` to share across handlers and the
//! pipeline.

pub mod event_handler;
pub mod service;
pub mod types;

pub use event_handler::{EmbeddingHandler, EmbeddingHandlerAdapter};
pub use service::Enricher;
