//! Mock embedding provider for testing

use crate::embeddings::{EmbeddingProvider, EmbeddingResponse, EmbeddingResult};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// Mock embedding provider for testing
///
/// Returns deterministic embeddings based on text hash, useful for unit tests
/// without requiring external services.
pub struct MockEmbeddingProvider {
    dimensions: usize,
    model_name: String,
    cache: std::sync::Mutex<HashMap<String, Vec<f32>>>,
}

impl MockEmbeddingProvider {
    /// Create a new mock provider with default dimensions (768)
    pub fn new() -> Self {
        Self {
            dimensions: 768,
            model_name: "mock-test-model".to_string(),
            cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Create a mock provider with custom dimensions
    pub fn with_dimensions(dimensions: usize) -> Self {
        Self {
            dimensions,
            model_name: "mock-test-model".to_string(),
            cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Create a mock provider with custom model name
    pub fn with_model(model_name: String) -> Self {
        Self {
            dimensions: 768,
            model_name,
            cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Set a pre-computed embedding for specific text
    ///
    /// This allows tests to provide controlled embeddings for specific queries,
    /// ensuring deterministic similarity calculations.
    pub fn set_embedding(&self, text: &str, embedding: Vec<f32>) {
        let mut cache = self.cache.lock().unwrap();
        cache.insert(text.to_string(), embedding);
    }

    /// Generate deterministic embedding from text
    fn generate_embedding(&self, text: &str) -> Vec<f32> {
        // Check cache first
        {
            let cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.get(text) {
                return cached.clone();
            }
        }

        // Generate deterministic embedding based on text hash
        let hash = self.hash_text(text);
        let mut embedding = Vec::with_capacity(self.dimensions);

        for i in 0..self.dimensions {
            let value = ((hash as f32 + i as f32).sin() * 0.5 + 0.5) * 2.0 - 1.0;
            embedding.push(value);
        }

        // Cache the result
        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(text.to_string(), embedding.clone());
        }

        embedding
    }

    /// Simple hash function for deterministic results
    fn hash_text(&self, text: &str) -> u32 {
        text.chars()
            .fold(0u32, |acc, c| acc.wrapping_mul(31).wrapping_add(c as u32))
    }
}

impl Default for MockEmbeddingProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        let embedding = self.generate_embedding(text);

        Ok(EmbeddingResponse {
            embedding,
            model: self.model_name.clone(),
            dimensions: self.dimensions,
            tokens: Some(text.split_whitespace().count()),
            metadata: None,
        })
    }

    async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        let mut results = Vec::with_capacity(texts.len());

        for text in texts {
            let response = self.embed(&text).await?;
            results.push(response);
        }

        Ok(results)
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    async fn list_models(&self) -> EmbeddingResult<Vec<crate::embeddings::provider::ModelInfo>> {
        use crate::embeddings::provider::{ModelFamily, ModelInfo, ParameterSize};

        // Return a hardcoded list of test models
        Ok(vec![
            ModelInfo::builder()
                .name("mock-test-model")
                .display_name("Mock Test Model")
                .dimensions(768)
                .family(ModelFamily::Bert)
                .parameter_size(ParameterSize::new(137, true)) // 137M
                .recommended(true)
                .build(),
            ModelInfo::builder()
                .name("mock-small-model")
                .display_name("Mock Small Model")
                .dimensions(384)
                .family(ModelFamily::Bert)
                .parameter_size(ParameterSize::new(50, true)) // 50M
                .build(),
            ModelInfo::builder()
                .name("mock-large-model")
                .display_name("Mock Large Model")
                .dimensions(1536)
                .family(ModelFamily::Gpt)
                .parameter_size(ParameterSize::new(1, false)) // 1B
                .build(),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_provider_basic() {
        let provider = MockEmbeddingProvider::new();
        let result = provider.embed("test text").await.unwrap();

        assert_eq!(result.embedding.len(), 768);
        assert_eq!(result.model, "mock-test-model");
    }

    #[tokio::test]
    async fn test_mock_provider_custom_dimensions() {
        let provider = MockEmbeddingProvider::with_dimensions(512);
        let result = provider.embed("test text").await.unwrap();

        assert_eq!(result.embedding.len(), 512);
    }

    #[tokio::test]
    async fn test_mock_provider_deterministic() {
        let provider = MockEmbeddingProvider::new();
        let text = "deterministic test";

        let result1 = provider.embed(text).await.unwrap();
        let result2 = provider.embed(text).await.unwrap();

        assert_eq!(result1.embedding, result2.embedding);
    }

    #[tokio::test]
    async fn test_mock_provider_different_texts() {
        let provider = MockEmbeddingProvider::new();

        let result1 = provider.embed("text1").await.unwrap();
        let result2 = provider.embed("text2").await.unwrap();

        assert_ne!(result1.embedding, result2.embedding);
    }

    #[tokio::test]
    async fn test_mock_provider_batch() {
        let provider = MockEmbeddingProvider::new();
        let texts = vec![
            "text1".to_string(),
            "text2".to_string(),
            "text3".to_string(),
        ];

        let results = provider.embed_batch(texts).await.unwrap();

        assert_eq!(results.len(), 3);
        for result in results {
            assert_eq!(result.embedding.len(), 768);
        }
    }
}

/// Pre-generated deterministic fixture data for testing
///
/// This structure contains pre-computed embedding vectors that should be
/// returned by the FixtureBasedMockProvider instead of generating them algorithmically.
/// This ensures deterministic behavior and allows for precise testing.
pub struct EmbeddingFixtures {
    /// Map of text content to pre-generated embedding vectors
    pub embeddings: HashMap<String, Vec<f32>>,

    /// Model information fixtures
    pub model_info: HashMap<String, crate::embeddings::provider::ModelInfo>,

    /// Expected dimensions for different models
    pub model_dimensions: HashMap<String, usize>,

    /// Sample batch embedding fixtures
    pub batch_embeddings: HashMap<Vec<String>, Vec<EmbeddingResponse>>,
}

impl EmbeddingFixtures {
    /// Load pre-generated fixture data
    ///
    /// In the real implementation, this would load from JSON/YAML files,
    /// but for test design purposes we use hardcoded values.
    pub fn load() -> Self {
        let mut embeddings = HashMap::new();
        let mut model_info = HashMap::new();
        let mut model_dimensions = HashMap::new();
        let mut batch_embeddings = HashMap::new();

        // Pre-generated embeddings for common test texts (768 dimensions for nomic model)
        embeddings.insert("Hello, world!".to_string(), {
            let base = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
            let mut full = vec![0.0; 768];
            full[..8.min(768)].copy_from_slice(&base);
            full
        });

        embeddings.insert("This is a test document".to_string(), {
            let base = vec![0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1];
            let mut full = vec![0.0; 768];
            full[..8.min(768)].copy_from_slice(&base);
            full
        });

        embeddings.insert("Search query example".to_string(), vec![0.5; 768]);

        embeddings.insert("Empty string".to_string(), vec![0.0; 768]);

        embeddings.insert("Unicode test: 🦀 Rust is awesome!".to_string(), {
            let base = vec![0.9, 0.1, 0.8, 0.2, 0.7, 0.3, 0.6, 0.4];
            let mut full = vec![0.0; 768];
            full[..8.min(768)].copy_from_slice(&base);
            full
        });

        // nomic-embed-text-v1.5 specific fixtures (768 dimensions)
        let nomic_base_embedding: Vec<f32> = (0..768)
            .map(|i| ((i as f32 * 0.01).sin() * 0.5 + 0.5) * 2.0 - 1.0)
            .collect();
        embeddings.insert("nomic-test-text".to_string(), nomic_base_embedding.clone());

        // Generate embeddings for large batch processing test
        for i in 0..50 {
            embeddings.insert(
                format!("Test document {}", i),
                vec![0.1 + i as f32 * 0.01; 768],
            );
        }

        // Batch embedding fixtures
        let batch_texts = vec![
            "First document".to_string(),
            "Second document".to_string(),
            "Third document".to_string(),
        ];

        let batch_responses = vec![
            EmbeddingResponse::new(
                {
                    let base = vec![0.1, 0.2, 0.3, 0.4];
                    let mut full = vec![0.0; 768];
                    full[..4.min(768)].copy_from_slice(&base);
                    full
                },
                "nomic-embed-text-v1.5".to_string(),
            )
            .with_tokens(2),
            EmbeddingResponse::new(
                {
                    let base = vec![0.4, 0.3, 0.2, 0.1];
                    let mut full = vec![0.0; 768];
                    full[..4.min(768)].copy_from_slice(&base);
                    full
                },
                "nomic-embed-text-v1.5".to_string(),
            )
            .with_tokens(2),
            EmbeddingResponse::new(vec![0.5; 768], "nomic-embed-text-v1.5".to_string())
                .with_tokens(2),
        ];

        batch_embeddings.insert(batch_texts, batch_responses);

        // Model information fixtures
        model_info.insert(
            "nomic-embed-text-v1.5".to_string(),
            crate::embeddings::provider::ModelInfo::builder()
                .name("nomic-embed-text-v1.5")
                .display_name("Nomic Embed Text v1.5")
                .dimensions(768)
                .family(crate::embeddings::provider::ModelFamily::Bert)
                .parameter_size(crate::embeddings::provider::ParameterSize::new(137, true)) // 137M
                .recommended(true)
                .max_tokens(8192)
                .build(),
        );

        model_info.insert(
            "text-embedding-3-small".to_string(),
            crate::embeddings::provider::ModelInfo::builder()
                .name("text-embedding-3-small")
                .display_name("Text Embedding 3 Small")
                .dimensions(1536)
                .family(crate::embeddings::provider::ModelFamily::Gpt)
                .parameter_size(crate::embeddings::provider::ParameterSize::new(0, true)) // Unknown size
                .max_tokens(8191)
                .build(),
        );

        // Model dimensions
        model_dimensions.insert("nomic-embed-text-v1.5".to_string(), 768);
        model_dimensions.insert("text-embedding-3-small".to_string(), 1536);
        model_dimensions.insert("mock-small-model".to_string(), 384);
        model_dimensions.insert("mock-large-model".to_string(), 1536);

        Self {
            embeddings,
            model_info,
            model_dimensions,
            batch_embeddings,
        }
    }

    /// Get embedding for text or return None if not found
    pub fn get_embedding(&self, text: &str) -> Option<&Vec<f32>> {
        self.embeddings.get(text)
    }

    /// Get model info or return None if not found
    pub fn get_model_info(&self, model: &str) -> Option<&crate::embeddings::provider::ModelInfo> {
        self.model_info.get(model)
    }
}

/// FixtureBasedMockProvider that uses pre-generated fixtures
///
/// This mock provider loads fixtures from EmbeddingFixtures and returns deterministic data.
/// It provides consistent behavior across test runs and eliminates test pollution.
pub struct FixtureBasedMockProvider {
    fixtures: Arc<EmbeddingFixtures>,
    model_name: String,
    dimensions: usize,
}

impl FixtureBasedMockProvider {
    /// Create a new fixture-based mock provider
    pub fn new(model_name: String) -> Self {
        let fixtures = Arc::new(EmbeddingFixtures::load());
        let dimensions = fixtures
            .model_dimensions
            .get(&model_name)
            .copied()
            .unwrap_or(768); // Default fallback

        Self {
            fixtures,
            model_name,
            dimensions,
        }
    }

    /// Create provider with nomic-embed-text-v1.5 model
    pub fn nomic() -> Self {
        Self::new("nomic-embed-text-v1.5".to_string())
    }

    /// Create provider with text-embedding-3-small model
    pub fn openai_small() -> Self {
        Self::new("text-embedding-3-small".to_string())
    }

    /// Generate embedding for text using fixtures or fallback
    async fn generate_embedding(&self, text: &str) -> EmbeddingResult<Vec<f32>> {
        // Try to get embedding from fixtures
        if let Some(embedding) = self.fixtures.get_embedding(text) {
            // If the fixture has different dimensions, pad or truncate
            if embedding.len() == self.dimensions {
                return Ok(embedding.clone());
            } else {
                // Adjust dimensions to match expected size
                let mut adjusted = Vec::with_capacity(self.dimensions);
                for i in 0..self.dimensions {
                    if i < embedding.len() {
                        adjusted.push(embedding[i]);
                    } else {
                        adjusted.push(0.0); // Pad with zeros
                    }
                }
                return Ok(adjusted);
            }
        }

        // Fallback: generate deterministic embedding for unknown texts
        let hash = self.hash_text(text);
        let mut embedding = Vec::with_capacity(self.dimensions);

        for i in 0..self.dimensions {
            let value = ((hash as f32 + i as f32).sin() * 0.5 + 0.5) * 2.0 - 1.0;
            embedding.push(value);
        }

        Ok(embedding)
    }

    /// Simple hash function for deterministic fallback generation
    fn hash_text(&self, text: &str) -> u32 {
        text.chars()
            .fold(0u32, |acc, c| acc.wrapping_mul(31).wrapping_add(c as u32))
    }

    /// Handle empty string by mapping to "Empty string" fixture
    fn normalize_text<'a>(&self, text: &'a str) -> &'a str {
        if text.is_empty() {
            "Empty string"
        } else {
            text
        }
    }
}

impl Default for FixtureBasedMockProvider {
    fn default() -> Self {
        Self::nomic()
    }
}

#[async_trait]
impl EmbeddingProvider for FixtureBasedMockProvider {
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        let normalized_text = self.normalize_text(text);
        let embedding = self.generate_embedding(normalized_text).await?;

        Ok(EmbeddingResponse {
            embedding,
            model: self.model_name.clone(),
            dimensions: self.dimensions,
            tokens: Some(text.split_whitespace().count()),
            metadata: None,
        })
    }

    async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        // Check if we have a pre-defined batch fixture
        if let Some(responses) = self.fixtures.batch_embeddings.get(&texts) {
            // Adjust responses to match this provider's model and dimensions
            let adjusted_responses: Vec<EmbeddingResponse> = responses
                .iter()
                .map(|r| {
                    let mut embedding = r.embedding.clone();
                    if embedding.len() != self.dimensions {
                        // Adjust dimensions
                        let mut adjusted = Vec::with_capacity(self.dimensions);
                        for i in 0..self.dimensions {
                            if i < embedding.len() {
                                adjusted.push(embedding[i]);
                            } else {
                                adjusted.push(0.0); // Pad with zeros
                            }
                        }
                        embedding = adjusted;
                    }
                    EmbeddingResponse {
                        embedding,
                        model: self.model_name.clone(),
                        dimensions: self.dimensions,
                        tokens: r.tokens,
                        metadata: None,
                    }
                })
                .collect();
            return Ok(adjusted_responses);
        }

        // Generate embeddings for each text
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            let response = self.embed(&text).await?;
            results.push(response);
        }

        Ok(results)
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    async fn health_check(&self) -> EmbeddingResult<bool> {
        // Fixture-based provider is always healthy
        Ok(true)
    }

    async fn list_models(&self) -> EmbeddingResult<Vec<crate::embeddings::provider::ModelInfo>> {
        let mut models = Vec::new();

        // Add models from fixtures
        for (_, model_info) in &self.fixtures.model_info {
            models.push(model_info.clone());
        }

        // Add some additional test models
        models.push(
            crate::embeddings::provider::ModelInfo::builder()
                .name("mock-small-model")
                .display_name("Mock Small Model")
                .dimensions(384)
                .family(crate::embeddings::provider::ModelFamily::Bert)
                .parameter_size(crate::embeddings::provider::ParameterSize::new(50, true)) // 50M
                .build(),
        );

        models.push(
            crate::embeddings::provider::ModelInfo::builder()
                .name("mock-large-model")
                .display_name("Mock Large Model")
                .dimensions(1536)
                .family(crate::embeddings::provider::ModelFamily::Gpt)
                .parameter_size(crate::embeddings::provider::ParameterSize::new(1, false)) // 1B
                .build(),
        );

        Ok(models)
    }
}
