///! Minimal test to isolate hash storage issue
///!
///! This test creates ONE file, processes it, then checks if the hash was stored correctly.

use anyhow::Result;
use crucible_surrealdb::{SurrealClient, SurrealDbConfig};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_hash_storage_single_file() -> Result<()> {
    println!("\n=== Single File Hash Storage Test ===\n");

    // Create temp vault
    let temp_dir = TempDir::new()?;
    let vault_path = temp_dir.path();

    // Create a single test file
    let test_file = vault_path.join("test.md");
    std::fs::write(&test_file, "# Test\nContent here")?;
    println!("Created file: {}", test_file.display());

    // Setup database
    let db_config = SurrealDbConfig {
        namespace: "crucible".to_string(),
        database: "test".to_string(),
        path: vault_path.join(".crucible/db").to_string_lossy().to_string(),
        max_connections: Some(10),
        timeout_seconds: Some(30),
    };

    let client = Arc::new(SurrealClient::new(db_config).await?);
    crucible_surrealdb::kiln_integration::initialize_kiln_schema(&client).await?;

    // Parse and store the file
    let parsed_doc = crucible_surrealdb::parse_file_to_document(&test_file).await?;
    println!("Parsed document hash: {}", parsed_doc.content_hash);

    let doc_id = crucible_surrealdb::kiln_integration::store_parsed_document(
        &client,
        &parsed_doc,
        vault_path,
    ).await?;
    println!("Stored document with ID: {}", doc_id);

    // Query the database directly to see what was stored
    let query_sql = "SELECT id, path, file_hash FROM notes";
    let result = client.query(query_sql, &[]).await?;

    println!("\n--- Database Contents ---");
    for (i, record) in result.records.iter().enumerate() {
        println!("Record {}:", i + 1);
        println!("  id: {:?}", record.id);
        println!("  data: {:?}", record.data);
    }

    // Check if we can find the file by path
    let path_query = "SELECT id, path, file_hash FROM notes WHERE path = $path";
    let path_result = client.query(path_query, &[serde_json::json!({"path": "test.md"})]).await?;

    println!("\n--- Query by path='test.md' ---");
    println!("Found {} records", path_result.records.len());
    for record in &path_result.records {
        println!("  Record: {:?}", record.data);
    }

    assert_eq!(path_result.records.len(), 1, "Should find exactly one record by path");

    // Extract the stored hash
    let stored_hash = path_result.records[0]
        .data
        .get("file_hash")
        .and_then(|v| v.as_str())
        .expect("file_hash field should exist");

    println!("\nParsed hash:  {}", parsed_doc.content_hash);
    println!("Stored hash:  {}", stored_hash);

    assert_eq!(parsed_doc.content_hash, stored_hash, "Parsed and stored hashes should match");

    println!("\n✅ Test passed!\n");
    Ok(())
}
