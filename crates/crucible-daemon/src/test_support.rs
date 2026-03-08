//! Canonical test mock implementations for daemon tests
//!
//! This module provides shared mock implementations for common traits used across
//! daemon tests. These mocks are simple stubs that return default/empty values,
//! suitable for testing code that depends on these traits without needing a full
//! implementation.

use async_trait::async_trait;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::completion_backend::{
    BackendCompletionChunk, BackendCompletionRequest, BackendError, BackendResult,
    CompletionBackend,
};
use crucible_core::traits::KnowledgeRepository;
use futures::stream::BoxStream;
use std::collections::VecDeque;
use std::sync::Mutex;

/// Canonical mock implementation of KnowledgeRepository for testing
///
/// Returns empty/default values for all methods. Use this in tests that need
/// a KnowledgeRepository but don't care about the actual data.
pub struct MockKnowledgeRepository;

#[async_trait]
impl KnowledgeRepository for MockKnowledgeRepository {
    async fn get_note_by_name(
        &self,
        _name: &str,
    ) -> crucible_core::Result<Option<crucible_core::parser::ParsedNote>> {
        Ok(None)
    }

    async fn list_notes(
        &self,
        _path: Option<&str>,
    ) -> crucible_core::Result<Vec<crucible_core::traits::knowledge::NoteInfo>> {
        Ok(vec![])
    }

    async fn search_vectors(
        &self,
        _vector: Vec<f32>,
    ) -> crucible_core::Result<Vec<crucible_core::types::SearchResult>> {
        Ok(vec![])
    }
}

/// Canonical mock implementation of EmbeddingProvider for testing
///
/// Returns mock embeddings (384-dimensional vectors of 0.1) for all inputs.
/// Use this in tests that need an EmbeddingProvider but don't care about
/// actual embedding quality.
pub struct MockEmbeddingProvider;

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(vec![0.1; 384])
    }

    async fn embed_batch(&self, _texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(vec![vec![0.1; 384]; _texts.len()])
    }

    fn model_name(&self) -> &str {
        "mock-model"
    }

    fn dimensions(&self) -> usize {
        384
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec!["mock-model".to_string()])
    }
}

#[derive(Default)]
pub struct MockCompletionBackend {
    queued_responses: Mutex<VecDeque<Vec<BackendResult<BackendCompletionChunk>>>>,
    requests: Mutex<Vec<BackendCompletionRequest>>,
}

impl MockCompletionBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_response_chunks(&self, chunks: Vec<BackendResult<BackendCompletionChunk>>) {
        self.queued_responses
            .lock()
            .expect("queue lock")
            .push_back(chunks);
    }

    pub fn push_text_response(&self, text: impl Into<String>) {
        self.push_response_chunks(vec![
            Ok(BackendCompletionChunk::text(text.into())),
            Ok(BackendCompletionChunk::finished(None)),
        ]);
    }

    pub fn request_count(&self) -> usize {
        self.requests.lock().expect("requests lock").len()
    }

    pub fn requests(&self) -> Vec<BackendCompletionRequest> {
        self.requests.lock().expect("requests lock").clone()
    }
}

#[async_trait]
impl CompletionBackend for MockCompletionBackend {
    fn complete_stream(
        &self,
        request: BackendCompletionRequest,
    ) -> BoxStream<'static, BackendResult<BackendCompletionChunk>> {
        self.requests.lock().expect("requests lock").push(request);

        let chunks = self
            .queued_responses
            .lock()
            .expect("queue lock")
            .pop_front()
            .unwrap_or_else(|| {
                vec![Err(BackendError::Internal(
                    "MockCompletionBackend response queue was empty".to_string(),
                ))]
            });

        Box::pin(futures::stream::iter(chunks))
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    fn model_name(&self) -> &str {
        "mock-model"
    }

    async fn health_check(&self) -> BackendResult<bool> {
        Ok(true)
    }
}
