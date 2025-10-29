//! Unit tests for delta processing functionality
//!
//! These tests verify the hash-based change detection and delta processing
//! implementation for Phase 1 of the feature.

use anyhow::Result;
use crucible_core::parser::ParsedDocument;
use crucible_surrealdb::{
    kiln_integration::*, process_kiln_delta, KilnScannerConfig, SurrealClient, SurrealDbConfig,
};
use std::path::PathBuf;
use tempfile::TempDir;
use tokio;

/// Helper to create a test database client
async fn create_test_client() -> Result<(TempDir, SurrealClient)> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");

    let config = SurrealDbConfig {
        path: db_path.to_str().unwrap().to_string(),
        namespace: "test".to_string(),
        database: "test".to_string(),
        max_connections: Some(10),
        timeout_seconds: Some(30),
    };

    let client = SurrealClient::new(config).await?;

    // Initialize schema
    initialize_kiln_schema(&client).await?;

    Ok((temp_dir, client))
}

/// Helper to create a test markdown file
async fn create_test_file(dir: &TempDir, name: &str, content: &str) -> Result<PathBuf> {
    let file_path = dir.path().join(name);
    tokio::fs::write(&file_path, content).await?;
    Ok(file_path)
}

/// Helper to create and store a test document
async fn create_and_store_document(
    client: &SurrealClient,
    path: PathBuf,
    content: &str,
    kiln_root: &PathBuf,
) -> Result<String> {
    let mut doc = ParsedDocument::new(path);
    doc.content.plain_text = content.to_string();

    // Calculate hash using MD5 to match what convert_paths_to_file_infos uses
    doc.content_hash = format!("{:x}", md5::compute(content.as_bytes()));

    doc.file_size = content.len() as u64;

    let doc_id = store_parsed_document(client, &doc, kiln_root).await?;
    Ok(doc_id)
}

#[tokio::test]
async fn test_delete_document_embeddings_callable() -> Result<()> {
    let (_temp, client) = create_test_client().await?;

    // Test that the function is callable and doesn't crash
    // Note: Full functionality depends on the mock client implementation
    let doc_id = "notes:test123";

    // Should return without error even if no embeddings exist
    let result = delete_document_embeddings(&client, doc_id).await;
    assert!(
        result.is_ok(),
        "delete_document_embeddings should not error on non-existent document"
    );

    Ok(())
}

#[tokio::test]
async fn test_detect_changed_files_single_change() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_root = temp_dir.path().to_path_buf();
    let (_db_temp, client) = create_test_client().await?;

    // Create 3 files
    let file1 = create_test_file(&temp_dir, "note1.md", "Content 1").await?;
    let file2 = create_test_file(&temp_dir, "note2.md", "Content 2").await?;
    let file3 = create_test_file(&temp_dir, "note3.md", "Content 3").await?;

    // Store all files in database
    create_and_store_document(&client, file1.clone(), "Content 1", &kiln_root).await?;
    create_and_store_document(&client, file2.clone(), "Content 2", &kiln_root).await?;
    create_and_store_document(&client, file3.clone(), "Content 3", &kiln_root).await?;

    // Modify one file
    tokio::fs::write(&file2, "Modified Content 2").await?;

    // Test delta processing
    let config = KilnScannerConfig::default();
    let changed_paths = vec![file1.clone(), file2.clone(), file3.clone()];

    let result = process_kiln_delta(changed_paths, &client, &config, None, &kiln_root).await?;

    // Should only process the one changed file
    assert_eq!(
        result.processed_count, 1,
        "Should process exactly 1 changed file"
    );

    Ok(())
}

#[tokio::test]
async fn test_detect_changed_files_no_changes() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_root = temp_dir.path().to_path_buf();
    let (_db_temp, client) = create_test_client().await?;

    // Create and store files
    let file1 = create_test_file(&temp_dir, "note1.md", "Content 1").await?;
    let file2 = create_test_file(&temp_dir, "note2.md", "Content 2").await?;

    create_and_store_document(&client, file1.clone(), "Content 1", &kiln_root).await?;
    create_and_store_document(&client, file2.clone(), "Content 2", &kiln_root).await?;

    // Don't modify anything
    let config = KilnScannerConfig::default();
    let changed_paths = vec![file1, file2];

    let result = process_kiln_delta(changed_paths, &client, &config, None, &kiln_root).await?;

    // Should not process any files
    assert_eq!(
        result.processed_count, 0,
        "Should process 0 files when nothing changed"
    );

    Ok(())
}

#[tokio::test]
async fn test_convert_paths_handles_missing_files() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_root = temp_dir.path().to_path_buf();
    let (_db_temp, client) = create_test_client().await?;

    // Create one file, leave another non-existent
    let file1 = create_test_file(&temp_dir, "note1.md", "Content 1").await?;
    let file2 = temp_dir.path().join("nonexistent.md"); // Doesn't exist

    create_and_store_document(&client, file1.clone(), "Content 1", &kiln_root).await?;

    // Try to process both
    let config = KilnScannerConfig::default();
    let changed_paths = vec![file1, file2];

    // Should gracefully skip missing file
    let result = process_kiln_delta(changed_paths, &client, &config, None, &kiln_root).await;
    assert!(result.is_ok(), "Should handle missing files gracefully");

    Ok(())
}

#[tokio::test]
async fn test_bulk_query_efficiency() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_root = temp_dir.path().to_path_buf();
    let (_db_temp, client) = create_test_client().await?;

    // Create 10 files (simulating bulk operation)
    let mut paths = Vec::new();
    for i in 0..10 {
        let file = create_test_file(
            &temp_dir,
            &format!("note{}.md", i),
            &format!("Content {}", i),
        )
        .await?;
        create_and_store_document(&client, file.clone(), &format!("Content {}", i), &kiln_root)
            .await?;
        paths.push(file);
    }

    // Process all files - should use single bulk query internally
    let config = KilnScannerConfig::default();
    let start = std::time::Instant::now();

    let result = process_kiln_delta(paths, &client, &config, None, &kiln_root).await?;

    let duration = start.elapsed();

    // Should complete quickly due to bulk query optimization
    assert!(
        duration.as_secs() < 5,
        "Bulk query should be fast (took {:?})",
        duration
    );

    // No files should be processed since hashes match
    assert_eq!(result.processed_count, 0, "No files changed");

    Ok(())
}

#[tokio::test]
async fn test_delta_processing_performance() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_root = temp_dir.path().to_path_buf();
    let (_db_temp, client) = create_test_client().await?;

    // Create and store a file
    let file = create_test_file(&temp_dir, "note.md", "Original content").await?;
    create_and_store_document(&client, file.clone(), "Original content", &kiln_root).await?;

    // Modify the file
    tokio::fs::write(&file, "Modified content").await?;

    // Measure delta processing time
    let config = KilnScannerConfig::default();
    let start = std::time::Instant::now();

    let result = process_kiln_delta(vec![file], &client, &config, None, &kiln_root).await?;

    let duration = start.elapsed();

    // Performance target: ≤1 second for single file
    assert!(
        duration.as_secs() <= 1,
        "Single file delta processing should complete within 1 second (took {:?})",
        duration
    );

    assert_eq!(result.processed_count, 1, "Should process 1 file");

    Ok(())
}

#[tokio::test]
async fn test_empty_input_handling() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_root = temp_dir.path().to_path_buf();
    let (_db_temp, client) = create_test_client().await?;

    let config = KilnScannerConfig::default();
    let result = process_kiln_delta(vec![], &client, &config, None, &kiln_root).await?;

    assert_eq!(result.processed_count, 0);
    assert_eq!(result.failed_count, 0);
    assert!(result.errors.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_new_files_detected_as_changed() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let kiln_root = temp_dir.path().to_path_buf();
    let (_db_temp, client) = create_test_client().await?;

    // Create files but DON'T store them in database
    let file1 = create_test_file(&temp_dir, "new1.md", "New content 1").await?;
    let file2 = create_test_file(&temp_dir, "new2.md", "New content 2").await?;

    let config = KilnScannerConfig::default();
    let result = process_kiln_delta(vec![file1, file2], &client, &config, None, &kiln_root).await?;

    // Should process both new files
    assert_eq!(result.processed_count, 2, "Should process both new files");

    Ok(())
}
