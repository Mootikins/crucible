//! Kiln Integration Tests
//!
//! These tests verify the integration between the parser system and SurrealDB.
//! This follows TDD principles - we write failing tests first, then implement.

use crucible_core::parser::{
    DocumentContent, Frontmatter, FrontmatterFormat, Heading, ParsedDocument, Tag, Wikilink,
};
use crucible_surrealdb::{kiln_integration, kiln_scanner, SurrealClient};

// Import consolidated test utilities
mod common;
use chrono::Utc;
use common::DocumentTestUtils;
use filetime;
use std::borrow::Cow;
use std::path::PathBuf;
use kiln_integration::{
    create_embed_relationships, create_tag_associations, create_wikilink_edges,
    find_all_placeholders_for_target, find_document_by_title, get_documents_by_tag,
    get_embed_metadata, get_embed_relations, get_embed_with_metadata, get_embedded_documents,
    get_embedded_documents_by_type, get_embedding_documents, get_linked_documents,
    get_placeholder_metadata, get_wikilink_relations, get_wikilinked_documents,
    initialize_kiln_schema, retrieve_parsed_document, store_parsed_document, EmbedMetadata,
    EmbedRelation, LinkRelation, PlaceholderMetadata,
};
use kiln_scanner::{
    create_kiln_scanner, create_kiln_scanner_with_embeddings, parse_file_to_document,
    validate_kiln_scanner_config, ChangeDetectionMethod, ErrorHandlingMode, KilnFileInfo,
    KilnProcessResult, KilnScanResult, KilnScanner, KilnScannerConfig, KilnScannerErrorType,
    KilnScannerMetrics,
};

// =============================================================================
// PHASE 1: DATABASE BRIDGE TESTS
// =============================================================================

/// Test: Store a ParsedDocument in SurrealDB and retrieve it
/// This should initially FAIL, then we implement the functionality
#[tokio::test]
async fn test_store_parsed_document_in_surrealdb() {
    // Setup: Create a test ParsedDocument
    let test_doc = create_test_parsed_document();

    // Create SurrealDB connection
    let client = SurrealClient::new_memory().await.unwrap();

    // Initialize the database schema (this will need to be implemented)
    initialize_kiln_schema(&client).await.unwrap();

    // Store the document (this will need to be implemented)
    let stored_id = store_parsed_document(&client, &test_doc).await.unwrap();

    // Retrieve and validate
    let retrieved = retrieve_parsed_document(&client, &stored_id).await.unwrap();

    // Assertions
    assert_eq!(retrieved.title(), test_doc.title());
    assert_eq!(retrieved.path, test_doc.path);
    assert_eq!(retrieved.all_tags(), test_doc.all_tags());
    assert_eq!(retrieved.wikilinks.len(), test_doc.wikilinks.len());
}

/// Test: Create wikilink relationships from parsed document
#[tokio::test]
async fn test_create_wikilink_relationships() {
    let test_doc = create_test_document_with_wikilinks();

    let client = SurrealClient::new_memory().await.unwrap();
    initialize_kiln_schema(&client).await.unwrap();

    // Store the main document
    let doc_id = store_parsed_document(&client, &test_doc).await.unwrap();

    // Create wikilink relationships (this will need to be implemented)
    create_wikilink_edges(&client, &doc_id, &test_doc)
        .await
        .unwrap();

    // Query relationships to verify they were created
    let linked_docs = get_linked_documents(&client, &doc_id).await.unwrap();

    // Should have relationships to the linked documents
    assert_eq!(linked_docs.len(), 2); // Our test doc has 2 wikilinks
    assert!(linked_docs.iter().any(|d| d.title() == "Linked Document 1"));
    assert!(linked_docs.iter().any(|d| d.title() == "Linked Document 2"));
}

/// Test: Handle tag associations from frontmatter and inline tags
#[tokio::test]
#[ignore] // This will fail until we implement the functionality
async fn test_tag_associations_from_parsed_document() {
    let test_doc = create_test_document_with_tags();

    let client = SurrealClient::new_memory().await.unwrap();
    initialize_kiln_schema(&client).await.unwrap();

    // Store the document
    let doc_id = store_parsed_document(&client, &test_doc).await.unwrap();

    // Create tag associations (this will need to be implemented)
    create_tag_associations(&client, &doc_id, &test_doc)
        .await
        .unwrap();

    // Query documents by tag
    let docs_with_rust_tag = get_documents_by_tag(&client, "rust").await.unwrap();
    let docs_with_ai_tag = get_documents_by_tag(&client, "ai").await.unwrap();

    // Should find our document in both tag queries
    assert!(docs_with_rust_tag.iter().any(|d| d.path == test_doc.path));
    assert!(docs_with_ai_tag.iter().any(|d| d.path == test_doc.path));
}

/// Test: Full kiln processing workflow
#[tokio::test]
#[ignore] // This will fail until we implement the functionality
async fn test_process_kiln_directory_incrementally() {
    // Create a temporary kiln structure
    let temp_kiln = create_test_kiln_directory().await;

    let client = SurrealClient::new_memory().await.unwrap();
    initialize_kiln_schema(&client).await.unwrap();

    // Process the kiln (this will need to be implemented)
    let processed_count = process_kiln_directory(&client, &temp_kiln).await.unwrap();

    // Should have processed all markdown files
    assert_eq!(processed_count, 3); // Our test kiln has 3 files

    // Verify all documents were stored
    let all_docs = get_all_documents(&client).await.unwrap();
    assert_eq!(all_docs.len(), 3);

    // Modify a file and test incremental update
    modify_test_file(&temp_kiln, "note1.md").await.unwrap();

    let updated_count = process_kiln_directory(&client, &temp_kiln).await.unwrap();
    assert_eq!(updated_count, 1); // Only 1 file should be updated

    // Verify the content was updated
    let updated_doc = find_document_by_path(&client, &temp_kiln.join("note1.md"))
        .await
        .unwrap();
    assert!(updated_doc.content.plain_text.contains("updated content"));
}

// =============================================================================
// EMBED RELATIONSHIP TESTS (TDD)
// =============================================================================

/// Test: Create embed relationships from parsed document
/// This should initially FAIL, then we implement the functionality
#[tokio::test]
async fn test_create_embed_relationships() {
    let test_doc = create_test_document_with_embeds();

    let client = SurrealClient::new_memory().await.unwrap();
    initialize_kiln_schema(&client).await.unwrap();

    // Store the main document
    let doc_id = store_parsed_document(&client, &test_doc).await.unwrap();

    // Create embed relationships (this will need to be implemented)
    create_embed_relationships(&client, &doc_id, &test_doc)
        .await
        .unwrap();

    // Query embed relationships to verify they were created correctly
    let embedded_docs = get_embedded_documents(&client, &doc_id).await.unwrap();

    // Should have relationships to all embedded documents
    assert_eq!(embedded_docs.len(), 5); // Our test doc has 5 different embed types

    // Verify simple embed: ![[Document]]
    assert!(embedded_docs.iter().any(|d| d.title() == "Simple Document"));

    // Verify heading embed: ![[Document#Heading]]
    assert!(embedded_docs
        .iter()
        .any(|d| d.title() == "Document With Heading"));

    // Verify block embed: ![[Document#^block-id]]
    assert!(embedded_docs
        .iter()
        .any(|d| d.title() == "Document With Block"));

    // Verify aliased embed: ![[Document|Alias]]
    assert!(embedded_docs
        .iter()
        .any(|d| d.title() == "Document With Alias"));

    // Verify complex embed: ![[Document#Heading|Alias]]
    assert!(embedded_docs
        .iter()
        .any(|d| d.title() == "Complex Document"));

    // Verify embed metadata is preserved
    let embed_metadata = get_embed_metadata(&client, &doc_id).await.unwrap();
    assert_eq!(embed_metadata.len(), 5);

    // Check specific embed types and references
    let simple_embed = embed_metadata
        .iter()
        .find(|e| e.target == "Simple Document")
        .unwrap();
    assert!(simple_embed.is_embed);
    assert!(simple_embed.heading_ref.is_none());
    assert!(simple_embed.block_ref.is_none());

    let heading_embed = embed_metadata
        .iter()
        .find(|e| e.target == "Document With Heading")
        .unwrap();
    assert_eq!(heading_embed.heading_ref.as_ref().unwrap(), "Introduction");

    let block_embed = embed_metadata
        .iter()
        .find(|e| e.target == "Document With Block")
        .unwrap();
    assert_eq!(block_embed.block_ref.as_ref().unwrap(), "block-123");

    let aliased_embed = embed_metadata
        .iter()
        .find(|e| e.target == "Document With Alias")
        .unwrap();
    assert_eq!(aliased_embed.alias.as_ref().unwrap(), "Custom Alias");

    let complex_embed = embed_metadata
        .iter()
        .find(|e| e.target == "Complex Document")
        .unwrap();
    assert_eq!(complex_embed.heading_ref.as_ref().unwrap(), "Methods");
    assert_eq!(complex_embed.alias.as_ref().unwrap(), "Research Methods");
}

/// Test: Ensure embeds and wikilinks are stored separately
#[tokio::test]
#[ignore] // This will fail until we implement embed separation
async fn test_embed_vs_wikilink_separation() {
    let test_doc = create_test_document_with_mixed_links();

    let client = SurrealClient::new_memory().await.unwrap();
    initialize_kiln_schema(&client).await.unwrap();

    // Store the main document
    let doc_id = store_parsed_document(&client, &test_doc).await.unwrap();

    // Create both wikilink and embed relationships
    create_wikilink_edges(&client, &doc_id, &test_doc)
        .await
        .unwrap();
    create_embed_relationships(&client, &doc_id, &test_doc)
        .await
        .unwrap();

    // Query wikilinks separately from embeds
    let wikilinked_docs = get_wikilinked_documents(&client, &doc_id).await.unwrap();
    let embedded_docs = get_embedded_documents(&client, &doc_id).await.unwrap();

    // Should have 2 wikilinks and 2 embeds
    assert_eq!(wikilinked_docs.len(), 2);
    assert_eq!(embedded_docs.len(), 2);

    // Verify wikilink docs are correct
    assert!(wikilinked_docs
        .iter()
        .any(|d| d.title() == "Linked Document 1"));
    assert!(wikilinked_docs
        .iter()
        .any(|d| d.title() == "Linked Document 2"));

    // Verify embed docs are correct
    assert!(embedded_docs
        .iter()
        .any(|d| d.title() == "Embedded Document 1"));
    assert!(embedded_docs
        .iter()
        .any(|d| d.title() == "Embedded Document 2"));

    // Verify no overlap between wikilinks and embeds
    let wikilink_titles: Vec<String> = wikilinked_docs.iter().map(|d| d.title()).collect();
    let embed_titles: Vec<String> = embedded_docs.iter().map(|d| d.title()).collect();

    for title in &wikilink_titles {
        assert!(
            !embed_titles.contains(title),
            "Wikilinked document should not appear in embeds: {}",
            title
        );
    }

    // Verify database stores them in separate tables/relations
    let wikilink_relations = get_wikilink_relations(&client, &doc_id).await.unwrap();
    let embed_relations = get_embed_relations(&client, &doc_id).await.unwrap();

    assert_eq!(wikilink_relations.len(), 2);
    assert_eq!(embed_relations.len(), 2);

    // Verify relation types are different
    for relation in &wikilink_relations {
        assert_eq!(relation.relation_type, "wikilink");
        assert!(!relation.is_embed);
    }

    for relation in &embed_relations {
        assert_eq!(relation.relation_type, "embed");
        assert!(relation.is_embed);
    }
}

/// Test: Query embed relationships with filtering
#[tokio::test]
#[ignore] // This will fail until we implement embed querying
async fn test_get_embedded_documents() {
    let test_doc = create_test_document_with_varied_embeds();

    let client = SurrealClient::new_memory().await.unwrap();
    initialize_kiln_schema(&client).await.unwrap();

    // Store the main document
    let doc_id = store_parsed_document(&client, &test_doc).await.unwrap();

    // Create embed relationships
    create_embed_relationships(&client, &doc_id, &test_doc)
        .await
        .unwrap();

    // Test basic embed query
    let all_embeds = get_embedded_documents(&client, &doc_id).await.unwrap();
    assert_eq!(all_embeds.len(), 4);

    // Test filtering by embed type
    let heading_embeds = get_embedded_documents_by_type(&client, &doc_id, "heading")
        .await
        .unwrap();
    assert_eq!(heading_embeds.len(), 2);

    let block_embeds = get_embedded_documents_by_type(&client, &doc_id, "block")
        .await
        .unwrap();
    assert_eq!(block_embeds.len(), 1);

    let simple_embeds = get_embedded_documents_by_type(&client, &doc_id, "simple")
        .await
        .unwrap();
    assert_eq!(simple_embeds.len(), 1);

    // Test reverse query: find documents that embed this document
    create_additional_test_documents(&client).await.unwrap();

    let embedding_parents = get_embedding_documents(&client, "Simple Document")
        .await
        .unwrap();
    assert!(embedding_parents.len() >= 1);

    // Test embed metadata preservation
    let embed_with_metadata = get_embed_with_metadata(&client, &doc_id, "Document With Heading")
        .await
        .unwrap();
    assert!(embed_with_metadata.is_some());

    let metadata = embed_with_metadata.unwrap();
    assert_eq!(metadata.target, "Document With Heading");
    assert_eq!(metadata.heading_ref.unwrap(), "Introduction");
    assert!(metadata.position > 0);
}

/// Test: Auto-create missing target documents for embeds
#[tokio::test]
#[ignore] // This will fail until we implement placeholder creation
async fn test_embed_placeholder_creation() {
    let test_doc = create_test_document_with_nonexistent_embeds();

    let client = SurrealClient::new_memory().await.unwrap();
    initialize_kiln_schema(&client).await.unwrap();

    // Store the main document
    let doc_id = store_parsed_document(&client, &test_doc).await.unwrap();

    // Verify target documents don't exist yet
    let nonexistent_before = find_document_by_title(&client, "Nonexistent Document")
        .await
        .unwrap();
    assert!(nonexistent_before.is_none());

    let placeholder_before = find_document_by_title(&client, "Another Missing Doc")
        .await
        .unwrap();
    assert!(placeholder_before.is_none());

    // Create embed relationships (should auto-create placeholders)
    create_embed_relationships(&client, &doc_id, &test_doc)
        .await
        .unwrap();

    // Verify placeholder documents were created
    let placeholder_after = find_document_by_title(&client, "Nonexistent Document")
        .await
        .unwrap();
    assert!(placeholder_after.is_some());

    let another_placeholder = find_document_by_title(&client, "Another Missing Doc")
        .await
        .unwrap();
    assert!(another_placeholder.is_some());

    // Verify placeholder documents have correct properties
    let placeholder_doc = placeholder_after.unwrap();
    assert_eq!(placeholder_doc.title(), "Nonexistent Document");
    assert!(
        placeholder_doc.content.plain_text.contains("placeholder")
            || placeholder_doc.content.plain_text.is_empty()
    ); // Empty content is also acceptable

    // Verify embed relationships point to placeholder documents
    let embedded_docs = get_embedded_documents(&client, &doc_id).await.unwrap();
    assert_eq!(embedded_docs.len(), 2);
    assert!(embedded_docs
        .iter()
        .any(|d| d.title() == "Nonexistent Document"));
    assert!(embedded_docs
        .iter()
        .any(|d| d.title() == "Another Missing Doc"));

    // Test placeholder metadata
    let placeholder_metadata = get_placeholder_metadata(&client, "Nonexistent Document")
        .await
        .unwrap();
    assert!(placeholder_metadata.is_placeholder);
    assert!(placeholder_metadata.created_by_embed);
    assert_eq!(placeholder_metadata.referenced_by.len(), 1);
    assert_eq!(placeholder_metadata.referenced_by[0], doc_id);

    // Test that subsequent embed to same document doesn't create duplicates
    let doc2_id = store_parsed_document(&client, &create_second_document_with_same_embed())
        .await
        .unwrap();
    create_embed_relationships(&client, &doc2_id, &create_second_document_with_same_embed())
        .await
        .unwrap();

    let placeholders = find_all_placeholders_for_target(&client, "Nonexistent Document")
        .await
        .unwrap();
    assert_eq!(placeholders.len(), 1); // Should still be only one placeholder document

    // Verify placeholder now has 2 references
    let updated_metadata = get_placeholder_metadata(&client, "Nonexistent Document")
        .await
        .unwrap();
    assert_eq!(updated_metadata.referenced_by.len(), 2);
    assert!(updated_metadata.referenced_by.contains(&doc2_id));
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

async fn process_kiln_directory(
    _client: &SurrealClient,
    _kiln_path: &PathBuf,
) -> Result<usize, Box<dyn std::error::Error>> {
    // TODO: This will be implemented in Phase 2
    unimplemented!("Kiln processing not yet implemented")
}

async fn get_all_documents(
    _client: &SurrealClient,
) -> Result<Vec<ParsedDocument>, Box<dyn std::error::Error>> {
    // TODO: This will be implemented in Phase 2
    unimplemented!("Get all documents not yet implemented")
}

async fn find_document_by_path(
    _client: &SurrealClient,
    _path: &PathBuf,
) -> Result<ParsedDocument, Box<dyn std::error::Error>> {
    // TODO: This will be implemented in Phase 2
    unimplemented!("Find by path not yet implemented")
}

async fn create_additional_test_documents(
    client: &SurrealClient,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create additional test documents for reverse embedding queries
    let embedding_doc = create_test_embedding_document();
    let doc_id = store_parsed_document(client, &embedding_doc).await?;
    create_embed_relationships(client, &doc_id, &embedding_doc).await?;
    Ok(())
}

// =============================================================================
// EMBED RELATIONSHIP HELPER FUNCTIONS ARE IMPLEMENTED ABOVE
// =============================================================================

// =============================================================================
// TEST FIXTURES
// =============================================================================

fn create_test_parsed_document() -> ParsedDocument {
    let mut doc = ParsedDocument::new(PathBuf::from("/test/notes/sample.md"));

    // Add frontmatter
    let frontmatter = Frontmatter::new(
        r#"title: "Test Document"
tags: [rust, programming]
author: "Test Author"
created: "2024-01-01""#
            .to_string(),
        FrontmatterFormat::Yaml,
    );
    doc.frontmatter = Some(frontmatter);

    // Add content
    doc.content = DocumentContent::new()
        .with_plain_text("This is a test document with some content.".to_string());
    doc.content.add_heading(Heading::new(1, "Test Document", 0));

    // Add tags
    doc.tags.push(Tag::new("test", 50));
    doc.tags.push(Tag::new("example", 55));

    doc.parsed_at = Utc::now();
    doc.content_hash = "test_hash_123".to_string();
    doc.file_size = 1024;

    doc
}

fn create_test_document_with_wikilinks() -> ParsedDocument {
    let mut doc = ParsedDocument::new(PathBuf::from("/test/notes/main.md"));

    // Add frontmatter
    let frontmatter = Frontmatter::new(
        r#"title: "Main Document""#.to_string(),
        FrontmatterFormat::Yaml,
    );
    doc.frontmatter = Some(frontmatter);

    // Add wikilinks
    doc.wikilinks.push(Wikilink::new("Linked Document 1", 20));
    doc.wikilinks
        .push(Wikilink::with_alias("document2", "Linked Document 2", 45));
    doc.wikilinks.push(Wikilink::embed("Important Note", 80));

    // Add content with wikilinks
    let content = r#"# Main Document

This document links to [[Linked Document 1]] and also references [[document2|Linked Document 2]].

Here's an embedded note: ![[Important Note]]"#;

    doc.content = DocumentContent::new().with_plain_text(content.to_string());
    doc.content.add_heading(Heading::new(1, "Main Document", 0));

    doc.parsed_at = Utc::now();
    doc.content_hash = "wikilink_hash_456".to_string();
    doc.file_size = 2048;

    doc
}

fn create_test_document_with_tags() -> ParsedDocument {
    let mut doc = ParsedDocument::new(PathBuf::from("/test/notes/technical.md"));

    // Add frontmatter with tags
    let frontmatter = Frontmatter::new(
        r#"title: "Technical Note"
tags: [rust, ai, machine-learning]
type: "technical"
difficulty: "intermediate""#
            .to_string(),
        FrontmatterFormat::Yaml,
    );
    doc.frontmatter = Some(frontmatter);

    // Add inline tags
    doc.tags.push(Tag::new("rust", 100));
    doc.tags.push(Tag::new("ai", 200));
    doc.tags.push(Tag::new("programming/tutorial", 300));

    // Add content
    let content = r#"# Technical Note

This is a technical note about #rust programming and #ai development.

It covers topics related to #programming/tutorial concepts."#;

    doc.content = DocumentContent::new().with_plain_text(content.to_string());
    doc.content
        .add_heading(Heading::new(1, "Technical Note", 0));

    doc.parsed_at = Utc::now();
    doc.content_hash = "tag_hash_789".to_string();
    doc.file_size = 1536;

    doc
}

async fn create_test_kiln_directory() -> PathBuf {
    use std::fs;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_path_buf();

    // Create test files
    let note1_content = r#"---
title: "First Note"
tags: [test, intro]
---

# First Note

This is the first note in our test kiln."#;

    let note2_content = r#"---
title: "Second Note"
tags: [test, reference]
---

# Second Note

This note references [[First Note]]."#;

    let note3_content = r#"# Third Note

This is a simple note without frontmatter.

It links to [[Second Note]] and has #tags inline."#;

    fs::write(kiln_path.join("note1.md"), note1_content).unwrap();
    fs::write(kiln_path.join("note2.md"), note2_content).unwrap();
    fs::write(kiln_path.join("note3.md"), note3_content).unwrap();

    // Note: TempDir will be cleaned up when it goes out of scope
    // For tests, we might need to use a different approach or extend the lifetime
    kiln_path
}

async fn modify_test_file(
    kiln_path: &PathBuf,
    filename: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::fs;

    let file_path = kiln_path.join(filename);
    let updated_content = r#"---
title: "First Note Updated"
tags: [test, intro, updated]
---

# First Note

This is the first note in our test kiln with updated content."#;

    fs::write(file_path, updated_content)?;
    Ok(())
}

// =============================================================================
// EMBED TEST FIXTURES
// =============================================================================

fn create_test_document_with_embeds() -> ParsedDocument {
    let mut doc = ParsedDocument::new(PathBuf::from("/test/notes/embed_test.md"));

    // Add frontmatter
    let frontmatter = Frontmatter::new(
        r#"title: "Embed Test Document"
tags: [test, embeds]
type: "test""#
            .to_string(),
        FrontmatterFormat::Yaml,
    );
    doc.frontmatter = Some(frontmatter);

    // Add various types of embeds using Wikilink struct
    // Simple embed: ![[Document]]
    doc.wikilinks.push(Wikilink::embed("Simple Document", 20));

    // Heading embed: ![[Document#Heading]]
    let mut heading_embed = Wikilink::embed("Document With Heading", 45);
    heading_embed.heading_ref = Some("Introduction".to_string());
    doc.wikilinks.push(heading_embed);

    // Block embed: ![[Document#^block-id]]
    let mut block_embed = Wikilink::embed("Document With Block", 70);
    block_embed.block_ref = Some("block-123".to_string());
    doc.wikilinks.push(block_embed);

    // Aliased embed: ![[Document|Alias]]
    let mut aliased_embed = Wikilink::embed("Document With Alias", 95);
    aliased_embed.alias = Some("Custom Alias".to_string());
    doc.wikilinks.push(aliased_embed);

    // Complex embed: ![[Document#Heading|Alias]]
    let mut complex_embed = Wikilink::embed("Complex Document", 120);
    complex_embed.heading_ref = Some("Methods".to_string());
    complex_embed.alias = Some("Research Methods".to_string());
    doc.wikilinks.push(complex_embed);

    // Add content that includes embeds
    let content = r#"# Embed Test Document

This document tests various embed patterns:

Simple embed: ![[Simple Document]]

Heading embed: ![[Document With Heading#Introduction]]

Block embed: ![[Document With Block#^block-123]]

Aliased embed: ![[Document With Alias|Custom Alias]]

Complex embed: ![[Complex Document#Methods|Research Methods]]"#;

    doc.content = DocumentContent::new().with_plain_text(content.to_string());
    doc.content
        .add_heading(Heading::new(1, "Embed Test Document", 0));

    doc.parsed_at = Utc::now();
    doc.content_hash = "embed_test_hash_123".to_string();
    doc.file_size = 3072;

    doc
}

fn create_test_document_with_mixed_links() -> ParsedDocument {
    let mut doc = ParsedDocument::new(PathBuf::from("/test/notes/mixed_links.md"));

    // Add frontmatter
    let frontmatter = Frontmatter::new(
        r#"title: "Mixed Links Document"
tags: [test, links]"#
            .to_string(),
        FrontmatterFormat::Yaml,
    );
    doc.frontmatter = Some(frontmatter);

    // Add regular wikilinks (not embeds)
    doc.wikilinks.push(Wikilink::new("Linked Document 1", 25));
    doc.wikilinks
        .push(Wikilink::with_alias("document2", "Linked Document 2", 50));

    // Add embeds
    doc.wikilinks
        .push(Wikilink::embed("Embedded Document 1", 75));
    doc.wikilinks
        .push(Wikilink::embed("Embedded Document 2", 100));

    // Add content with mixed links
    let content = r#"# Mixed Links Document

This document has both regular links and embeds:

Regular link to [[Linked Document 1]].

Another link to [[document2|Linked Document 2]].

Embed: ![[Embedded Document 1]]

Another embed: ![[Embedded Document 2]]"#;

    doc.content = DocumentContent::new().with_plain_text(content.to_string());
    doc.content
        .add_heading(Heading::new(1, "Mixed Links Document", 0));

    doc.parsed_at = Utc::now();
    doc.content_hash = "mixed_links_hash_456".to_string();
    doc.file_size = 2048;

    doc
}

fn create_test_document_with_varied_embeds() -> ParsedDocument {
    let mut doc = ParsedDocument::new(PathBuf::from("/test/notes/varied_embeds.md"));

    // Add frontmatter
    let frontmatter = Frontmatter::new(
        r#"title: "Varied Embeds Document""#.to_string(),
        FrontmatterFormat::Yaml,
    );
    doc.frontmatter = Some(frontmatter);

    // Add simple embed
    doc.wikilinks.push(Wikilink::embed("Simple Document", 20));

    // Add heading embeds (2 of them)
    let mut heading1 = Wikilink::embed("Document With Heading", 45);
    heading1.heading_ref = Some("Introduction".to_string());
    doc.wikilinks.push(heading1);

    let mut heading2 = Wikilink::embed("Another Document", 70);
    heading2.heading_ref = Some("Methods".to_string());
    doc.wikilinks.push(heading2);

    // Add block embed
    let mut block_embed = Wikilink::embed("Document With Block", 95);
    block_embed.block_ref = Some("block-456".to_string());
    doc.wikilinks.push(block_embed);

    // Add content
    let content = r#"# Varied Embeds Document

This document has different types of embeds:

Simple: ![[Simple Document]]

Heading 1: ![[Document With Heading#Introduction]]

Heading 2: ![[Another Document#Methods]]

Block: ![[Document With Block#^block-456]]"#;

    doc.content = DocumentContent::new().with_plain_text(content.to_string());
    doc.content
        .add_heading(Heading::new(1, "Varied Embeds Document", 0));

    doc.parsed_at = Utc::now();
    doc.content_hash = "varied_embeds_hash_789".to_string();
    doc.file_size = 2560;

    doc
}

fn create_test_document_with_nonexistent_embeds() -> ParsedDocument {
    let mut doc = ParsedDocument::new(PathBuf::from("/test/notes/nonexistent_embeds.md"));

    // Add frontmatter
    let frontmatter = Frontmatter::new(
        r#"title: "Nonexistent Embeds Document"
tags: [test, placeholders]"#
            .to_string(),
        FrontmatterFormat::Yaml,
    );
    doc.frontmatter = Some(frontmatter);

    // Add embeds to non-existent documents
    doc.wikilinks
        .push(Wikilink::embed("Nonexistent Document", 20));
    doc.wikilinks
        .push(Wikilink::embed("Another Missing Doc", 45));

    // Add content
    let content = r#"# Nonexistent Embeds Document

This document embeds files that don't exist yet:

![[Nonexistent Document]]

![[Another Missing Doc]]"#;

    doc.content = DocumentContent::new().with_plain_text(content.to_string());
    doc.content
        .add_heading(Heading::new(1, "Nonexistent Embeds Document", 0));

    doc.parsed_at = Utc::now();
    doc.content_hash = "nonexistent_embeds_hash_999".to_string();
    doc.file_size = 1024;

    doc
}

fn create_second_document_with_same_embed() -> ParsedDocument {
    let mut doc = ParsedDocument::new(PathBuf::from("/test/notes/second_doc.md"));

    // Add frontmatter
    let frontmatter = Frontmatter::new(
        r#"title: "Second Document"
tags: [test]"#
            .to_string(),
        FrontmatterFormat::Yaml,
    );
    doc.frontmatter = Some(frontmatter);

    // Add embed to the same nonexistent document
    doc.wikilinks
        .push(Wikilink::embed("Nonexistent Document", 15));

    // Add content
    let content = r#"# Second Document

This document also embeds the same nonexistent file:

![[Nonexistent Document]]"#;

    doc.content = DocumentContent::new().with_plain_text(content.to_string());
    doc.content
        .add_heading(Heading::new(1, "Second Document", 0));

    doc.parsed_at = Utc::now();
    doc.content_hash = "second_doc_hash_111".to_string();
    doc.file_size = 512;

    doc
}

fn create_test_embedding_document() -> ParsedDocument {
    let mut doc = ParsedDocument::new(PathBuf::from("/test/notes/embedding_doc.md"));

    // Add frontmatter
    let frontmatter = Frontmatter::new(
        r#"title: "Embedding Document"
tags: [test]"#
            .to_string(),
        FrontmatterFormat::Yaml,
    );
    doc.frontmatter = Some(frontmatter);

    // Add simple embed
    doc.wikilinks.push(Wikilink::embed("Simple Document", 15));

    // Add content
    let content = r#"# Embedding Document

This document embeds the simple document:

![[Simple Document]]"#;

    doc.content = DocumentContent::new().with_plain_text(content.to_string());
    doc.content
        .add_heading(Heading::new(1, "Embedding Document", 0));

    doc.parsed_at = Utc::now();
    doc.content_hash = "embedding_doc_hash_222".to_string();
    doc.file_size = 256;

    doc
}

// =============================================================================
// VECTOR EMBEDDING THREAD POOL TESTS (TDD)
// =============================================================================

/// Test: Basic embedding thread pool creation and configuration
/// This should initially FAIL, then we implement the functionality
#[tokio::test]
#[ignore] // This will fail until we implement the thread pool
async fn test_embedding_thread_pool_creation() {
    // Test configuration with default settings
    let default_config = EmbeddingConfig::default();
    let thread_pool = create_embedding_thread_pool(default_config).await.unwrap();

    // Validate thread pool properties
    assert!(thread_pool.worker_count().await > 0);
    assert!(thread_pool.batch_size().await > 0);
    assert!(thread_pool.is_privacy_focused().await); // Should emphasize local processing

    // Test custom configuration
    let custom_config = EmbeddingConfig {
        worker_count: 4,
        batch_size: 32,
        model_type: EmbeddingModel::LocalMini,
        privacy_mode: PrivacyMode::StrictLocal,
        max_queue_size: 1000,
        timeout_ms: 30000,
        retry_attempts: 3,
        retry_delay_ms: 1000,
        circuit_breaker_threshold: 10,
        circuit_breaker_timeout_ms: 30000,
    };

    let custom_pool = create_embedding_thread_pool(custom_config).await.unwrap();

    // Validate custom configuration
    assert_eq!(custom_pool.worker_count().await, 4);
    assert_eq!(custom_pool.batch_size().await, 32);
    assert_eq!(custom_pool.model_type().await, EmbeddingModel::LocalMini);
    assert_eq!(custom_pool.privacy_mode().await, PrivacyMode::StrictLocal);

    // Test configuration validation
    let invalid_config = EmbeddingConfig {
        worker_count: 0, // Invalid: must be > 0
        batch_size: 0,   // Invalid: must be > 0
        model_type: EmbeddingModel::LocalMini,
        privacy_mode: PrivacyMode::StrictLocal,
        max_queue_size: 1000,
        timeout_ms: 30000,
        retry_attempts: 3,
        retry_delay_ms: 1000,
        circuit_breaker_threshold: 10,
        circuit_breaker_timeout_ms: 30000,
    };

    let result = create_embedding_thread_pool(invalid_config).await;
    assert!(result.is_err(), "Should reject invalid configuration");

    // Test privacy mode enforcement
    let privacy_config = EmbeddingConfig {
        worker_count: 2,
        batch_size: 16,
        model_type: EmbeddingModel::LocalMini,
        privacy_mode: PrivacyMode::StrictLocal,
        max_queue_size: 500,
        timeout_ms: 15000,
        retry_attempts: 3,
        retry_delay_ms: 1000,
        circuit_breaker_threshold: 10,
        circuit_breaker_timeout_ms: 30000,
    };

    let privacy_pool = create_embedding_thread_pool(privacy_config).await.unwrap();
    assert!(privacy_pool.enforces_privacy().await);
    assert!(!privacy_pool.allows_external_processing().await);
}

/// Test: Bulk embedding processing for multiple documents
/// This should initially FAIL, then we implement the functionality
#[tokio::test]
#[ignore] // This will fail until we implement bulk processing
async fn test_bulk_embedding_processing() {
    // Setup: Create test documents for bulk processing
    let test_documents = vec![
        create_bulk_test_document("doc1", "First document with content about technology"),
        create_bulk_test_document("doc2", "Second document discussing science topics"),
        create_bulk_test_document("doc3", "Third document covering art and design"),
        create_bulk_test_document("doc4", "Fourth document about business strategy"),
        create_bulk_test_document("doc5", "Fifth document on education methods"),
    ];

    // Create thread pool with bulk processing configuration
    let config = EmbeddingConfig {
        worker_count: 2,
        batch_size: 2, // Small batch for testing
        model_type: EmbeddingModel::LocalMini,
        privacy_mode: PrivacyMode::StrictLocal,
        max_queue_size: 100,
        timeout_ms: 10000,
        retry_attempts: 3,
        retry_delay_ms: 1000,
        circuit_breaker_threshold: 10,
        circuit_breaker_timeout_ms: 30000,
    };

    let thread_pool = create_embedding_thread_pool(config).await.unwrap();

    // Store documents in database first
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_kiln_schema(&client).await.unwrap();

    let mut document_ids = Vec::new();
    for doc in &test_documents {
        let doc_id = store_parsed_document(&client, doc).await.unwrap();
        document_ids.push(doc_id);
    }

    // Process documents with embeddings
    let processing_result = process_documents_with_embeddings(&thread_pool, &client, &document_ids)
        .await
        .unwrap();

    // Validate processing results
    assert_eq!(processing_result.processed_count, 5);
    assert_eq!(processing_result.failed_count, 0);
    assert!(processing_result.total_processing_time.as_millis() > 0);

    // Verify embeddings were stored
    for doc_id in &document_ids {
        let embeddings = get_document_embeddings(&client, doc_id).await.unwrap();
        assert!(!embeddings.is_empty(), "Document should have embeddings");

        // Verify embedding vector properties
        for embedding in &embeddings {
            assert!(
                !embedding.vector.is_empty(),
                "Embedding vector should not be empty"
            );
            assert!(
                embedding.vector.len() >= 128,
                "Embedding should have reasonable dimensions"
            );
            assert!(
                embedding.chunk_id.is_some(),
                "Embedding should have chunk reference"
            );
        }
    }

    // Test batch size optimization
    let large_batch_config = EmbeddingConfig {
        worker_count: 4,
        batch_size: 10, // Larger batch
        model_type: EmbeddingModel::LocalStandard,
        privacy_mode: PrivacyMode::StrictLocal,
        max_queue_size: 200,
        timeout_ms: 15000,
        retry_attempts: 3,
        retry_delay_ms: 1000,
        circuit_breaker_threshold: 10,
        circuit_breaker_timeout_ms: 30000,
    };

    let large_batch_pool = create_embedding_thread_pool(large_batch_config)
        .await
        .unwrap();

    // Create more documents for batch testing
    let large_document_set: Vec<ParsedDocument> = (0..20)
        .map(|i| {
            create_bulk_test_document(
                &format!("batch_doc_{}", i),
                &format!("Content for document {}", i),
            )
        })
        .collect();

    let mut large_doc_ids = Vec::new();
    for doc in &large_document_set {
        let doc_id = store_parsed_document(&client, doc).await.unwrap();
        large_doc_ids.push(doc_id);
    }

    let large_batch_result =
        process_documents_with_embeddings(&large_batch_pool, &client, &large_doc_ids)
            .await
            .unwrap();

    // Batch processing should handle all documents efficiently
    assert_eq!(large_batch_result.processed_count, 20);
    assert_eq!(large_batch_result.failed_count, 0);
    assert!(large_batch_result.total_processing_time.as_millis() > 0);

    // Verify parallel processing occurred (should be faster than sequential)
    // This is a rough check - in real tests we'd use more precise timing
    let sequential_estimate = large_doc_ids.len() as u128 * 100; // ~100ms per document
    assert!(large_batch_result.total_processing_time.as_millis() < sequential_estimate);
}

/// Test: Incremental embedding updates for changed documents
/// This should initially FAIL, then we implement the functionality
#[tokio::test]
#[ignore] // This will fail until we implement incremental processing
async fn test_incremental_embedding_updates() {
    // Setup: Create initial documents and process them
    let mut test_doc = create_incremental_test_document("incremental_doc", "Original content");

    let client = SurrealClient::new_memory().await.unwrap();
    initialize_kiln_schema(&client).await.unwrap();

    // Store and process initial document
    let doc_id = store_parsed_document(&client, &test_doc).await.unwrap();

    let config = EmbeddingConfig::default();
    let thread_pool = create_embedding_thread_pool(config).await.unwrap();

    let initial_result = process_document_incremental(&thread_pool, &client, &doc_id)
        .await
        .unwrap();

    // Validate initial processing
    assert!(initial_result.processed);
    assert!(initial_result.embeddings_created > 0);
    assert!(!initial_result.content_hash.is_empty());

    let initial_embeddings = get_document_embeddings(&client, &doc_id).await.unwrap();
    assert!(!initial_embeddings.is_empty());

    // Modify document content
    test_doc.content.plain_text = "Modified content with new information".to_string();
    test_doc.content_hash = "modified_content_hash_456".to_string();
    test_doc.file_size = 1500;

    // Update document in database
    update_document_content(&client, &doc_id, &test_doc)
        .await
        .unwrap();

    // Process incremental update
    let incremental_result = process_document_incremental(&thread_pool, &client, &doc_id)
        .await
        .unwrap();

    // Validate incremental processing
    assert!(incremental_result.processed);
    assert!(incremental_result.embeddings_updated > 0);
    assert_ne!(incremental_result.content_hash, initial_result.content_hash);

    // Verify embeddings were updated (should be different from initial)
    let updated_embeddings = get_document_embeddings(&client, &doc_id).await.unwrap();
    assert!(!updated_embeddings.is_empty());

    // Embeddings should be different due to content change
    assert_ne!(initial_embeddings.len(), updated_embeddings.len());

    // Test processing unchanged document (should be skipped)
    let unchanged_result = process_document_incremental(&thread_pool, &client, &doc_id)
        .await
        .unwrap();

    // Should not process if content hash matches
    assert!(!unchanged_result.processed);
    assert_eq!(unchanged_result.embeddings_updated, 0);
    assert_eq!(
        unchanged_result.content_hash,
        incremental_result.content_hash
    );

    // Test processing multiple documents with mixed changes
    let mut docs = vec![
        create_incremental_test_document("doc1", "Content 1"),
        create_incremental_test_document("doc2", "Content 2"),
        create_incremental_test_document("doc3", "Content 3"),
    ];

    let mut doc_ids = Vec::new();
    for doc in &docs {
        let id = store_parsed_document(&client, doc).await.unwrap();
        doc_ids.push(id);
    }

    // Process all documents initially
    for doc_id in &doc_ids {
        process_document_incremental(&thread_pool, &client, doc_id)
            .await
            .unwrap();
    }

    // Modify only some documents
    docs[1].content.plain_text = "Modified content 2".to_string();
    docs[1].content_hash = "modified_hash_2".to_string();

    update_document_content(&client, &doc_ids[1], &docs[1])
        .await
        .unwrap();

    // Batch incremental processing
    let batch_incremental_result = process_documents_incremental(&thread_pool, &client, &doc_ids)
        .await
        .unwrap();

    // Should only process changed documents
    assert_eq!(batch_incremental_result.processed_count, 1);
    assert_eq!(batch_incremental_result.skipped_count, 2);
    assert!(batch_incremental_result.total_processing_time.as_millis() > 0);
}

/// Test: Embedding error handling and recovery
/// This should initially FAIL, then we implement the functionality
#[tokio::test]
#[ignore] // This will fail until we implement error handling
async fn test_embedding_error_handling() {
    // Setup: Create test documents
    let test_documents = vec![
        create_error_test_document("valid_doc", "Valid content"),
        create_error_test_document("problem_doc", "Content that might cause issues"),
        create_error_test_document("large_doc", &"x".repeat(1_000_000)), // Very large content
    ];

    let client = SurrealClient::new_memory().await.unwrap();
    initialize_kiln_schema(&client).await.unwrap();

    let mut doc_ids = Vec::new();
    for doc in &test_documents {
        let doc_id = store_parsed_document(&client, doc).await.unwrap();
        doc_ids.push(doc_id);
    }

    // Create thread pool with error handling configuration
    let config = EmbeddingConfig {
        worker_count: 2,
        batch_size: 2,
        model_type: EmbeddingModel::LocalMini,
        privacy_mode: PrivacyMode::StrictLocal,
        max_queue_size: 50,
        timeout_ms: 5000, // Short timeout for testing
        retry_attempts: 3,
        retry_delay_ms: 100,
        circuit_breaker_threshold: 10,
        circuit_breaker_timeout_ms: 30000,
    };

    let thread_pool = create_embedding_thread_pool(config).await.unwrap();

    // Process documents with potential errors
    let processing_result = process_documents_with_embeddings(&thread_pool, &client, &doc_ids)
        .await
        .unwrap();

    // Validate error handling
    assert!(processing_result.processed_count > 0);
    assert!(processing_result.failed_count > 0); // Some should fail
    assert!(!processing_result.errors.is_empty());

    // Check error details
    for error in &processing_result.errors {
        assert!(!error.document_id.is_empty());
        assert!(!error.error_message.is_empty());
        assert!(error.timestamp > chrono::Utc::now() - chrono::Duration::minutes(1));

        // Verify error is properly categorized
        assert!(matches!(
            error.error_type,
            EmbeddingErrorType::ProcessingError
                | EmbeddingErrorType::TimeoutError
                | EmbeddingErrorType::ResourceError
        ));
    }

    // Test retry logic
    let retry_config = EmbeddingConfig {
        worker_count: 1,
        batch_size: 1,
        model_type: EmbeddingModel::LocalMini,
        privacy_mode: PrivacyMode::StrictLocal,
        max_queue_size: 10,
        timeout_ms: 1000,
        retry_attempts: 2,
        retry_delay_ms: 50,
        circuit_breaker_threshold: 10,
        circuit_breaker_timeout_ms: 30000,
    };

    let retry_pool = create_embedding_thread_pool(retry_config).await.unwrap();

    // Create a document that will initially fail but succeed on retry
    let flaky_doc = create_flaky_test_document("flaky_doc", "Flaky content");
    let flaky_doc_id = store_parsed_document(&client, &flaky_doc).await.unwrap();

    let retry_result = process_document_with_retry(&retry_pool, &client, &flaky_doc_id)
        .await
        .unwrap();

    // Should eventually succeed after retries
    assert!(retry_result.succeeded);
    assert!(retry_result.attempt_count > 1);
    assert!(retry_result.total_time.as_millis() > 0);

    // Test circuit breaker pattern
    let circuit_breaker_config = EmbeddingConfig {
        worker_count: 2,
        batch_size: 1,
        model_type: EmbeddingModel::LocalMini,
        privacy_mode: PrivacyMode::StrictLocal,
        max_queue_size: 20,
        timeout_ms: 2000,
        retry_attempts: 3,
        retry_delay_ms: 1000,
        circuit_breaker_threshold: 3,
        circuit_breaker_timeout_ms: 5000,
    };

    let circuit_pool = create_embedding_thread_pool(circuit_breaker_config)
        .await
        .unwrap();

    // Simulate multiple failures to trigger circuit breaker
    let failing_docs: Vec<ParsedDocument> = (0..5)
        .map(|i| create_failing_test_document(&format!("failing_doc_{}", i)))
        .collect();

    let mut failing_doc_ids = Vec::new();
    for doc in &failing_docs {
        let doc_id = store_parsed_document(&client, doc).await.unwrap();
        failing_doc_ids.push(doc_id);
    }

    let circuit_result =
        process_documents_with_embeddings(&circuit_pool, &client, &failing_doc_ids)
            .await
            .unwrap();

    // Circuit breaker should trigger and prevent further processing
    assert!(circuit_result.circuit_breaker_triggered);
    assert!(circuit_result.processed_count < failing_doc_ids.len());
    assert!(circuit_result
        .errors
        .iter()
        .any(|e| e.error_type == EmbeddingErrorType::CircuitBreakerOpen));
}

/// Test: Thread pool configuration validation and optimization
/// This should initially FAIL, then we implement the functionality
#[tokio::test]
#[ignore] // This will fail until we implement configuration management
async fn test_thread_pool_configuration() {
    // Test default configuration
    let default_config = EmbeddingConfig::default();
    assert_eq!(default_config.worker_count, num_cpus::get());
    assert_eq!(default_config.batch_size, 16);
    assert_eq!(default_config.model_type, EmbeddingModel::LocalStandard);
    assert_eq!(default_config.privacy_mode, PrivacyMode::StrictLocal);
    assert_eq!(default_config.max_queue_size, 1000);
    assert_eq!(default_config.timeout_ms, 30000);

    // Test configuration validation
    let invalid_configs = vec![
        // Zero worker count
        EmbeddingConfig {
            worker_count: 0,
            batch_size: 16,
            model_type: EmbeddingModel::LocalStandard,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 1000,
            timeout_ms: 30000,
            retry_attempts: 3,
            retry_delay_ms: 1000,
            circuit_breaker_threshold: 10,
            circuit_breaker_timeout_ms: 30000,
        },
        // Zero batch size
        EmbeddingConfig {
            worker_count: 2,
            batch_size: 0,
            model_type: EmbeddingModel::LocalStandard,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 1000,
            timeout_ms: 30000,
            retry_attempts: 3,
            retry_delay_ms: 1000,
            circuit_breaker_threshold: 10,
            circuit_breaker_timeout_ms: 30000,
        },
        // Zero timeout
        EmbeddingConfig {
            worker_count: 2,
            batch_size: 16,
            model_type: EmbeddingModel::LocalStandard,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 1000,
            timeout_ms: 0,
            retry_attempts: 3,
            retry_delay_ms: 1000,
            circuit_breaker_threshold: 10,
            circuit_breaker_timeout_ms: 30000,
        },
        // Too large batch size
        EmbeddingConfig {
            worker_count: 2,
            batch_size: 10000,
            model_type: EmbeddingModel::LocalStandard,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 1000,
            timeout_ms: 30000,
            retry_attempts: 3,
            retry_delay_ms: 1000,
            circuit_breaker_threshold: 10,
            circuit_breaker_timeout_ms: 30000,
        },
    ];

    for invalid_config in invalid_configs {
        let result = validate_embedding_config(&invalid_config).await;
        assert!(result.is_err(), "Should reject invalid configuration");
    }

    // Test optimal configuration for different scenarios
    let high_throughput_config = EmbeddingConfig::optimize_for_throughput();
    assert!(high_throughput_config.worker_count >= 4);
    assert!(high_throughput_config.batch_size >= 32);
    assert_eq!(
        high_throughput_config.privacy_mode,
        PrivacyMode::StrictLocal
    );

    let low_latency_config = EmbeddingConfig::optimize_for_latency();
    assert!(low_latency_config.batch_size <= 8);
    assert!(low_latency_config.timeout_ms <= 5000);
    assert_eq!(low_latency_config.privacy_mode, PrivacyMode::StrictLocal);

    let resource_constrained_config = EmbeddingConfig::optimize_for_resources();
    assert!(resource_constrained_config.worker_count <= 2);
    assert!(resource_constrained_config.batch_size <= 16);
    assert_eq!(
        resource_constrained_config.model_type,
        EmbeddingModel::LocalMini
    );

    // Test configuration for different machine capabilities
    let weak_machine_config = EmbeddingConfig::for_machine_specs(
        num_cpus::get() / 2,    // Half the cores
        2 * 1024 * 1024 * 1024, // 2GB memory
    );
    assert!(weak_machine_config.worker_count <= 2);
    assert!(weak_machine_config.batch_size <= 8);
    assert_eq!(weak_machine_config.model_type, EmbeddingModel::LocalMini);

    let strong_machine_config = EmbeddingConfig::for_machine_specs(
        num_cpus::get() * 2,     // Double the cores (if available)
        16 * 1024 * 1024 * 1024, // 16GB memory
    );
    assert!(strong_machine_config.worker_count >= num_cpus::get());
    assert!(strong_machine_config.batch_size >= 32);
    assert_eq!(
        strong_machine_config.model_type,
        EmbeddingModel::LocalStandard
    );

    // Test thread pool behavior with different configurations
    let configs = vec![
        high_throughput_config,
        low_latency_config,
        resource_constrained_config,
    ];

    for config in configs {
        let thread_pool = create_embedding_thread_pool(config.clone()).await.unwrap();

        // Validate thread pool respects configuration
        assert_eq!(thread_pool.worker_count().await, config.worker_count);
        assert_eq!(thread_pool.batch_size().await, config.batch_size);
        assert_eq!(thread_pool.model_type().await, config.model_type);
        assert_eq!(thread_pool.privacy_mode().await, config.privacy_mode);

        // Test thread pool metrics
        let metrics = thread_pool.get_metrics().await;
        assert!(metrics.total_tasks_processed >= 0);
        assert!(metrics.active_workers <= config.worker_count as u32);
        assert!(metrics.queue_size <= config.max_queue_size as u32);
        assert!(metrics.average_processing_time.as_millis() >= 0);

        // Test thread pool shutdown
        thread_pool.shutdown().await.unwrap();
        assert!(thread_pool.is_shutdown().await);
    }
}

// =============================================================================
// VECTOR EMBEDDING TEST FIXTURES
// =============================================================================

/// Create a test document for bulk processing
fn create_bulk_test_document(id: &str, content: &str) -> ParsedDocument {
    let mut doc = ParsedDocument::new(PathBuf::from(format!("/test/bulk/{}.md", id)));

    // Add frontmatter
    let frontmatter = Frontmatter::new(
        format!(
            r#"title: "Bulk Document {}"
tags: [bulk, test, {}]
type: "test"
created: "2024-01-01""#,
            id, id
        )
        .to_string(),
        FrontmatterFormat::Yaml,
    );
    doc.frontmatter = Some(frontmatter);

    // Add content with enough text for meaningful embeddings
    let full_content = format!(
        r#"# Bulk Document {}

This is a test document for bulk embedding processing.

Content: {}

This document contains sufficient text to generate meaningful vector embeddings for testing the thread pool system. The content should be long enough to create multiple chunks for embedding processing.

Additional information about {} goes here. This ensures we have adequate content for the embedding generation process."#,
        id, content, id
    );

    doc.content = DocumentContent::new().with_plain_text(full_content.clone());
    doc.content
        .add_heading(Heading::new(1, &format!("Bulk Document {}", id), 0));

    doc.parsed_at = Utc::now();
    doc.content_hash = format!("bulk_doc_hash_{}", id);
    doc.file_size = full_content.len() as u64;

    doc
}

/// Create a test document for incremental processing
fn create_incremental_test_document(id: &str, content: &str) -> ParsedDocument {
    let mut doc = ParsedDocument::new(PathBuf::from(format!("/test/incremental/{}.md", id)));

    // Add frontmatter
    let frontmatter = Frontmatter::new(
        format!(
            r#"title: "Incremental Document {}"
tags: [incremental, test, {}]"#,
            id, id
        )
        .to_string(),
        FrontmatterFormat::Yaml,
    );
    doc.frontmatter = Some(frontmatter);

    // Add content
    let full_content = format!(
        r#"# Incremental Document {}

Content for incremental testing: {}

This document will be modified to test incremental embedding updates. The system should detect content changes and only process modified documents."#,
        id, content
    );

    doc.content = DocumentContent::new().with_plain_text(full_content.clone());
    doc.content
        .add_heading(Heading::new(1, &format!("Incremental Document {}", id), 0));

    doc.parsed_at = Utc::now();
    doc.content_hash = format!("incremental_doc_hash_{}", id);
    doc.file_size = full_content.len() as u64;

    doc
}

/// Create a test document for error handling
fn create_error_test_document(id: &str, content: &str) -> ParsedDocument {
    let mut doc = ParsedDocument::new(PathBuf::from(format!("/test/errors/{}.md", id)));

    // Add frontmatter
    let frontmatter = Frontmatter::new(
        format!(
            r#"title: "Error Test Document {}"
tags: [error, test, {}]"#,
            id, id
        )
        .to_string(),
        FrontmatterFormat::Yaml,
    );
    doc.frontmatter = Some(frontmatter);

    // Add content
    let full_content = format!(
        r#"# Error Test Document {}

Content for error testing: {}

This document is designed to test various error conditions in the embedding system."#,
        id, content
    );

    doc.content = DocumentContent::new().with_plain_text(full_content.clone());
    doc.content
        .add_heading(Heading::new(1, &format!("Error Test Document {}", id), 0));

    doc.parsed_at = Utc::now();
    doc.content_hash = format!("error_doc_hash_{}", id);
    doc.file_size = full_content.len() as u64;

    doc
}

/// Create a test document that might fail initially but succeed on retry
fn create_flaky_test_document(id: &str, content: &str) -> ParsedDocument {
    let mut doc = ParsedDocument::new(PathBuf::from(format!("/test/flaky/{}.md", id)));

    // Add frontmatter
    let frontmatter = Frontmatter::new(
        format!(
            r#"title: "Flaky Document {}"
tags: [flaky, test, retry]
special_flag: "flaky_processing""#,
            id
        )
        .to_string(),
        FrontmatterFormat::Yaml,
    );
    doc.frontmatter = Some(frontmatter);

    // Add content
    let full_content = format!(
        r#"# Flaky Document {}

Content: {}

This document simulates flaky processing that might fail initially but succeed on retry."#,
        id, content
    );

    doc.content = DocumentContent::new().with_plain_text(full_content.clone());
    doc.content
        .add_heading(Heading::new(1, &format!("Flaky Document {}", id), 0));

    doc.parsed_at = Utc::now();
    doc.content_hash = format!("flaky_doc_hash_{}", id);
    doc.file_size = full_content.len() as u64;

    doc
}

/// Create a test document that will always fail
fn create_failing_test_document(id: &str) -> ParsedDocument {
    let mut doc = ParsedDocument::new(PathBuf::from(format!("/test/failing/{}.md", id)));

    // Add frontmatter with error flag
    let frontmatter = Frontmatter::new(
        format!(
            r#"title: "Failing Document {}"
tags: [failing, test, error]
force_error: true
error_type: "processing_failure""#,
            id
        )
        .to_string(),
        FrontmatterFormat::Yaml,
    );
    doc.frontmatter = Some(frontmatter);

    // Add content
    let full_content = format!(
        r#"# Failing Document {}

This document is designed to always fail processing to test error handling and circuit breaker functionality."#,
        id
    );

    doc.content = DocumentContent::new().with_plain_text(full_content.clone());
    doc.content
        .add_heading(Heading::new(1, &format!("Failing Document {}", id), 0));

    doc.parsed_at = Utc::now();
    doc.content_hash = format!("failing_doc_hash_{}", id);
    doc.file_size = full_content.len() as u64;

    doc
}

// =============================================================================
// VECTOR EMBEDDING TYPE DEFINITIONS (THESE DON'T EXIST YET)
// =============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingConfig {
    pub worker_count: usize,
    pub batch_size: usize,
    pub model_type: EmbeddingModel,
    pub privacy_mode: PrivacyMode,
    pub max_queue_size: usize,
    pub timeout_ms: u64,
    pub retry_attempts: u32,
    pub retry_delay_ms: u64,
    pub circuit_breaker_threshold: u32,
    pub circuit_breaker_timeout_ms: u64,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            worker_count: num_cpus::get(),
            batch_size: 16,
            model_type: EmbeddingModel::LocalStandard,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 1000,
            timeout_ms: 30000,
            retry_attempts: 3,
            retry_delay_ms: 1000,
            circuit_breaker_threshold: 10,
            circuit_breaker_timeout_ms: 30000,
        }
    }
}

impl EmbeddingConfig {
    pub fn optimize_for_throughput() -> Self {
        Self {
            worker_count: num_cpus::get(),
            batch_size: 64,
            model_type: EmbeddingModel::LocalStandard,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 2000,
            timeout_ms: 60000,
            retry_attempts: 2,
            retry_delay_ms: 500,
            circuit_breaker_threshold: 20,
            circuit_breaker_timeout_ms: 60000,
        }
    }

    pub fn optimize_for_latency() -> Self {
        Self {
            worker_count: num_cpus::get(),
            batch_size: 4,
            model_type: EmbeddingModel::LocalMini,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 100,
            timeout_ms: 5000,
            retry_attempts: 1,
            retry_delay_ms: 100,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout_ms: 10000,
        }
    }

    pub fn optimize_for_resources() -> Self {
        Self {
            worker_count: 1,
            batch_size: 8,
            model_type: EmbeddingModel::LocalMini,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 50,
            timeout_ms: 15000,
            retry_attempts: 1,
            retry_delay_ms: 200,
            circuit_breaker_threshold: 3,
            circuit_breaker_timeout_ms: 15000,
        }
    }

    pub fn for_machine_specs(cpu_cores: usize, memory_bytes: usize) -> Self {
        let worker_count = cpu_cores.min(8); // Cap at 8 workers
        let batch_size = match memory_bytes {
            0..=2_147_483_648 => 8,              // <= 2GB
            2_147_483_649..=8_589_934_592 => 16, // 2GB - 8GB
            _ => 32,                             // > 8GB
        };

        Self {
            worker_count,
            batch_size,
            model_type: if memory_bytes < 4_294_967_296 {
                EmbeddingModel::LocalMini
            } else {
                EmbeddingModel::LocalStandard
            },
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: worker_count * 100,
            timeout_ms: 30000,
            retry_attempts: 3,
            retry_delay_ms: 1000,
            circuit_breaker_threshold: 10,
            circuit_breaker_timeout_ms: 30000,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EmbeddingModel {
    LocalMini,
    LocalStandard,
    LocalLarge,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PrivacyMode {
    StrictLocal,
    AllowExternalFallback,
    HybridMode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingThreadPool {
    // These will be implemented
}

impl EmbeddingThreadPool {
    pub async fn worker_count(&self) -> usize {
        unimplemented!("worker_count not implemented yet")
    }

    pub async fn batch_size(&self) -> usize {
        unimplemented!("batch_size not implemented yet")
    }

    pub async fn model_type(&self) -> EmbeddingModel {
        unimplemented!("model_type not implemented yet")
    }

    pub async fn privacy_mode(&self) -> PrivacyMode {
        unimplemented!("privacy_mode not implemented yet")
    }

    pub async fn is_privacy_focused(&self) -> bool {
        unimplemented!("is_privacy_focused not implemented yet")
    }

    pub async fn enforces_privacy(&self) -> bool {
        unimplemented!("enforces_privacy not implemented yet")
    }

    pub async fn allows_external_processing(&self) -> bool {
        unimplemented!("allows_external_processing not implemented yet")
    }

    pub async fn get_metrics(&self) -> ThreadPoolMetrics {
        unimplemented!("get_metrics not implemented yet")
    }

    pub async fn shutdown(&self) -> Result<(), Box<dyn std::error::Error>> {
        unimplemented!("shutdown not implemented yet")
    }

    pub async fn is_shutdown(&self) -> bool {
        unimplemented!("is_shutdown not implemented yet")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThreadPoolMetrics {
    pub total_tasks_processed: u64,
    pub active_workers: u32,
    pub queue_size: u32,
    pub average_processing_time: std::time::Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingProcessingResult {
    pub processed_count: usize,
    pub failed_count: usize,
    pub total_processing_time: std::time::Duration,
    pub errors: Vec<EmbeddingError>,
    pub circuit_breaker_triggered: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingError {
    pub document_id: String,
    pub error_type: EmbeddingErrorType,
    pub error_message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EmbeddingErrorType {
    ProcessingError,
    TimeoutError,
    ResourceError,
    ConfigurationError,
    CircuitBreakerOpen,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IncrementalProcessingResult {
    pub processed: bool,
    pub embeddings_created: usize,
    pub embeddings_updated: usize,
    pub content_hash: String,
    pub processing_time: std::time::Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchIncrementalResult {
    pub processed_count: usize,
    pub skipped_count: usize,
    pub total_processing_time: std::time::Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetryProcessingResult {
    pub succeeded: bool,
    pub attempt_count: u32,
    pub total_time: std::time::Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentEmbedding {
    pub document_id: String,
    pub chunk_id: Option<String>,
    pub vector: Vec<f32>,
    pub embedding_model: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// =============================================================================
// VECTOR EMBEDDING FUNCTIONS (THESE DON'T EXIST YET)
// =============================================================================

pub async fn create_embedding_thread_pool(
    config: EmbeddingConfig,
) -> Result<EmbeddingThreadPool, Box<dyn std::error::Error>> {
    unimplemented!("create_embedding_thread_pool not implemented yet")
}

pub async fn validate_embedding_config(
    config: &EmbeddingConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    unimplemented!("validate_embedding_config not implemented yet")
}

pub async fn process_documents_with_embeddings(
    thread_pool: &EmbeddingThreadPool,
    client: &SurrealClient,
    document_ids: &[String],
) -> Result<EmbeddingProcessingResult, Box<dyn std::error::Error>> {
    unimplemented!("process_documents_with_embeddings not implemented yet")
}

pub async fn process_document_incremental(
    thread_pool: &EmbeddingThreadPool,
    client: &SurrealClient,
    document_id: &str,
) -> Result<IncrementalProcessingResult, Box<dyn std::error::Error>> {
    unimplemented!("process_document_incremental not implemented yet")
}

pub async fn process_documents_incremental(
    thread_pool: &EmbeddingThreadPool,
    client: &SurrealClient,
    document_ids: &[String],
) -> Result<BatchIncrementalResult, Box<dyn std::error::Error>> {
    unimplemented!("process_documents_incremental not implemented yet")
}

pub async fn process_document_with_retry(
    thread_pool: &EmbeddingThreadPool,
    client: &SurrealClient,
    document_id: &str,
) -> Result<RetryProcessingResult, Box<dyn std::error::Error>> {
    unimplemented!("process_document_with_retry not implemented yet")
}

pub async fn update_document_content(
    client: &SurrealClient,
    document_id: &str,
    document: &ParsedDocument,
) -> Result<(), Box<dyn std::error::Error>> {
    unimplemented!("update_document_content not implemented yet")
}

pub async fn get_document_embeddings(
    client: &SurrealClient,
    document_id: &str,
) -> Result<Vec<DocumentEmbedding>, Box<dyn std::error::Error>> {
    unimplemented!("get_document_embeddings not implemented yet")
}

// =============================================================================
// KILN SCANNER TESTS (PHASE 2 - TDD)
// =============================================================================

/// Test: Basic kiln scanner functionality with real file system
/// This should initially FAIL, then we implement the kiln scanner
#[tokio::test]
#[ignore] // This will fail until we implement the kiln scanner
async fn test_kiln_scanner_basic_functionality() {
    // Create a temporary kiln directory structure
    let test_kiln = create_comprehensive_test_kiln().await;

    // Create kiln scanner with default configuration
    let scanner_config = KilnScannerConfig::default();
    let mut scanner = create_kiln_scanner(scanner_config).await.unwrap();

    // Scan the kiln directory
    let scan_result = scanner.scan_kiln_directory(&test_kiln).await.unwrap();

    // Verify file discovery results
    assert_eq!(scan_result.total_files_found, 8); // We'll create 8 markdown files
    assert_eq!(scan_result.markdown_files_found, 6); // 6 markdown files
    assert_eq!(scan_result.directories_scanned, 4); // Including subdirectories
    assert!(scan_result.scan_duration.as_millis() > 0);

    // Verify discovered files
    assert!(!scan_result.discovered_files.is_empty());
    assert_eq!(scan_result.discovered_files.len(), 6);

    // Check that we found files in subdirectories
    let has_subdir_files = scan_result
        .discovered_files
        .iter()
        .any(|f| f.path.starts_with(&test_kiln.join("subdir")));
    assert!(has_subdir_files, "Should discover files in subdirectories");

    // Verify file metadata was captured
    for file_info in &scan_result.discovered_files {
        assert!(file_info.file_size > 0);
        assert!(file_info.last_modified > chrono::DateTime::UNIX_EPOCH);
        assert!(file_info.content_hash.is_empty()); // Hash calculated during processing
        assert!(!file_info.relative_path.is_empty());
    }

    // Process the discovered files
    let process_result = scanner
        .process_kiln_files(&scan_result.discovered_files)
        .await
        .unwrap();

    // Verify processing results
    assert_eq!(process_result.processed_count, 6);
    assert_eq!(process_result.failed_count, 0);
    assert!(process_result.total_processing_time.as_millis() > 0);
    assert!(process_result.errors.is_empty());

    // Verify documents were stored in database
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_kiln_schema(&client).await.unwrap();

    // Process documents through the full pipeline
    let mut stored_documents = Vec::new();
    for file_info in &scan_result.discovered_files {
        let parsed_doc = parse_file_to_document(&file_info.path).await.unwrap();
        let doc_id = store_parsed_document(&client, &parsed_doc).await.unwrap();
        stored_documents.push(doc_id);
    }

    // Verify all documents were stored
    let all_docs = get_all_documents(&client).await.unwrap();
    assert_eq!(all_docs.len(), 6);

    // Test scanner state management
    let scanner_state = scanner.get_state().await;
    assert_eq!(scanner_state.files_scanned, 6);
    assert_eq!(scanner_state.files_processed, 6);
    assert!(scanner_state.last_scan_time > chrono::DateTime::UNIX_EPOCH);
    assert_eq!(scanner_state.current_kiln_path, Some(test_kiln));

    // Verify scanner maintains file index
    let file_index = scanner.get_file_index().await;
    assert_eq!(file_index.len(), 6);
    for (path, metadata) in file_index {
        assert!(path.exists());
        assert!(metadata.last_modified > chrono::DateTime::UNIX_EPOCH);
        assert!(metadata.file_size > 0);
    }
}

/// Test: End-to-end kiln processing with embeddings
/// This should initially FAIL, then we implement the full pipeline
#[tokio::test]
#[ignore] // This will fail until we implement end-to-end processing
async fn test_kiln_scanner_with_embeddings() {
    // Create a test kiln with rich content for embedding
    let test_kiln = create_embedding_test_kiln().await;

    // Setup database and embedding thread pool
    let client = SurrealClient::new_memory().await.unwrap();
    initialize_kiln_schema(&client).await.unwrap();

    let embedding_config = EmbeddingConfig::default();
    let embedding_pool = create_embedding_thread_pool(embedding_config)
        .await
        .unwrap();

    // Create kiln scanner with embedding integration
    let scanner_config = KilnScannerConfig {
        enable_embeddings: true,
        recursive_scan: true,
        process_embeds: true,
        process_wikilinks: true,
        batch_processing: true,
        batch_size: 4,
        ..Default::default()
    };

    let mut scanner =
        create_kiln_scanner_with_embeddings(scanner_config, &client, &embedding_pool)
            .await
            .unwrap();

    // Run complete kiln processing pipeline
    let pipeline_result = scanner.scan_and_process_kiln(&test_kiln).await.unwrap();

    // Verify pipeline results
    assert_eq!(pipeline_result.files_found, 5);
    assert_eq!(pipeline_result.documents_processed, 5);
    assert_eq!(pipeline_result.embeddings_generated, 5);
    assert!(pipeline_result.total_pipeline_time.as_millis() > 0);

    // Verify document storage
    let stored_docs = get_all_documents(&client).await.unwrap();
    assert_eq!(stored_docs.len(), 5);

    // Verify embeddings were created
    let mut total_embeddings = 0;
    for doc in &stored_docs {
        let embeddings = get_document_embeddings(&client, &doc.id).await.unwrap();
        assert!(!embeddings.is_empty());
        total_embeddings += embeddings.len();
    }
    assert!(total_embeddings >= 5); // At least one embedding per document

    // Verify embed relationships were processed
    let embed_relations_count = 0;
    for doc in &stored_docs {
        let embeds = get_embedded_documents(&client, &doc.id).await.unwrap();
        embed_relations_count += embeds.len();
    }
    assert!(
        embed_relations_count > 0,
        "Should have processed embed relationships"
    );

    // Verify wikilink relationships were processed
    let wikilink_relations_count = 0;
    for doc in &stored_docs {
        let links = get_linked_documents(&client, &doc.id).await.unwrap();
        wikilink_relations_count += links.len();
    }
    assert!(
        wikilink_relations_count > 0,
        "Should have processed wikilink relationships"
    );

    // Test batch processing efficiency
    let individual_processing_time = pipeline_result.average_processing_time_per_document;
    let batch_processing_time =
        pipeline_result.total_pipeline_time / pipeline_result.documents_processed as u32;

    // Batch processing should be more efficient (rough estimate)
    assert!(batch_processing_time <= individual_processing_time * 2);

    // Verify scanner maintains embedding statistics
    let embedding_stats = scanner.get_embedding_statistics().await;
    assert_eq!(embedding_stats.documents_processed, 5);
    assert_eq!(embedding_stats.embeddings_generated, total_embeddings);
    assert!(embedding_stats.total_embedding_time.as_millis() > 0);
    assert_eq!(
        embedding_stats.average_embedding_time.as_millis(),
        embedding_stats.total_embedding_time.as_millis() / total_embeddings as u128
    );
}

/// Test: Incremental kiln updates and change detection
/// This should initially FAIL, then we implement incremental scanning
#[tokio::test]
#[ignore] // This will fail until we implement incremental updates
async fn test_kiln_scanner_incremental_updates() {
    // Create initial kiln structure
    let test_kiln = create_incremental_test_kiln().await;

    let client = SurrealClient::new_memory().await.unwrap();
    initialize_kiln_schema(&client).await.unwrap();

    // Create scanner with incremental tracking
    let scanner_config = KilnScannerConfig {
        enable_incremental: true,
        track_file_changes: true,
        change_detection_method: ChangeDetectionMethod::ContentHash,
        ..Default::default()
    };

    let mut scanner = create_kiln_scanner(scanner_config).await.unwrap();

    // Perform initial scan
    let initial_scan = scanner.scan_incremental(&test_kiln).await.unwrap();

    // Verify initial scan
    assert_eq!(initial_scan.files_found, 4);
    assert_eq!(initial_scan.files_processed, 4);
    assert_eq!(initial_scan.files_updated, 0); // All new files
    assert_eq!(initial_scan.files_skipped, 0); // No files to skip on first scan
    assert!(initial_scan.initial_scan);

    // Store initial scan state
    let initial_state = scanner.get_incremental_state().await;
    assert_eq!(initial_state.tracked_files.len(), 4);

    // Modify an existing file
    let file_to_modify = test_kiln.join("document1.md");
    modify_file_content(&file_to_modify, "Modified content with new information")
        .await
        .unwrap();

    // Add a new file
    let new_file = test_kiln.join("new_document.md");
    create_test_file(&new_file, NEW_DOCUMENT_CONTENT)
        .await
        .unwrap();

    // Perform incremental scan
    let incremental_scan = scanner.scan_incremental(&test_kiln).await.unwrap();

    // Verify incremental scan results
    assert_eq!(incremental_scan.files_found, 5); // 4 original + 1 new
    assert_eq!(incremental_scan.files_processed, 2); // 1 modified + 1 new
    assert_eq!(incremental_scan.files_updated, 1); // Only the modified file
    assert_eq!(incremental_scan.files_skipped, 3); // 3 unchanged files
    assert!(!incremental_scan.initial_scan);

    // Verify modified file was reprocessed
    let modified_doc = find_document_by_path(&client, &file_to_modify)
        .await
        .unwrap();
    assert!(modified_doc.content.plain_text.contains("Modified content"));
    assert_ne!(modified_doc.content_hash, "original_hash");

    // Verify new file was processed
    let new_doc = find_document_by_path(&client, &new_file).await.unwrap();
    assert!(new_doc.content.plain_text.contains("New document content"));

    // Delete a file and scan again
    std::fs::remove_file(&test_kiln.join("document2.md")).unwrap();

    let deletion_scan = scanner.scan_incremental(&test_kiln).await.unwrap();
    assert_eq!(deletion_scan.files_found, 4); // One file deleted
    assert_eq!(deletion_scan.files_deleted, 1);
    assert_eq!(deletion_scan.files_processed, 0); // No content changes

    // Verify deleted file was marked as such
    let incremental_state = scanner.get_incremental_state().await;
    assert_eq!(incremental_state.tracked_files.len(), 4); // 3 existing + 1 deleted
    let deleted_entry = incremental_state
        .tracked_files
        .get(&test_kiln.join("document2.md"))
        .unwrap();
    assert!(deleted_entry.is_deleted);
    assert!(deleted_entry.deleted_at.is_some());

    // Test file move/rename detection
    let old_path = test_kiln.join("document3.md");
    let new_path = test_kiln.join("renamed_document.md");
    std::fs::rename(&old_path, &new_path).unwrap();

    let rename_scan = scanner.scan_incremental(&test_kiln).await.unwrap();
    assert_eq!(rename_scan.files_moved, 1);
    assert_eq!(rename_scan.files_processed, 1); // Moved file needs reprocessing

    // Verify moved file was tracked correctly
    let rename_state = scanner.get_incremental_state().await;
    assert!(!rename_state.tracked_files.contains_key(&old_path));
    assert!(rename_state.tracked_files.contains_key(&new_path));

    // Test content hash calculation accuracy
    let unchanged_file = test_kiln.join("document4.md");
    let original_content = std::fs::read_to_string(&unchanged_file).unwrap();

    // Write same content back (should be detected as unchanged)
    std::fs::write(&unchanged_file, &original_content).unwrap();

    let unchanged_scan = scanner.scan_incremental(&test_kiln).await.unwrap();
    assert_eq!(unchanged_scan.files_skipped, 4); // All files unchanged
    assert_eq!(unchanged_scan.files_processed, 0);

    // Verify timestamp-only changes don't trigger reprocessing
    let file_metadata = std::fs::metadata(&unchanged_file).unwrap();
    let modified_time = file_metadata.modified().unwrap();

    // Touch the file (update modification time without changing content)
    let new_modified_time = modified_time + std::time::Duration::from_secs(1);
    filetime::set_file_mtime(&unchanged_file, new_modified_time).unwrap();

    let timestamp_scan = scanner.scan_incremental(&test_kiln).await.unwrap();
    assert_eq!(timestamp_scan.files_skipped, 4); // Should still be skipped
    assert_eq!(timestamp_scan.files_processed, 0);
}

/// Test: Kiln scanner error handling and recovery
/// This should initially FAIL, then we implement robust error handling
#[tokio::test]
#[ignore] // This will fail until we implement error handling
async fn test_kiln_scanner_error_handling() {
    // Create a kiln with various problematic files
    let test_kiln = create_error_prone_test_kiln().await;

    let client = SurrealClient::new_memory().await.unwrap();
    initialize_kiln_schema(&client).await.unwrap();

    // Create scanner with error handling configuration
    let scanner_config = KilnScannerConfig {
        error_handling_mode: ErrorHandlingMode::ContinueOnError,
        max_error_count: 10,
        error_retry_attempts: 2,
        error_retry_delay_ms: 100,
        skip_problematic_files: true,
        log_errors_detailed: true,
        ..Default::default()
    };

    let mut scanner = create_kiln_scanner(scanner_config).await.unwrap();

    // Scan the problematic kiln
    let scan_result = scanner.scan_kiln_directory(&test_kiln).await.unwrap();

    // Verify error handling during scan
    assert!(scan_result.total_files_found >= 8); // Including problematic files
    assert!(scan_result.scan_errors.len() > 0); // Should encounter errors
    assert!(scan_result.successful_files < scan_result.total_files_found); // Some files failed

    // Check specific error types were handled
    let error_types: std::collections::HashSet<_> = scan_result
        .scan_errors
        .iter()
        .map(|e| &e.error_type)
        .collect();

    assert!(error_types.contains(&KilnScannerErrorType::PermissionDenied));
    assert!(error_types.contains(&KilnScannerErrorType::InvalidMarkdown));
    assert!(error_types.contains(&KilnScannerErrorType::MalformedFrontmatter));
    assert!(error_types.contains(&KilnScannerErrorType::FileTooLarge));

    // Process files with error recovery
    let process_result = scanner
        .process_kiln_files_with_error_handling(&scan_result.discovered_files)
        .await
        .unwrap();

    // Verify processing error handling
    assert!(process_result.processed_count > 0); // Some files processed successfully
    assert!(process_result.failed_count > 0); // Some files failed
    assert!(!process_result.errors.is_empty());

    // Verify error details are captured
    for error in &process_result.errors {
        assert!(!error.file_path.to_string_lossy().is_empty());
        assert!(!error.error_message.is_empty());
        assert!(error.error_type != KilnScannerErrorType::Unknown);
        assert!(error.timestamp > chrono::DateTime::UNIX_EPOCH);

        // Verify retry attempts were made
        assert!(error.retry_attempts >= 0);
        assert!(error.retry_attempts <= 2); // Configured max retries

        if error.recovered {
            assert!(error.final_error_message.is_none());
        } else {
            assert!(error.final_error_message.is_some());
        }
    }

    // Test partial recovery scenarios
    let recovered_files = process_result.errors.iter().filter(|e| e.recovered).count();

    let unrecovered_files = process_result
        .errors
        .iter()
        .filter(|e| !e.recovered)
        .count();

    assert!(
        recovered_files > 0,
        "Some files should be recovered through retries"
    );
    assert!(
        unrecovered_files > 0,
        "Some files should remain unrecoverable"
    );

    // Verify graceful degradation with permission errors
    let permission_kiln = create_permission_denied_kiln().await;
    let permission_result = scanner
        .scan_kiln_directory(&permission_kiln)
        .await
        .unwrap();

    // Should skip inaccessible files but continue processing others
    assert!(permission_result.successful_files > 0);
    assert!(permission_result
        .scan_errors
        .iter()
        .any(|e| e.error_type == KilnScannerErrorType::PermissionDenied));

    // Test timeout handling for large files
    let timeout_config = KilnScannerConfig {
        max_file_size_bytes: 1024 * 1024, // 1MB limit
        processing_timeout_ms: 1000,      // 1 second timeout
        ..Default::default()
    };

    let mut timeout_scanner = create_kiln_scanner(timeout_config).await.unwrap();
    let timeout_result = timeout_scanner
        .scan_kiln_directory(&test_kiln)
        .await
        .unwrap();

    // Should handle large files gracefully
    assert!(timeout_result
        .scan_errors
        .iter()
        .any(|e| e.error_type == KilnScannerErrorType::FileTooLarge));

    // Test circuit breaker for repeated failures
    let circuit_breaker_config = KilnScannerConfig {
        error_threshold_circuit_breaker: 3,
        circuit_breaker_timeout_ms: 5000,
        ..Default::default()
    };

    let mut circuit_scanner = create_kiln_scanner(circuit_breaker_config).await.unwrap();

    // Create multiple problematic kilns to trigger circuit breaker
    for i in 0..5 {
        let problematic_kiln = create_problematic_kiln_iteration(i).await;
        let result = circuit_scanner
            .scan_kiln_directory(&problematic_kiln)
            .await
            .unwrap();

        if i >= 3 {
            // Circuit breaker should be triggered
            assert!(result.circuit_breaker_triggered);
            assert!(result.early_termination);
            break;
        }
    }

    // Verify circuit breaker state
    let circuit_state = circuit_scanner.get_circuit_breaker_state().await;
    assert!(circuit_state.is_open);
    assert!(circuit_state.open_until > chrono::Utc::now());
    assert_eq!(circuit_state.failure_count, 3);

    // Test circuit breaker recovery
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // After timeout, circuit breaker should allow operations
    let recovery_kiln = create_simple_test_kiln().await;
    let recovery_result = circuit_scanner
        .scan_kiln_directory(&recovery_kiln)
        .await
        .unwrap();

    assert!(!recovery_result.circuit_breaker_triggered);
    assert!(recovery_result.successful_files > 0);

    // Verify error statistics are maintained
    let error_stats = scanner.get_error_statistics().await;
    assert!(error_stats.total_errors > 0);
    assert!(error_stats.recovered_errors > 0);
    assert!(error_stats.unrecovered_errors > 0);
    assert!(error_stats.error_rate > 0.0);
    assert!(error_stats.error_rate <= 1.0);
}

/// Test: Kiln scanner configuration and optimization
/// This should initially FAIL, then we implement configuration management
#[tokio::test]
#[ignore] // This will fail until we implement configuration system
async fn test_kiln_scanner_configuration() {
    // Test default configuration
    let default_config = KilnScannerConfig::default();
    assert_eq!(default_config.max_file_size_bytes, 50 * 1024 * 1024); // 50MB
    assert_eq!(default_config.max_recursion_depth, 10);
    assert!(default_config.recursive_scan);
    assert!(default_config.include_hidden_files == false);
    assert_eq!(
        default_config.file_extensions,
        vec!["md".to_string(), "markdown".to_string()]
    );
    assert_eq!(default_config.parallel_processing, num_cpus::get());
    assert_eq!(default_config.batch_size, 16);

    // Test configuration validation
    let invalid_configs = vec![
        // Invalid: zero parallel workers
        KilnScannerConfig {
            parallel_processing: 0,
            ..Default::default()
        },
        // Invalid: zero batch size
        KilnScannerConfig {
            batch_size: 0,
            ..Default::default()
        },
        // Invalid: empty file extensions
        KilnScannerConfig {
            file_extensions: vec![],
            ..Default::default()
        },
        // Invalid: zero max file size
        KilnScannerConfig {
            max_file_size_bytes: 0,
            ..Default::default()
        },
        // Invalid: zero recursion depth
        KilnScannerConfig {
            max_recursion_depth: 0,
            ..Default::default()
        },
    ];

    for invalid_config in invalid_configs {
        let result = validate_kiln_scanner_config(&invalid_config).await;
        assert!(result.is_err(), "Should reject invalid configuration");
    }

    // Test configuration presets for different use cases
    let large_kiln_config = KilnScannerConfig::for_large_kiln();
    assert!(large_kiln_config.parallel_processing >= 8);
    assert!(large_kiln_config.batch_size >= 32);
    assert!(large_kiln_config.max_file_size_bytes >= 100 * 1024 * 1024);
    assert_eq!(
        large_kiln_config.change_detection_method,
        ChangeDetectionMethod::ContentHash
    );

    let small_kiln_config = KilnScannerConfig::for_small_kiln();
    assert_eq!(small_kiln_config.parallel_processing, 1);
    assert_eq!(small_kiln_config.batch_size, 4);
    assert!(small_kiln_config.enable_incremental == false);

    let resource_constrained_config = KilnScannerConfig::for_resource_constrained();
    assert_eq!(resource_constrained_config.parallel_processing, 1);
    assert_eq!(resource_constrained_config.batch_size, 2);
    assert!(resource_constrained_config.max_file_size_bytes <= 10 * 1024 * 1024);

    let development_config = KilnScannerConfig::for_development();
    assert!(development_config.include_hidden_files);
    assert!(development_config.log_errors_detailed);
    assert_eq!(
        development_config.error_handling_mode,
        ErrorHandlingMode::PanicOnError
    );

    // Test configuration for specific machine specs
    let high_spec_config = KilnScannerConfig::for_machine_specs(
        num_cpus::get() * 2,     // Double cores
        16 * 1024 * 1024 * 1024, // 16GB memory
    );
    assert!(high_spec_config.parallel_processing >= num_cpus::get());
    assert!(high_spec_config.batch_size >= 32);

    let low_spec_config = KilnScannerConfig::for_machine_specs(
        2,                      // 2 cores
        2 * 1024 * 1024 * 1024, // 2GB memory
    );
    assert_eq!(low_spec_config.parallel_processing, 1);
    assert!(low_spec_config.batch_size <= 8);

    // Test configuration overrides and merging
    let base_config = KilnScannerConfig::default();
    let overrides = KilnScannerConfig {
        parallel_processing: 4,
        batch_size: 8,
        enable_embeddings: true,
        ..Default::default()
    };

    let merged_config = base_config.merge_with(overrides);
    assert_eq!(merged_config.parallel_processing, 4);
    assert_eq!(merged_config.batch_size, 8);
    assert!(merged_config.enable_embeddings);
    // Other fields should be from base config
    assert_eq!(
        merged_config.max_file_size_bytes,
        base_config.max_file_size_bytes
    );

    // Test configuration serialization/deserialization
    let serialized = serde_json::to_string(&default_config).unwrap();
    let deserialized: KilnScannerConfig = serde_json::from_str(&serialized).unwrap();
    assert_eq!(default_config, deserialized);

    // Test configuration validation for different scenarios
    let test_kiln = create_configuration_test_kiln().await;

    // Test with different configurations
    let configs = vec![
        large_kiln_config,
        small_kiln_config,
        resource_constrained_config,
    ];

    for config in configs {
        let mut scanner = create_kiln_scanner(config.clone()).await.unwrap();

        // Verify scanner respects configuration
        let scanner_state = scanner.get_config().await;
        assert_eq!(
            scanner_state.parallel_processing,
            config.parallel_processing
        );
        assert_eq!(scanner_state.batch_size, config.batch_size);
        assert_eq!(
            scanner_state.max_file_size_bytes,
            config.max_file_size_bytes
        );

        // Test scanning with different configurations
        let scan_result = scanner.scan_kiln_directory(&test_kiln).await.unwrap();

        // All configurations should successfully scan the test kiln
        assert!(scan_result.successful_files > 0);
        assert!(scan_result.scan_duration.as_millis() > 0);

        // Performance characteristics should differ based on configuration
        let config_metrics = scanner.get_performance_metrics().await;
        assert!(config_metrics.average_scan_time_per_file.as_millis() > 0);
        assert!(config_metrics.memory_usage_mb > 0.0);

        // Resource-constrained config should use less memory
        if config.parallel_processing == 1 {
            assert!(config_metrics.memory_usage_mb < 100.0); // Rough estimate
        }
    }

    // Test configuration hot-reloading
    let mut hot_reload_scanner = create_kiln_scanner(KilnScannerConfig::default())
        .await
        .unwrap();

    // Perform initial scan
    let initial_result = hot_reload_scanner
        .scan_kiln_directory(&test_kiln)
        .await
        .unwrap();
    assert!(initial_result.successful_files > 0);

    // Update configuration
    let new_config = KilnScannerConfig {
        parallel_processing: 2,
        batch_size: 4,
        ..Default::default()
    };

    hot_reload_scanner
        .update_configuration(new_config)
        .await
        .unwrap();

    // Verify configuration was updated
    let updated_config = hot_reload_scanner.get_config().await;
    assert_eq!(updated_config.parallel_processing, 2);
    assert_eq!(updated_config.batch_size, 4);

    // Scan with new configuration
    let updated_result = hot_reload_scanner
        .scan_kiln_directory(&test_kiln)
        .await
        .unwrap();
    assert!(updated_result.successful_files > 0);

    // Performance should reflect new configuration
    let updated_metrics = hot_reload_scanner.get_performance_metrics().await;
    assert!(updated_metrics.average_scan_time_per_file.as_millis() > 0);
}

// =============================================================================
// MISSING CONSTANTS AND HELPER FUNCTIONS FOR TESTS
// =============================================================================

const NEW_DOCUMENT_CONTENT: &str = r#"---
title: "New Document"
tags: [new, document]
---

# New Document

This is a newly created document."#;

// Missing test helper functions that are referenced in the tests
async fn create_comprehensive_test_kiln() -> PathBuf {
    // This would create a comprehensive test kiln structure
    // For now, return the basic test kiln
    create_test_kiln_directory().await
}

async fn create_embedding_test_kiln() -> PathBuf {
    // This would create a test kiln optimized for embedding tests
    // For now, return the basic test kiln
    create_test_kiln_directory().await
}

async fn create_incremental_test_kiln() -> PathBuf {
    // This would create a test kiln for incremental testing
    // For now, return the basic test kiln
    create_test_kiln_directory().await
}

async fn create_error_prone_test_kiln() -> PathBuf {
    // This would create a test kiln with problematic files
    // For now, return the basic test kiln
    create_test_kiln_directory().await
}

async fn create_permission_denied_kiln() -> PathBuf {
    // This would create a test kiln with permission issues
    // For now, return the basic test kiln
    create_test_kiln_directory().await
}

async fn create_problematic_kiln_iteration(_i: usize) -> PathBuf {
    // This would create problematic kilns for testing
    // For now, return the basic test kiln
    create_test_kiln_directory().await
}

async fn create_simple_test_kiln() -> PathBuf {
    // This would create a simple test kiln
    // For now, return the basic test kiln
    create_test_kiln_directory().await
}

async fn create_configuration_test_kiln() -> PathBuf {
    // This would create a test kiln for configuration testing
    // For now, return the basic test kiln
    create_test_kiln_directory().await
}

async fn modify_file_content(
    _file_path: &PathBuf,
    _content: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // This would modify file content for testing
    // For now, do nothing
    Ok(())
}

async fn create_test_file(
    _file_path: &PathBuf,
    _content: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // This would create a test file
    // For now, do nothing
    Ok(())
}
