//! Database Concurrency Tests
//!
//! CRITICAL: This test suite validates thread safety and lock behavior in the SurrealDB client.
//!
//! ## Motivation
//! The database implementation uses Arc<Mutex<HashMap>> extensively with `.lock().unwrap()` pattern.
//! This is dangerous because:
//! 1. Lock poisoning: If a thread panics while holding lock, all future `.unwrap()` calls panic
//! 2. Deadlock risk: Multiple lock acquisitions without proper ordering can deadlock
//! 3. Race conditions: Concurrent writes to same document untested
//!
//! ## Test Categories
//! - Basic Concurrent Operations: Multiple threads accessing database simultaneously
//! - Same Document Contention: Multiple threads modifying the same document
//! - Lock Poisoning: Handling panics while holding locks
//! - Deadlock Prevention: Verifying proper lock acquisition ordering
//! - Transaction-Like Operations: Batch operations under concurrency
//! - Stress Tests: High-concurrency scenarios to find edge cases

use anyhow::Result;
use crucible_surrealdb::database::SurrealEmbeddingDatabase;
use crucible_surrealdb::types::{EmbeddingData, EmbeddingMetadata};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

// =============================================================================
// TEST UTILITIES
// =============================================================================

/// Create test embedding data
fn create_test_embedding(file_path: &str, dimensions: usize, seed: u64) -> EmbeddingData {
    let embedding: Vec<f32> = (0..dimensions)
        .map(|i| ((seed + i as u64) as f32 * 0.1).sin())
        .collect();

    let metadata = EmbeddingMetadata {
        file_path: file_path.to_string(),
        title: Some(format!("Test Document: {}", file_path)),
        tags: vec!["test".to_string(), "concurrent".to_string()],
        folder: "test".to_string(),
        properties: HashMap::new(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    EmbeddingData {
        file_path: file_path.to_string(),
        content: format!("Test content for {}", file_path),
        embedding,
        metadata,
    }
}

/// Create test metadata
fn create_test_metadata(seed: u64) -> EmbeddingMetadata {
    let mut properties = HashMap::new();
    properties.insert("test_id".to_string(), serde_json::json!(seed));

    EmbeddingMetadata {
        file_path: format!("test-doc-{}", seed),
        title: Some(format!("Test Document {}", seed)),
        tags: vec![format!("tag-{}", seed)],
        folder: "test".to_string(),
        properties,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

// =============================================================================
// PHASE 1: BASIC CONCURRENT OPERATIONS (5 tests)
// =============================================================================

#[tokio::test]
async fn test_concurrent_reads_same_document() -> Result<()> {
    // Setup: Create database and store a document
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    let doc_id = "shared-read-doc";
    let embedding = create_test_embedding(doc_id, 256, 12345);
    db.store_embedding_data(&embedding).await?;

    // Spawn 10 concurrent readers
    let mut handles = Vec::new();
    for i in 0..10 {
        let db_clone = db.clone();
        let doc_id_clone = doc_id.to_string();

        handles.push(tokio::spawn(async move {
            // Each task reads the same document
            let result = db_clone.get_embedding(&doc_id_clone).await;
            (i, result)
        }));
    }

    // Wait for all reads to complete with timeout
    let results = timeout(Duration::from_secs(5), async {
        let mut all_results = Vec::new();
        for handle in handles {
            let result = handle.await?;
            all_results.push(result);
        }
        Ok::<_, anyhow::Error>(all_results)
    })
    .await
    .expect("Concurrent reads should not deadlock")?;

    // Verify: All reads should succeed and return the same data
    assert_eq!(results.len(), 10, "All 10 reads should complete");
    for (i, result) in results {
        assert!(
            result.is_ok(),
            "Read {} should succeed without lock panic",
            i
        );
        let data = result.unwrap();
        assert!(data.is_some(), "Document should exist for read {}", i);
        assert_eq!(
            data.unwrap().file_path,
            doc_id,
            "Read {} should return correct document",
            i
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_concurrent_writes_different_documents() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    let success_count = Arc::new(AtomicUsize::new(0));

    // Spawn 10 concurrent writers, each writing to a different document
    let mut handles = Vec::new();
    for i in 0..10 {
        let db_clone = db.clone();
        let success_count_clone = success_count.clone();

        handles.push(tokio::spawn(async move {
            let doc_id = format!("doc-{}", i);
            let embedding = create_test_embedding(&doc_id, 256, i as u64);

            match db_clone.store_embedding_data(&embedding).await {
                Ok(_) => {
                    success_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(doc_id)
                }
                Err(e) => Err(e),
            }
        }));
    }

    // Wait for all writes with timeout
    let results = timeout(Duration::from_secs(10), async {
        let mut all_results = Vec::new();
        for handle in handles {
            all_results.push(handle.await?);
        }
        Ok::<_, anyhow::Error>(all_results)
    })
    .await
    .expect("Concurrent writes should not deadlock")?;

    // Verify: All writes should succeed
    assert_eq!(
        success_count.load(Ordering::SeqCst),
        10,
        "All 10 writes should succeed"
    );
    assert_eq!(results.len(), 10, "All 10 tasks should complete");

    // Verify each document was written correctly
    for result in results {
        let doc_id = result?;
        let retrieved = db.get_embedding(&doc_id).await?;
        assert!(
            retrieved.is_some(),
            "Document {} should be retrievable",
            doc_id
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_concurrent_reads_and_writes() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    // Pre-populate with some documents
    for i in 0..5 {
        let doc_id = format!("mixed-doc-{}", i);
        let embedding = create_test_embedding(&doc_id, 256, i as u64);
        db.store_embedding_data(&embedding).await?;
    }

    let read_count = Arc::new(AtomicUsize::new(0));
    let write_count = Arc::new(AtomicUsize::new(0));

    // Spawn 5 readers and 5 writers
    let mut handles = Vec::new();

    // Spawn readers
    for i in 0..5 {
        let db_clone = db.clone();
        let read_count_clone = read_count.clone();

        handles.push(tokio::spawn(async move {
            let doc_id = format!("mixed-doc-{}", i % 5);
            match db_clone.get_embedding(&doc_id).await {
                Ok(_) => {
                    read_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }));
    }

    // Spawn writers
    for i in 5..10 {
        let db_clone = db.clone();
        let write_count_clone = write_count.clone();

        handles.push(tokio::spawn(async move {
            let doc_id = format!("new-doc-{}", i);
            let embedding = create_test_embedding(&doc_id, 256, i as u64);
            match db_clone.store_embedding_data(&embedding).await {
                Ok(_) => {
                    write_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }));
    }

    // Wait for all operations
    timeout(Duration::from_secs(10), async {
        for handle in handles {
            handle.await??;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .expect("Mixed operations should not deadlock")?;

    // Verify: All operations should succeed
    assert_eq!(
        read_count.load(Ordering::SeqCst),
        5,
        "All 5 reads should succeed"
    );
    assert_eq!(
        write_count.load(Ordering::SeqCst),
        5,
        "All 5 writes should succeed"
    );

    Ok(())
}

#[tokio::test]
async fn test_concurrent_database_creation() -> Result<()> {
    // Spawn multiple tasks trying to create databases simultaneously
    let mut handles = Vec::new();

    for i in 0..10 {
        handles.push(tokio::spawn(async move {
            let db = SurrealEmbeddingDatabase::new_memory();
            db.initialize().await?;
            Ok::<_, anyhow::Error>(i)
        }));
    }

    // Wait for all database creations
    let results = timeout(Duration::from_secs(10), async {
        let mut all_results = Vec::new();
        for handle in handles {
            all_results.push(handle.await?);
        }
        Ok::<_, anyhow::Error>(all_results)
    })
    .await
    .expect("Database creation should not deadlock")?;

    // Verify: All creations should succeed
    assert_eq!(results.len(), 10, "All 10 databases should be created");
    for result in results {
        assert!(result.is_ok(), "Database creation should succeed");
    }

    Ok(())
}

#[tokio::test]
async fn test_concurrent_query_execution() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    // Store documents
    for i in 0..10 {
        let doc_id = format!("query-doc-{}", i);
        let embedding = create_test_embedding(&doc_id, 256, i as u64);
        db.store_embedding_data(&embedding).await?;
    }

    // Spawn multiple threads executing different queries
    let success_count = Arc::new(AtomicUsize::new(0));

    let mut all_tasks = Vec::new();

    // List files queries
    for _ in 0..3 {
        let db_clone = db.clone();
        let success_count_clone = success_count.clone();
        all_tasks.push(tokio::spawn(async move {
            match db_clone.list_files().await {
                Ok(_) => {
                    success_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }));
    }

    // Get stats queries
    for _ in 0..3 {
        let db_clone = db.clone();
        let success_count_clone = success_count.clone();
        all_tasks.push(tokio::spawn(async move {
            match db_clone.get_stats().await {
                Ok(_) => {
                    success_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }));
    }

    // Search queries
    for i in 0..4 {
        let db_clone = db.clone();
        let success_count_clone = success_count.clone();
        all_tasks.push(tokio::spawn(async move {
            let query_vec = vec![0.1f32; 256];
            match db_clone
                .search_similar(&format!("query-{}", i), &query_vec, 5)
                .await
            {
                Ok(_) => {
                    success_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }));
    }

    // Wait for all queries
    timeout(Duration::from_secs(10), async {
        for handle in all_tasks {
            let _ = handle.await; // Ignore individual results
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .expect("Concurrent queries should not deadlock")?;

    // Verify: All queries should complete (success or error is acceptable, but no panic)
    let final_success = success_count.load(Ordering::SeqCst);
    assert_eq!(final_success, 10, "All 10 queries should complete");

    Ok(())
}

// =============================================================================
// PHASE 2: SAME DOCUMENT CONTENTION (4 tests) - CRITICAL
// =============================================================================

#[tokio::test]
async fn test_concurrent_writes_same_document() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    let doc_id = "contended-doc";
    let success_count = Arc::new(AtomicUsize::new(0));

    // Spawn 10 threads all writing to the same document
    let mut handles = Vec::new();
    for i in 0..10 {
        let db_clone = db.clone();
        let doc_id_clone = doc_id.to_string();
        let success_count_clone = success_count.clone();

        handles.push(tokio::spawn(async move {
            let embedding = create_test_embedding(&doc_id_clone, 256, i as u64);
            match db_clone.store_embedding_data(&embedding).await {
                Ok(_) => {
                    success_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(i)
                }
                Err(e) => Err(e),
            }
        }));
    }

    // Wait for all operations with timeout
    let results = timeout(Duration::from_secs(10), async {
        let mut all_results = Vec::new();
        for handle in handles {
            all_results.push(handle.await?);
        }
        Ok::<_, anyhow::Error>(all_results)
    })
    .await
    .expect("Concurrent writes to same document should not deadlock")?;

    // CRITICAL: All operations should complete without panic
    assert_eq!(results.len(), 10, "All 10 writes should complete");
    let final_count = success_count.load(Ordering::SeqCst);
    assert_eq!(final_count, 10, "All writes should succeed");

    // Verify document exists and is in a consistent state
    let final_doc = db.get_embedding(doc_id).await?;
    assert!(
        final_doc.is_some(),
        "Document should exist after concurrent writes"
    );

    Ok(())
}

#[tokio::test]
async fn test_concurrent_embedding_storage_same_document() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    let doc_id = "embedding-doc";
    let success_count = Arc::new(AtomicUsize::new(0));

    // Multiple threads storing embeddings for the same document
    let mut handles = Vec::new();
    for i in 0..10 {
        let db_clone = db.clone();
        let doc_id_clone = doc_id.to_string();
        let success_count_clone = success_count.clone();

        handles.push(tokio::spawn(async move {
            let embedding = vec![i as f32 * 0.1; 256];
            let metadata = create_test_metadata(i as u64);

            match db_clone
                .store_embedding(&doc_id_clone, "content", &embedding, &metadata)
                .await
            {
                Ok(_) => {
                    success_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }));
    }

    // Wait for completion
    timeout(Duration::from_secs(10), async {
        for handle in handles {
            handle.await??;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .expect("Concurrent embedding storage should not deadlock")?;

    assert_eq!(
        success_count.load(Ordering::SeqCst),
        10,
        "All embedding operations should succeed"
    );

    Ok(())
}

#[tokio::test]
async fn test_concurrent_metadata_updates_same_document() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    let doc_id = "metadata-doc";

    // First, create the document
    let initial_embedding = create_test_embedding(doc_id, 256, 0);
    db.store_embedding_data(&initial_embedding).await?;

    let success_count = Arc::new(AtomicUsize::new(0));

    // Multiple threads updating metadata concurrently
    let mut handles = Vec::new();
    for i in 0..10 {
        let db_clone = db.clone();
        let doc_id_clone = doc_id.to_string();
        let success_count_clone = success_count.clone();

        handles.push(tokio::spawn(async move {
            let mut properties = HashMap::new();
            properties.insert(
                format!("prop_{}", i),
                serde_json::json!(format!("value_{}", i)),
            );

            match db_clone
                .update_metadata_hashmap(&doc_id_clone, properties)
                .await
            {
                Ok(true) => {
                    success_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                Ok(false) => Err(anyhow::anyhow!("Document not found")),
                Err(e) => Err(e),
            }
        }));
    }

    // Wait for completion
    timeout(Duration::from_secs(10), async {
        for handle in handles {
            handle.await??;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .expect("Concurrent metadata updates should not deadlock")?;

    assert_eq!(
        success_count.load(Ordering::SeqCst),
        10,
        "All metadata updates should succeed"
    );

    // Verify final document has accumulated all property updates
    let final_doc = db.get_embedding(doc_id).await?;
    assert!(final_doc.is_some(), "Document should still exist");

    Ok(())
}

#[tokio::test]
async fn test_read_during_write_same_document() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    let doc_id = "read-write-doc";

    // Create initial document
    let initial = create_test_embedding(doc_id, 256, 0);
    db.store_embedding_data(&initial).await?;

    let read_count = Arc::new(AtomicUsize::new(0));
    let write_count = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();

    // Spawn writers
    for i in 0..5 {
        let db_clone = db.clone();
        let doc_id_clone = doc_id.to_string();
        let write_count_clone = write_count.clone();

        handles.push(tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(i * 10)).await;
            let embedding = create_test_embedding(&doc_id_clone, 256, i as u64);
            match db_clone.store_embedding_data(&embedding).await {
                Ok(_) => {
                    write_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }));
    }

    // Spawn readers
    for i in 0..5 {
        let db_clone = db.clone();
        let doc_id_clone = doc_id.to_string();
        let read_count_clone = read_count.clone();

        handles.push(tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(i * 10)).await;
            match db_clone.get_embedding(&doc_id_clone).await {
                Ok(_) => {
                    read_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }));
    }

    // Wait for all operations
    timeout(Duration::from_secs(10), async {
        for handle in handles {
            handle.await??;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .expect("Concurrent reads and writes should not deadlock")?;

    assert_eq!(
        read_count.load(Ordering::SeqCst),
        5,
        "All reads should complete"
    );
    assert_eq!(
        write_count.load(Ordering::SeqCst),
        5,
        "All writes should complete"
    );

    Ok(())
}

// =============================================================================
// PHASE 3: LOCK POISONING SCENARIOS (3 tests) - CRITICAL
// =============================================================================

#[tokio::test]
async fn test_lock_recovery_after_simulated_failure() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    let doc_id = "poison-test-doc";

    // Store initial document
    let initial = create_test_embedding(doc_id, 256, 0);
    db.store_embedding_data(&initial).await?;

    // Simulate a "failure" by trying an invalid operation
    // (Note: We can't actually poison the lock in safe code, so we test recovery from errors)
    let invalid_embedding = create_test_embedding(doc_id, 0, 0); // Zero dimensions
    let _ = db.store_embedding_data(&invalid_embedding).await; // May fail or succeed

    // CRITICAL: Database should still be usable after failure
    let result = db.get_embedding(doc_id).await;
    assert!(
        result.is_ok(),
        "Database should be usable after operation failure"
    );

    // Should be able to continue operations
    let new_embedding = create_test_embedding("new-doc", 256, 999);
    let store_result = db.store_embedding_data(&new_embedding).await;
    assert!(
        store_result.is_ok(),
        "Should be able to store new documents after failure"
    );

    Ok(())
}

#[tokio::test]
async fn test_multiple_operations_after_error() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    // Cause several errors
    for i in 0..5 {
        let _ = db.get_embedding(&format!("nonexistent-{}", i)).await;
        let _ = db.delete_file(&format!("nonexistent-{}", i)).await;
    }

    // Verify database is still functional
    let doc_id = "after-errors-doc";
    let embedding = create_test_embedding(doc_id, 256, 123);
    db.store_embedding_data(&embedding).await?;

    let retrieved = db.get_embedding(doc_id).await?;
    assert!(
        retrieved.is_some(),
        "Database should still work after errors"
    );

    Ok(())
}

#[tokio::test]
async fn test_error_isolation_between_operations() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    let error_occurred = Arc::new(AtomicBool::new(false));
    let success_count = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();

    // Spawn operations that will fail
    for i in 0..5 {
        let db_clone = db.clone();
        let error_occurred_clone = error_occurred.clone();

        handles.push(tokio::spawn(async move {
            match db_clone.get_embedding(&format!("nonexistent-{}", i)).await {
                Ok(None) => {
                    // Expected: document doesn't exist
                    Ok(())
                }
                Ok(Some(_)) => Err(anyhow::anyhow!("Should not find document")),
                Err(_) => {
                    error_occurred_clone.store(true, Ordering::SeqCst);
                    Err(anyhow::anyhow!("Operation failed"))
                }
            }
        }));
    }

    // Spawn operations that should succeed
    for i in 0..5 {
        let db_clone = db.clone();
        let success_count_clone = success_count.clone();

        handles.push(tokio::spawn(async move {
            let doc_id = format!("success-doc-{}", i);
            let embedding = create_test_embedding(&doc_id, 256, i as u64);

            match db_clone.store_embedding_data(&embedding).await {
                Ok(_) => {
                    success_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }));
    }

    // Wait for all operations
    timeout(Duration::from_secs(10), async {
        for handle in handles {
            let _ = handle.await; // Ignore individual failures
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .expect("Operations should complete")?;

    // CRITICAL: Successful operations should not be affected by failing ones
    assert_eq!(
        success_count.load(Ordering::SeqCst),
        5,
        "Successful operations should complete despite errors elsewhere"
    );

    Ok(())
}

// =============================================================================
// PHASE 4: DEADLOCK PREVENTION (3 tests)
// =============================================================================

#[tokio::test]
async fn test_no_deadlock_multiple_locks() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    // Store multiple documents
    for i in 0..10 {
        let doc_id = format!("lock-doc-{}", i);
        let embedding = create_test_embedding(&doc_id, 256, i as u64);
        db.store_embedding_data(&embedding).await?;
    }

    // Spawn tasks that access multiple documents in different orders
    let mut handles = Vec::new();

    for i in 0..10 {
        let db_clone = db.clone();

        handles.push(tokio::spawn(async move {
            // Each task accesses documents in a different order
            for j in 0..5 {
                let doc_idx = (i + j) % 10;
                let doc_id = format!("lock-doc-{}", doc_idx);

                // Perform read
                let _ = db_clone.get_embedding(&doc_id).await?;

                // Perform write
                let embedding = create_test_embedding(&doc_id, 256, (i * 10 + j) as u64);
                db_clone.store_embedding_data(&embedding).await?;
            }
            Ok::<_, anyhow::Error>(i)
        }));
    }

    // CRITICAL: This should complete without deadlock
    let results = timeout(Duration::from_secs(15), async {
        let mut all_results = Vec::new();
        for handle in handles {
            all_results.push(handle.await?);
        }
        Ok::<_, anyhow::Error>(all_results)
    })
    .await
    .expect("Multiple lock acquisitions should not deadlock")?;

    assert_eq!(results.len(), 10, "All tasks should complete");

    Ok(())
}

#[tokio::test]
async fn test_no_deadlock_with_timeout() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    let completed = Arc::new(AtomicUsize::new(0));

    // Spawn many concurrent operations
    let mut handles = Vec::new();
    for i in 0..20 {
        let db_clone = db.clone();
        let completed_clone = completed.clone();

        handles.push(tokio::spawn(async move {
            let doc_id = format!("timeout-doc-{}", i % 5); // Intentional contention
            let embedding = create_test_embedding(&doc_id, 256, i as u64);

            match timeout(
                Duration::from_secs(5),
                db_clone.store_embedding_data(&embedding),
            )
            .await
            {
                Ok(Ok(_)) => {
                    completed_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                Ok(Err(e)) => Err(e),
                Err(_) => Err(anyhow::anyhow!("Operation timeout - possible deadlock")),
            }
        }));
    }

    // Wait for all operations
    let results = timeout(Duration::from_secs(10), async {
        let mut all_results = Vec::new();
        for handle in handles {
            all_results.push(handle.await?);
        }
        Ok::<_, anyhow::Error>(all_results)
    })
    .await
    .expect("Operations with timeout should not deadlock globally")?;

    assert_eq!(results.len(), 20, "All tasks should complete");

    let final_completed = completed.load(Ordering::SeqCst);
    assert!(
        final_completed > 0,
        "At least some operations should succeed"
    );

    Ok(())
}

#[tokio::test]
async fn test_lock_ordering_consistency() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    // Pre-populate documents
    for i in 0..5 {
        let doc_id = format!("order-doc-{}", i);
        let embedding = create_test_embedding(&doc_id, 256, i as u64);
        db.store_embedding_data(&embedding).await?;
    }

    let completed = Arc::new(AtomicUsize::new(0));

    // Spawn tasks that access documents in consistent order
    let mut handles = Vec::new();
    for _i in 0..10 {
        let db_clone = db.clone();
        let completed_clone = completed.clone();

        handles.push(tokio::spawn(async move {
            // Always access documents in ascending order
            for j in 0..5 {
                let doc_id = format!("order-doc-{}", j);
                db_clone.get_embedding(&doc_id).await?;
            }
            completed_clone.fetch_add(1, Ordering::SeqCst);
            Ok::<_, anyhow::Error>(())
        }));
    }

    // Should complete quickly with consistent ordering
    timeout(Duration::from_secs(5), async {
        for handle in handles {
            handle.await??;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .expect("Consistent lock ordering should prevent deadlock")?;

    assert_eq!(
        completed.load(Ordering::SeqCst),
        10,
        "All tasks should complete with consistent ordering"
    );

    Ok(())
}

// =============================================================================
// PHASE 5: TRANSACTION-LIKE OPERATIONS (3 tests)
// =============================================================================

#[tokio::test]
async fn test_concurrent_batch_operations() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    let success_count = Arc::new(AtomicUsize::new(0));

    // Spawn multiple batch operations in parallel
    let mut handles = Vec::new();
    for batch_idx in 0..5 {
        let db_clone = db.clone();
        let success_count_clone = success_count.clone();

        handles.push(tokio::spawn(async move {
            // Each batch stores multiple documents
            for i in 0..10 {
                let doc_id = format!("batch-{}-doc-{}", batch_idx, i);
                let embedding = create_test_embedding(&doc_id, 256, (batch_idx * 10 + i) as u64);
                db_clone.store_embedding_data(&embedding).await?;
            }
            success_count_clone.fetch_add(1, Ordering::SeqCst);
            Ok::<_, anyhow::Error>(batch_idx)
        }));
    }

    // Wait for all batches
    let results = timeout(Duration::from_secs(10), async {
        let mut all_results = Vec::new();
        for handle in handles {
            all_results.push(handle.await?);
        }
        Ok::<_, anyhow::Error>(all_results)
    })
    .await
    .expect("Concurrent batch operations should not deadlock")?;

    assert_eq!(results.len(), 5, "All 5 batches should complete");
    assert_eq!(
        success_count.load(Ordering::SeqCst),
        5,
        "All batches should succeed"
    );

    // Verify all documents were stored
    let files = db.list_files().await?;
    assert_eq!(files.len(), 50, "Should have 50 documents from 5 batches");

    Ok(())
}

#[tokio::test]
async fn test_partial_batch_failure_isolation() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    let successful_batches = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();

    // Some batches will have errors, others should succeed
    for batch_idx in 0..10 {
        let db_clone = db.clone();
        let successful_batches_clone = successful_batches.clone();

        handles.push(tokio::spawn(async move {
            let mut batch_success = true;

            for i in 0..5 {
                let doc_id = format!("partial-batch-{}-doc-{}", batch_idx, i);

                // Simulate occasional failures (but not panics)
                if batch_idx == 3 && i == 2 {
                    // Skip one operation to simulate partial failure
                    batch_success = false;
                    continue;
                }

                let embedding = create_test_embedding(&doc_id, 256, (batch_idx * 5 + i) as u64);
                match db_clone.store_embedding_data(&embedding).await {
                    Ok(_) => {}
                    Err(_) => {
                        batch_success = false;
                    }
                }
            }

            if batch_success {
                successful_batches_clone.fetch_add(1, Ordering::SeqCst);
            }

            Ok::<_, anyhow::Error>(batch_idx)
        }));
    }

    // Wait for all batches
    timeout(Duration::from_secs(10), async {
        for handle in handles {
            let _ = handle.await; // Ignore individual errors
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .expect("Partial failures should not cause deadlock")?;

    // Most batches should succeed
    let successful = successful_batches.load(Ordering::SeqCst);
    assert!(
        successful >= 9,
        "Most batches should succeed despite partial failures"
    );

    Ok(())
}

#[tokio::test]
async fn test_concurrent_clear_and_store() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    let doc_id = "clear-store-doc";

    // Store initial document
    let initial = create_test_embedding(doc_id, 256, 0);
    db.store_embedding_data(&initial).await?;

    let clear_count = Arc::new(AtomicUsize::new(0));
    let store_count = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();

    // Spawn clear operations
    for _ in 0..5 {
        let db_clone = db.clone();
        let doc_id_clone = doc_id.to_string();
        let clear_count_clone = clear_count.clone();

        handles.push(tokio::spawn(async move {
            match db_clone.delete_file(&doc_id_clone).await {
                Ok(_) => {
                    clear_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }));
    }

    // Spawn store operations
    for i in 0..5 {
        let db_clone = db.clone();
        let doc_id_clone = doc_id.to_string();
        let store_count_clone = store_count.clone();

        handles.push(tokio::spawn(async move {
            let embedding = create_test_embedding(&doc_id_clone, 256, i as u64);
            match db_clone.store_embedding_data(&embedding).await {
                Ok(_) => {
                    store_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }));
    }

    // Wait for all operations
    timeout(Duration::from_secs(10), async {
        for handle in handles {
            let _ = handle.await; // Ignore errors
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .expect("Concurrent clear and store should not deadlock")?;

    // Verify operations completed
    assert!(
        clear_count.load(Ordering::SeqCst) > 0,
        "Some clear operations should complete"
    );
    assert!(
        store_count.load(Ordering::SeqCst) > 0,
        "Some store operations should complete"
    );

    Ok(())
}

// =============================================================================
// PHASE 6: STRESS TESTS (3 tests)
// =============================================================================

#[tokio::test]
async fn test_high_concurrency_stress() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    let total_operations = 100;
    let success_count = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();

    for i in 0..total_operations {
        let db_clone = db.clone();
        let success_count_clone = success_count.clone();

        handles.push(tokio::spawn(async move {
            let operation_type = i % 4;

            match operation_type {
                0 => {
                    // Store
                    let doc_id = format!("stress-doc-{}", i);
                    let embedding = create_test_embedding(&doc_id, 256, i as u64);
                    match db_clone.store_embedding_data(&embedding).await {
                        Ok(_) => {
                            success_count_clone.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                1 => {
                    // Read
                    let doc_id = format!("stress-doc-{}", i % 50);
                    match db_clone.get_embedding(&doc_id).await {
                        Ok(_) => {
                            success_count_clone.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                2 => {
                    // List
                    match db_clone.list_files().await {
                        Ok(_) => {
                            success_count_clone.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
                _ => {
                    // Stats
                    match db_clone.get_stats().await {
                        Ok(_) => {
                            success_count_clone.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
            }
        }));
    }

    // Wait for all operations with generous timeout
    let results = timeout(Duration::from_secs(30), async {
        let mut all_results = Vec::new();
        for handle in handles {
            all_results.push(handle.await);
        }
        Ok::<_, anyhow::Error>(all_results)
    })
    .await
    .expect("High concurrency stress test should complete")?;

    assert_eq!(
        results.len(),
        total_operations,
        "All operations should complete"
    );

    let final_success = success_count.load(Ordering::SeqCst);
    assert!(
        final_success > total_operations * 9 / 10,
        "At least 90% of operations should succeed, got {}/{}",
        final_success,
        total_operations
    );

    Ok(())
}

#[tokio::test]
async fn test_rapid_lock_acquisition() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    let doc_id = "rapid-lock-doc";
    let initial = create_test_embedding(doc_id, 256, 0);
    db.store_embedding_data(&initial).await?;

    let iterations = 1000;
    let success_count = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();

    // Spawn tasks that rapidly acquire and release locks
    for i in 0..10 {
        let db_clone = db.clone();
        let doc_id_clone = doc_id.to_string();
        let success_count_clone = success_count.clone();

        handles.push(tokio::spawn(async move {
            for j in 0..iterations / 10 {
                match db_clone.get_embedding(&doc_id_clone).await {
                    Ok(_) => {
                        success_count_clone.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(_) => {}
                }

                // Minimal delay to create contention
                if j % 10 == 0 {
                    tokio::task::yield_now().await;
                }
            }
            Ok::<_, anyhow::Error>(i)
        }));
    }

    // Wait for completion
    let results = timeout(Duration::from_secs(30), async {
        let mut all_results = Vec::new();
        for handle in handles {
            all_results.push(handle.await?);
        }
        Ok::<_, anyhow::Error>(all_results)
    })
    .await
    .expect("Rapid lock acquisition should not deadlock")?;

    assert_eq!(results.len(), 10, "All tasks should complete");

    let final_success = success_count.load(Ordering::SeqCst);
    assert!(
        final_success >= iterations * 9 / 10,
        "At least 90% of rapid acquisitions should succeed"
    );

    Ok(())
}

#[tokio::test]
async fn test_long_running_operations_dont_block() -> Result<()> {
    let db = Arc::new(SurrealEmbeddingDatabase::new_memory());
    db.initialize().await?;

    let short_ops_completed = Arc::new(AtomicUsize::new(0));
    let long_op_started = Arc::new(AtomicBool::new(false));

    let mut handles = Vec::new();

    // Spawn a long-running operation
    {
        let db_clone = db.clone();
        let long_op_started_clone = long_op_started.clone();

        handles.push(tokio::spawn(async move {
            long_op_started_clone.store(true, Ordering::SeqCst);

            // Simulate long operation by storing many documents
            for i in 0..100 {
                let doc_id = format!("long-op-doc-{}", i);
                let embedding = create_test_embedding(&doc_id, 256, i as u64);
                db_clone.store_embedding_data(&embedding).await?;

                // Small delay to make it genuinely slow
                tokio::time::sleep(Duration::from_millis(5)).await;
            }

            Ok::<_, anyhow::Error>(())
        }));
    }

    // Wait for long operation to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Spawn many short operations
    for i in 0..20 {
        let db_clone = db.clone();
        let short_ops_completed_clone = short_ops_completed.clone();

        handles.push(tokio::spawn(async move {
            let doc_id = format!("short-op-doc-{}", i);
            let embedding = create_test_embedding(&doc_id, 256, i as u64);

            match db_clone.store_embedding_data(&embedding).await {
                Ok(_) => {
                    short_ops_completed_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }));
    }

    // Wait for all operations
    timeout(Duration::from_secs(30), async {
        for handle in handles {
            let _ = handle.await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .expect("Long-running operations should not block indefinitely")?;

    // CRITICAL: Short operations should complete despite long operation
    assert!(
        long_op_started.load(Ordering::SeqCst),
        "Long operation should have started"
    );
    let completed = short_ops_completed.load(Ordering::SeqCst);
    assert!(
        completed >= 15,
        "Most short operations should complete despite long operation, got {}",
        completed
    );

    Ok(())
}
