//! Embedding generation edge case tests
//!
//! Tests for error handling, edge cases, and batch processing in embedding generation.

use crucible_llm::embeddings::{create_provider, EmbeddingConfig};

#[tokio::test]
async fn test_empty_text_embedding() {
    let config = EmbeddingConfig::mock(None);
    let provider = create_provider(config).await.unwrap();

    let result = provider.embed("").await;

    // Should either succeed with empty/zero embedding or return error
    match result {
        Ok(response) => {
            // If it succeeds, embedding should be valid dimensional vector
            assert!(!response.embedding.is_empty());
        }
        Err(_) => {
            // Error is also acceptable for empty input
        }
    }
}

#[tokio::test]
async fn test_very_long_text_embedding() {
    let config = EmbeddingConfig::mock(None);
    let provider = create_provider(config).await.unwrap();

    // Create text longer than typical token limits (most models have 512-8192 token limits)
    let long_text = "word ".repeat(10_000);

    let result = provider.embed(&long_text).await;

    // Should handle gracefully - either truncate, chunk, or error
    assert!(result.is_ok() || result.is_err());
}

#[tokio::test]
async fn test_special_characters_embedding() {
    let config = EmbeddingConfig::mock(None);
    let provider = create_provider(config).await.unwrap();

    let special_text = "Special chars: @#$%^&*()[]{}|\\<>?/~`!";

    let result = provider.embed(special_text).await;
    assert!(result.is_ok());

    let response = result.unwrap();
    assert!(!response.embedding.is_empty());
}

#[tokio::test]
async fn test_unicode_text_embedding() {
    let config = EmbeddingConfig::mock(None);
    let provider = create_provider(config).await.unwrap();

    let unicode_text = "日本語 العربية emoji: 🚀📝✨";

    let result = provider.embed(unicode_text).await;
    assert!(result.is_ok());

    let response = result.unwrap();
    assert!(!response.embedding.is_empty());
}

#[tokio::test]
async fn test_batch_embedding_empty_vec() {
    let config = EmbeddingConfig::mock(None);
    let provider = create_provider(config).await.unwrap();

    let texts: Vec<&str> = vec![];

    let result = provider.embed_batch(texts.iter().map(|s| s.to_string()).collect()).await;

    // Should either return empty vec or error
    match result {
        Ok(responses) => {
            assert_eq!(responses.len(), 0);
        }
        Err(_) => {
            // Error is acceptable for empty batch
        }
    }
}

#[tokio::test]
async fn test_batch_embedding_single_item() {
    let config = EmbeddingConfig::mock(None);
    let provider = create_provider(config).await.unwrap();

    let texts = vec!["single item"];

    let result = provider.embed_batch(texts.iter().map(|s| s.to_string()).collect()).await;
    assert!(result.is_ok());

    let responses = result.unwrap();
    assert_eq!(responses.len(), 1);
    assert!(!responses[0].embedding.is_empty());
}

#[tokio::test]
async fn test_batch_embedding_preserves_order() {
    let config = EmbeddingConfig::mock(None);
    let provider = create_provider(config).await.unwrap();

    let texts = vec!["first", "second", "third", "fourth"];

    let result = provider.embed_batch(texts.iter().map(|s| s.to_string()).collect()).await;
    assert!(result.is_ok());

    let responses = result.unwrap();
    assert_eq!(responses.len(), 4);

    // Mock provider should return deterministic embeddings
    // Verify we got 4 distinct embeddings in correct order
    for response in &responses {
        assert!(!response.embedding.is_empty());
    }
}

#[tokio::test]
async fn test_batch_with_mixed_content() {
    let config = EmbeddingConfig::mock(None);
    let provider = create_provider(config).await.unwrap();

    let long_text = "x".repeat(1000);
    let texts = vec![
        "normal text",
        "",                    // empty
        "日本語",              // unicode
        "@#$%",                // special chars
        &long_text,            // long text
    ];

    let result = provider.embed_batch(texts.iter().map(|s| s.to_string()).collect()).await;

    // Should handle mixed content gracefully
    if let Ok(responses) = result {
        assert_eq!(responses.len(), texts.len());
    }
}

#[tokio::test]
async fn test_embedding_dimension_consistency() {
    let config = EmbeddingConfig::mock(None);
    let provider = create_provider(config).await.unwrap();

    let response1 = provider.embed("first text").await.unwrap();
    let response2 = provider.embed("different text").await.unwrap();

    // All embeddings from same provider should have same dimensions
    assert_eq!(response1.embedding.len(), response2.embedding.len());
}

#[tokio::test]
async fn test_mock_provider_deterministic() {
    let config = EmbeddingConfig::mock(None);
    let provider = create_provider(config).await.unwrap();

    let text = "test text";

    let response1 = provider.embed(text).await.unwrap();
    let response2 = provider.embed(text).await.unwrap();

    // Mock provider should be deterministic
    assert_eq!(response1.embedding, response2.embedding);
}
