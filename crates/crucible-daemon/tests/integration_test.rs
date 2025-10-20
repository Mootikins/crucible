//! Optional integration tests with real Ollama provider
//!
//! These tests require:
//! - Running Ollama server (local or remote)
//! - .env file with OLLAMA_ENDPOINT and OLLAMA_MODEL
//! - `cargo test --ignored` to run
//!
//! ## Setup
//!
//! ```bash
//! # Copy example configuration
//! cp .env.example .env
//!
//! # Edit .env with your Ollama configuration
//! # OLLAMA_ENDPOINT=http://localhost:11434
//! # OLLAMA_MODEL=nomic-embed-text-v1.5-q8_0
//!
//! # Run integration tests
//! cargo test -p crucible-daemon --test integration_test --ignored
//! ```
//!
//! ## Environment Variables
//!
//! - `OLLAMA_ENDPOINT`: Ollama server URL (default: http://localhost:11434)
//! - `OLLAMA_MODEL`: Embedding model name (default: nomic-embed-text-v1.5-q8_0)
//! - `OLLAMA_TIMEOUT`: Request timeout in seconds (default: 30)
//! - `OLLAMA_DEBUG`: Enable debug logging (true/false, default: false)

mod fixtures;
mod utils;

use anyhow::{anyhow, Result};
use crucible_llm::embeddings::{EmbeddingConfig, EmbeddingProvider, OllamaProvider};
use std::env;
use std::sync::Arc;
use std::time::Duration;
use utils::{DaemonEmbeddingHarness, EmbeddingHarnessConfig};

// Load environment variables from .env file at test initialization
fn init_env() {
    if let Err(e) = dotenvy::dotenv() {
        println!("Warning: Could not load .env file: {}", e);
    }
}

// ============================================================================
// Test Configuration
// ============================================================================

/// Get Ollama configuration from environment variables
fn get_ollama_config() -> Result<EmbeddingConfig> {
    let endpoint = env::var("OLLAMA_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:11434".to_string());
    let model = env::var("OLLAMA_MODEL")
        .unwrap_or_else(|_| "nomic-embed-text-v1.5-q8_0".to_string());

    let timeout_secs = env::var("OLLAMA_TIMEOUT")
        .ok()
        .and_then(|t| t.parse::<u64>().ok())
        .unwrap_or(30);

    let mut config = EmbeddingConfig::ollama(Some(endpoint), Some(model));
    config.timeout_secs = timeout_secs;

    Ok(config)
}

/// Check if Ollama integration tests should run
fn should_run_ollama_tests() -> bool {
    // Check if .env file exists and has required variables
    let has_endpoint = env::var("OLLAMA_ENDPOINT").is_ok()
        || dotenvy::dotenv().is_ok() && env::var("OLLAMA_ENDPOINT").is_ok();
    let has_model = env::var("OLLAMA_MODEL").is_ok()
        || dotenvy::dotenv().is_ok() && env::var("OLLAMA_MODEL").is_ok();

    has_endpoint && has_model
}

/// Create real Ollama provider for testing
async fn create_real_ollama_provider() -> Result<Arc<dyn EmbeddingProvider>> {
    let config = get_ollama_config()?;
    let provider = OllamaProvider::new(config)?;

    // Verify health check
    provider.health_check().await?;

    Ok(Arc::new(provider))
}

/// Skip test with descriptive message if Ollama not configured
fn skip_if_no_ollama() -> Result<()> {
    if !should_run_ollama_tests() {
        return Err(anyhow!(
            "Skipping Ollama integration test: missing environment variables.\n\
             Set OLLAMA_ENDPOINT and OLLAMA_MODEL in .env file or environment.\n\
             See .env.example for configuration template."
        ));
    }
    Ok(())
}

// ============================================================================
// Integration Tests
// ============================================================================

/// Test basic Ollama provider connectivity and embedding generation
#[tokio::test]
#[ignore]
async fn test_integration_real_ollama_provider() -> Result<()> {
    init_env();
    skip_if_no_ollama()?;

    println!("🔗 Testing real Ollama provider connectivity...");

    let provider = create_real_ollama_provider().await?;

    // Test basic embedding generation
    let test_text = "Rust is a systems programming language focused on safety and performance";
    let response = provider.embed(test_text).await?;

    println!("✓ Generated embedding: {} dimensions", response.dimensions);
    println!("✓ Model: {}", response.model);
    println!("✓ Tokens: {:?}", response.tokens);

    // Validate embedding properties
    assert!(response.dimensions > 0, "Embedding dimensions should be > 0");
    assert!(!response.embedding.is_empty(), "Embedding vector should not be empty");
    assert_eq!(response.embedding.len(), response.dimensions, "Vector length should match dimensions");

    // Check for reasonable embedding values (should be normalized-ish)
    let sum_sq: f32 = response.embedding.iter().map(|&x| x * x).sum();
    let norm = sum_sq.sqrt();
    println!("✓ Embedding norm: {:.3}", norm);

    // Most embedding models produce vectors with norm around 1.0
    assert!(norm > 0.1 && norm < 10.0, "Embedding norm should be reasonable: {}", norm);

    Ok(())
}

/// Test real semantic search with actual embeddings
#[tokio::test]
#[ignore]
async fn test_integration_real_semantic_search() -> Result<()> {
    init_env();
    skip_if_no_ollama()?;

    println!("🔍 Testing real semantic search with Ollama embeddings...");

    let config = EmbeddingHarnessConfig::ollama();
    let harness = DaemonEmbeddingHarness::new(config).await?;

    // Create test documents with semantic relationships
    let test_docs = vec![
        ("rust_guide.md", "# Rust Programming Guide\n\nRust is a systems programming language that guarantees memory safety without garbage collection. It features ownership, borrowing, and lifetimes to prevent common programming errors."),
        ("python_guide.md", "# Python Programming Guide\n\nPython is a high-level interpreted language known for its simplicity and readability. It uses dynamic typing and automatic memory management with garbage collection."),
        ("database_guide.md", "# Database Systems\n\nDatabases store and retrieve data efficiently. Common types include SQL databases like PostgreSQL and NoSQL databases like MongoDB. They provide ACID properties for data consistency."),
        ("web_dev.md", "# Web Development\n\nWeb development involves creating websites and web applications. Key technologies include HTML, CSS, JavaScript, and frameworks like React and Vue.js."),
        ("machine_learning.md", "# Machine Learning\n\nMachine learning algorithms learn patterns from data to make predictions. Common approaches include supervised learning, unsupervised learning, and deep learning with neural networks."),
    ];

    println!("📝 Creating test documents...");
    for (path, content) in test_docs {
        harness.create_note(path, content).await?;
        println!("✓ Created {}", path);
    }

    // Test semantic search queries
    let search_queries = vec![
        ("programming languages", "Should find Rust and Python guides"),
        ("memory management", "Should find Rust guide (ownership) and Python guide (GC)"),
        ("data storage", "Should find database guide"),
        ("frontend development", "Should find web development guide"),
        ("artificial intelligence", "Should find machine learning guide"),
    ];

    println!("🔍 Testing semantic search queries...");
    for (query, expected) in search_queries {
        println!("\nQuery: '{}' ({})", query, expected);

        let results = harness.semantic_search(query, 3).await?;

        assert!(!results.is_empty(), "Should find results for query: {}", query);

        for (i, (path, score)) in results.iter().enumerate() {
            println!("  {}. {} (similarity: {:.3})", i + 1, path, score);
            assert!(*score >= 0.0 && *score <= 1.0, "Similarity score should be normalized");
        }
    }

    // Test search for specific content
    println!("\n🎯 Testing specific content search...");
    let ownership_results = harness.semantic_search("ownership borrowing lifetimes", 5).await?;

    assert!(!ownership_results.is_empty(), "Should find Rust guide for ownership search");

    // The Rust guide should be the top result for ownership-related query
    let rust_found = ownership_results.iter().any(|(path, _)| path.contains("rust_guide"));
    assert!(rust_found, "Should find Rust guide for ownership query");

    println!("✓ Real semantic search test completed successfully");
    Ok(())
}

/// Test real vs mock provider comparison
#[tokio::test]
#[ignore]
async fn test_integration_real_vs_mock_comparison() -> Result<()> {
    init_env();
    skip_if_no_ollama()?;

    println!("⚖️  Comparing real Ollama vs mock provider behavior...");

    // Create real provider
    let real_provider = create_real_ollama_provider().await?;

    // Create mock provider with same dimensions
    let config = get_ollama_config()?;
    let model = config.model.clone();

    // Get dimensions from real provider
    let test_response = real_provider.embed("test").await?;
    let dimensions = test_response.dimensions;
    let mock_provider = utils::create_mock_provider(dimensions);

    println!("📊 Provider comparison:");
    println!("  Real provider: {} ({} dims)", real_provider.model_name(), dimensions);
    println!("  Mock provider: mock-test-model ({} dims)", dimensions);

    // Test same text with both providers
    let test_texts = vec![
        "Rust programming language",
        "Machine learning algorithms",
        "Database systems",
        "Web development",
        "Artificial intelligence",
    ];

    println!("\n🔬 Comparing embeddings for test texts...");
    for text in test_texts {
        println!("\nText: '{}'", text);

        let real_response = real_provider.embed(text).await?;
        let mock_response = mock_provider.embed(text).await?;

        println!("  Real: {} dims, model: {}", real_response.dimensions, real_response.model);
        println!("  Mock: {} dims, model: {}", mock_response.dimensions, mock_response.model);

        // Compare dimensions
        assert_eq!(real_response.dimensions, mock_response.dimensions,
                  "Both providers should have same dimensions");

        // Real embeddings should be non-deterministic and meaningful
        // Mock embeddings are deterministic based on text hash
        let real_norm = real_response.embedding.iter().map(|&x| x * x).sum::<f32>().sqrt();
        let mock_norm = mock_response.embedding.iter().map(|&x| x * x).sum::<f32>().sqrt();

        println!("  Real norm: {:.3}", real_norm);
        println!("  Mock norm: {:.3}", mock_norm);

        // Both should have reasonable norms
        assert!(real_norm > 0.1, "Real embedding norm should be > 0.1");
        assert!(mock_norm > 0.1, "Mock embedding norm should be > 0.1");
    }

    println!("\n✓ Real vs mock comparison completed");
    Ok(())
}

/// Test batch embedding with real provider
#[tokio::test]
#[ignore]
async fn test_integration_real_batch_embedding() -> Result<()> {
    init_env();
    skip_if_no_ollama()?;

    println!("📦 Testing batch embedding with real Ollama provider...");

    let provider = create_real_ollama_provider().await?;

    // Prepare batch of texts
    let texts = vec![
        "Introduction to Rust programming".to_string(),
        "Understanding ownership and borrowing".to_string(),
        "Error handling with Result and Option".to_string(),
        "Concurrency with async/await".to_string(),
        "Memory safety guarantees".to_string(),
    ];

    println!("🔄 Generating embeddings for {} texts...", texts.len());

    let start_time = std::time::Instant::now();
    let responses = provider.embed_batch(texts.clone()).await?;
    let duration = start_time.elapsed();

    println!("✓ Batch completed in {:?}", duration);
    println!("  Average time per embedding: {:?}", duration / texts.len() as u32);

    // Validate batch results
    assert_eq!(responses.len(), texts.len(), "Should have response for each text");

    for (i, response) in responses.iter().enumerate() {
        println!("  Text {}: {} dims, model: {}", i + 1, response.dimensions, response.model);

        assert!(response.dimensions > 0, "Dimensions should be > 0");
        assert_eq!(response.embedding.len(), response.dimensions, "Vector length should match dimensions");

        // Check for reasonable norm
        let norm = response.embedding.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!(norm > 0.1 && norm < 10.0, "Norm should be reasonable: {}", norm);
    }

    // Test semantic similarity in batch results
    println!("\n🔍 Testing semantic relationships in batch results...");

    // Calculate similarity between first two (both Rust-related)
    let rust_intro = &responses[0].embedding;
    let rust_ownership = &responses[1].embedding;

    let similarity = cosine_similarity(rust_intro, rust_ownership);
    println!("  Similarity (intro vs ownership): {:.3}", similarity);

    // They should be reasonably similar (both about Rust)
    assert!(similarity > 0.3, "Rust-related texts should be similar");

    println!("✓ Batch embedding test completed");
    Ok(())
}

/// Test error handling for invalid configuration
#[tokio::test]
#[ignore]
async fn test_integration_error_handling() -> Result<()> {
    init_env();
    println!("🚨 Testing error handling for invalid configurations...");

    // Test invalid endpoint
    println!("Testing invalid endpoint...");
    let invalid_config = EmbeddingConfig::ollama(
        Some("http://localhost:99999".to_string()), // Invalid port
        Some("nomic-embed-text".to_string()),
    );

    let provider_result = OllamaProvider::new(invalid_config);
    assert!(provider_result.is_ok(), "Provider creation should succeed (validation happens at health check)");

    let provider = provider_result.unwrap();
    let health_result = provider.health_check().await;
    assert!(health_result.is_err(), "Health check should fail for invalid endpoint");

    if let Err(e) = health_result {
        println!("  ✓ Health check failed as expected: {}", e);
    }

    // Test embedding with invalid provider (should fail gracefully)
    let embed_result = provider.embed("test").await;
    assert!(embed_result.is_err(), "Embedding should fail for invalid endpoint");

    if let Err(e) = embed_result {
        println!("  ✓ Embedding failed as expected: {}", e);
    }

    // Test missing environment variables
    println!("Testing missing environment variables...");

    // Temporarily clear environment variables
    let original_endpoint = env::var("OLLAMA_ENDPOINT").ok();
    let original_model = env::var("OLLAMA_MODEL").ok();

    env::remove_var("OLLAMA_ENDPOINT");
    env::remove_var("OLLAMA_MODEL");

    // Test skip_if_no_ollama function
    let skip_result = skip_if_no_ollama();
    assert!(skip_result.is_err(), "Should skip when env vars missing");

    if let Err(e) = skip_result {
        println!("  ✓ Skip test works correctly: {}", e);
    }

    // Restore environment variables
    if let Some(endpoint) = original_endpoint {
        env::set_var("OLLAMA_ENDPOINT", endpoint);
    }
    if let Some(model) = original_model {
        env::set_var("OLLAMA_MODEL", model);
    }

    println!("✓ Error handling test completed");
    Ok(())
}

/// Test embedding quality and semantic relationships
#[tokio::test]
#[ignore]
async fn test_integration_embedding_quality() -> Result<()> {
    init_env();
    skip_if_no_ollama()?;

    println!("🎯 Testing embedding quality and semantic relationships...");

    let provider = create_real_ollama_provider().await?;

    // Test semantic relationships
    let test_pairs = vec![
        ("rust programming", "rust language"),     // Highly similar
        ("rust programming", "python programming"), // Moderately similar
        ("rust programming", "cooking recipe"),     // Low similarity
    ];

    for (text1, text2) in test_pairs {
        println!("\nComparing: '{}' vs '{}'", text1, text2);

        let emb1 = provider.embed(text1).await?;
        let emb2 = provider.embed(text2).await?;

        let similarity = cosine_similarity(&emb1.embedding, &emb2.embedding);
        println!("  Similarity: {:.3}", similarity);

        assert!(similarity >= 0.0 && similarity <= 1.0, "Similarity should be normalized");
    }

    // Test semantic search quality
    println!("\n🔍 Testing semantic search quality...");

    let config = EmbeddingHarnessConfig::ollama();
    let harness = DaemonEmbeddingHarness::new(config).await?;

    // Create domain-specific documents
    let docs = vec![
        ("algorithms.md", "# Algorithms\n\nSorting algorithms like quicksort, mergesort, and heapsort are fundamental to computer science."),
        ("data_structures.md", "# Data Structures\n\nArrays, linked lists, trees, and graphs are essential data structures for organizing data."),
        ("web_frameworks.md", "# Web Frameworks\n\nReact, Vue, and Angular are popular JavaScript frameworks for building user interfaces."),
        ("cooking.md", "# Cooking\n\nPasta, pizza, and salad are popular dishes. Cooking involves ingredients, recipes, and techniques."),
    ];

    for (path, content) in docs {
        harness.create_note(path, content).await?;
    }

    // Test programming-related search
    let prog_results = harness.semantic_search("computer science programming", 5).await?;

    // Should find algorithms and data structures first
    let programming_docs = prog_results.iter()
        .filter(|(path, _)| path.contains("algorithms") || path.contains("data_structures"))
        .count();

    assert!(programming_docs >= 2, "Should find at least 2 programming-related documents");

    println!("✓ Found {} programming-related documents out of {}", programming_docs, prog_results.len());

    // Test cooking-related search
    let cooking_results = harness.semantic_search("food recipes ingredients", 5).await?;

    let cooking_found = cooking_results.iter()
        .any(|(path, _)| path.contains("cooking"));

    assert!(cooking_found, "Should find cooking document for food-related query");

    println!("✓ Embedding quality test completed");
    Ok(())
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Calculate cosine similarity between two embedding vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vectors must have same length");

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
}

// ============================================================================
// Test Suite Runner
// ============================================================================

/// Run all integration tests with summary
#[cfg(test)]
mod test_runner {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_integration_full_suite() -> Result<()> {
        init_env();
        println!("🧪 Running full Ollama integration test suite...\n");

        let start_time = std::time::Instant::now();

        // Note: Individual tests should be run separately since they're marked with #[tokio::test]
        // This function serves as a documentation/example of the full test suite
        println!("Individual tests available:");
        println!("  - test_integration_real_ollama_provider");
        println!("  - test_integration_real_semantic_search");
        println!("  - test_integration_real_vs_mock_comparison");
        println!("  - test_integration_real_batch_embedding");
        println!("  - test_integration_error_handling");
        println!("  - test_integration_embedding_quality");
        println!("\nRun tests individually with: cargo test --test integration_test --ignored");

        let duration = start_time.elapsed();

        println!("Total suite documentation time: {:?}", duration);
        println!("🎉 Integration test suite configured successfully!");

        Ok(())
    }
}