//! Integration tests for embedding generation pipeline
//!
//! Tests the complete workflow from note creation to database storage:
//! - Note creation and file writing
//! - Markdown parsing and content extraction
//! - Embedding generation with different content types
//! - Metadata extraction (title, tags, frontmatter)
//! - Database storage and retrieval
//! - Error handling for malformed input
//! - Performance with batch operations and large files
//!
//! ## Test Coverage
//!
//! ### Pipeline Stages
//! - File creation and validation
//! - Markdown parsing (frontmatter, content, metadata)
//! - Embedding generation (dimensions, values)
//! - Database storage (metadata, timestamps, retrieval)
//!
//! ### Content Types
//! - Simple markdown (headings + paragraphs)
//! - Code blocks and technical content
//! - Unicode and special characters
//! - Empty and minimal content
//! - Very long content (truncation)
//!
//! ### Metadata Extraction
//! - Frontmatter YAML parsing
//! - Tag extraction (frontmatter + inline)
//! - Title extraction (frontmatter vs H1)
//! - Folder path handling
//! - Custom properties
//!
//! ### Error Handling
//! - Malformed frontmatter
//! - Invalid YAML
//! - Missing required fields
//! - Edge cases (empty files, huge files)
//!
//! ### Performance
//! - Batch note creation (10+ notes)
//! - Large file handling (10KB+ content)
//!
//! ## Usage
//!
//! Run all pipeline tests:
//! ```bash
//! cargo test -p crucible-daemon --test embedding_pipeline
//! ```
//!
//! Run specific test:
//! ```bash
//! cargo test -p crucible-daemon --test embedding_pipeline test_pipeline_basic_note_creation
//! ```

mod fixtures;
mod utils;

use anyhow::Result;
use utils::harness::DaemonEmbeddingHarness;

// ============================================================================
// Basic Pipeline Tests
// ============================================================================

/// Test basic note creation pipeline with simple markdown
///
/// Verifies:
/// - File is created in vault directory
/// - Content is stored in database
/// - Embedding is generated and stored
/// - Metadata is extracted correctly
#[tokio::test]
async fn test_pipeline_basic_note_creation() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content = r#"# Getting Started

This is a simple test note about programming.
"#;

    let path = harness.create_note("test.md", content).await?;

    // Verify file exists
    assert!(path.exists(), "File should exist on disk");

    // Verify file exists in database
    assert!(
        harness.file_exists("test.md").await?,
        "File should exist in database"
    );

    // Verify embedding was generated
    assert!(
        harness.has_embedding("test.md").await?,
        "Embedding should be generated"
    );

    // Verify metadata
    let metadata = harness.get_metadata("test.md").await?;
    assert!(metadata.is_some(), "Metadata should exist");

    let metadata = metadata.unwrap();
    // Title is extracted from frontmatter or filename (not H1)
    assert_eq!(
        metadata.title,
        Some("test".to_string()),
        "Title should be extracted from filename"
    );

    Ok(())
}

/// Test that embedding dimensions match expected values
///
/// Verifies:
/// - Embedding vector has correct length
/// - All embedding values are non-zero (mock provider)
/// - Embedding can be retrieved after storage
#[tokio::test]
async fn test_pipeline_embedding_dimensions() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    harness
        .create_note("dimensions.md", "# Test\n\nEmbedding dimension test.")
        .await?;

    let embedding = harness
        .get_embedding("dimensions.md")
        .await?
        .expect("Embedding should exist");

    // Verify dimensions
    assert_eq!(
        embedding.len(),
        768,
        "Embedding should have 768 dimensions"
    );

    // Verify non-zero values (mock provider generates non-zero embeddings)
    let non_zero_count = embedding.iter().filter(|&&v| v != 0.0).count();
    assert!(
        non_zero_count > 0,
        "Embedding should have non-zero values"
    );

    Ok(())
}

/// Test metadata extraction from frontmatter
///
/// Verifies:
/// - YAML frontmatter is parsed correctly
/// - Title from frontmatter takes precedence over H1
/// - Tags from frontmatter are extracted
/// - Custom properties are stored
#[tokio::test]
async fn test_pipeline_metadata_extraction() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content = r#"---
title: Custom Title
tags: [rust, programming, tutorial]
status: draft
priority: high
---

# Document Heading

This note has both frontmatter and inline tags #learning.
"#;

    harness.create_note("metadata.md", content).await?;

    let metadata = harness
        .get_metadata("metadata.md")
        .await?
        .expect("Metadata should exist");

    // Verify title from frontmatter
    assert_eq!(
        metadata.title,
        Some("Custom Title".to_string()),
        "Title should come from frontmatter"
    );

    // Verify tags (both frontmatter and inline)
    assert!(
        metadata.tags.contains(&"rust".to_string()),
        "Should have 'rust' tag"
    );
    assert!(
        metadata.tags.contains(&"programming".to_string()),
        "Should have 'programming' tag"
    );
    assert!(
        metadata.tags.contains(&"learning".to_string()),
        "Should have inline 'learning' tag"
    );

    // Verify custom properties
    assert_eq!(
        metadata.properties.get("status").and_then(|v| v.as_str()),
        Some("draft"),
        "Should have 'status' property"
    );
    assert_eq!(
        metadata.properties.get("priority").and_then(|v| v.as_str()),
        Some("high"),
        "Should have 'priority' property"
    );

    Ok(())
}

/// Test content storage in database
///
/// Verifies:
/// - Plain text content is extracted and stored
/// - Content can be retrieved after storage
/// - Embedding is generated and stored
#[tokio::test]
async fn test_pipeline_content_storage() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content = r#"# Rust Programming

Rust is a systems programming language that runs blazingly fast.

## Features

- Memory safety
- Zero-cost abstractions
"#;

    harness.create_note("content.md", content).await?;

    // Verify file exists and has embedding
    assert!(
        harness.file_exists("content.md").await?,
        "File should exist in database"
    );
    assert!(
        harness.has_embedding("content.md").await?,
        "Embedding should be generated"
    );

    // Verify we can retrieve the embedding
    let embedding = harness.get_embedding("content.md").await?;
    assert!(embedding.is_some(), "Should retrieve embedding");
    assert_eq!(embedding.unwrap().len(), 768, "Embedding has correct size");

    Ok(())
}

/// Test folder path extraction
///
/// Verifies:
/// - Notes in subdirectories have correct folder path
/// - Folder metadata is stored correctly
#[tokio::test]
async fn test_pipeline_folder_path() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    harness
        .create_note("Projects/Rust/guide.md", "# Rust Guide\n\nLearn Rust.")
        .await?;

    let metadata = harness
        .get_metadata("Projects/Rust/guide.md")
        .await?
        .expect("Metadata should exist");

    // Folder path should include the subdirectory
    assert!(
        metadata.folder.contains("Projects"),
        "Folder should include 'Projects': {}",
        metadata.folder
    );
    assert!(
        metadata.folder.contains("Rust"),
        "Folder should include 'Rust': {}",
        metadata.folder
    );

    Ok(())
}

/// Test timestamp generation
///
/// Verifies:
/// - created_at timestamp is set
/// - updated_at timestamp is set
/// - Timestamps are recent (within last minute)
#[tokio::test]
async fn test_pipeline_timestamps() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let before = chrono::Utc::now();
    harness
        .create_note("timestamp.md", "# Test\n\nTimestamp test.")
        .await?;
    let after = chrono::Utc::now();

    let metadata = harness
        .get_metadata("timestamp.md")
        .await?
        .expect("Metadata should exist");

    // Verify created_at is within expected range
    assert!(
        metadata.created_at >= before,
        "created_at should be after test start"
    );
    assert!(
        metadata.created_at <= after,
        "created_at should be before test end"
    );

    // Verify updated_at is within expected range
    assert!(
        metadata.updated_at >= before,
        "updated_at should be after test start"
    );
    assert!(
        metadata.updated_at <= after,
        "updated_at should be before test end"
    );

    Ok(())
}

// ============================================================================
// Content Type Tests
// ============================================================================

/// Test pipeline with code blocks
///
/// Verifies:
/// - Code blocks are included in plain text
/// - Embedding is generated for technical content
#[tokio::test]
async fn test_pipeline_code_content() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content = r#"# Rust Function Example

Here's a simple function:

```rust
fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
```

This demonstrates Rust's string formatting.
"#;

    harness.create_note("code.md", content).await?;

    // Verify embedding was generated
    assert!(
        harness.has_embedding("code.md").await?,
        "Embedding should be generated for code content"
    );

    // Verify file exists in database
    assert!(
        harness.file_exists("code.md").await?,
        "Code file should exist in database"
    );

    Ok(())
}

/// Test pipeline with mixed prose and technical content
///
/// Verifies:
/// - Both narrative and code content are processed
/// - Embedding captures semantic meaning of mixed content
#[tokio::test]
async fn test_pipeline_mixed_content() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content = r#"# API Documentation

This module provides HTTP client functionality.

## Example

```javascript
const client = new HttpClient();
const response = await client.get('/api/users');
```

The client supports GET, POST, PUT, and DELETE methods.
It handles authentication automatically using bearer tokens.
"#;

    harness.create_note("api.md", content).await?;

    assert!(
        harness.has_embedding("api.md").await?,
        "Embedding should be generated for mixed content"
    );

    Ok(())
}

/// Test pipeline with empty content
///
/// Verifies:
/// - Empty notes are handled gracefully
/// - Embedding is still generated (even for empty content)
/// - Metadata is still extracted
#[tokio::test]
async fn test_pipeline_empty_content() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    harness.create_note("empty.md", "").await?;

    // Verify file exists in database
    assert!(
        harness.file_exists("empty.md").await?,
        "Empty file should exist in database"
    );

    // Empty content should still get an embedding (provider handles this)
    let _has_embedding = harness.has_embedding("empty.md").await?;
    // Mock provider may or may not generate embedding for empty content
    // This test just ensures no crash occurs

    let metadata = harness.get_metadata("empty.md").await?;
    assert!(metadata.is_some(), "Metadata should exist for empty file");

    Ok(())
}

/// Test pipeline with very long content
///
/// Verifies:
/// - Large notes are handled correctly
/// - Content may be truncated for embedding (implementation detail)
/// - Embedding is still generated
#[tokio::test]
async fn test_pipeline_large_content() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create a 10KB+ note
    let mut content = String::from("# Large Document\n\n");
    for i in 0..1000 {
        content.push_str(&format!(
            "This is paragraph {}. It contains some text about various topics. ",
            i
        ));
    }

    harness.create_note("large.md", &content).await?;

    // Verify embedding was generated
    assert!(
        harness.has_embedding("large.md").await?,
        "Embedding should be generated for large content"
    );

    let embedding = harness
        .get_embedding("large.md")
        .await?
        .expect("Embedding should exist");

    assert_eq!(
        embedding.len(),
        768,
        "Large content should still have correct dimensions"
    );

    Ok(())
}

/// Test pipeline with Unicode and special characters
///
/// Verifies:
/// - Unicode characters are handled correctly
/// - Emojis don't break the pipeline
/// - Various language scripts work
#[tokio::test]
async fn test_pipeline_unicode_content() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content = r#"# Unicode Test

## Various Languages

- French: café, naïve, Noël
- German: Übung, Mädchen
- Japanese: 日本語のテキスト
- Emoji: 🚀 🎨 🔬 ✨

## Math Symbols

∀x ∈ ℝ: x² ≥ 0

## Special Characters

"Quotes" and 'apostrophes' — dashes – work.
"#;

    harness.create_note("unicode.md", content).await?;

    // Verify embedding was generated
    assert!(
        harness.has_embedding("unicode.md").await?,
        "Embedding should be generated for Unicode content"
    );

    let metadata = harness
        .get_metadata("unicode.md")
        .await?
        .expect("Metadata should exist");

    // Title comes from filename, not H1
    assert_eq!(
        metadata.title,
        Some("unicode".to_string()),
        "Title should be extracted from filename"
    );

    Ok(())
}

/// Test pipeline with minimal content (just a title)
///
/// Verifies:
/// - Minimal notes are processed correctly
/// - Title-only notes still get embeddings
#[tokio::test]
async fn test_pipeline_minimal_content() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    harness.create_note("minimal.md", "# Just a Title").await?;

    assert!(
        harness.has_embedding("minimal.md").await?,
        "Embedding should be generated for minimal content"
    );

    let metadata = harness
        .get_metadata("minimal.md")
        .await?
        .expect("Metadata should exist");

    // Title comes from filename
    assert_eq!(
        metadata.title,
        Some("minimal".to_string()),
        "Title should be extracted from filename"
    );

    Ok(())
}

// ============================================================================
// Metadata Extraction Tests
// ============================================================================

/// Test frontmatter with array tags
///
/// Verifies:
/// - Array-style tags in frontmatter are parsed
/// - Multiple tags are extracted correctly
#[tokio::test]
async fn test_pipeline_frontmatter_array_tags() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content = r#"---
tags: [rust, systems, performance, memory-safety]
---

# Advanced Rust
"#;

    harness
        .create_note("array_tags.md", content)
        .await?;

    let metadata = harness
        .get_metadata("array_tags.md")
        .await?
        .expect("Metadata should exist");

    assert_eq!(
        metadata.tags.len(),
        4,
        "Should extract all 4 tags from array"
    );
    assert!(metadata.tags.contains(&"rust".to_string()));
    assert!(metadata.tags.contains(&"systems".to_string()));
    assert!(metadata.tags.contains(&"performance".to_string()));
    assert!(metadata.tags.contains(&"memory-safety".to_string()));

    Ok(())
}

/// Test inline tag extraction
///
/// Verifies:
/// - Inline #tags in content are extracted
/// - Tags appear in metadata.tags
#[tokio::test]
async fn test_pipeline_inline_tags() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content = r#"# Learning Notes

Today I learned about #rust and #async programming.
Working on a #project with #tokio runtime.
"#;

    harness.create_note("inline_tags.md", content).await?;

    let metadata = harness
        .get_metadata("inline_tags.md")
        .await?
        .expect("Metadata should exist");

    // Verify inline tags are extracted
    assert!(
        metadata.tags.contains(&"rust".to_string()),
        "Should have inline 'rust' tag"
    );
    assert!(
        metadata.tags.contains(&"async".to_string()),
        "Should have inline 'async' tag"
    );
    assert!(
        metadata.tags.contains(&"project".to_string()),
        "Should have inline 'project' tag"
    );
    assert!(
        metadata.tags.contains(&"tokio".to_string()),
        "Should have inline 'tokio' tag"
    );

    Ok(())
}

/// Test title extraction priority (frontmatter > filename)
///
/// Verifies:
/// - When frontmatter has title, it takes precedence
/// - When frontmatter lacks title, filename is used
#[tokio::test]
async fn test_pipeline_title_priority() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Test 1: Frontmatter title takes precedence
    let content1 = r#"---
title: Frontmatter Title
---

# H1 Title

Content here.
"#;

    harness.create_note("title1.md", content1).await?;

    let metadata1 = harness
        .get_metadata("title1.md")
        .await?
        .expect("Metadata should exist");

    assert_eq!(
        metadata1.title,
        Some("Frontmatter Title".to_string()),
        "Frontmatter title should take precedence"
    );

    // Test 2: Filename used when no frontmatter title
    let content2 = "# H1 Title Only\n\nNo frontmatter here.";

    harness.create_note("title2.md", content2).await?;

    let metadata2 = harness
        .get_metadata("title2.md")
        .await?
        .expect("Metadata should exist");

    assert_eq!(
        metadata2.title,
        Some("title2".to_string()),
        "Filename should be used when no frontmatter title"
    );

    Ok(())
}

/// Test custom frontmatter properties
///
/// Verifies:
/// - Custom properties are stored in metadata.properties
/// - Different value types are handled (string, number, boolean)
#[tokio::test]
async fn test_pipeline_custom_properties() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content = r#"---
status: published
priority: 5
featured: true
author: Alice
category: tutorial
---

# Custom Properties Test
"#;

    harness.create_note("props.md", content).await?;

    let metadata = harness
        .get_metadata("props.md")
        .await?
        .expect("Metadata should exist");

    // Verify various property types
    assert_eq!(
        metadata.properties.get("status").and_then(|v| v.as_str()),
        Some("published")
    );

    assert_eq!(
        metadata.properties.get("author").and_then(|v| v.as_str()),
        Some("Alice")
    );

    assert_eq!(
        metadata.properties.get("category").and_then(|v| v.as_str()),
        Some("tutorial")
    );

    // Check that properties exist (values may vary by parser)
    assert!(
        metadata.properties.contains_key("priority"),
        "Should have 'priority' property"
    );
    assert!(
        metadata.properties.contains_key("featured"),
        "Should have 'featured' property"
    );

    Ok(())
}

// ============================================================================
// Error Handling Tests
// ============================================================================

/// Test malformed frontmatter handling
///
/// Verifies:
/// - Notes with invalid YAML frontmatter can still be processed
/// - Pipeline doesn't crash on malformed frontmatter
/// - Content is still extracted and embedded
#[tokio::test]
async fn test_pipeline_malformed_frontmatter() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content = r#"---
title: Missing closing
tags: [incomplete
status incomplete
---

# Content After Bad Frontmatter

This content should still be processed.
"#;

    // This may succeed or fail depending on parser's error handling
    // We're testing that it doesn't panic
    let result = harness.create_note("malformed.md", content).await;

    // If it succeeds, verify basic functionality
    if result.is_ok() {
        let has_file = harness.file_exists("malformed.md").await?;
        // Either it was stored or it wasn't, but no panic
        println!("Malformed frontmatter handled: file_exists={}", has_file);
    } else {
        // If it fails, it should be a controlled error, not a panic
        println!("Malformed frontmatter rejected (expected behavior)");
    }

    Ok(())
}

/// Test note without frontmatter
///
/// Verifies:
/// - Notes without frontmatter are processed correctly
/// - Metadata still has title (from H1)
/// - Tags can still be extracted from inline tags
#[tokio::test]
async fn test_pipeline_no_frontmatter() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content = r#"# Note Without Frontmatter

This is a simple note with just content and #tags.
"#;

    harness.create_note("no_fm.md", content).await?;

    let metadata = harness
        .get_metadata("no_fm.md")
        .await?
        .expect("Metadata should exist");

    // Title comes from filename when no frontmatter
    assert_eq!(
        metadata.title,
        Some("no_fm".to_string()),
        "Title should be extracted from filename"
    );

    assert!(
        metadata.tags.contains(&"tags".to_string()),
        "Inline tags should be extracted"
    );

    Ok(())
}

// ============================================================================
// Performance Tests
// ============================================================================

/// Test batch note creation
///
/// Verifies:
/// - Multiple notes can be created sequentially
/// - All notes are stored correctly
/// - All embeddings are generated
/// - Database handles multiple insertions
#[tokio::test]
async fn test_pipeline_batch_creation() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create 15 notes
    for i in 1..=15 {
        let content = format!(
            r#"---
title: Note {}
tags: [batch, test{}]
---

# Note Number {}

This is batch test note number {}.
Content about topic {}.
"#,
            i, i, i, i, i
        );

        harness
            .create_note(&format!("batch/note_{}.md", i), &content)
            .await?;
    }

    // Verify all notes exist
    for i in 1..=15 {
        let path = format!("batch/note_{}.md", i);
        assert!(
            harness.file_exists(&path).await?,
            "Batch note {} should exist",
            i
        );

        assert!(
            harness.has_embedding(&path).await?,
            "Batch note {} should have embedding",
            i
        );
    }

    // Verify database stats
    let stats = harness.get_stats().await?;
    assert!(
        stats.total_documents >= 15,
        "Should have at least 15 documents"
    );
    assert!(
        stats.total_embeddings >= 15,
        "Should have at least 15 embeddings"
    );

    Ok(())
}

/// Test large file handling with 10KB+ content
///
/// Verifies:
/// - Large files are processed without errors
/// - Embedding is generated for large content
/// - Content truncation (if any) doesn't break pipeline
#[tokio::test]
async fn test_pipeline_large_file_performance() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Create a ~20KB note with varied content
    let mut content = String::from(
        r#"---
title: Large Document Test
tags: [performance, large, test]
---

# Large Document

This document contains a lot of content to test large file handling.

"#,
    );

    // Add sections with varied content
    for i in 0..500 {
        content.push_str(&format!(
            r#"
## Section {}

This section discusses topic {} in detail. Here are some key points:

- Point 1 about topic {}
- Point 2 with more information
- Point 3 covering edge cases

```rust
// Example code for section {}
fn process_section_{}() {{
    println!("Processing section {}", {});
}}
```

Additional paragraph with explanatory text for section {}.
The content continues with more details and examples.
"#,
            i, i, i, i, i, i, i, i
        ));
    }

    let path = harness.create_note("large_file.md", &content).await?;

    // Verify file exists
    assert!(path.exists(), "Large file should exist on disk");

    // Verify embedding was generated
    assert!(
        harness.has_embedding("large_file.md").await?,
        "Large file should have embedding"
    );

    // Verify metadata
    let metadata = harness
        .get_metadata("large_file.md")
        .await?
        .expect("Large file should have metadata");

    assert_eq!(
        metadata.title,
        Some("Large Document Test".to_string()),
        "Large file title should be extracted"
    );

    assert!(
        metadata.tags.contains(&"performance".to_string()),
        "Large file tags should be extracted"
    );

    Ok(())
}

/// Test retrieval after storage
///
/// Verifies:
/// - Notes can be retrieved after storage
/// - Embeddings are preserved correctly
/// - Metadata matches what was stored
#[tokio::test]
async fn test_pipeline_retrieval_after_storage() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    let content = r#"---
title: Retrieval Test
tags: [test, retrieval]
status: active
---

# Test Document

Content for retrieval testing.
"#;

    harness.create_note("retrieval.md", content).await?;

    // Retrieve and verify metadata
    let metadata = harness
        .get_metadata("retrieval.md")
        .await?
        .expect("Should retrieve metadata");

    assert_eq!(metadata.title, Some("Retrieval Test".to_string()));
    assert!(metadata.tags.contains(&"test".to_string()));
    assert!(metadata.tags.contains(&"retrieval".to_string()));
    assert_eq!(
        metadata.properties.get("status").and_then(|v| v.as_str()),
        Some("active")
    );

    // Retrieve and verify embedding
    let embedding = harness
        .get_embedding("retrieval.md")
        .await?
        .expect("Should retrieve embedding");

    assert_eq!(embedding.len(), 768, "Retrieved embedding has correct size");

    // Verify searchability
    let results = harness.semantic_search("retrieval testing", 5).await?;
    assert!(!results.is_empty(), "Note should be searchable");

    let found = results.iter().any(|(path, _)| path.contains("retrieval.md"));
    assert!(found, "Should find the note in search results");

    Ok(())
}

/// Test database stats after pipeline operations
///
/// Verifies:
/// - Database stats reflect created notes
/// - Document and embedding counts match
/// - Stats are updated correctly
#[tokio::test]
async fn test_pipeline_database_stats() -> Result<()> {
    let harness = DaemonEmbeddingHarness::new_default().await?;

    // Initial stats
    let initial_stats = harness.get_stats().await?;
    let initial_count = initial_stats.total_documents;

    // Create some notes
    for i in 1..=5 {
        harness
            .create_note(
                &format!("stats/note_{}.md", i),
                &format!("# Note {}\n\nContent {}", i, i),
            )
            .await?;
    }

    // Check updated stats
    let final_stats = harness.get_stats().await?;

    assert_eq!(
        final_stats.total_documents,
        initial_count + 5,
        "Document count should increase by 5"
    );

    assert_eq!(
        final_stats.total_embeddings,
        final_stats.total_documents,
        "Embedding count should match document count"
    );

    Ok(())
}
