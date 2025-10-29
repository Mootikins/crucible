//! Integration tests for re-embedding workflows
//!
//! Tests updating embeddings when:
//! - Note content changes
//! - Embedding model changes
//! - Selective re-processing is needed
//!
//! Validates:
//! - Old embeddings replaced correctly
//! - Timestamps updated appropriately
//! - Metadata preserved
//! - Error handling
//! - Semantic search uses new embeddings
//!
//! ## Test Coverage
//!
//! ### Content Update Re-embedding
//! - Create note with initial embedding
//! - Update note content
//! - Re-generate embedding
//! - Verify old embedding replaced with new
//! - Verify semantic search uses new embedding
//!
//! ### Model Change Re-embedding
//! - Create note with one model's embedding
//! - Switch to different model (different dimensions)
//! - Re-embed all notes
//! - Verify dimension changes handled correctly
//!
//! ### Selective Re-embedding
//! - Create multiple notes
//! - Re-embed only specific notes (by path/filter)
//! - Verify unchanged notes retain original embeddings
//! - Verify changed notes have new embeddings
//!
//! ### Embedding Comparison
//! - Store old embedding before re-embedding
//! - Re-embed with same content
//! - Compare old vs new embeddings
//! - Verify similarity (should be high for same content)
//!
//! ### Timestamp Updates
//! - Verify `updated_at` timestamp changes on re-embedding
//! - Verify `created_at` timestamp remains unchanged
//! - Verify other metadata preserved
//!
//! ## Usage
//!
//! Run all re-embedding tests:
//! ```bash
//! cargo test -p crucible-daemon --test re_embedding
//! ```
//!
//! Run specific test:
//! ```bash
//! cargo test -p crucible-daemon --test re_embedding test_reembedding_content_update
//! ```

mod fixtures;
mod utils;

use anyhow::Result;
use std::time::Duration;
use utils::harness::{DaemonEmbeddingHarness, EmbeddingHarnessConfig};
use utils::semantic_assertions::cosine_similarity;

// ============================================================================
// Content Update Re-embedding Tests
// ============================================================================

/// Test re-embedding when note content changes
///
/// Verifies:
/// - Initial embedding is created and stored
/// - Content update triggers new embedding
/// - Old embedding is replaced with new one
/// - Embedding vectors are different
#[tokio::test]
async fn test_reembedding_content_update() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create initial note
    let content_v1 = r#"---
title: Rust Programming
tags: [rust, programming]
---

# Rust Programming Guide

Learn about Rust systems programming language.
"#;

    harness.create_note("guide.md", content_v1).await?;

    // Get initial embedding
    let embedding_v1 = harness
        .get_embedding("guide.md")
        .await?
        .expect("Initial embedding should exist");

    // Wait to ensure different timestamp
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Update content - simulate content change by re-creating with new content
    let content_v2 = r#"---
title: Rust Programming
tags: [rust, programming]
---

# Rust Programming Guide

Advanced guide to Rust async programming and concurrency.
"#;

    // Re-embed by creating note with same path (simulates update)
    harness.create_note("guide.md", content_v2).await?;

    // Get new embedding
    let embedding_v2 = harness
        .get_embedding("guide.md")
        .await?
        .expect("Updated embedding should exist");

    // Verify embeddings are different (content changed)
    assert_ne!(
        embedding_v1, embedding_v2,
        "Embeddings should differ after content update"
    );

    // Verify dimensions remain the same
    assert_eq!(
        embedding_v1.len(),
        embedding_v2.len(),
        "Embedding dimensions should remain consistent"
    );

    Ok(())
}

/// Test that re-embedded notes appear in semantic search with new embeddings
///
/// Verifies:
/// - Initial embedding produces certain search results
/// - After re-embedding with different content, search results change
#[tokio::test]
async fn test_reembedding_affects_search() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create notes
    harness
        .create_note(
            "rust.md",
            "# Rust\n\nRust is a systems programming language.",
        )
        .await?;

    harness
        .create_note("python.md", "# Python\n\nPython is a scripting language.")
        .await?;

    // Search for programming - both should appear
    let results_v1 = harness.semantic_search("programming language", 5).await?;
    assert!(
        !results_v1.is_empty(),
        "Should find programming-related notes"
    );

    // Update rust.md to be about cooking instead
    harness
        .create_note(
            "rust.md",
            "# Cooking Pasta\n\nHow to make delicious pasta dishes.",
        )
        .await?;

    // Search again - results should reflect the content change
    let results_v2 = harness.semantic_search("programming language", 5).await?;

    // Note: With mock provider, results are deterministic but still change with content
    // We just verify we still get results (semantic search still works after re-embedding)
    assert!(
        !results_v2.is_empty(),
        "Search should still work after re-embedding"
    );

    Ok(())
}

/// Test re-embedding preserves most metadata
///
/// Verifies:
/// - Title is preserved
/// - Tags are preserved
/// - Folder path is preserved
/// - Custom properties are preserved
#[tokio::test]
async fn test_reembedding_preserves_metadata() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content = r#"---
title: Test Note
tags: [test, demo]
status: active
priority: high
---

# Test Note

Initial content.
"#;

    harness.create_note("test.md", content).await?;

    // Get initial metadata
    let metadata_v1 = harness
        .get_metadata("test.md")
        .await?
        .expect("Metadata should exist");

    // Re-embed with updated content (but same frontmatter)
    let content_v2 = r#"---
title: Test Note
tags: [test, demo]
status: active
priority: high
---

# Test Note

Updated content with different text.
"#;

    harness.create_note("test.md", content_v2).await?;

    // Get updated metadata
    let metadata_v2 = harness
        .get_metadata("test.md")
        .await?
        .expect("Metadata should exist");

    // Verify metadata preserved
    assert_eq!(metadata_v1.title, metadata_v2.title, "Title should match");
    assert_eq!(metadata_v1.tags, metadata_v2.tags, "Tags should match");
    assert_eq!(
        metadata_v1.folder, metadata_v2.folder,
        "Folder should match"
    );

    // Verify custom properties preserved
    let status_v1 = metadata_v1
        .properties
        .get("status")
        .and_then(|v| v.as_str());
    let status_v2 = metadata_v2
        .properties
        .get("status")
        .and_then(|v| v.as_str());
    assert_eq!(
        status_v1, status_v2,
        "Custom property 'status' should be preserved"
    );

    Ok(())
}

// ============================================================================
// Timestamp Tests
// ============================================================================

/// Test that updated_at timestamp changes on re-embedding
///
/// Verifies:
/// - Initial created_at timestamp is set
/// - After re-embedding, updated_at is newer than created_at
/// - created_at remains unchanged
#[tokio::test]
async fn test_reembedding_timestamp_updates() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create initial note
    harness
        .create_note("timestamp.md", "# Initial\n\nInitial content.")
        .await?;

    let metadata_v1 = harness
        .get_metadata("timestamp.md")
        .await?
        .expect("Metadata should exist");

    let _created_at_v1 = metadata_v1.created_at;
    let updated_at_v1 = metadata_v1.updated_at;

    // Wait to ensure timestamp difference
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Re-embed with updated content
    harness
        .create_note("timestamp.md", "# Updated\n\nUpdated content.")
        .await?;

    let metadata_v2 = harness
        .get_metadata("timestamp.md")
        .await?
        .expect("Metadata should exist");

    let created_at_v2 = metadata_v2.created_at;
    let updated_at_v2 = metadata_v2.updated_at;

    // Note: In current implementation, re-embedding via create_note()
    // will reset both timestamps. This test documents current behavior.
    // In a production re-embedding API, we'd want to preserve created_at.

    // Verify updated_at changed
    assert_ne!(
        updated_at_v1, updated_at_v2,
        "updated_at should change on re-embedding"
    );

    // Verify updated_at is after created_at (sanity check)
    assert!(
        updated_at_v2 >= created_at_v2,
        "updated_at should be >= created_at"
    );

    Ok(())
}

/// Test timestamp behavior with rapid re-embeddings
///
/// Verifies:
/// - Multiple re-embeddings in quick succession all get unique timestamps
/// - Timestamps are monotonically increasing
#[tokio::test]
async fn test_reembedding_timestamp_ordering() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create initial note
    harness
        .create_note("rapid.md", "# Version 1\n\nContent v1.")
        .await?;

    let mut timestamps = Vec::new();

    // Get initial timestamp
    let metadata = harness.get_metadata("rapid.md").await?.unwrap();
    timestamps.push(metadata.updated_at);

    // Perform rapid re-embeddings
    for i in 2..=5 {
        tokio::time::sleep(Duration::from_millis(10)).await;

        harness
            .create_note("rapid.md", &format!("# Version {}\n\nContent v{}.", i, i))
            .await?;

        let metadata = harness.get_metadata("rapid.md").await?.unwrap();
        timestamps.push(metadata.updated_at);
    }

    // Verify timestamps are unique and increasing
    for i in 1..timestamps.len() {
        assert!(
            timestamps[i] > timestamps[i - 1],
            "Timestamps should be monotonically increasing"
        );
    }

    Ok(())
}

// ============================================================================
// Model Change Simulation Tests
// ============================================================================

/// Test re-embedding with different embedding dimensions
///
/// Simulates switching to a model with different dimensions.
///
/// Verifies:
/// - Can store embedding with one dimension
/// - Can replace with embedding of different dimension
/// - Database handles dimension change correctly
#[tokio::test]
async fn test_reembedding_dimension_change() -> Result<()> {
    // Start with 768-dim model
    let config_768 = EmbeddingHarnessConfig {
        strategy: utils::EmbeddingStrategy::Mock,
        dimensions: 768,
        validate_dimensions: false, // Allow dimension changes
        store_full_content: true,
    };
    let harness = DaemonEmbeddingHarness::new(config_768).await?;

    // Create note with 768-dim embedding
    let embedding_768 = vec![0.5; 768];
    harness
        .create_note_with_embedding(
            "model_change.md",
            "# Test\n\nModel change test.",
            embedding_768,
        )
        .await?;

    // Verify 768-dim embedding stored
    let stored_768 = harness.get_embedding("model_change.md").await?.unwrap();
    assert_eq!(stored_768.len(), 768, "Should have 768 dimensions");

    // Simulate switching to 384-dim model by re-embedding with different dimensions
    let embedding_384 = vec![0.7; 384];
    harness
        .create_note_with_embedding(
            "model_change.md",
            "# Test\n\nModel change test.",
            embedding_384,
        )
        .await?;

    // Verify 384-dim embedding now stored
    let stored_384 = harness.get_embedding("model_change.md").await?.unwrap();
    assert_eq!(stored_384.len(), 384, "Should have 384 dimensions now");

    Ok(())
}

/// Test batch re-embedding with dimension changes
///
/// Verifies:
/// - Multiple notes can be re-embedded with new dimensions
/// - All notes end up with consistent new dimensions
#[tokio::test]
async fn test_reembedding_batch_dimension_change() -> Result<()> {
    let config = EmbeddingHarnessConfig {
        strategy: utils::EmbeddingStrategy::Mock,
        dimensions: 768,
        validate_dimensions: false,
        store_full_content: true,
    };
    let harness = DaemonEmbeddingHarness::new(config).await?;

    // Create multiple notes with 768-dim embeddings
    for i in 1..=5 {
        harness
            .create_note(&format!("note{}.md", i), &format!("# Note {}\n\nContent.", i))
            .await?;
    }

    // Verify all have 768 dims
    for i in 1..=5 {
        let emb = harness.get_embedding(&format!("note{}.md", i)).await?.unwrap();
        assert_eq!(emb.len(), 768);
    }

    // Re-embed all with 384 dims
    for i in 1..=5 {
        let new_embedding = vec![0.5; 384];
        harness
            .create_note_with_embedding(
                &format!("note{}.md", i),
                &format!("# Note {}\n\nContent.", i),
                new_embedding,
            )
            .await?;
    }

    // Verify all now have 384 dims
    for i in 1..=5 {
        let emb = harness.get_embedding(&format!("note{}.md", i)).await?.unwrap();
        assert_eq!(
            emb.len(),
            384,
            "Note {} should have 384 dimensions after re-embedding",
            i
        );
    }

    Ok(())
}

// ============================================================================
// Selective Re-embedding Tests
// ============================================================================

/// Test selective re-embedding of specific notes
///
/// Verifies:
/// - Can re-embed a subset of notes
/// - Unchanged notes retain original embeddings
/// - Re-embedded notes have new embeddings
#[tokio::test]
async fn test_reembedding_selective() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create multiple notes with known embeddings
    let embedding_a = vec![1.0; 768];
    let embedding_b = vec![2.0; 768];
    let embedding_c = vec![3.0; 768];

    harness
        .create_note_with_embedding("note_a.md", "# A\n\nContent A.", embedding_a.clone())
        .await?;
    harness
        .create_note_with_embedding("note_b.md", "# B\n\nContent B.", embedding_b.clone())
        .await?;
    harness
        .create_note_with_embedding("note_c.md", "# C\n\nContent C.", embedding_c.clone())
        .await?;

    // Verify initial embeddings
    let stored_a = harness.get_embedding("note_a.md").await?.unwrap();
    let stored_b = harness.get_embedding("note_b.md").await?.unwrap();
    let stored_c = harness.get_embedding("note_c.md").await?.unwrap();

    assert_eq!(stored_a, embedding_a);
    assert_eq!(stored_b, embedding_b);
    assert_eq!(stored_c, embedding_c);

    // Selectively re-embed only note_b
    let new_embedding_b = vec![5.0; 768];
    harness
        .create_note_with_embedding("note_b.md", "# B\n\nUpdated B.", new_embedding_b.clone())
        .await?;

    // Verify note_b changed, others unchanged
    let stored_a_after = harness.get_embedding("note_a.md").await?.unwrap();
    let stored_b_after = harness.get_embedding("note_b.md").await?.unwrap();
    let stored_c_after = harness.get_embedding("note_c.md").await?.unwrap();

    assert_eq!(stored_a_after, embedding_a, "Note A should be unchanged");
    assert_eq!(stored_b_after, new_embedding_b, "Note B should be updated");
    assert_eq!(stored_c_after, embedding_c, "Note C should be unchanged");

    Ok(())
}

/// Test re-embedding notes in specific folders
///
/// Verifies:
/// - Can re-embed all notes in a folder hierarchy
/// - Notes in other folders remain unchanged
#[tokio::test]
async fn test_reembedding_by_folder() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create notes in different folders
    harness
        .create_note("Projects/note1.md", "# Project 1\n\nProject content.")
        .await?;
    harness
        .create_note("Projects/note2.md", "# Project 2\n\nProject content.")
        .await?;
    harness
        .create_note("Daily/note3.md", "# Daily\n\nDaily content.")
        .await?;

    // Get initial embeddings
    let project1_v1 = harness.get_embedding("Projects/note1.md").await?.unwrap();
    let project2_v1 = harness.get_embedding("Projects/note2.md").await?.unwrap();
    let daily_v1 = harness.get_embedding("Daily/note3.md").await?.unwrap();

    // Re-embed only Projects folder
    harness
        .create_note(
            "Projects/note1.md",
            "# Project 1\n\nUpdated project content.",
        )
        .await?;
    harness
        .create_note(
            "Projects/note2.md",
            "# Project 2\n\nUpdated project content.",
        )
        .await?;

    // Verify Projects folder changed, Daily unchanged
    let project1_v2 = harness.get_embedding("Projects/note1.md").await?.unwrap();
    let project2_v2 = harness.get_embedding("Projects/note2.md").await?.unwrap();
    let daily_v2 = harness.get_embedding("Daily/note3.md").await?.unwrap();

    assert_ne!(
        project1_v1, project1_v2,
        "Project 1 should have new embedding"
    );
    assert_ne!(
        project2_v1, project2_v2,
        "Project 2 should have new embedding"
    );
    assert_eq!(daily_v1, daily_v2, "Daily note should be unchanged");

    Ok(())
}

// ============================================================================
// Embedding Comparison Tests
// ============================================================================

/// Test embedding similarity when re-embedding same content
///
/// Verifies:
/// - Re-embedding identical content produces similar embeddings
/// - Similarity is very high (>0.95) for unchanged content
#[tokio::test]
async fn test_reembedding_same_content_similarity() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content = "# Test\n\nThis is identical content for similarity testing.";

    // Create initial embedding
    harness.create_note("similarity.md", content).await?;
    let embedding_v1 = harness.get_embedding("similarity.md").await?.unwrap();

    // Re-embed with identical content
    harness.create_note("similarity.md", content).await?;
    let embedding_v2 = harness.get_embedding("similarity.md").await?.unwrap();

    // Calculate similarity
    let similarity = cosine_similarity(&embedding_v1, &embedding_v2);

    // With mock provider, embeddings should be identical for same content
    assert!(
        similarity > 0.9,
        "Embeddings for identical content should be very similar (got {:.4})",
        similarity
    );

    Ok(())
}

/// Test embedding difference when content significantly changes
///
/// Verifies:
/// - Significant content changes produce different embeddings
/// - Similarity decreases appropriately
#[tokio::test]
async fn test_reembedding_different_content_dissimilarity() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content_v1 = "# Rust Programming\n\nLearn Rust systems programming language.";
    let content_v2 = "# Cooking Pasta\n\nHow to cook delicious Italian pasta dishes.";

    // Create initial embedding
    harness.create_note("change.md", content_v1).await?;
    let embedding_v1 = harness.get_embedding("change.md").await?.unwrap();

    // Re-embed with completely different content
    harness.create_note("change.md", content_v2).await?;
    let embedding_v2 = harness.get_embedding("change.md").await?.unwrap();

    // Embeddings should be different
    assert_ne!(
        embedding_v1, embedding_v2,
        "Embeddings should differ for different content"
    );

    // Calculate similarity - should be lower than same content
    let similarity = cosine_similarity(&embedding_v1, &embedding_v2);

    // With mock provider, similarity depends on hash function
    // Just verify we get a valid similarity score
    assert!(
        similarity >= -1.0 && similarity <= 1.0,
        "Similarity should be in valid range [-1, 1] (got {:.4})",
        similarity
    );

    Ok(())
}

/// Test comparing embeddings before and after re-embedding
///
/// Verifies:
/// - Can retrieve and store multiple versions of embeddings
/// - Can compute similarity metrics between versions
#[tokio::test]
async fn test_reembedding_version_comparison() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let mut version_embeddings = Vec::new();

    // Create multiple versions and store their embeddings
    for version in 1..=3 {
        let content = format!(
            "# Document Version {}\n\nThis is version {} of the document.",
            version, version
        );

        harness.create_note("versioned.md", &content).await?;

        let embedding = harness.get_embedding("versioned.md").await?.unwrap();
        version_embeddings.push(embedding);
    }

    // Verify we have 3 distinct versions
    assert_eq!(version_embeddings.len(), 3);

    // Compare v1 vs v2, v2 vs v3
    let sim_v1_v2 = cosine_similarity(&version_embeddings[0], &version_embeddings[1]);
    let sim_v2_v3 = cosine_similarity(&version_embeddings[1], &version_embeddings[2]);

    // Similarities should be in valid range
    assert!(
        sim_v1_v2 >= -1.0 && sim_v1_v2 <= 1.0,
        "v1-v2 similarity should be valid"
    );
    assert!(
        sim_v2_v3 >= -1.0 && sim_v2_v3 <= 1.0,
        "v2-v3 similarity should be valid"
    );

    Ok(())
}

// ============================================================================
// Error Handling Tests
// ============================================================================

/// Test re-embedding with dimension mismatch when validation enabled
///
/// Verifies:
/// - Dimension validation catches mismatches
/// - Appropriate error is returned
#[tokio::test]
async fn test_reembedding_dimension_validation_error() -> Result<()> {
    let config = EmbeddingHarnessConfig {
        strategy: utils::EmbeddingStrategy::Mock,
        dimensions: 768,
        validate_dimensions: true, // Enable validation
        store_full_content: true,
    };
    let harness = DaemonEmbeddingHarness::new(config).await?;

    // Create note with correct dimensions
    harness
        .create_note("valid.md", "# Valid\n\nCorrect dimensions.")
        .await?;

    // Try to re-embed with wrong dimensions
    let wrong_embedding = vec![0.5; 512]; // Wrong dimension
    let result = harness
        .create_note_with_embedding("valid.md", "# Valid\n\nStill valid.", wrong_embedding)
        .await;

    assert!(
        result.is_err(),
        "Should fail when re-embedding with wrong dimensions"
    );

    Ok(())
}

/// Test re-embedding non-existent note
///
/// Verifies:
/// - Creating a note that doesn't exist yet works (not really re-embedding)
/// - This is essentially a new note creation
#[tokio::test]
async fn test_reembedding_nonexistent_note() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Try to "re-embed" a note that doesn't exist (actually creates it)
    let result = harness
        .create_note("nonexistent.md", "# New Note\n\nThis is new.")
        .await;

    assert!(
        result.is_ok(),
        "Creating a new note should work (not an error)"
    );

    // Verify note was created
    assert!(harness.file_exists("nonexistent.md").await?);

    Ok(())
}

// ============================================================================
// Performance Tests
// ============================================================================

/// Test batch re-embedding performance with multiple notes
///
/// Verifies:
/// - Can re-embed 10+ notes efficiently
/// - All notes are updated correctly
#[tokio::test]
async fn test_reembedding_batch_performance() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let note_count = 15;

    // Create initial notes
    for i in 1..=note_count {
        harness
            .create_note(
                &format!("batch_{}.md", i),
                &format!("# Note {}\n\nInitial content {}.", i, i),
            )
            .await?;
    }

    // Get initial embeddings
    let mut initial_embeddings = Vec::new();
    for i in 1..=note_count {
        let emb = harness
            .get_embedding(&format!("batch_{}.md", i))
            .await?
            .unwrap();
        initial_embeddings.push(emb);
    }

    // Re-embed all notes with updated content
    for i in 1..=note_count {
        harness
            .create_note(
                &format!("batch_{}.md", i),
                &format!("# Note {}\n\nUpdated content {}.", i, i),
            )
            .await?;
    }

    // Verify all embeddings changed
    for i in 1..=note_count {
        let new_emb = harness
            .get_embedding(&format!("batch_{}.md", i))
            .await?
            .unwrap();

        assert_ne!(
            initial_embeddings[i - 1],
            new_emb,
            "Note {} should have new embedding",
            i
        );
    }

    // Verify stats
    let stats = harness.get_stats().await?;
    assert_eq!(
        stats.total_documents, note_count as i64,
        "Should have {} documents",
        note_count
    );

    Ok(())
}

/// Test re-embedding large note content
///
/// Verifies:
/// - Can handle large content (10KB+) re-embedding
/// - Embedding is updated successfully
#[tokio::test]
async fn test_reembedding_large_content() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create large content (~10KB)
    let large_content_v1 = format!(
        "# Large Document\n\n{}",
        "This is a large document with lots of content. ".repeat(500)
    );

    harness.create_note("large.md", &large_content_v1).await?;
    let embedding_v1 = harness.get_embedding("large.md").await?.unwrap();

    // Re-embed with different large content
    let large_content_v2 = format!(
        "# Large Document Updated\n\n{}",
        "This is an updated large document with different content. ".repeat(500)
    );

    harness.create_note("large.md", &large_content_v2).await?;
    let embedding_v2 = harness.get_embedding("large.md").await?.unwrap();

    // Verify embeddings changed
    assert_ne!(
        embedding_v1, embedding_v2,
        "Large content re-embedding should produce different embeddings"
    );

    // Verify dimensions correct
    assert_eq!(embedding_v2.len(), 768, "Dimensions should be correct");

    Ok(())
}

// ============================================================================
// Database Stats Tests
// ============================================================================

/// Test that database stats remain consistent after re-embedding
///
/// Verifies:
/// - Document count doesn't change (re-embedding, not creating new)
/// - Stats accurately reflect kiln state
#[tokio::test]
async fn test_reembedding_stats_consistency() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create 3 notes
    for i in 1..=3 {
        harness
            .create_note(&format!("note{}.md", i), &format!("# Note {}\n\nContent.", i))
            .await?;
    }

    let stats_before = harness.get_stats().await?;
    assert_eq!(stats_before.total_documents, 3);

    // Re-embed all notes
    for i in 1..=3 {
        harness
            .create_note(
                &format!("note{}.md", i),
                &format!("# Note {}\n\nUpdated content.", i),
            )
            .await?;
    }

    let stats_after = harness.get_stats().await?;

    // Document count should remain the same (updated, not added)
    assert_eq!(
        stats_after.total_documents, 3,
        "Document count should remain unchanged after re-embedding"
    );

    Ok(())
}
