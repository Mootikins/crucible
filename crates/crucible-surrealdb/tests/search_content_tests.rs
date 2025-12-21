//! Phase 1A: Content-Based Search Tests
//!
//! These tests validate grep-based text search and semantic search functionality
//! using the docs kiln at `docs/`.
//!
//! Test Categories:
//! - Grep-based text search (case sensitivity, regex, scoping)
//! - Semantic search (vector similarity, reranking)

mod common;

use common::setup_test_db_with_kiln;

// ============================================================================
// TEST 1: Grep-Based Text Search - Basic Keyword Match
// ============================================================================

#[tokio::test]
async fn grep_basic_keyword() {
    // ARRANGE: Set up database with dev-kiln data
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // ACT: Search for "wikilinks" in note titles (from dev-kiln Help section)
    // Note: Current schema stores title in data.title field
    let sql = r#"
        SELECT type, data.title as title, data.path as path
        FROM entities
        WHERE type = 'note'
          AND string::lowercase(data.title) CONTAINS 'wikilinks'
        LIMIT 20
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    // ASSERT: Should find at least one document
    assert!(
        !result.records.is_empty(),
        "Expected to find documents with 'wikilinks' in title, found none"
    );

    // Verify at least one result contains the expected title
    let titles: Vec<String> = result
        .records
        .iter()
        .filter_map(|r| r.data.get("title").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect();

    assert!(
        titles.iter().any(|t| t.contains("Wikilinks")),
        "Expected to find 'Wikilinks' but got titles: {:?}",
        titles
    );
}

// ============================================================================
// TEST 2: Grep-Based Text Search - Multi-Word Phrase
// ============================================================================

#[tokio::test]
async fn grep_multi_word() {
    // ARRANGE: Set up database
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // ACT: Search for multi-word phrase "getting started" in title (from dev-kiln Guides)
    let sql = r#"
        SELECT type, data.title as title, data.path as path
        FROM entities
        WHERE type = 'note'
          AND string::lowercase(data.title) CONTAINS 'getting started'
        LIMIT 20
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    // ASSERT: Should find at least one document
    assert!(
        !result.records.is_empty(),
        "Expected to find documents containing 'getting started'"
    );

    let titles: Vec<String> = result
        .records
        .iter()
        .filter_map(|r| r.data.get("title").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect();

    assert!(
        titles.iter().any(|t| t.contains("Getting Started")),
        "Expected to find 'Getting Started' document, got: {:?}",
        titles
    );
}

// ============================================================================
// TEST 3: Grep-Based Text Search - Case Sensitivity Toggle
// ============================================================================

#[tokio::test]
async fn grep_case_sensitivity() {
    // ARRANGE: Set up database
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // ACT 1: Case-insensitive search (lowercase query)
    // Note: Searching in data.title since we don't have full-text content yet
    let sql_insensitive = r#"
        SELECT data.title as title, data.path as path
        FROM entities
        WHERE type = 'note'
          AND string::lowercase(data.title) CONTAINS 'para'
        LIMIT 20
    "#;

    let result_insensitive = client
        .query(sql_insensitive, &[])
        .await
        .expect("Case-insensitive query failed");

    // ACT 2: Case-sensitive search (exact case)
    let sql_sensitive = r#"
        SELECT data.title as title, data.path as path
        FROM entities
        WHERE type = 'note'
          AND data.title CONTAINS 'PARA'
        LIMIT 20
    "#;

    let result_sensitive = client
        .query(sql_sensitive, &[])
        .await
        .expect("Case-sensitive query failed");

    // ASSERT: Case-insensitive should find results
    assert!(
        !result_insensitive.records.is_empty(),
        "Case-insensitive search should find 'para' in titles"
    );

    // Case-sensitive should also find results (files with "PARA" in title)
    assert!(
        !result_sensitive.records.is_empty(),
        "Case-sensitive search should find 'PARA' in titles"
    );

    // Verify we can control case sensitivity
    let insensitive_count = result_insensitive.records.len();
    let sensitive_count = result_sensitive.records.len();

    // Case-insensitive should find at least as many results as case-sensitive
    assert!(
        insensitive_count >= sensitive_count,
        "Case-insensitive ({}) should find >= case-sensitive ({}) results",
        insensitive_count,
        sensitive_count
    );
}

// ============================================================================
// TEST 4: Grep-Based Text Search - Folder-Scoped Search
// ============================================================================

#[tokio::test]
async fn grep_folder_scoped() {
    // ARRANGE: Set up database
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // ACT: Search for notes in Help folder (scoped to dev-kiln path)
    // Note: Using data.title for searching since full-text content isn't indexed yet
    let sql = r#"
        SELECT data.title as title, data.path as path
        FROM entities
        WHERE type = 'note'
          AND string::contains(data.path, 'Help')
          AND string::lowercase(data.title) CONTAINS 'frontmatter'
        LIMIT 20
    "#;

    let result = client.query(sql, &[]).await.expect("Query failed");

    // ASSERT: Should find scoped results
    assert!(
        !result.records.is_empty(),
        "Expected to find documents in Help folder with 'frontmatter' in title"
    );

    // Verify all results are actually in the Help path
    for record in &result.records {
        let path = record
            .data
            .get("path")
            .and_then(|v| v.as_str())
            .expect("Path should exist");

        assert!(
            path.contains("Help"),
            "Result path should be in Help folder scope: {}",
            path
        );
    }
}

// ============================================================================
// TEST 5: Grep-Based Text Search - Code Blocks
// ============================================================================

#[tokio::test]
async fn grep_code_blocks() {
    // ARRANGE: Set up database
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // ACT: Search for documents that have code blocks
    // Get all blocks with code type, then fetch their parent entities
    let sql = r#"
        SELECT entity_id FROM blocks WHERE block_type = 'code' LIMIT 50 FETCH entity_id
    "#;

    let fetch_result = client.query(sql, &[]).await.expect("Query failed");

    // Extract unique titles from fetched entities
    let mut titles = std::collections::HashSet::new();
    for record in &fetch_result.records {
        if let Some(entity_obj) = record.data.get("entity_id") {
            if let Some(data_obj) = entity_obj.get("data") {
                if let Some(title) = data_obj.get("title").and_then(|v| v.as_str()) {
                    titles.insert(title.to_string());
                }
            }
        }
    }

    // ASSERT: Should find documents with code blocks
    assert!(
        !titles.is_empty(),
        "Expected to find documents with code blocks (block_type='code')"
    );

    // Dev-kiln documentation should contain code blocks (Agent Cards, Commands, etc.)
    assert!(
        titles
            .iter()
            .any(|t| t.contains("Agent") || t.contains("Commands") || t.contains("Block")),
        "Expected to find documentation with code blocks, got: {:?}",
        titles
    );
}

// ============================================================================
// TEST 6: Grep-Based Text Search - Regex Patterns
// ============================================================================

#[tokio::test]
async fn grep_regex_patterns() {
    // ARRANGE: Set up database
    let client = setup_test_db_with_kiln()
        .await
        .expect("Failed to set up test database");

    // ACT: Search using pattern matching for email-like patterns or URLs in block content
    // Use FETCH to retrieve the related entity records
    let sql = r#"
        SELECT entity_id
        FROM blocks
        WHERE content CONTAINS '@'
           OR content CONTAINS 'http://'
           OR content CONTAINS 'https://'
        LIMIT 50
        FETCH entity_id
    "#;

    let fetch_result = client.query(sql, &[]).await.expect("Query failed");

    // Extract unique titles from fetched entities
    let mut titles = std::collections::HashSet::new();
    for record in &fetch_result.records {
        if let Some(entity_obj) = record.data.get("entity_id") {
            if let Some(data_obj) = entity_obj.get("data") {
                if let Some(title) = data_obj.get("title").and_then(|v| v.as_str()) {
                    titles.insert(title.to_string());
                }
            }
        }
    }

    // ASSERT: Should find documents with URLs or emails
    // Test-kiln has external links and possibly contact information
    assert!(
        !titles.is_empty(),
        "Expected to find documents with URLs or email patterns in blocks"
    );

    // Contact Management or API Documentation likely have URLs
    eprintln!("Found titles with URL/email patterns: {:?}", titles);
}

// ============================================================================
// TEST 7: Semantic Search - Basic Similarity
// ============================================================================

#[tokio::test]
#[ignore = "Requires embedding generation and test-utils feature"]
async fn semantic_basic_similarity() {
    // This test requires the embeddings feature and embedding generation
    // during document ingestion, which is not yet implemented.
}

// ============================================================================
// TEST 8: Semantic Search - Reranking
// ============================================================================

#[tokio::test]
#[ignore = "Requires embedding generation and test-utils feature"]
async fn semantic_reranking() {
    // This test requires the embeddings feature and reranking implementation.
}

// ============================================================================
// TEST 9: Semantic Search - Empty Results Handling
// ============================================================================

#[tokio::test]
#[ignore = "Requires embedding generation and test-utils feature"]
async fn semantic_empty_results() {
    // This test requires semantic search infrastructure.
}

// ============================================================================
// TEST 10: Semantic Search - Chunk vs Note Level
// ============================================================================

#[tokio::test]
#[ignore = "Requires embedding generation and test-utils feature"]
async fn semantic_chunk_vs_note() {
    // This test requires embeddings stored at both note and chunk level.
}
