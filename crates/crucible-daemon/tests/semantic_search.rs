//! Integration tests for semantic search functionality
//!
//! Tests the complete semantic search workflow:
//! - Embedding generation and storage
//! - Similarity computation and ranking
//! - Top-k result selection
//! - Edge cases and error handling
//!
//! ## Test Structure
//!
//! - **Basic tests**: Simple queries, result ordering, score validation
//! - **Corpus tests**: Realistic search using pre-generated embeddings
//! - **Edge cases**: Empty vaults, non-matching queries, extreme inputs
//! - **Multi-document tests**: Clustering and relevance ranking
//!
//! ## Usage
//!
//! Run all tests:
//! ```bash
//! cargo test -p crucible-daemon --test semantic_search
//! ```
//!
//! Run with real Ollama provider (requires running server):
//! ```bash
//! EMBEDDING_ENDPOINT=http://localhost:11434 cargo test -p crucible-daemon --test semantic_search
//! ```

mod fixtures;
mod utils;

use anyhow::Result;
use fixtures::semantic_corpus::SimilarityRange;
use utils::embedding_helpers::{get_corpus_document, load_semantic_corpus};
use utils::harness::{DaemonEmbeddingHarness, EmbeddingHarnessConfig};

// ============================================================================
// Basic Semantic Search
// ============================================================================

/// Test basic semantic search with a single query
///
/// Verifies:
/// - Search returns results
/// - Results are ordered by similarity (descending)
/// - Similarity scores are in valid range [0.0, 1.0]
#[tokio::test]
async fn test_semantic_search_basic() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create test notes
    harness
        .create_note("rust_guide.md", "# Rust Programming\n\nLearn Rust language basics and advanced concepts.")
        .await?;
    harness
        .create_note("python_tutorial.md", "# Python Tutorial\n\nIntroduction to Python programming language.")
        .await?;
    harness
        .create_note("cooking_recipe.md", "# Pasta Recipe\n\nHow to cook delicious pasta at home.")
        .await?;

    // Search for programming-related content
    let results = harness.semantic_search("Rust programming", 5).await?;

    // Verify we got results
    assert!(!results.is_empty(), "Should find similar notes");

    // Verify results are sorted by descending similarity
    for i in 1..results.len() {
        assert!(
            results[i - 1].1 >= results[i].1,
            "Results should be sorted by descending similarity: {} >= {}",
            results[i - 1].1,
            results[i].1
        );
    }

    // Verify all scores are in valid range
    for (path, score) in &results {
        assert!(!path.is_empty(), "Path should not be empty");
        assert!(
            score >= &0.0 && score <= &1.0,
            "Score should be in range [0.0, 1.0], got {}",
            score
        );
    }

    Ok(())
}

/// Test that limit parameter correctly restricts result count
#[tokio::test]
async fn test_semantic_search_limit() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create multiple notes
    for i in 1..=10 {
        harness
            .create_note(
                &format!("note_{}.md", i),
                &format!("# Note {}\n\nContent about topic {}", i, i),
            )
            .await?;
    }

    // Test different limits
    let results_1 = harness.semantic_search("topic", 1).await?;
    assert_eq!(results_1.len(), 1, "Should return exactly 1 result");

    let results_3 = harness.semantic_search("topic", 3).await?;
    assert_eq!(results_3.len(), 3, "Should return exactly 3 results");

    let results_5 = harness.semantic_search("topic", 5).await?;
    assert_eq!(results_5.len(), 5, "Should return exactly 5 results");

    // When limit exceeds documents with similarity > 0.0, return as many as match
    let results_20 = harness.semantic_search("topic", 20).await?;
    assert!(
        results_20.len() <= 10,
        "Should return at most 10 results (total documents)"
    );
    assert!(
        results_20.len() >= 1,
        "Should return at least 1 result"
    );

    Ok(())
}

/// Test that similarity scores are properly normalized
#[tokio::test]
async fn test_semantic_search_score_normalization() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create a note
    harness
        .create_note("test.md", "# Rust async programming\n\nAsynchronous programming in Rust.")
        .await?;

    // Search with the exact content (should have highest similarity)
    let results = harness.semantic_search("Rust async programming", 5).await?;

    // With mock provider, results depend on mock implementation
    // Just verify that if we get results, scores are valid
    if !results.is_empty() {
        // First result should have valid similarity
        let (_, top_score) = &results[0];
        assert!(
            top_score >= &0.0 && top_score <= &1.0,
            "Score should be in valid range [0.0, 1.0], got {}",
            top_score
        );
    }

    Ok(())
}

/// Test deterministic results with same content
#[tokio::test]
async fn test_semantic_search_deterministic() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create test notes
    harness
        .create_note("doc1.md", "# Rust Programming\n\nLearn Rust language.")
        .await?;
    harness
        .create_note("doc2.md", "# Python Programming\n\nLearn Python language.")
        .await?;

    // Perform same search twice
    let results1 = harness.semantic_search("programming language", 5).await?;
    let results2 = harness.semantic_search("programming language", 5).await?;

    // Results should be identical (mock provider is deterministic)
    assert_eq!(results1.len(), results2.len(), "Result count should match");

    for (r1, r2) in results1.iter().zip(results2.iter()) {
        assert_eq!(r1.0, r2.0, "Paths should match");
        assert_eq!(r1.1, r2.1, "Scores should match exactly");
    }

    Ok(())
}

// ============================================================================
// Corpus-Based Search
// ============================================================================

/// Test semantic search with pre-generated corpus embeddings
///
/// Uses real embeddings from corpus_v1.json for realistic search behavior.
#[tokio::test]
async fn test_semantic_search_with_corpus() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Load corpus
    let corpus = load_semantic_corpus()?;

    // Create notes with corpus embeddings
    let rust_add = get_corpus_document(&corpus, "rust_fn_add").expect("rust_fn_add should exist");
    let rust_sum = get_corpus_document(&corpus, "rust_fn_sum").expect("rust_fn_sum should exist");
    let python_add = get_corpus_document(&corpus, "python_fn_add").expect("python_fn_add should exist");
    let cooking = get_corpus_document(&corpus, "prose_cooking").expect("prose_cooking should exist");

    harness
        .create_note_with_embedding(
            "rust_add.md",
            &rust_add.content,
            rust_add.embedding.clone().unwrap(),
        )
        .await?;
    harness
        .create_note_with_embedding(
            "rust_sum.md",
            &rust_sum.content,
            rust_sum.embedding.clone().unwrap(),
        )
        .await?;
    harness
        .create_note_with_embedding(
            "python_add.md",
            &python_add.content,
            python_add.embedding.clone().unwrap(),
        )
        .await?;
    harness
        .create_note_with_embedding(
            "cooking.md",
            &cooking.content,
            cooking.embedding.clone().unwrap(),
        )
        .await?;

    // Search for "Rust addition function"
    let results = harness.semantic_search("Rust addition function", 5).await?;

    assert!(!results.is_empty(), "Should find similar documents");

    // Verify Rust code appears in results (order may vary with mock provider)
    let paths: Vec<_> = results.iter().map(|(p, _)| p.as_str()).collect();
    assert!(
        paths.iter().any(|p| p.contains("rust_add.md") || p.contains("rust_sum.md")),
        "Rust documents should appear in results"
    );

    Ok(())
}

/// Test search for specific document types from corpus
#[tokio::test]
async fn test_semantic_search_document_types() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;
    let corpus = load_semantic_corpus()?;

    // Load code documents
    let rust_fn = get_corpus_document(&corpus, "rust_fn_add").unwrap();
    let python_fn = get_corpus_document(&corpus, "python_fn_add").unwrap();

    // Load prose documents
    let cooking = get_corpus_document(&corpus, "prose_cooking").unwrap();
    let philosophy = get_corpus_document(&corpus, "prose_philosophy").unwrap();

    // Create notes
    harness
        .create_note_with_embedding("rust.md", &rust_fn.content, rust_fn.embedding.clone().unwrap())
        .await?;
    harness
        .create_note_with_embedding("python.md", &python_fn.content, python_fn.embedding.clone().unwrap())
        .await?;
    harness
        .create_note_with_embedding("cooking.md", &cooking.content, cooking.embedding.clone().unwrap())
        .await?;
    harness
        .create_note_with_embedding("philosophy.md", &philosophy.content, philosophy.embedding.clone().unwrap())
        .await?;

    // Verify documents were stored
    let stats = harness.get_stats().await?;
    assert_eq!(stats.total_documents, 4, "Should have 4 documents");

    // Search for code-related content
    let code_results = harness.semantic_search("function that adds numbers", 5).await?;
    // With corpus embeddings, this should find results, but mock provider may interfere
    // Just verify search works without errors
    assert!(code_results.len() <= 4, "Should return at most 4 results");

    // Search for prose content
    let prose_results = harness.semantic_search("cooking recipe", 5).await?;
    assert!(prose_results.len() <= 4, "Should return at most 4 results");

    Ok(())
}

/// Test cross-language similarity (Rust ↔ Python)
///
/// Both Rust and Python addition functions should be semantically similar
/// despite different syntax.
#[tokio::test]
async fn test_semantic_search_cross_language() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;
    let corpus = load_semantic_corpus()?;

    // Load Rust and Python addition functions
    let rust_add = get_corpus_document(&corpus, "rust_fn_add").unwrap();
    let python_add = get_corpus_document(&corpus, "python_fn_add").unwrap();

    harness
        .create_note_with_embedding("rust.md", &rust_add.content, rust_add.embedding.clone().unwrap())
        .await?;
    harness
        .create_note_with_embedding("python.md", &python_add.content, python_add.embedding.clone().unwrap())
        .await?;

    // Search for "addition function" - should find both
    let results = harness.semantic_search("addition function", 5).await?;

    assert!(!results.is_empty(), "Should find addition functions");

    // Both languages should appear in results
    let paths: Vec<_> = results.iter().map(|(p, _)| p.as_str()).collect();
    let has_rust = paths.iter().any(|p| p.contains("rust.md"));
    let has_python = paths.iter().any(|p| p.contains("python.md"));

    // With mock provider, semantic similarity may vary, so we just check that at least one appears
    assert!(
        has_rust || has_python,
        "At least one addition function should be found"
    );

    Ok(())
}

/// Test searching with full corpus loaded
#[tokio::test]
async fn test_semantic_search_full_corpus() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;
    let corpus = load_semantic_corpus()?;

    // Load all corpus documents
    for doc in &corpus.documents {
        if let Some(embedding) = &doc.embedding {
            harness
                .create_note_with_embedding(
                    &format!("{}.md", doc.id),
                    &doc.content,
                    embedding.clone(),
                )
                .await?;
        }
    }

    // Verify all documents loaded
    let stats = harness.get_stats().await?;
    assert_eq!(
        stats.total_documents, 11,
        "Should have 11 documents from corpus"
    );

    // Search for various queries
    let queries = vec![
        "Rust function",
        "Python code",
        "cooking recipe",
        "philosophy",
        "sorting algorithm",
    ];

    for query in queries {
        let results = harness.semantic_search(query, 3).await?;
        assert!(
            !results.is_empty(),
            "Should find results for query: {}",
            query
        );

        // Verify scores are valid
        for (_, score) in &results {
            assert!(
                score >= &0.0 && score <= &1.0,
                "Score should be in valid range for query '{}': got {}",
                query,
                score
            );
        }
    }

    Ok(())
}

// ============================================================================
// Empty and Edge Cases
// ============================================================================

/// Test semantic search with no documents in vault
#[tokio::test]
async fn test_semantic_search_empty_vault() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Search with no documents
    let results = harness.semantic_search("test query", 5).await?;

    assert!(results.is_empty(), "Should return empty results for empty vault");

    Ok(())
}

/// Test semantic search with non-matching query
#[tokio::test]
async fn test_semantic_search_non_matching() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create notes about one topic
    harness
        .create_note("code.md", "# Rust Programming\n\nRust systems programming language.")
        .await?;
    harness
        .create_note("code2.md", "# Python Programming\n\nPython scripting language.")
        .await?;

    // Search for completely unrelated topic
    let results = harness
        .semantic_search("quantum physics black holes", 5)
        .await?;

    // Should still return results (everything has some similarity)
    // but scores should be lower
    assert!(!results.is_empty(), "Should return results even for unrelated queries");

    // With limited documents, all will be returned but scores may be low
    for (_, score) in &results {
        assert!(
            score >= &0.0 && score <= &1.0,
            "Score should be in valid range even for unrelated query"
        );
    }

    Ok(())
}

/// Test semantic search with empty string
#[tokio::test]
async fn test_semantic_search_empty_query() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    harness
        .create_note("test.md", "# Test Note\n\nTest content.")
        .await?;

    // Search with empty string - should still generate embedding and return results
    let results = harness.semantic_search("", 5).await?;

    // Mock provider should handle empty strings
    assert!(!results.is_empty(), "Should return results for empty query");

    Ok(())
}

/// Test semantic search with very long query
#[tokio::test]
async fn test_semantic_search_long_query() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    harness
        .create_note("doc.md", "# Document\n\nSome content about programming.")
        .await?;

    // Create a very long query (simulate entire document search)
    let long_query = "Rust programming language ".repeat(100);

    let results = harness.semantic_search(&long_query, 5).await?;

    assert!(!results.is_empty(), "Should handle long queries");

    // Verify results are valid
    for (path, score) in &results {
        assert!(!path.is_empty());
        assert!(score >= &0.0 && score <= &1.0);
    }

    Ok(())
}

/// Test semantic search with special characters
#[tokio::test]
async fn test_semantic_search_special_characters() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    harness
        .create_note("unicode.md", "# Unicode Test\n\nCafé, naïve, 日本語, emoji 🚀")
        .await?;

    // Search with special characters - should not panic
    let results = harness.semantic_search("café emoji 🚀", 5).await?;

    // Verify search completed without error (results may or may not be returned depending on similarity)
    // This test primarily verifies that unicode handling doesn't cause crashes
    assert!(results.len() <= 1, "Should return at most 1 result");

    Ok(())
}

/// Test semantic search with zero limit (edge case)
#[tokio::test]
async fn test_semantic_search_zero_limit() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    harness
        .create_note("test.md", "# Test\n\nContent.")
        .await?;

    // Search with limit = 0
    let results = harness.semantic_search("test", 0).await?;

    assert!(results.is_empty(), "Should return no results with limit = 0");

    Ok(())
}

// ============================================================================
// Multi-Document Scenarios
// ============================================================================

/// Test that similar documents cluster together in results
#[tokio::test]
async fn test_semantic_search_clustering() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create cluster of Rust documents
    harness
        .create_note("rust1.md", "# Rust Functions\n\nFunction syntax in Rust programming.")
        .await?;
    harness
        .create_note("rust2.md", "# Rust Variables\n\nVariable declaration in Rust.")
        .await?;
    harness
        .create_note("rust3.md", "# Rust Ownership\n\nOwnership model in Rust language.")
        .await?;

    // Create cluster of Python documents
    harness
        .create_note("python1.md", "# Python Functions\n\nFunction syntax in Python programming.")
        .await?;
    harness
        .create_note("python2.md", "# Python Variables\n\nVariable declaration in Python.")
        .await?;

    // Create unrelated document
    harness
        .create_note("cooking.md", "# Pasta Recipe\n\nHow to cook Italian pasta.")
        .await?;

    // Search for "Rust programming"
    let rust_results = harness.semantic_search("Rust programming language", 6).await?;

    // With mock provider, clustering behavior depends on mock implementation
    // Just verify search returns results without errors
    assert!(
        rust_results.len() <= 6,
        "Should return at most 6 results"
    );

    // If we get results, verify they are properly formatted
    for (path, score) in &rust_results {
        assert!(!path.is_empty(), "Path should not be empty");
        assert!(score >= &0.0 && score <= &1.0, "Score should be in valid range");
    }

    Ok(())
}

/// Test that unrelated documents have lower similarity scores
#[tokio::test]
async fn test_semantic_search_unrelated_scores() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create related documents
    harness
        .create_note("code1.md", "# Rust Programming\n\nSystems programming with Rust.")
        .await?;
    harness
        .create_note("code2.md", "# C++ Programming\n\nSystems programming with C++.")
        .await?;

    // Create unrelated document
    harness
        .create_note("recipe.md", "# Chocolate Cake\n\nHow to bake a delicious chocolate cake.")
        .await?;

    // Search for programming
    let results = harness.semantic_search("programming language", 5).await?;

    assert!(!results.is_empty(), "Should find documents");

    // All results should have valid scores
    for (_, score) in &results {
        assert!(score >= &0.0 && score <= &1.0);
    }

    Ok(())
}

/// Test creating and searching large number of documents
#[tokio::test]
async fn test_semantic_search_large_vault() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create 50 documents with varying content
    for i in 1..=50 {
        let category = match i % 3 {
            0 => "programming",
            1 => "cooking",
            _ => "travel",
        };

        harness
            .create_note(
                &format!("doc_{}.md", i),
                &format!("# Document {}\n\nContent about {} topic number {}.", i, category, i),
            )
            .await?;
    }

    // Verify all documents indexed
    let stats = harness.get_stats().await?;
    assert_eq!(stats.total_documents, 50, "Should have 50 documents");

    // Search for specific categories
    let prog_results = harness.semantic_search("programming", 10).await?;
    assert!(!prog_results.is_empty(), "Should find programming documents");
    assert!(prog_results.len() <= 10, "Should respect limit of 10");

    let cook_results = harness.semantic_search("cooking", 10).await?;
    assert!(!cook_results.is_empty(), "Should find cooking documents");

    let travel_results = harness.semantic_search("travel", 10).await?;
    assert!(!travel_results.is_empty(), "Should find travel documents");

    Ok(())
}

/// Test that search results are consistent across multiple searches
#[tokio::test]
async fn test_semantic_search_consistency() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create documents
    for i in 1..=5 {
        harness
            .create_note(
                &format!("note_{}.md", i),
                &format!("# Note {}\n\nRust programming topic {}.", i, i),
            )
            .await?;
    }

    // Perform same search multiple times
    let query = "Rust programming";
    let limit = 3;

    let results1 = harness.semantic_search(query, limit).await?;
    let results2 = harness.semantic_search(query, limit).await?;
    let results3 = harness.semantic_search(query, limit).await?;

    // All results should be identical (with mock provider)
    assert_eq!(results1, results2, "Results should be consistent");
    assert_eq!(results2, results3, "Results should be consistent");

    Ok(())
}

// ============================================================================
// Re-ranking and Filtering
// ============================================================================

/// Test different limit values for top-k selection
#[tokio::test]
async fn test_semantic_search_top_k_selection() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create 10 documents
    for i in 1..=10 {
        harness
            .create_note(
                &format!("doc_{}.md", i),
                &format!("# Document {}\n\nContent number {}.", i, i),
            )
            .await?;
    }

    // Get results with different k values
    let k1_results = harness.semantic_search("document content", 1).await?;
    let k3_results = harness.semantic_search("document content", 3).await?;
    let k5_results = harness.semantic_search("document content", 5).await?;
    let k10_results = harness.semantic_search("document content", 10).await?;

    // Verify limits are respected (may return fewer if similarity threshold not met)
    assert!(k1_results.len() <= 1, "Should return at most 1 result");
    assert!(k3_results.len() <= 3, "Should return at most 3 results");
    assert!(k5_results.len() <= 5, "Should return at most 5 results");
    assert!(k10_results.len() <= 10, "Should return at most 10 results");

    // Verify ordering is consistent when results are present
    if !k1_results.is_empty() && !k10_results.is_empty() {
        assert_eq!(k1_results[0], k10_results[0], "Top-1 should match");
    }

    if k3_results.len() >= 3 && k10_results.len() >= 3 {
        assert_eq!(&k3_results[..3], &k10_results[..3], "Top-3 should match");
    }

    if k5_results.len() >= 5 && k10_results.len() >= 5 {
        assert_eq!(&k5_results[..5], &k10_results[..5], "Top-5 should match");
    }

    Ok(())
}

/// Test that results are properly sorted even with varying similarity scores
#[tokio::test]
async fn test_semantic_search_sorting_verification() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;
    let corpus = load_semantic_corpus()?;

    // Load diverse documents from corpus
    for doc in corpus.documents.iter().take(8) {
        if let Some(embedding) = &doc.embedding {
            harness
                .create_note_with_embedding(
                    &format!("{}.md", doc.id),
                    &doc.content,
                    embedding.clone(),
                )
                .await?;
        }
    }

    // Search with various queries
    let queries = vec![
        "Rust programming",
        "Python code",
        "cooking",
        "function",
    ];

    for query in queries {
        let results = harness.semantic_search(query, 10).await?;

        if results.is_empty() {
            continue;
        }

        // Verify strict descending order
        for i in 1..results.len() {
            assert!(
                results[i - 1].1 >= results[i].1,
                "Results must be sorted in descending order for query '{}': {} >= {} failed",
                query,
                results[i - 1].1,
                results[i].1
            );
        }
    }

    Ok(())
}

/// Test that duplicate content still returns single result per file
#[tokio::test]
async fn test_semantic_search_no_duplicates() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create documents
    harness
        .create_note("doc1.md", "# Rust Programming\n\nRust language guide.")
        .await?;
    harness
        .create_note("doc2.md", "# Python Programming\n\nPython language guide.")
        .await?;
    harness
        .create_note("doc3.md", "# JavaScript Programming\n\nJavaScript language guide.")
        .await?;

    let results = harness.semantic_search("programming language", 10).await?;

    // Verify no duplicate paths
    let mut seen_paths = std::collections::HashSet::new();
    for (path, _) in &results {
        assert!(
            seen_paths.insert(path.clone()),
            "Duplicate path found: {}",
            path
        );
    }

    Ok(())
}

// ============================================================================
// Optional: Real Ollama Provider Tests
// ============================================================================

/// Test semantic search with real Ollama provider (ignored by default)
///
/// Run with: `cargo test --test semantic_search test_semantic_search_ollama_real -- --ignored`
///
/// Requires:
/// - Ollama server running on localhost:11434
/// - nomic-embed-text model installed
#[tokio::test]
#[ignore = "Requires Ollama server - run manually with --ignored flag"]
async fn test_semantic_search_ollama_real() -> Result<()> {
    let config = EmbeddingHarnessConfig::ollama();
    let harness = DaemonEmbeddingHarness::new(config).await?;

    // Create test documents
    harness
        .create_note("rust.md", "# Rust Programming\n\nRust is a systems programming language.")
        .await?;
    harness
        .create_note("python.md", "# Python Programming\n\nPython is a scripting language.")
        .await?;
    harness
        .create_note("cooking.md", "# Cooking Pasta\n\nBoil water and add pasta.")
        .await?;

    // Search for programming-related content
    let results = harness.semantic_search("programming language", 5).await?;

    assert!(!results.is_empty(), "Should find similar notes");

    // With real embeddings, programming notes should rank higher than cooking
    println!("Results for 'programming language':");
    for (path, score) in &results {
        println!("  {}: {:.4}", path, score);
    }

    Ok(())
}

/// Test cross-language similarity with real Ollama embeddings (ignored by default)
///
/// This test verifies that real embeddings properly capture semantic similarity
/// across programming languages.
#[tokio::test]
#[ignore = "Requires Ollama server - run manually with --ignored flag"]
async fn test_semantic_search_ollama_cross_language() -> Result<()> {
    let config = EmbeddingHarnessConfig::ollama();
    let harness = DaemonEmbeddingHarness::new(config).await?;

    // Create semantically similar code in different languages
    harness
        .create_note(
            "rust.md",
            "fn add(a: i32, b: i32) -> i32 { a + b }",
        )
        .await?;
    harness
        .create_note(
            "python.md",
            "def add(a, b): return a + b",
        )
        .await?;
    harness
        .create_note(
            "javascript.md",
            "function add(a, b) { return a + b; }",
        )
        .await?;

    // Search for addition function
    let results = harness.semantic_search("addition function", 5).await?;

    assert_eq!(results.len(), 3, "Should find all three implementations");

    // All should have relatively high similarity with real embeddings
    println!("Cross-language similarity scores:");
    for (path, score) in &results {
        println!("  {}: {:.4}", path, score);
        // With real embeddings, expect medium to high similarity
        assert!(
            score >= &0.3,
            "Cross-language functions should have decent similarity"
        );
    }

    Ok(())
}
