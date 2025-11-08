//! Demonstration of Mock Implementations for Testing
//!
//! This example shows how to use the comprehensive mock implementations
//! provided in the test_support module for testing purposes.
//!
//! Run with:
//! ```sh
//! cargo run --example test_mocks_demo
//! ```

use crucible_core::hashing::algorithm::HashingAlgorithm;
use crucible_core::storage::traits::{BlockOperations, StorageManagement};
use crucible_core::test_support::mocks::{
    MockChangeDetector, MockContentHasher, MockHashLookupStorage, MockHashingAlgorithm, MockStorage,
};
use crucible_core::traits::change_detection::{ChangeDetector, ContentHasher, HashLookupStorage};
use crucible_core::types::hashing::{FileHash, FileHashInfo, HashAlgorithm};
use std::path::Path;
use std::time::SystemTime;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Mock Implementations Demo ===\n");

    // ========================================================================
    // 1. MockHashingAlgorithm - Deterministic Hashing
    // ========================================================================
    println!("1. Mock Hashing Algorithm");
    println!("   Simple, deterministic hashing for tests\n");

    let hasher = MockHashingAlgorithm::new();
    let data = b"test data for hashing";
    let hash = hasher.hash(data);

    println!("   Input:     {:?}", std::str::from_utf8(data).unwrap());
    println!("   Hash:      {}", hasher.to_hex(&hash));
    println!("   Algorithm: {}", hasher.algorithm_name());
    println!("   Length:    {} bytes\n", hash.len());

    // Verify determinism
    let hash2 = hasher.hash(data);
    assert_eq!(hash, hash2, "Hash should be deterministic");
    println!("   ✓ Verified: Same input produces same hash\n");

    // ========================================================================
    // 2. MockStorage - In-Memory Storage with Call Tracking
    // ========================================================================
    println!("2. Mock Storage");
    println!("   In-memory storage with operation tracking\n");

    let storage = MockStorage::new();

    // Store some blocks
    storage.store_block("hash1", b"data one").await?;
    storage.store_block("hash2", b"data two").await?;
    storage.store_block("hash3", b"data three").await?;

    println!("   Stored 3 blocks");

    // Retrieve a block
    let retrieved = storage.get_block("hash2").await?;
    assert_eq!(retrieved, Some(b"data two".to_vec()));
    println!("   ✓ Retrieved block 'hash2'");

    // Check statistics
    let stats = storage.stats();
    println!("\n   Storage Statistics:");
    println!("   - Store operations: {}", stats.store_count);
    println!("   - Get operations:   {}", stats.get_count);
    println!("   - Total blocks:     {}", storage.block_count());
    println!("   - Bytes stored:     {}", stats.total_bytes_stored);
    println!("   - Bytes retrieved:  {}\n", stats.total_bytes_retrieved);

    // Error simulation
    storage.set_simulate_errors(true, "Simulated storage failure");
    let error_result = storage.store_block("hash4", b"will fail").await;
    assert!(error_result.is_err());
    println!("   ✓ Error simulation works\n");

    // ========================================================================
    // 3. MockContentHasher - Configurable Content Hashing
    // ========================================================================
    println!("3. Mock Content Hasher");
    println!("   Configurable hashing with operation tracking\n");

    let content_hasher = MockContentHasher::new();

    // Configure specific hash for a path
    let custom_hash = vec![0x42; 32];
    content_hasher.set_file_hash("test.md", custom_hash.clone());

    // Hash file
    let file_hash = content_hasher.hash_file(Path::new("test.md")).await?;
    assert_eq!(file_hash.as_bytes(), &custom_hash[..]);
    println!("   ✓ Custom hash returned for configured path");

    // Hash unconfigured path (uses fallback)
    let fallback_hash = content_hasher.hash_file(Path::new("other.md")).await?;
    println!(
        "   ✓ Fallback hash for unconfigured path: {}",
        fallback_hash.to_hex()
    );

    // Check operation counts
    let (file_count, block_count) = content_hasher.operation_counts();
    println!("\n   Operation Counts:");
    println!("   - File hashes:  {}", file_count);
    println!("   - Block hashes: {}\n", block_count);

    // ========================================================================
    // 4. MockHashLookupStorage - Hash Storage and Lookup
    // ========================================================================
    println!("4. Mock Hash Lookup Storage");
    println!("   In-memory hash storage with batch operations\n");

    let hash_storage = MockHashLookupStorage::new();

    // Store some file hashes
    let file_infos = vec![
        FileHashInfo::new(
            FileHash::new([1u8; 32]),
            1024,
            SystemTime::now(),
            HashAlgorithm::Blake3,
            "file1.md".to_string(),
        ),
        FileHashInfo::new(
            FileHash::new([2u8; 32]),
            2048,
            SystemTime::now(),
            HashAlgorithm::Blake3,
            "file2.md".to_string(),
        ),
    ];

    hash_storage.store_hashes(&file_infos).await?;
    println!("   Stored 2 file hashes");

    // Lookup single file
    let lookup_result = hash_storage.lookup_file_hash("file1.md").await?;
    assert!(lookup_result.is_some());
    println!("   ✓ Found file1.md in storage");

    // Batch lookup
    let paths = vec![
        "file1.md".to_string(),
        "file2.md".to_string(),
        "missing.md".to_string(),
    ];
    let batch_result = hash_storage.lookup_file_hashes_batch(&paths, None).await?;

    println!("\n   Batch Lookup Results:");
    println!("   - Total queried:  {}", batch_result.total_queried);
    println!("   - Found:          {}", batch_result.found_files.len());
    println!("   - Missing:        {}", batch_result.missing_files.len());
    println!(
        "   - DB roundtrips:  {}\n",
        batch_result.database_round_trips
    );

    // Check operation counts
    let (lookups, batch_lookups, stores) = hash_storage.operation_counts();
    println!("   Operation Counts:");
    println!("   - Single lookups: {}", lookups);
    println!("   - Batch lookups:  {}", batch_lookups);
    println!("   - Stores:         {}\n", stores);

    // ========================================================================
    // 5. MockChangeDetector - Change Detection
    // ========================================================================
    println!("5. Mock Change Detector");
    println!("   Complete change detection with metrics\n");

    let detector = MockChangeDetector::new();

    // Add some stored files to the detector's storage
    detector.storage().store_hashes(&file_infos).await?;

    // Create current files (one changed, one new, one unchanged)
    let current_files = vec![
        FileHashInfo::new(
            FileHash::new([1u8; 32]), // Same hash - unchanged
            1024,
            SystemTime::now(),
            HashAlgorithm::Blake3,
            "file1.md".to_string(),
        ),
        FileHashInfo::new(
            FileHash::new([99u8; 32]), // Different hash - changed
            2048,
            SystemTime::now(),
            HashAlgorithm::Blake3,
            "file2.md".to_string(),
        ),
        FileHashInfo::new(
            FileHash::new([3u8; 32]), // New file
            4096,
            SystemTime::now(),
            HashAlgorithm::Blake3,
            "file3.md".to_string(),
        ),
    ];

    // Detect changes with metrics
    let result = detector.detect_changes_with_metrics(&current_files).await?;

    println!("   Change Detection Results:");
    println!("   - Unchanged:    {}", result.changes.unchanged.len());
    println!("   - Changed:      {}", result.changes.changed.len());
    println!("   - New:          {}", result.changes.new.len());
    println!("   - Deleted:      {}", result.changes.deleted.len());
    println!("   - Has changes:  {}", result.has_changes());

    println!("\n   Performance Metrics:");
    println!("   - Total files:      {}", result.metrics.total_files);
    println!("   - Changed files:    {}", result.metrics.changed_files);
    println!("   - Skipped files:    {}", result.metrics.skipped_files);
    println!(
        "   - Detection time:   {:?}",
        result.metrics.change_detection_time
    );
    println!(
        "   - Files/second:     {:.0}",
        result.metrics.files_per_second
    );
    println!(
        "   - Cache hit rate:   {:.1}%\n",
        result.metrics.cache_hit_rate * 100.0
    );

    // ========================================================================
    // Summary
    // ========================================================================
    println!("=== Summary ===\n");
    println!("All mock implementations work correctly!");
    println!("\nKey Features:");
    println!("✓ Deterministic behavior for reliable tests");
    println!("✓ In-memory operations (fast, no I/O)");
    println!("✓ Comprehensive operation tracking");
    println!("✓ Error injection capabilities");
    println!("✓ Complete trait implementations");
    println!("\nThese mocks provide a solid foundation for testing");
    println!("all aspects of the Crucible system without external dependencies.\n");

    Ok(())
}
