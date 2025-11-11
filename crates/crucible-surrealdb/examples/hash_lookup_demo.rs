//! Hash Lookup Demo
//!
//! This example demonstrates how to use the new file hash lookup functionality
//! for efficient change detection during file scanning.

use anyhow::Result;
use crucible_surrealdb::{
    create_kiln_scanner_with_embeddings, lookup_file_hash, lookup_file_hashes_batch,
    BatchLookupConfig, EmbeddingConfig, EmbeddingThreadPool, HashLookupCache, KilnScannerConfig,
    SurrealClient,
};
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::fs;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🔍 Hash Lookup Demo for Crucible");
    println!("================================");

    // Create a temporary kiln directory
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().to_path_buf();
    println!("📁 Created temporary kiln: {}", kiln_path.display());

    // Create some test files
    setup_test_files(&kiln_path).await?;
    println!("📝 Created test files");

    // Initialize database client
    let db_config = crucible_surrealdb::SurrealDbConfig {
        path: ":memory:".to_string(),
        ..Default::default()
    };
    let client = SurrealClient::new(db_config).await?;
    println!("🗄️  Initialized database (schema auto-initialized)");

    // Create scanner with hash lookup enabled
    let mut config = KilnScannerConfig::default();
    config.enable_incremental = true;
    config.track_file_changes = true;
    config.change_detection_method = crucible_surrealdb::ChangeDetectionMethod::ContentHash;

    let embedding_config = EmbeddingConfig::default();
    let embedding_pool = EmbeddingThreadPool::new(embedding_config).await?;

    let mut scanner = create_kiln_scanner_with_embeddings(config, &client, &embedding_pool).await?;
    println!("🔬 Created scanner with hash lookup");

    // === Demo 1: Basic Hash Lookup ===
    println!("\n=== Demo 1: Basic Hash Lookup ===");

    let hash_result = lookup_file_hash(&client, "test1.md").await?;
    match hash_result {
        Some(stored_hash) => {
            println!("✅ Found stored hash for test1.md:");
            println!("   Record ID: {}", stored_hash.record_id);
            println!("   File Hash: {}...", &stored_hash.file_hash[..16]);
            println!("   File Size: {} bytes", stored_hash.file_size);
        }
        None => {
            println!("ℹ️  No stored hash found for test1.md (expected for first run)");
        }
    }

    // === Demo 2: Batch Hash Lookup ===
    println!("\n=== Demo 2: Batch Hash Lookup ===");

    let paths = vec![
        "test1.md".to_string(),
        "test2.md".to_string(),
        "test3.md".to_string(),
        "nonexistent.md".to_string(),
    ];

    let batch_config = BatchLookupConfig {
        max_batch_size: 50,
        use_parameterized_queries: true,
        enable_session_cache: false,
    };

    let batch_result = lookup_file_hashes_batch(&client, &paths, Some(batch_config)).await?;
    println!("📊 Batch lookup results:");
    println!("   Total queried: {}", batch_result.total_queried);
    println!("   Files found: {}", batch_result.found_files.len());
    println!("   Files missing: {}", batch_result.missing_files.len());
    println!(
        "   Database round trips: {}",
        batch_result.database_round_trips
    );

    // === Demo 3: Scan with Hash Lookup ===
    println!("\n=== Demo 3: Scan with Hash Lookup ===");

    let scan_result = scanner
        .scan_kiln_directory_with_hash_lookup(&kiln_path)
        .await?;
    println!("📈 Scan results:");
    println!("   Total files: {}", scan_result.total_files_found);
    println!("   Markdown files: {}", scan_result.markdown_files_found);
    println!("   Scan duration: {:?}", scan_result.scan_duration);

    if let Some(hash_lookup) = &scan_result.hash_lookup_results {
        println!("🔍 Hash lookup results:");
        println!("   Files found in DB: {}", hash_lookup.found_files.len());
        println!("   New files: {}", hash_lookup.missing_files.len());
        println!(
            "   Database round trips: {}",
            hash_lookup.database_round_trips
        );
    }

    // === Demo 4: Change Detection ===
    println!("\n=== Demo 4: Change Detection ===");

    if let Some(summary) = scanner.get_change_detection_summary(&scan_result) {
        println!("🔄 Change detection summary:");
        println!("   Total files: {}", summary.total_files);
        println!("   Unchanged files: {}", summary.unchanged_files);
        println!("   Changed files: {}", summary.changed_files);
        println!("   New files: {}", summary.new_files);
    }

    // Get files that need processing
    let files_needing_processing = scanner.get_files_needing_processing(&scan_result);
    println!(
        "📋 Files needing processing: {}",
        files_needing_processing.len()
    );
    for file in &files_needing_processing {
        println!("   - {}", file.relative_path);
    }

    // === Demo 5: Hash Cache Performance ===
    println!("\n=== Demo 5: Hash Cache Performance ===");

    let cache_stats = scanner.get_hash_cache_stats();
    println!("💾 Cache statistics:");
    println!("   Entries: {}", cache_stats.entries);
    println!("   Hits: {}", cache_stats.hits);
    println!("   Misses: {}", cache_stats.misses);
    println!("   Hit rate: {:.1}%", cache_stats.hit_rate * 100.0);

    // Perform a second scan to see cache benefits
    println!("🔄 Performing second scan to test cache...");
    let scan_result2 = scanner
        .scan_kiln_directory_with_hash_lookup(&kiln_path)
        .await?;

    let cache_stats2 = scanner.get_hash_cache_stats();
    println!("💾 Updated cache statistics:");
    println!("   Entries: {}", cache_stats2.entries);
    println!("   Hits: {}", cache_stats2.hits);
    println!("   Misses: {}", cache_stats2.misses);
    println!("   Hit rate: {:.1}%", cache_stats2.hit_rate * 100.0);

    // === Demo 6: File Change Simulation ===
    println!("\n=== Demo 6: File Change Simulation ===");

    // Modify a file
    fs::write(
        kiln_path.join("test1.md"),
        "# Modified Test Note\n\nThis content has been changed.",
    )
    .await?;
    println!("✏️  Modified test1.md");

    // Scan again to detect changes
    let scan_result3 = scanner
        .scan_kiln_directory_with_hash_lookup(&kiln_path)
        .await?;

    if let Some(summary) = scanner.get_change_detection_summary(&scan_result3) {
        println!("🔄 Updated change detection:");
        println!("   Total files: {}", summary.total_files);
        println!("   Unchanged files: {}", summary.unchanged_files);
        println!("   Changed files: {}", summary.changed_files);
        println!("   New files: {}", summary.new_files);
    }

    // === Demo 7: Individual File Update Check ===
    println!("\n=== Demo 7: Individual File Update Check ===");

    // Check if specific file needs update
    let needs_update = crucible_surrealdb::check_file_needs_update(
        &client,
        "test1.md",
        "new_hash_value_1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
    )
    .await?;
    println!("🔍 test1.md needs update with fake hash: {}", needs_update);

    println!("\n✅ Hash lookup demo completed successfully!");
    Ok(())
}

/// Setup test files in the temporary directory
async fn setup_test_files(kiln_path: &PathBuf) -> Result<()> {
    // Create test markdown files
    fs::write(
        kiln_path.join("test1.md"),
        r#"# Test Note 1

This is the first test note.

## Section 1

Some content here.

## Section 2

More content here.
"#,
    )
    .await?;

    fs::write(
        kiln_path.join("test2.md"),
        r#"# Test Note 2

This is the second test note with different content.

## Features

- Feature 1
- Feature 2
- Feature 3

## Conclusion

This note demonstrates file hashing.
"#,
    )
    .await?;

    fs::write(
        kiln_path.join("test3.md"),
        r#"# Test Note 3

This note contains code examples:

```rust
fn hello_world() {
    println!("Hello, world!");
}
```

## Usage

Use the function above to greet the world.
"#,
    )
    .await?;

    // Create a subdirectory with more files
    fs::create_dir_all(kiln_path.join("subdir")).await?;

    fs::write(
        kiln_path.join("subdir/nested.md"),
        r#"# Nested Note

This note is in a subdirectory.
"#,
    )
    .await?;

    // Create a non-markdown file (should be ignored by hash lookup for markdown)
    fs::write(
        kiln_path.join("readme.txt"),
        "This is a text file, not markdown.",
    )
    .await?;

    Ok(())
}
