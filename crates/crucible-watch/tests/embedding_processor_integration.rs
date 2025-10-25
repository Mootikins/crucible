//! Integration tests for the EventDrivenEmbeddingProcessor
//!
//! These tests verify the core embedding processing functionality:
//! - Event batching and timeout handling
//! - Deduplication of duplicate events
//! - Error handling and recovery
//! - Metrics tracking

use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info};

use crucible_watch::{
    EventDrivenEmbeddingProcessor,
    EventDrivenEmbeddingConfig,
    EmbeddingEvent,
    FileEventKind,
    create_embedding_metadata,
};
use crucible_surrealdb::{
    embedding_pool::EmbeddingThreadPool,
    embedding_config::EmbeddingConfig,
};
use tokio::sync::mpsc;
use tempfile::TempDir;
use std::path::PathBuf;

/// Test basic event processing functionality
#[tokio::test]
async fn test_basic_event_processing() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    info!("Starting basic event processing test");

    // Create temporary directory
    let temp_dir = TempDir::new()?;
    let test_path = temp_dir.path().to_path_buf();

    // Create embedding configuration
    let embedding_config = EmbeddingConfig {
        worker_count: 2,
        batch_size: 4,
        model_type: crucible_surrealdb::embedding_config::EmbeddingModel::LocalStandard,
        privacy_mode: crucible_surrealdb::embedding_config::PrivacyMode::StrictLocal,
        max_queue_size: 100,
        timeout_ms: 10000,
        retry_attempts: 2,
        retry_delay_ms: 500,
        circuit_breaker_threshold: 5,
        circuit_breaker_timeout_ms: 30000,
    };

    // Create embedding thread pool
    let embedding_pool = EmbeddingThreadPool::new(embedding_config).await?;
    let embedding_pool = std::sync::Arc::new(embedding_pool);

    // Create event-driven embedding processor
    let event_config = EventDrivenEmbeddingConfig {
        max_batch_size: 2,
        batch_timeout_ms: 500,
        enable_deduplication: false, // Disable for basic test
        ..Default::default()
    };

    let processor = EventDrivenEmbeddingProcessor::new(event_config, embedding_pool.clone()).await?;

    // Create event channel
    let (embedding_tx, embedding_rx) = mpsc::unbounded_channel::<EmbeddingEvent>();
    let processor_with_rx = processor.with_embedding_event_receiver(embedding_rx).await;
    let processor_arc = std::sync::Arc::new(processor_with_rx);

    // Start the processor
    processor_arc.start().await?;

    // Create test embedding event
    let test_file = test_path.join("test.md");
    let test_content = "# Test Document\nThis is a test document for embedding.";
    let metadata = create_embedding_metadata(&test_file, &FileEventKind::Created, Some(test_content.len() as u64));

    let embedding_event = EmbeddingEvent::new(
        test_file,
        FileEventKind::Created,
        test_content.to_string(),
        metadata,
    );

    // Send the event
    embedding_tx.send(embedding_event)?;
    debug!("Sent embedding event");

    // Wait for processing
    sleep(Duration::from_secs(1)).await;

    // Check metrics
    let metrics = processor_arc.get_metrics().await;
    info!("Metrics after basic test: {:?}", metrics);

    assert_eq!(metrics.total_events_received, 1, "Should have received 1 event");
    assert_eq!(metrics.total_events_processed, 1, "Should have processed 1 event");
    assert_eq!(metrics.total_batches_processed, 1, "Should have processed 1 batch");
    assert_eq!(metrics.failed_events, 0, "Should have no failed events");

    // Shutdown
    processor_arc.shutdown().await?;

    info!("✅ Basic event processing test passed!");
    Ok(())
}

/// Test batch processing with multiple events
#[tokio::test]
async fn test_batch_processing() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    info!("Starting batch processing test");

    // Create temporary directory
    let temp_dir = TempDir::new()?;
    let test_path = temp_dir.path().to_path_buf();

    // Create embedding configuration
    let embedding_config = EmbeddingConfig {
        worker_count: 2,
        batch_size: 8,
        model_type: crucible_surrealdb::embedding_config::EmbeddingModel::LocalStandard,
        privacy_mode: crucible_surrealdb::embedding_config::PrivacyMode::StrictLocal,
        max_queue_size: 100,
        timeout_ms: 10000,
        retry_attempts: 2,
        retry_delay_ms: 500,
        circuit_breaker_threshold: 5,
        circuit_breaker_timeout_ms: 30000,
    };

    // Create embedding thread pool
    let embedding_pool = EmbeddingThreadPool::new(embedding_config).await?;
    let embedding_pool = std::sync::Arc::new(embedding_pool);

    // Create event-driven embedding processor with small batch size and short timeout
    let event_config = EventDrivenEmbeddingConfig {
        max_batch_size: 3,
        batch_timeout_ms: 800, // Shorter timeout
        enable_deduplication: false,
        ..Default::default()
    };

    let processor = EventDrivenEmbeddingProcessor::new(event_config, embedding_pool.clone()).await?;

    // Create event channel
    let (embedding_tx, embedding_rx) = mpsc::unbounded_channel::<EmbeddingEvent>();
    let processor_with_rx = processor.with_embedding_event_receiver(embedding_rx).await;
    let processor_arc = std::sync::Arc::new(processor_with_rx);

    // Start the processor
    processor_arc.start().await?;

    // Create multiple test events
    let test_files = vec![
        ("batch1.md", "# Batch Test 1\nFirst document"),
        ("batch2.md", "# Batch Test 2\nSecond document"),
        ("batch3.md", "# Batch Test 3\nThird document"),
        ("batch4.md", "# Batch Test 4\nFourth document"),
    ];

    for (i, (filename, content)) in test_files.iter().enumerate() {
        let test_file = test_path.join(filename);
        let metadata = create_embedding_metadata(&test_file, &FileEventKind::Created, Some(content.len() as u64));

        let embedding_event = EmbeddingEvent::new(
            test_file,
            FileEventKind::Created,
            content.to_string(),
            metadata,
        );

        embedding_tx.send(embedding_event)?;
        debug!("Sent event {} for {}", i + 1, filename);

        // Small delay between events
        sleep(Duration::from_millis(100)).await;
    }

    // Wait for batch processing (should trigger due to batch size)
    sleep(Duration::from_millis(200)).await;

    // Wait for the remaining event to be processed via timeout
    sleep(Duration::from_millis(1200)).await;

    // Check metrics
    let metrics = processor_arc.get_metrics().await;
    info!("Metrics after batch test: {:?}", metrics);

    assert_eq!(metrics.total_events_received, 4, "Should have received 4 events");
    assert_eq!(metrics.total_events_processed, 4, "Should have processed 4 events");
    assert!(metrics.total_batches_processed >= 1, "Should have processed at least 1 batch");

    // Shutdown
    processor_arc.shutdown().await?;

    info!("✅ Batch processing test passed!");
    Ok(())
}

/// Test batch timeout functionality
#[tokio::test]
async fn test_batch_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    info!("Starting batch timeout test");

    // Create temporary directory
    let temp_dir = TempDir::new()?;
    let test_path = temp_dir.path().to_path_buf();

    // Create embedding configuration
    let embedding_config = EmbeddingConfig {
        worker_count: 2,
        batch_size: 8,
        model_type: crucible_surrealdb::embedding_config::EmbeddingModel::LocalStandard,
        privacy_mode: crucible_surrealdb::embedding_config::PrivacyMode::StrictLocal,
        max_queue_size: 100,
        timeout_ms: 10000,
        retry_attempts: 2,
        retry_delay_ms: 500,
        circuit_breaker_threshold: 5,
        circuit_breaker_timeout_ms: 30000,
    };

    // Create embedding thread pool
    let embedding_pool = EmbeddingThreadPool::new(embedding_config).await?;
    let embedding_pool = std::sync::Arc::new(embedding_pool);

    // Create event-driven embedding processor with short timeout
    let event_config = EventDrivenEmbeddingConfig {
        max_batch_size: 10, // Large batch size
        batch_timeout_ms: 500, // Short timeout
        enable_deduplication: false,
        ..Default::default()
    };

    let processor = EventDrivenEmbeddingProcessor::new(event_config, embedding_pool.clone()).await?;

    // Create event channel
    let (embedding_tx, embedding_rx) = mpsc::unbounded_channel::<EmbeddingEvent>();
    let processor_with_rx = processor.with_embedding_event_receiver(embedding_rx).await;
    let processor_arc = std::sync::Arc::new(processor_with_rx);

    // Start the processor
    processor_arc.start().await?;

    // Send a single event (won't trigger batch size)
    let test_file = test_path.join("timeout_test.md");
    let test_content = "# Timeout Test\nThis should trigger batch timeout.";
    let metadata = create_embedding_metadata(&test_file, &FileEventKind::Created, Some(test_content.len() as u64));

    let embedding_event = EmbeddingEvent::new(
        test_file,
        FileEventKind::Created,
        test_content.to_string(),
        metadata,
    );

    embedding_tx.send(embedding_event)?;
    debug!("Sent single event for timeout test");

    // Wait for timeout (longer than the batch timeout)
    sleep(Duration::from_millis(1000)).await;

    // Check metrics
    let metrics = processor_arc.get_metrics().await;
    info!("Metrics after timeout test: {:?}", metrics);

    assert_eq!(metrics.total_events_received, 1, "Should have received 1 event");
    assert_eq!(metrics.total_events_processed, 1, "Should have processed 1 event");
    assert_eq!(metrics.total_batches_processed, 1, "Should have processed 1 batch due to timeout");

    // Shutdown
    processor_arc.shutdown().await?;

    info!("✅ Batch timeout test passed!");
    Ok(())
}

/// Test event deduplication
#[tokio::test]
async fn test_event_deduplication() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    info!("Starting event deduplication test");

    // Create temporary directory
    let temp_dir = TempDir::new()?;
    let test_path = temp_dir.path().to_path_buf();

    // Create embedding configuration
    let embedding_config = EmbeddingConfig {
        worker_count: 2,
        batch_size: 8,
        model_type: crucible_surrealdb::embedding_config::EmbeddingModel::LocalStandard,
        privacy_mode: crucible_surrealdb::embedding_config::PrivacyMode::StrictLocal,
        max_queue_size: 100,
        timeout_ms: 10000,
        retry_attempts: 2,
        retry_delay_ms: 500,
        circuit_breaker_threshold: 5,
        circuit_breaker_timeout_ms: 30000,
    };

    // Create embedding thread pool
    let embedding_pool = EmbeddingThreadPool::new(embedding_config).await?;
    let embedding_pool = std::sync::Arc::new(embedding_pool);

    // Create event-driven embedding processor with deduplication enabled
    let event_config = EventDrivenEmbeddingConfig {
        max_batch_size: 5,
        batch_timeout_ms: 2000,
        enable_deduplication: true,
        deduplication_window_ms: 1000, // 1 second deduplication window
        ..Default::default()
    };

    let processor = EventDrivenEmbeddingProcessor::new(event_config, embedding_pool.clone()).await?;

    // Create event channel
    let (embedding_tx, embedding_rx) = mpsc::unbounded_channel::<EmbeddingEvent>();
    let processor_with_rx = processor.with_embedding_event_receiver(embedding_rx).await;
    let processor_arc = std::sync::Arc::new(processor_with_rx);

    // Start the processor
    processor_arc.start().await?;

    // Create the same event multiple times quickly
    let test_file = test_path.join("dedup_test.md");
    let test_content = "# Deduplication Test\nThis content should be deduplicated.";
    let metadata = create_embedding_metadata(&test_file, &FileEventKind::Created, Some(test_content.len() as u64));

    // Send the same event 3 times
    for i in 0..3 {
        let embedding_event = EmbeddingEvent::new(
            test_file.clone(),
            FileEventKind::Created,
            test_content.to_string(),
            metadata.clone(),
        );

        embedding_tx.send(embedding_event)?;
        debug!("Sent duplicate event {}", i + 1);

        // Small delay between events
        sleep(Duration::from_millis(100)).await;
    }

    // Wait for processing
    sleep(Duration::from_millis(1000)).await;

    // Check metrics
    let metrics = processor_arc.get_metrics().await;
    info!("Metrics after deduplication test: {:?}", metrics);

    assert_eq!(metrics.total_events_received, 3, "Should have received 3 events");
    assert_eq!(metrics.deduplicated_events, 2, "Should have deduplicated 2 events");
    assert_eq!(metrics.total_events_processed, 1, "Should have processed only 1 unique event");

    // Wait for deduplication window to expire
    sleep(Duration::from_millis(1500)).await;

    // Send the same event again (should not be deduplicated)
    let embedding_event = EmbeddingEvent::new(
        test_file,
        FileEventKind::Modified,
        test_content.to_string(),
        metadata,
    );

    embedding_tx.send(embedding_event)?;
    debug!("Sent same event after deduplication window expired");

    // Wait for processing
    sleep(Duration::from_millis(500)).await;

    // Check final metrics
    let final_metrics = processor_arc.get_metrics().await;
    info!("Final metrics: {:?}", final_metrics);

    assert_eq!(final_metrics.total_events_received, 4, "Should have received 4 events total");
    assert_eq!(final_metrics.deduplicated_events, 2, "Should still have 2 deduplicated events");
    assert_eq!(final_metrics.total_events_processed, 2, "Should have processed 2 unique events");

    // Shutdown
    processor_arc.shutdown().await?;

    info!("✅ Event deduplication test passed!");
    Ok(())
}

/// Test error handling and recovery
#[tokio::test]
async fn test_error_handling() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    info!("Starting error handling test");

    // Create temporary directory
    let temp_dir = TempDir::new()?;
    let test_path = temp_dir.path().to_path_buf();

    // Create embedding configuration
    let embedding_config = EmbeddingConfig {
        worker_count: 2,
        batch_size: 8,
        model_type: crucible_surrealdb::embedding_config::EmbeddingModel::LocalStandard,
        privacy_mode: crucible_surrealdb::embedding_config::PrivacyMode::StrictLocal,
        max_queue_size: 100,
        timeout_ms: 1000, // Short timeout to potentially cause errors
        retry_attempts: 2,
        retry_delay_ms: 200,
        circuit_breaker_threshold: 5,
        circuit_breaker_timeout_ms: 30000,
    };

    // Create embedding thread pool
    let embedding_pool = EmbeddingThreadPool::new(embedding_config).await?;
    let embedding_pool = std::sync::Arc::new(embedding_pool);

    // Create event-driven embedding processor
    let event_config = EventDrivenEmbeddingConfig {
        max_batch_size: 3,
        batch_timeout_ms: 1000,
        enable_deduplication: false,
        ..Default::default()
    };

    let processor = EventDrivenEmbeddingProcessor::new(event_config, embedding_pool.clone()).await?;

    // Create event channel
    let (embedding_tx, embedding_rx) = mpsc::unbounded_channel::<EmbeddingEvent>();
    let processor_with_rx = processor.with_embedding_event_receiver(embedding_rx).await;
    let processor_arc = std::sync::Arc::new(processor_with_rx);

    // Start the processor
    processor_arc.start().await?;

    // Create some valid events
    let test_events = vec![
        ("valid1.md", "# Valid Document 1\nThis should work fine."),
        ("valid2.md", "# Valid Document 2\nThis should also work."),
    ];

    for (filename, content) in test_events {
        let test_file = test_path.join(filename);
        let metadata = create_embedding_metadata(&test_file, &FileEventKind::Created, Some(content.len() as u64));

        let embedding_event = EmbeddingEvent::new(
            test_file,
            FileEventKind::Created,
            content.to_string(),
            metadata,
        );

        embedding_tx.send(embedding_event)?;
    }

    // Wait for processing
    sleep(Duration::from_secs(2)).await;

    // Check metrics
    let metrics = processor_arc.get_metrics().await;
    info!("Metrics after error handling test: {:?}", metrics);

    assert!(metrics.total_events_received >= 2, "Should have received at least 2 events");
    // Some events might fail due to timeout, which is expected for this test
    info!("Processed {} out of {} received events", metrics.total_events_processed, metrics.total_events_received);

    // Verify processor is still running and not shutdown due to errors
    assert!(!processor_arc.is_shutdown().await, "Processor should still be running after errors");

    // Shutdown
    processor_arc.shutdown().await?;

    info!("✅ Error handling test passed!");
    Ok(())
}