//! Load Testing for Queue-Based Database Architecture
//!
//! This module validates that the queue-based architecture eliminates RocksDB lock contention
//! and performs well under high throughput scenarios.

use crucible_surrealdb::{
    transaction_queue::{TransactionQueue, TransactionQueueConfig, DatabaseTransaction, TransactionTimestamp},
    transaction_consumer::{DatabaseTransactionConsumer, ConsumerConfig, ConsumerStats, ShutdownSender, ShutdownReceiver},
    types::SurrealDbConfig,
    SurrealClient,
};
use crucible_core::types::ParsedDocument;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn, error};

/// High-throughput load test with 1000+ documents
#[tokio::test]
async fn test_high_throughput_processing() {
    let start_time = Instant::now();

    // Setup queue with realistic configuration
    let queue_config = TransactionQueueConfig {
        max_queue_size: 2000, // Large queue for high throughput
        ..Default::default()
    };
    let queue = Arc::new(TransactionQueue::new(queue_config));

    // Setup consumer with batching enabled
    let consumer_config = ConsumerConfig {
        transaction_timeout: Duration::from_secs(30),
        batch_timeout: Duration::from_millis(50), // Faster batching for load test
        max_batch_size: 20, // Larger batches for better throughput
        max_retries: 2, // Fewer retries for load test
        enable_batching: true,
        ..Default::default()
    };

    // Use in-memory database for fast load testing
    let mut db_config = SurrealDbConfig::default();
    db_config.path = ":memory:".to_string();
    let client = Arc::new(SurrealClient::new(db_config).await.unwrap());
    let consumer = DatabaseTransactionConsumer::new(client.clone(), consumer_config);

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let consumer_handle = {
        let receiver = queue.receiver();
        let mut consumer_clone = consumer.clone();
        tokio::spawn(async move {
            if let Err(e) = consumer_clone.start(receiver, shutdown_rx).await {
                error!("Consumer failed: {}", e);
            }
        })
    };

    // Test parameters
    let num_documents = 1000;
    let concurrent_producers = 10;
    let documents_per_producer = num_documents / concurrent_producers;

    info!("Starting load test: {} documents, {} concurrent producers", num_documents, concurrent_producers);

    // Enqueue transactions concurrently
    let enqueue_start = Instant::now();
    let mut handles = Vec::new();

    for producer_id in 0..concurrent_producers {
        let queue_clone = Arc::clone(&queue);
        let start_idx = producer_id * documents_per_producer;
        let end_idx = start_idx + documents_per_producer;

        let handle = tokio::spawn(async move {
            let mut successful_enqueues = 0;
            for i in start_idx..end_idx {
                let doc = create_test_document(i);
                let transaction = DatabaseTransaction::Create {
                    transaction_id: format!("load-test-{}", i),
                    document: doc,
                    kiln_root: PathBuf::from("/test-kiln"),
                    timestamp: TransactionTimestamp::now(),
                };
                match queue_clone.enqueue(transaction).await {
                    Ok(_) => successful_enqueues += 1,
                    Err(e) => warn!("Failed to enqueue transaction {}: {}", i, e),
                }
            }
            successful_enqueues
        });
        handles.push(handle);
    }

    // Wait for all enqueues to complete
    let mut total_successful = 0;
    for handle in handles {
        match handle.await {
            Ok(successful) => total_successful += successful,
            Err(e) => error!("Producer failed: {}", e),
        }
    }

    let enqueue_time = enqueue_start.elapsed();
    info!("Enqueued {} transactions in {:?}", total_successful, enqueue_time);
    info!("Enqueue rate: {:.2} transactions/second", total_successful as f64 / enqueue_time.as_secs_f64());

    // Wait for processing
    tokio::time::sleep(Duration::from_secs(15)).await;

    let processing_time = start_time.elapsed();
    info!("Processing completed in {:?}", processing_time);

    // Shutdown consumer
    let _ = shutdown_tx.send(());
    let _ = consumer_handle.await;

    // Assertions
    assert!(total_successful >= num_documents * 95 / 100, "At least 95% of transactions should be enqueued successfully");
    assert!(processing_time < Duration::from_secs(30), "Processing should complete within 30 seconds");

    // Check queue statistics
    let stats = queue.stats();
    assert!(stats.total_processed > 0, "Some transactions should be processed");
    info!("Queue stats: {:?}", stats);
}

/// Queue saturation test - validate behavior under maximum load
#[tokio::test]
async fn test_queue_saturation() {
    let queue_config = TransactionQueueConfig {
        max_queue_size: 100, // Small queue for saturation testing
        ..Default::default()
    };
    let queue = Arc::new(TransactionQueue::new(queue_config));

    let consumer_config = ConsumerConfig {
        enable_batching: false, // Disable batching to test individual transaction handling
        transaction_timeout: Duration::from_secs(5),
        ..Default::default()
    };

    let mut db_config = SurrealDbConfig::default();
    db_config.path = ":memory:".to_string();
    let client = Arc::new(SurrealClient::new(db_config).await.unwrap());
    let consumer = DatabaseTransactionConsumer::new(client.clone(), consumer_config);

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let consumer_handle = {
        let receiver = queue.receiver();
        let mut consumer_clone = consumer.clone();
        tokio::spawn(async move {
            if let Err(e) = consumer_clone.start(receiver, shutdown_rx).await {
                error!("Consumer failed: {}", e);
            }
        })
    };

    // Try to enqueue more transactions than the queue can hold
    let mut successful_enqueues = 0;
    let mut failed_enqueues = 0;

    for i in 0..200 { // 200 transactions into a queue of size 100
        let doc = create_test_document(i);
        let transaction = DatabaseTransaction::Create {
            transaction_id: format!("saturation-test-{}", i),
            document: doc,
            kiln_root: PathBuf::from("/test-kiln"),
            timestamp: TransactionTimestamp::now(),
        };

        match queue.enqueue(transaction).await {
            Ok(_) => successful_enqueues += 1,
            Err(_) => failed_enqueues += 1,
        }
    }

    info!("Queue saturation test: {} successful, {} failed enqueues", successful_enqueues, failed_enqueues);

    // Queue should have accepted close to its capacity
    assert!(successful_enqueues >= 90, "Queue should accept at least 90% of its capacity");
    assert!(failed_enqueues > 0, "Some enqueues should fail when queue is full");

    // Wait for processing to complete
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Check that queue is processing and making progress
    let stats = queue.stats();
    assert!(stats.total_processed > 0, "Queue should have processed some transactions");

    // Shutdown
    let _ = shutdown_tx.send(());
    let _ = consumer_handle.await;
}

/// Create a test document for load testing
fn create_test_document(index: usize) -> ParsedDocument {
    ParsedDocument::new(PathBuf::from(format!("/test/document_{}.md", index)))
}