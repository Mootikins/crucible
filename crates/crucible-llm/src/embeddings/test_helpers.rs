//! Shared test helpers for embedding provider tests

use super::config::EmbeddingConfig;

/// Create a test embedding configuration for OpenAI tests
pub(crate) fn create_test_openai_config() -> EmbeddingConfig {
    EmbeddingConfig::openai(
        "sk-test-api-key-for-testing-only".to_string(),
        Some("text-embedding-3-small".to_string()),
    )
}

/// Create a test embedding configuration for Ollama tests
pub(crate) fn create_test_ollama_config() -> EmbeddingConfig {
    EmbeddingConfig::ollama(
        Some("https://llm.example.com".to_string()),
        Some("nomic-embed-text-v1.5-q8_0".to_string()),
    )
}

/// Alias for backward compatibility (uses OpenAI config)
pub(crate) fn create_test_embedding_config() -> EmbeddingConfig {
    create_test_openai_config()
}

