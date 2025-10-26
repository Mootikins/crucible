//! Vault Scanner Compilation Tests
//!
//! Minimal tests to verify vault scanner compiles and basic functionality works

use crucible_core::parser::{
    DocumentContent, Frontmatter, FrontmatterFormat, Heading, ParsedDocument, Tag,
};
use crucible_surrealdb::vault_scanner;
use std::path::PathBuf;
use tempfile::TempDir;
use vault_scanner::{create_vault_scanner, VaultScannerConfig};

#[tokio::test]
async fn test_vault_scanner_compilation() {
    // Test that we can create a vault scanner configuration
    let config = VaultScannerConfig::default();
    assert!(config.max_file_size_bytes > 0);
    assert!(config.max_recursion_depth > 0);
    assert!(config.recursive_scan);
}

#[tokio::test]
async fn test_vault_scanner_creation() {
    let config = VaultScannerConfig::default();
    let scanner_result = create_vault_scanner(config).await;
    assert!(scanner_result.is_ok());
}

#[tokio::test]
async fn test_vault_scanner_basic_scan() {
    // Create temporary directory with test files
    let temp_dir = TempDir::new().unwrap();
    let test_path = temp_dir.path().to_path_buf();

    // Create test markdown files
    tokio::fs::write(
        test_path.join("test1.md"),
        "# Test Document\n\nContent here.",
    )
    .await
    .unwrap();
    tokio::fs::write(test_path.join("test2.txt"), "Not a markdown file")
        .await
        .unwrap();

    // Test scanning
    let config = VaultScannerConfig::default();
    let mut scanner = create_vault_scanner(config).await.unwrap();

    let result = scanner.scan_vault_directory(&test_path).await.unwrap();

    // Verify results
    assert!(result.total_files_found >= 1); // At least 1 markdown file
    assert!(result.markdown_files_found >= 1); // At least 1 markdown file
    assert!(result.successful_files >= 1); // At least 1 successful file
}

#[tokio::test]
async fn test_vault_scanner_configuration() {
    // Test default configuration
    let config = VaultScannerConfig::default();
    assert_eq!(config.max_file_size_bytes, 50 * 1024 * 1024);
    assert_eq!(config.max_recursion_depth, 10);
    assert!(config.recursive_scan);
    assert!(!config.include_hidden_files);
    assert_eq!(
        config.file_extensions,
        vec!["md".to_string(), "markdown".to_string()]
    );
    assert!(config.enable_embeddings);
    assert!(config.process_embeds);
    assert!(config.process_wikilinks);

    // Test configuration presets
    let large_config = VaultScannerConfig::for_large_vault();
    assert!(large_config.parallel_processing >= 8);
    assert!(large_config.batch_size >= 32);
    assert!(large_config.enable_incremental);

    let small_config = VaultScannerConfig::for_small_vault();
    assert_eq!(small_config.parallel_processing, 1);
    assert_eq!(small_config.batch_size, 4);
    assert!(!small_config.enable_incremental);

    let resource_config = VaultScannerConfig::for_resource_constrained();
    assert_eq!(resource_config.parallel_processing, 1);
    assert_eq!(resource_config.batch_size, 2);
    assert!(!resource_config.enable_embeddings);
}

#[tokio::test]
async fn test_vault_scanner_config_serialization() {
    // Test configuration serialization/deserialization
    let default_config = VaultScannerConfig::default();
    let serialized = serde_json::to_string(&default_config).unwrap();
    let deserialized: VaultScannerConfig = serde_json::from_str(&serialized).unwrap();
    assert_eq!(default_config, deserialized);
}

#[tokio::test]
async fn test_vault_scanner_metrics() {
    let config = VaultScannerConfig::default();
    let scanner = create_vault_scanner(config).await.unwrap();

    let metrics = scanner.get_performance_metrics().await;
    assert!(metrics.memory_usage_mb > 0);
    assert_eq!(metrics.files_scanned, 0);
    assert_eq!(metrics.files_processed, 0);
}

// Helper function to create test document
fn create_test_parsed_document() -> ParsedDocument {
    let mut doc = ParsedDocument::new(PathBuf::from("/test/notes/sample.md"));

    // Add frontmatter
    let frontmatter = Frontmatter::new(
        r#"title: "Test Document"
tags: [rust, programming]
author: "Test Author"
created: "2024-01-01"#
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
    doc.tags.push(Tag::new("rust", 45));

    doc
}
