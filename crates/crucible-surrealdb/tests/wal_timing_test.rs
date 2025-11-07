//! Minimal test to verify SurrealDB write-ahead-log timing hypothesis
//!
//! This test isolates the specific issue of whether writes are immediately
//! visible to subsequent reads, or if there's a delay due to WAL buffering.

use crucible_surrealdb::{SurrealClient, SurrealDbConfig};
use std::time::Instant;
use tokio::time::Duration;

/// Test: Write → Immediate Read → Verify visibility
///
/// This tests whether a write is immediately visible to a subsequent read
/// with NO sleep in between. If this fails, it confirms WAL buffering is the issue.
#[tokio::test]
async fn test_immediate_write_read_visibility() {
    println!("\n=== Test 1: Immediate Write-Read Visibility ===\n");

    // Setup: Create in-memory SurrealDB (same config as e2e tests use)
    let config = SurrealDbConfig::memory();
    let client = SurrealClient::new(config).await
        .expect("Failed to create SurrealDB client");

    // Write a hash record
    let write_start = Instant::now();
    let write_sql = "CREATE notes:test_file SET file_hash = 'abc123def456', path = 'test.md', file_size = 1024";
    client.query(write_sql, &[]).await
        .expect("Failed to write record");
    let write_time = write_start.elapsed();
    println!("✓ Write completed in {:?}", write_time);

    // Immediately read it back (no sleep)
    let read_start = Instant::now();
    let read_sql = "SELECT file_hash, path FROM notes:test_file";
    let result = client.query(read_sql, &[]).await
        .expect("Failed to read record");
    let read_time = read_start.elapsed();

    // Extract the records
    let records = &result.records;

    println!("✓ Read completed in {:?}", read_time);
    println!("  Records found: {}", records.len());

    if !records.is_empty() {
        println!("  First record: {:?}", records[0].data);
    }

    // CRITICAL ASSERTION: Can we read what we just wrote with NO sleep?
    assert_eq!(records.len(), 1,
        "❌ FAILED: Write not immediately visible! This confirms WAL buffering issue.");

    assert_eq!(
        records[0].data.get("file_hash").and_then(|v| v.as_str()),
        Some("abc123def456"),
        "❌ FAILED: Hash value mismatch"
    );

    println!("\n✅ SUCCESS: Writes are immediately visible (no WAL buffering issue)\n");
}

/// Test: Write multiple records → Batch read → Count visibility
///
/// This mirrors what the e2e tests do: write 8 records, then query for all of them.
#[tokio::test]
async fn test_batch_write_visibility() {
    println!("\n=== Test 2: Batch Write Visibility (8 records) ===\n");

    let config = SurrealDbConfig::memory();
    let client = SurrealClient::new(config).await.unwrap();

    // Write 8 records (like e2e test does)
    println!("Writing 8 records...");
    for i in 0..8 {
        let write_sql = format!(
            "CREATE notes:file{} SET file_hash = 'hash{}', path = 'file{}.md', file_size = 1024",
            i, i, i
        );
        client.query(&write_sql, &[]).await
            .expect(&format!("Failed to write record {}", i));
    }
    println!("✓ All 8 records written");

    // Immediately query for count (no sleep)
    let count_sql = "SELECT COUNT() AS count FROM notes GROUP ALL";
    let result = client.query(count_sql, &[]).await
        .expect("Failed to count records");

    let count_records = &result.records;

    let count = count_records[0].data.get("count")
        .and_then(|v| v.as_u64())
        .expect("Failed to extract count") as usize;

    println!("✓ Query returned {} records", count);

    // CRITICAL ASSERTION: Are all 8 writes immediately visible?
    assert_eq!(count, 8,
        "❌ FAILED: Expected 8 records, found {}. This confirms WAL buffering!", count);

    println!("\n✅ SUCCESS: All 8 writes immediately visible\n");
}

/// Test: Write → Sleep various durations → Read
///
/// This measures exactly how long we need to sleep for writes to become visible.
#[tokio::test]
async fn test_sleep_duration_needed() {
    println!("\n=== Test 3: Sleep Duration Analysis ===\n");

    let sleep_durations = [0, 10, 50, 100, 250, 500, 1000, 2000];

    for &sleep_ms in &sleep_durations {
        let config = SurrealDbConfig::memory();
        let client = SurrealClient::new(config).await.unwrap();

        // Write a record
        let record_id = format!("test_{}", sleep_ms);
        let write_sql = format!(
            "CREATE notes:{} SET file_hash = 'hash', path = 'test.md'",
            record_id
        );
        client.query(&write_sql, &[]).await.unwrap();

        // Sleep for specified duration
        if sleep_ms > 0 {
            tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
        }

        // Try to read it back
        let read_sql = format!("SELECT * FROM notes:{}", record_id);
        let result = client.query(&read_sql, &[]).await.unwrap();
        let records = &result.records;
        let visible = !records.is_empty();

        println!("  {}ms sleep: visible = {} (found {} records)",
            sleep_ms, visible, records.len());

        if !visible && sleep_ms == 0 {
            println!("    ❌ Not visible immediately - WAL buffering confirmed!");
        } else if visible && sleep_ms == 0 {
            println!("    ✅ Visible immediately - no WAL buffering issue!");
        }
    }

    println!("\n");
}

/// Test: Compare Memory backend vs RocksDB backend
///
/// This determines if the issue is specific to RocksDB or affects all backends.
#[tokio::test]
async fn test_memory_vs_rocksdb_backend() {
    println!("\n=== Test 4: Memory vs RocksDB Backend Comparison ===\n");

    // Test with memory backend
    println!("Testing MEMORY backend:");
    let mem_config = SurrealDbConfig::memory();
    let mem_client = SurrealClient::new(mem_config).await.unwrap();

    mem_client.query(
        "CREATE notes:mem_test SET file_hash = 'hash123', path = 'test.md'",
        &[]
    ).await.unwrap();

    let mem_result = mem_client.query("SELECT * FROM notes:mem_test", &[]).await.unwrap();
    let mem_records = &mem_result.records;
    let mem_visible = !mem_records.is_empty();

    println!("  Memory backend: immediate visibility = {}", mem_visible);

    // Test with RocksDB backend (if path exists)
    println!("\nTesting ROCKSDB backend:");
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let rocks_config = SurrealDbConfig {
        namespace: "test".to_string(),
        database: "test".to_string(),
        path: temp_dir.path().to_str().unwrap().to_string(),
        max_connections: Some(10),
        timeout_seconds: Some(30),
    };

    let rocks_client = SurrealClient::new(rocks_config).await.unwrap();

    rocks_client.query(
        "CREATE notes:rocks_test SET file_hash = 'hash456', path = 'test.md'",
        &[]
    ).await.unwrap();

    let rocks_result = rocks_client.query("SELECT * FROM notes:rocks_test", &[]).await.unwrap();
    let rocks_records = &rocks_result.records;
    let rocks_visible = !rocks_records.is_empty();

    println!("  RocksDB backend: immediate visibility = {}", rocks_visible);

    // Compare
    if mem_visible && !rocks_visible {
        println!("\n❌ RocksDB has WAL buffering, Memory does not");
    } else if mem_visible && rocks_visible {
        println!("\n✅ Both backends have immediate visibility");
    } else {
        println!("\n❓ Unexpected: neither backend shows immediate visibility");
    }

    println!("\n");
}

/// Test: Exact scenario from failing e2e test
///
/// This replicates the exact pattern that fails in e2e tests:
/// 1. Write 8 hashes
/// 2. Query for all hashes
/// 3. Verify all 8 are found
#[tokio::test]
async fn test_exact_e2e_scenario() {
    println!("\n=== Test 5: Exact E2E Scenario Replication ===\n");

    let config = SurrealDbConfig::memory();
    let client = SurrealClient::new(config).await.unwrap();

    // Simulate: Initial scan finds 8 files, all are NEW
    println!("Step 1: Process 8 new files (store hashes)");
    let files = vec![
        ("getting-started.md", "hash1"),
        ("api-reference.md", "hash2"),
        ("troubleshooting.md", "hash3"),
        ("best-practices.md", "hash4"),
        ("changelog.md", "hash5"),
        ("docs/advanced-guide.md", "hash6"),
        ("notes/quick-note.md", "hash7"),
        ("tools/automation.md", "hash8"),
    ];

    for (path, hash) in &files {
        let sql = format!(
            "CREATE notes:`{}` SET file_hash = '{}', path = '{}', file_size = 1024",
            path.replace('/', "_").replace('.', "_"),
            hash,
            path
        );
        client.query(&sql, &[]).await
            .expect(&format!("Failed to store hash for {}", path));
    }
    println!("✓ Stored {} hashes", files.len());

    // Simulate: Second scan queries for existing hashes
    println!("\nStep 2: Query for all stored hashes (immediate, no sleep)");
    let query_sql = "SELECT path, file_hash FROM notes";
    let result = client.query(query_sql, &[]).await
        .expect("Failed to query hashes");

    let records = &result.records;

    println!("✓ Found {} records", records.len());

    // Print what we found
    for (i, record) in records.iter().enumerate() {
        println!("  Record {}: path='{}', hash='{}'",
            i + 1,
            record.data.get("path").and_then(|v| v.as_str()).unwrap_or(""),
            record.data.get("file_hash").and_then(|v| v.as_str()).unwrap_or("")
        );
    }

    // CRITICAL ASSERTION: Do we find all 8 immediately?
    if records.len() < 8 {
        println!("\n❌ FAILURE: Only found {}/8 records immediately!", records.len());
        println!("   This confirms WAL buffering is causing e2e test failures!");
        println!("   Missing {} records that were just written.", 8 - records.len());
    } else {
        println!("\n✅ SUCCESS: All 8 records immediately visible");
        println!("   WAL buffering is NOT the issue - look elsewhere!");
    }

    // This assertion will fail if WAL buffering is the issue
    assert_eq!(records.len(), 8,
        "Expected all 8 records to be immediately visible after write");
}
