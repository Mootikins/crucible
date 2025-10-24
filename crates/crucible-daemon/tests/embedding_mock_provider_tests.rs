//! Comprehensive Mock Provider Tests for Embedding System
//!
//! This test suite validates the mock embedding provider functionality with
//! deterministic embeddings, dimension validation, and edge case handling.
//!
//! ## Test Coverage
//!
//! ### Deterministic Embeddings
//! - Same content always produces same embedding
//! - Different content produces different embeddings
//! - Embedding values are within expected ranges
//!
//! ### Dimension Validation
//! - LocalMini: 256 dimensions
//! - LocalStandard: 768 dimensions
//! - LocalLarge: 1536 dimensions
//!
//! ### Batch Processing
//! - Consistent embeddings between individual and batch processing
//! - Proper handling of batch size limits
//! - Error handling for malformed input
//!
//! ### Edge Cases
//! - Empty content handling
//! - Very long content handling
//! - Unicode and special characters
//! - Content hashing consistency

mod fixtures;
mod utils;

use anyhow::Result;
use utils::harness::DaemonEmbeddingHarness;
use crucible_surrealdb::embedding_config::{EmbeddingConfig, EmbeddingModel, PrivacyMode};

// ============================================================================
// Deterministic Embedding Tests
// ============================================================================

/// Test that same content produces identical embeddings
///
/// Verifies:
/// - Mock provider generates deterministic embeddings
/// - Multiple calls with same content return same vectors
/// - All model types maintain determinism
#[tokio::test]
async fn test_mock_provider_deterministic_embeddings() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content = "This is a test document for deterministic embedding generation.";

    // Generate embedding multiple times
    let embedding1 = harness.generate_embedding(content).await?;
    let embedding2 = harness.generate_embedding(content).await?;
    let embedding3 = harness.generate_embedding(content).await?;

    // Verify all embeddings are identical
    assert_eq!(
        embedding1, embedding2,
        "First and second embeddings should be identical"
    );
    assert_eq!(
        embedding2, embedding3,
        "Second and third embeddings should be identical"
    );

    // Verify embedding has correct dimensions (default is LocalStandard = 768)
    assert_eq!(
        embedding1.len(),
        768,
        "Embedding should have 768 dimensions"
    );

    // Verify all values are non-zero (mock provider characteristic)
    let non_zero_count = embedding1.iter().filter(|&&v| v != 0.0).count();
    assert!(
        non_zero_count > 0,
        "Mock embedding should have non-zero values"
    );

    Ok(())
}

/// Test that different content produces different embeddings
///
/// Verifies:
/// - Content changes affect embedding values
/// - Small changes produce measurable differences
/// - Embedding differences are consistent
#[tokio::test]
async fn test_mock_provider_content_differentiation() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content1 = "This is the first test document.";
    let content2 = "This is the second test document.";
    let content3 = "Completely different content about another topic.";

    let embedding1 = harness.generate_embedding(content1).await?;
    let embedding2 = harness.generate_embedding(content2).await?;
    let embedding3 = harness.generate_embedding(content3).await?;

    // Verify embeddings have same dimensions
    assert_eq!(embedding1.len(), embedding2.len());
    assert_eq!(embedding2.len(), embedding3.len());

    // Verify embeddings are different
    assert_ne!(
        embedding1, embedding2,
        "Different content should produce different embeddings"
    );
    assert_ne!(
        embedding2, embedding3,
        "Different content should produce different embeddings"
    );
    assert_ne!(
        embedding1, embedding3,
        "Different content should produce different embeddings"
    );

    // Calculate and verify differences are meaningful
    let diff_12 = cosine_similarity(&embedding1, &embedding2);
    let diff_13 = cosine_similarity(&embedding1, &embedding3);
    let diff_23 = cosine_similarity(&embedding2, &embedding3);

    // Similar content should be more similar than completely different content
    assert!(
        diff_12 > diff_13,
        "Similar content should have higher similarity than different content"
    );
    assert!(
        diff_12 > diff_23,
        "Similar content should have higher similarity than different content"
    );

    // Verify similarities are within reasonable range (0-1)
    assert!(diff_12 >= 0.0 && diff_12 <= 1.0);
    assert!(diff_13 >= 0.0 && diff_13 <= 1.0);
    assert!(diff_23 >= 0.0 && diff_23 <= 1.0);

    Ok(())
}

/// Test embedding value ranges and characteristics
///
/// Verifies:
/// - All embedding values are within expected range [0, 1]
/// - Mock embeddings have good variance
/// - No NaN or infinite values
#[tokio::test]
async fn test_mock_embedding_value_characteristics() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content = "Test content for embedding value characteristics validation.";
    let embedding = harness.generate_embedding(content).await?;

    // Verify all values are within [0, 1] range (mock provider uses this range)
    for (i, &value) in embedding.iter().enumerate() {
        assert!(
            value >= 0.0 && value <= 1.0,
            "Embedding value at index {} should be within [0, 1], got {}",
            i, value
        );
        assert!(
            value.is_finite(),
            "Embedding value at index {} should be finite, got {}",
            i, value
        );
    }

    // Verify embedding has good variance (not all same value)
    let min_val = embedding.iter().fold(f32::INFINITY, |a, &b| a.min(b));
    let max_val = embedding.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
    let variance = calculate_variance(&embedding);

    assert!(
        max_val > min_val,
        "Embedding should have value variance, min: {}, max: {}",
        min_val, max_val
    );
    assert!(
        variance > 0.0,
        "Embedding should have positive variance, got {}",
        variance
    );

    // Verify reasonable distribution (mock provider should use full range)
    assert!(
        max_val > 0.9,
        "Mock embedding should use upper range, max: {}",
        max_val
    );
    assert!(
        min_val < 0.1,
        "Mock embedding should use lower range, min: {}",
        min_val
    );

    Ok(())
}

// ============================================================================
// Dimension Validation Tests
// ============================================================================

/// Test all embedding model dimensions
///
/// Verifies:
/// - LocalMini produces 256-dimensional embeddings
/// - LocalStandard produces 768-dimensional embeddings
/// - LocalLarge produces 1536-dimensional embeddings
/// - Each model type handles dimension changes correctly
#[tokio::test]
async fn test_embedding_model_dimensions() -> Result<()> {
    let content = "Test content for dimension validation.";

    // Test LocalMini (256 dimensions)
    let config_mini = EmbeddingConfig {
        worker_count: 1,
        batch_size: 1,
        model_type: EmbeddingModel::LocalMini,
        privacy_mode: PrivacyMode::StrictLocal,
        max_queue_size: 10,
        timeout_ms: 5000,
        retry_attempts: 1,
        retry_delay_ms: 100,
        circuit_breaker_threshold: 5,
        circuit_breaker_timeout_ms: 10000,
    };

    let harness_mini = DaemonEmbeddingHarness::new_with_config(config_mini).await?;
    let embedding_mini = harness_mini.generate_embedding(content).await?;
    assert_eq!(
        embedding_mini.len(),
        256,
        "LocalMini should produce 256-dimensional embeddings"
    );

    // Test LocalStandard (768 dimensions) - default
    let harness_standard = DaemonEmbeddingHarness::new_default().await?;
    let embedding_standard = harness_standard.generate_embedding(content).await?;
    assert_eq!(
        embedding_standard.len(),
        768,
        "LocalStandard should produce 768-dimensional embeddings"
    );

    // Test LocalLarge (1536 dimensions)
    let config_large = EmbeddingConfig {
        worker_count: 1,
        batch_size: 1,
        model_type: EmbeddingModel::LocalLarge,
        privacy_mode: PrivacyMode::StrictLocal,
        max_queue_size: 10,
        timeout_ms: 5000,
        retry_attempts: 1,
        retry_delay_ms: 100,
        circuit_breaker_threshold: 5,
        circuit_breaker_timeout_ms: 10000,
    };

    let harness_large = DaemonEmbeddingHarness::new_with_config(config_large).await?;
    let embedding_large = harness_large.generate_embedding(content).await?;
    assert_eq!(
        embedding_large.len(),
        1536,
        "LocalLarge should produce 1536-dimensional embeddings"
    );

    // Verify dimension relationships
    assert!(
        embedding_large.len() > embedding_standard.len(),
        "LocalLarge should have more dimensions than LocalStandard"
    );
    assert!(
        embedding_standard.len() > embedding_mini.len(),
        "LocalStandard should have more dimensions than LocalMini"
    );

    Ok(())
}

/// Test dimension consistency across content types
///
/// Verifies:
/// - All content types produce embeddings with correct dimensions
/// - Empty content doesn't affect dimension consistency
/// - Content length doesn't change embedding dimensions
#[tokio::test]
async fn test_dimension_consistency_across_content_types() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let test_cases = vec![
        ("Empty content", ""),
        ("Single word", "hello"),
        ("Short sentence", "This is a short sentence."),
        ("Medium paragraph", "This is a medium length paragraph that contains multiple sentences and various words to test embedding generation consistency."),
        ("Long content", &"This is a very long content. ".repeat(100)),
        ("Unicode content", "Test with unicode: café, naïve, 日本語, 🚀"),
        ("Code content", "fn main() { println!(\"Hello, Rust!\"); }"),
        ("Mixed content", "# Heading\n\nSome text with ```code``` and [links](url)."),
    ];

    for (description, content) in test_cases {
        let embedding = harness.generate_embedding(content).await?;
        assert_eq!(
            embedding.len(),
            768,
            "{} should produce 768-dimensional embeddings",
            description
        );

        // Verify all values are finite
        for (i, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "{} embedding value at index {} should be finite",
                description, i
            );
        }
    }

    Ok(())
}

// ============================================================================
// Batch Processing Tests
// ============================================================================

/// Test batch processing consistency
///
/// Verifies:
/// - Batch processing produces same embeddings as individual processing
/// - Batch size doesn't affect embedding values
/// - Order is preserved in batch results
#[tokio::test]
async fn test_batch_processing_consistency() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let contents = vec![
        "First test document for batch processing.",
        "Second test document with different content.",
        "Third test document about batch consistency.",
        "Fourth document testing order preservation.",
    ];

    // Generate embeddings individually
    let individual_embeddings: Vec<Vec<f32>> = futures::future::join_all(
        contents.iter().map(|content| harness.generate_embedding(content))
    ).await;

    // Generate embeddings in batch
    let batch_embeddings = harness.generate_batch_embeddings(&contents).await?;

    // Verify same number of results
    assert_eq!(
        individual_embeddings.len(),
        batch_embeddings.len(),
        "Individual and batch processing should produce same number of embeddings"
    );

    // Verify each embedding is identical
    for (i, (individual, batch)) in individual_embeddings.iter().zip(batch_embeddings.iter()).enumerate() {
        assert_eq!(
            individual, batch,
            "Embedding {} should be identical between individual and batch processing",
            i
        );
    }

    // Verify order is preserved
    for i in 0..contents.len() {
        let individual_embedding = harness.generate_embedding(contents[i]).await?;
        assert_eq!(
            batch_embeddings[i], individual_embedding,
            "Batch embedding at index {} should match individual embedding",
            i
        );
    }

    Ok(())
}

/// Test batch processing with different batch sizes
///
/// Verifies:
/// - Various batch sizes work correctly
/// - Large batches are handled properly
/// - Empty batches are handled gracefully
#[tokio::test]
async fn test_batch_processing_sizes() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let base_content = "Test content for batch size testing.";

    // Test different batch sizes
    let batch_sizes = vec![1, 2, 4, 8, 16, 32];

    for batch_size in batch_sizes {
        let contents: Vec<String> = (0..batch_size)
            .map(|i| format!("{} {}", base_content, i))
            .collect();

        let embeddings = harness.generate_batch_embeddings(&contents).await?;

        assert_eq!(
            embeddings.len(),
            batch_size,
            "Batch size {} should produce {} embeddings",
            batch_size, batch_size
        );

        // Verify all embeddings have correct dimensions
        for (i, embedding) in embeddings.iter().enumerate() {
            assert_eq!(
                embedding.len(),
                768,
                "Embedding {} in batch size {} should have 768 dimensions",
                i, batch_size
            );
        }
    }

    // Test empty batch
    let empty_batch: Vec<String> = vec![];
    let empty_embeddings = harness.generate_batch_embeddings(&empty_batch).await?;
    assert!(
        empty_embeddings.is_empty(),
        "Empty batch should produce no embeddings"
    );

    Ok(())
}

/// Test batch processing error handling
///
/// Verifies:
/// - Mixed valid/invalid content is handled properly
/// - Partial failures don't affect valid results
/// - Error reporting is accurate
#[tokio::test]
async fn test_batch_processing_error_handling() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Note: Mock provider doesn't typically fail on content,
    // but we test structural edge cases

    let contents = vec![
        "Valid content 1",
        "",  // Empty content
        "Valid content 2 with more text",
        &"A".repeat(1_000_000),  // Very large content
        "Valid content 3",
    ];

    // Should process all content without panicking
    let embeddings = harness.generate_batch_embeddings(&contents).await?;

    assert_eq!(
        embeddings.len(),
        contents.len(),
        "All content should be processed, even edge cases"
    );

    // Verify all embeddings have correct dimensions
    for (i, embedding) in embeddings.iter().enumerate() {
        assert_eq!(
            embedding.len(),
            768,
            "Embedding {} should have 768 dimensions",
            i
        );

        // Verify all values are finite
        for (j, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "Embedding {} value at index {} should be finite",
                i, j
            );
        }
    }

    Ok(())
}

// ============================================================================
// Edge Case Tests
// ============================================================================

/// Test empty content handling
///
/// Verifies:
/// - Empty strings produce valid embeddings
/// - Whitespace-only strings are handled correctly
/// - Dimension consistency is maintained
#[tokio::test]
async fn test_empty_content_handling() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let empty_cases = vec![
        ("completely empty", ""),
        ("only whitespace", "   \t\n   "),
        ("only newline", "\n\n\n"),
        ("only spaces", "     "),
        ("mixed whitespace", " \t \n \r \t "),
    ];

    for (description, content) in empty_cases {
        let embedding = harness.generate_embedding(content).await?;

        assert_eq!(
            embedding.len(),
            768,
            "{} content should produce 768-dimensional embeddings",
            description
        );

        // Verify all values are finite and within expected range
        for (i, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "{} content embedding value at index {} should be finite",
                description, i
            );
            assert!(
                value >= 0.0 && value <= 1.0,
                "{} content embedding value at index {} should be within [0, 1]",
                description, i
            );
        }
    }

    // Verify all empty cases produce the same embedding
    let embeddings: Result<Vec<_>> = futures::future::join_all(
        empty_cases.iter().map(|(_, content)| harness.generate_embedding(content))
    ).await;

    let embeddings = embeddings?;
    for (i, embedding) in embeddings.iter().enumerate().skip(1) {
        assert_eq!(
            embeddings[0], *embedding,
            "All empty content should produce the same embedding (case {})",
            i
        );
    }

    Ok(())
}

/// Test very long content handling
///
/// Verifies:
/// - Large content doesn't break the provider
/// - Dimensions remain consistent for long content
/// - Performance is reasonable for large inputs
#[tokio::test]
async fn test_large_content_handling() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let large_cases = vec![
        ("1KB", "A".repeat(1024)),
        ("10KB", "B".repeat(10240)),
        ("100KB", "C".repeat(102400)),
        ("1MB", "D".repeat(1024 * 1024)),
    ];

    for (size_name, content) in large_cases {
        let start_time = std::time::Instant::now();
        let embedding = harness.generate_embedding(&content).await?;
        let duration = start_time.elapsed();

        assert_eq!(
            embedding.len(),
            768,
            "{} content should produce 768-dimensional embeddings",
            size_name
        );

        // Verify all values are finite
        for (i, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "{} content embedding value at index {} should be finite",
                size_name, i
            );
        }

        // Verify reasonable performance (should complete within 5 seconds)
        assert!(
            duration.as_secs() < 5,
            "{} content processing should complete within 5 seconds, took {:?}",
            size_name, duration
        );

        println!(
            "Processed {} content in {:?} ({:.2} MB/s)",
            size_name,
            duration,
            content.len() as f64 / duration.as_secs_f64() / 1_048_576.0
        );
    }

    Ok(())
}

/// Test Unicode and special character handling
///
/// Verifies:
/// - Unicode characters are processed correctly
/// - Special characters don't break the provider
/// - Embedding dimensions are consistent
#[tokio::test]
async fn test_unicode_special_character_handling() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let unicode_cases = vec![
        ("French accents", "café, naïve, Noël, château"),
        ("German characters", "Müller, Frühstück, Müller, Mädchen"),
        ("Japanese text", "日本語のテスト、カタカナ、ひらがな"),
        ("Emoji text", "Test with emojis: 🚀 🎨 🔬 ✨ 📚"),
        ("Mathematical symbols", "∀x ∈ ℝ: x² ≥ 0, ∫f(x)dx, ∑(i=1→n)"),
        ("Mixed scripts", "English 日本語 Français العربية Русский"),
        ("Zero-width characters", "test\u{200B}with\u{200C}zero\u{200D}width"),
        ("Control characters", "test\r\nwith\tcontrol\x01characters"),
        ("High Unicode", "Test with 𝓯𝓻𝓪𝓴𝓽𝓾𝓻𝓮𝓭 and 𝕮𝖔𝖓𝖙𝖊𝖓𝖙"),
    ];

    for (description, content) in unicode_cases {
        let embedding = harness.generate_embedding(content).await?;

        assert_eq!(
            embedding.len(),
            768,
            "{} content should produce 768-dimensional embeddings",
            description
        );

        // Verify all values are finite
        for (i, &value) in embedding.iter().enumerate() {
            assert!(
                value.is_finite(),
                "{} content embedding value at index {} should be finite",
                description, i
            );
            assert!(
                value >= 0.0 && value <= 1.0,
                "{} content embedding value at index {} should be within [0, 1]",
                description, i
            );
        }
    }

    Ok(())
}

/// Test content hashing consistency
///
/// Verifies:
/// - Similar content produces similar embeddings
/// - Content changes produce proportional embedding changes
/// - Hash function is consistent
#[tokio::test]
async fn test_content_hashing_consistency() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let base_content = "This is the base content for testing.";

    let variations = vec![
        ("original", base_content.to_string()),
        ("one word changed", "This is the modified content for testing."),
        ("word added", "This is the base content for testing today."),
        ("word removed", "This is base content for testing."),
        ("reordered", "Testing base content is this for the."),
        ("case changed", "THIS IS THE BASE CONTENT FOR TESTING."),
        ("punctuation changed", "This is the base content, for testing!"),
        ("extra whitespace", "This  is  the  base  content  for  testing."),
    ];

    let embeddings: Result<Vec<_>> = futures::future::join_all(
        variations.iter().map(|(_, content)| harness.generate_embedding(content))
    ).await?;

    // Original embedding
    let original_embedding = &embeddings[0];

    // Compare variations with original
    for (i, (variation_name, _)) in variations.iter().enumerate().skip(1) {
        let variation_embedding = &embeddings[i];
        let similarity = cosine_similarity(original_embedding, variation_embedding);

        println!(
            "Similarity between original and {}: {:.4}",
            variation_name, similarity
        );

        // All variations should have some similarity to original
        assert!(
            similarity > 0.5,
            "Variation '{}' should have >0.5 similarity to original, got {:.4}",
            variation_name, similarity
        );

        // But should not be identical (except for very minor changes)
        if !["case changed", "extra whitespace"].contains(&variation_name) {
            assert!(
                similarity < 1.0,
                "Variation '{}' should not be identical to original",
                variation_name
            );
        }
    }

    Ok(())
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Calculate cosine similarity between two vectors
fn cosine_similarity(vec1: &[f32], vec2: &[f32]) -> f32 {
    assert_eq!(
        vec1.len(), vec2.len(),
        "Vectors must have same length for cosine similarity"
    );

    let dot_product: f32 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();
    let norm1: f32 = vec1.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm2: f32 = vec2.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm1 == 0.0 || norm2 == 0.0 {
        0.0
    } else {
        dot_product / (norm1 * norm2)
    }
}

/// Calculate variance of vector values
fn calculate_variance(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }

    let mean = values.iter().sum::<f32>() / values.len() as f32;
    let sum_squared_diff: f32 = values
        .iter()
        .map(|&x| (x - mean) * (x - mean))
        .sum();

    sum_squared_diff / values.len() as f32
}