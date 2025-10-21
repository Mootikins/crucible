//! Embedding provider integration for crucible-tools
//!
//! This module provides integration with crucible-llm embedding providers.
//! It serves as a bridge between crucible-tools and the crucible-llm crate,
//! handling configuration conversion and provider creation.

pub use crucible_llm::{
    EmbeddingConfig, EmbeddingError, EmbeddingProvider, EmbeddingResponse, EmbeddingResult,
    OllamaProvider, OpenAIProvider,
};
pub use crucible_llm::embeddings::ProviderType;

use std::sync::Arc;

/// Create an embedding provider from configuration
pub async fn create_provider(config: EmbeddingConfig) -> EmbeddingResult<Arc<dyn EmbeddingProvider>> {
    crucible_llm::embeddings::create_provider(config).await
}

/// Create a mock embedding provider for testing
#[cfg(test)]
pub fn create_mock_provider(dimensions: usize) -> Arc<dyn EmbeddingProvider> {
    crucible_llm::embeddings::create_mock_provider(dimensions)
}

/// Default embedding model configurations
pub mod default_models {
    use super::EmbeddingConfig;

    /// Default Ollama configuration
    pub fn ollama_default() -> EmbeddingConfig {
        EmbeddingConfig::ollama(
            Some("http://localhost:11434".to_string()),
            Some("nomic-embed-text".to_string()),
        )
    }

    /// Default OpenAI configuration
    pub fn openai_default() -> EmbeddingConfig {
        // This will need API key from environment
        EmbeddingConfig::from_env().unwrap_or_else(|_| {
            EmbeddingConfig::openai(
                "dummy-key".to_string(), // Should be set from environment
                Some("text-embedding-3-small".to_string()),
            )
        })
    }
}

/// Utility functions for working with embeddings
pub mod utils {
    use ndarray::Array1;

    /// Calculate cosine similarity between two embeddings
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot_product / (norm_a * norm_b)
        }
    }

    /// Calculate Euclidean distance between two embeddings
    pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return f32::INFINITY;
        }

        a.iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f32>()
            .sqrt()
    }

    /// Normalize an embedding vector
    pub fn normalize_vector(vector: &mut [f32]) {
        let norm: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm != 0.0 {
            for value in vector.iter_mut() {
                *value /= norm;
            }
        }
    }

    /// Get a normalized copy of an embedding
    pub fn normalized(vector: &[f32]) -> Vec<f32> {
        let mut result = vector.to_vec();
        normalize_vector(&mut result);
        result
    }

    /// Convert embedding to ndarray
    pub fn to_ndarray(embedding: &[f32]) -> Array1<f32> {
        Array1::from_vec(embedding.to_vec())
    }

    /// Convert ndarray to embedding
    pub fn from_ndarray(array: &Array1<f32>) -> Vec<f32> {
        array.to_vec()
    }

    /// Find most similar embeddings to a query
    pub fn find_most_similar(
        query: &[f32],
        candidates: &[Vec<f32>],
        top_k: usize,
    ) -> Vec<(usize, f32)> {
        let mut similarities: Vec<(usize, f32)> = candidates
            .iter()
            .enumerate()
            .map(|(i, candidate)| (i, cosine_similarity(query, candidate)))
            .collect();

        // Sort by similarity (descending)
        similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top_k results
        similarities.into_iter().take(top_k).collect()
    }

    /// Batch calculate similarities
    pub fn batch_cosine_similarity(query: &[f32], candidates: &[Vec<f32>]) -> Vec<f32> {
        candidates
            .iter()
            .map(|candidate| cosine_similarity(query, candidate))
            .collect()
    }

    /// Calculate average embedding from a list of embeddings
    pub fn average_embedding(embeddings: &[Vec<f32>]) -> Option<Vec<f32>> {
        if embeddings.is_empty() {
            return None;
        }

        let dimension = embeddings[0].len();
        if embeddings.iter().any(|e| e.len() != dimension) {
            return None;
        }

        let mut average = vec![0.0f32; dimension];
        for embedding in embeddings {
            for (i, &value) in embedding.iter().enumerate() {
                average[i] += value;
            }
        }

        let count = embeddings.len() as f32;
        for value in average.iter_mut() {
            *value /= count;
        }

        Some(average)
    }

    /// Weighted average of embeddings
    pub fn weighted_average_embedding(embeddings: &[(Vec<f32>, f32)]) -> Option<Vec<f32>> {
        if embeddings.is_empty() {
            return None;
        }

        let dimension = embeddings[0].0.len();
        if embeddings.iter().any(|(e, _)| e.len() != dimension) {
            return None;
        }

        let mut weighted_sum = vec![0.0f32; dimension];
        let mut total_weight = 0.0f32;

        for (embedding, weight) in embeddings {
            for (i, &value) in embedding.iter().enumerate() {
                weighted_sum[i] += value * weight;
            }
            total_weight += weight;
        }

        if total_weight == 0.0 {
            return None;
        }

        for value in weighted_sum.iter_mut() {
            *value /= total_weight;
        }

        Some(weighted_sum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let c = vec![1.0, 0.0, 0.0];

        assert_eq!(cosine_similarity(&a, &b), 0.0);
        assert_eq!(cosine_similarity(&a, &c), 1.0);

        let d = vec![1.0, 1.0, 0.0];
        let e = vec![1.0, 0.0, 1.0];
        let similarity = cosine_similarity(&d, &e);
        assert!((similarity - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_euclidean_distance() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![1.0, 1.0, 0.0];

        assert_eq!(euclidean_distance(&a, &b), 1.0);
        assert_eq!(euclidean_distance(&a, &c), (2.0_f32).sqrt());

        let d = vec![1.0, 2.0, 3.0];
        let e = vec![4.0, 6.0, 8.0];
        let distance = euclidean_distance(&d, &e);
        assert!((distance - 7.071).abs() < 0.01);
    }

    #[test]
    fn test_normalize_vector() {
        let mut vector = vec![3.0, 4.0];
        normalize_vector(&mut vector);
        assert!((vector[0] - 0.6).abs() < 0.001);
        assert!((vector[1] - 0.8).abs() < 0.001);

        let norm = (vector[0].powi(2) + vector[1].powi(2)).sqrt();
        assert!((norm - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_find_most_similar() {
        let query = vec![1.0, 0.0, 0.0];
        let candidates = vec![
            vec![1.0, 0.0, 0.0], // identical
            vec![0.0, 1.0, 0.0], // orthogonal
            vec![0.707, 0.707, 0.0], // 45 degrees
            vec![-1.0, 0.0, 0.0], // opposite
        ];

        let results = find_most_similar(&query, &candidates, 3);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, 0); // identical should be first
        assert_eq!(results[1].0, 2); // 45 degrees should be second
        assert_eq!(results[2].0, 1); // orthogonal should be third
    }

    #[test]
    fn test_average_embedding() {
        let embeddings = vec![
            vec![1.0, 0.0],
            vec![0.0, 1.0],
            vec![1.0, 1.0],
        ];

        let average = average_embedding(&embeddings).unwrap();
        assert!((average[0] - 0.6667).abs() < 0.001);
        assert!((average[1] - 0.6667).abs() < 0.001);
    }

    #[test]
    fn test_weighted_average_embedding() {
        let embeddings = vec![
            (vec![1.0, 0.0], 2.0), // weight 2
            (vec![0.0, 1.0], 1.0), // weight 1
        ];

        let weighted_avg = weighted_average_embedding(&embeddings).unwrap();
        assert!((weighted_avg[0] - 0.6667).abs() < 0.001);
        assert!((weighted_avg[1] - 0.3333).abs() < 0.001);
    }

    #[test]
    fn test_provider_creation_requires_valid_config() {
        // Test that we can create basic configs
        let config = EmbeddingConfig {
            provider: ProviderType::Ollama,
            endpoint: "https://llama.terminal.krohnos.io".to_string(),
            api_key: None,
            model: "nomic-embed-text".to_string(),
            timeout_secs: 30,
            max_retries: 3,
            batch_size: 10,
        };

        assert_eq!(config.provider, ProviderType::Ollama);
        assert_eq!(config.model, "nomic-embed-text");
    }

    #[test]
    fn test_default_model_configs() {
        let ollama_config = default_models::ollama_default();
        assert_eq!(ollama_config.provider, ProviderType::Ollama);
        assert_eq!(ollama_config.model, "nomic-embed-text");

        let openai_config = default_models::openai_default();
        assert_eq!(openai_config.provider, ProviderType::OpenAI);
        assert_eq!(openai_config.model, "text-embedding-ada-002");
    }
}