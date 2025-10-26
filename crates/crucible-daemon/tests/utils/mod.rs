// crates/crucible-daemon/tests/utils/mod.rs

//! Test utilities for daemon integration tests

pub mod embedding_helpers;
pub mod semantic_assertions;

// Re-export commonly used embedding utilities
pub use embedding_helpers::{
    batch_embed, create_mock_provider, create_ollama_provider, extract_corpus_embeddings,
    get_corpus_document, load_semantic_corpus, EmbeddingStrategy, TestDocumentBuilder,
};
