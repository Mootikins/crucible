//! Simple Embed Relationship Test
//!
//! Minimal test to verify embed functionality works

use crucible_core::{
    parser::{ParsedDocument, Frontmatter, FrontmatterFormat, Wikilink, DocumentContent, Heading},
};
use crucible_surrealdb::{SurrealClient, vault_integration};
use vault_integration::{EmbedMetadata, PlaceholderMetadata};
use std::path::PathBuf;
use chrono::Utc;

#[tokio::test]
async fn test_simple_embed_relationship() {
    // Create a test document with embeds
    let mut doc = ParsedDocument::new(PathBuf::from("/test/notes/embed_test.md"));

    // Add frontmatter
    let frontmatter = Frontmatter::new(
        r#"title: "Embed Test Document""#.to_string(),
        FrontmatterFormat::Yaml
    );
    doc.frontmatter = Some(frontmatter);

    // Add a simple embed
    doc.wikilinks.push(Wikilink::embed("Target Document", 20));

    // Add content
    let content = r#"# Embed Test Document

This document has an embed: ![[Target Document]]"#;

    doc.content = DocumentContent::new()
        .with_plain_text(content.to_string());
    doc.content.add_heading(Heading::new(1, "Embed Test Document", 0));

    doc.parsed_at = Utc::now();
    doc.content_hash = "embed_test_hash".to_string();
    doc.file_size = 1024;

    // Create SurrealDB connection
    let client = SurrealClient::new_memory().await.unwrap();

    // Initialize the database schema
    vault_integration::initialize_vault_schema(&client).await.unwrap();

    // Store the main document
    let doc_id = vault_integration::store_parsed_document(&client, &doc).await.unwrap();

    // Create embed relationships
    vault_integration::create_embed_relationships(&client, &doc_id, &doc).await.unwrap();

    // Query embed relationships
    let embedded_docs = vault_integration::get_embedded_documents(&client, &doc_id).await.unwrap();

    // Should have one embedded document
    assert_eq!(embedded_docs.len(), 1);
    assert_eq!(embedded_docs[0].title(), "Target Document");

    // Query embed metadata
    let embed_metadata = vault_integration::get_embed_metadata(&client, &doc_id).await.unwrap();
    assert_eq!(embed_metadata.len(), 1);

    let metadata = &embed_metadata[0];
    assert_eq!(metadata.target, "Target Document");
    assert!(metadata.is_embed);
    assert!(metadata.heading_ref.is_none());
    assert!(metadata.block_ref.is_none());
    assert_eq!(metadata.position, 20);

    println!("✅ Simple embed relationship test passed!");
}

#[tokio::test]
async fn test_find_document_by_title() {
    // Create a test document
    let mut doc = ParsedDocument::new(PathBuf::from("/test/notes/find_test.md"));

    // Add frontmatter
    let frontmatter = Frontmatter::new(
        r#"title: "Findable Document""#.to_string(),
        FrontmatterFormat::Yaml
    );
    doc.frontmatter = Some(frontmatter);

    // Add content
    doc.content = DocumentContent::new()
        .with_plain_text("# Findable Document\n\nThis is a test document.".to_string());
    doc.content.add_heading(Heading::new(1, "Findable Document", 0));

    doc.parsed_at = Utc::now();
    doc.content_hash = "find_test_hash".to_string();
    doc.file_size = 512;

    // Create SurrealDB connection
    let client = SurrealClient::new_memory().await.unwrap();

    // Initialize the database schema
    vault_integration::initialize_vault_schema(&client).await.unwrap();

    // Store the document
    let doc_id = vault_integration::store_parsed_document(&client, &doc).await.unwrap();

    // Find the document by title
    let found_doc = vault_integration::find_document_by_title(&client, "Findable Document").await.unwrap();

    assert!(found_doc.is_some(), "Should find the document by title");
    let found = found_doc.unwrap();
    assert_eq!(found.title(), "Findable Document");
    assert_eq!(found.path, doc.path);

    // Try to find a non-existent document
    let not_found = vault_integration::find_document_by_title(&client, "Nonexistent Document").await.unwrap();
    assert!(not_found.is_none());

    println!("✅ Find document by title test passed!");
}