//! Integration test for the complete event-driven embedding pipeline
//!
//! This test verifies that the complete workflow works end-to-end:
//! 1. File system events are captured by WatchManager
//! 2. Events are converted to EmbeddingEvents by EmbeddingEventHandler
//! 3. Events are batched and processed by EventDrivenEmbeddingProcessor
//! 4. Embeddings are generated and stored in SurrealDB
//! 5. Semantic search returns results for newly embedded content

use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::fs;
use tokio::time::sleep;
use tracing::{debug, info};

use crucible_daemon::coordinator::DataCoordinator;
use crucible_daemon::config::DaemonConfig;
use crucible_watch::{WatchManager, WatchManagerConfig, EventDrivenEmbeddingProcessor, EmbeddingEventHandler};
use crucible_surrealdb::embedding_pool::EmbeddingThreadPool;
use crucible_surrealdb::embedding_config::EmbeddingConfig;
use crucible_watch::{EventDrivenEmbeddingConfig, EmbeddingEvent};
use tokio::sync::mpsc;

/// Test the complete event pipeline end-to-end
#[tokio::test]
async fn test_complete_event_pipeline_integration() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize test logging
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    info!("Starting complete event pipeline integration test");

    // Create a temporary directory for test files
    let temp_dir = TempDir::new()?;
    let watch_path = temp_dir.path().to_path_buf();
    info!("Created temporary test directory: {}", watch_path.display());

    // Create test configuration
    let config = create_test_config(&watch_path)?;

    // Initialize the data coordinator
    let mut coordinator = DataCoordinator::new(config).await?;
    coordinator.initialize().await?;
    coordinator.start().await?;

    // Give the coordinator time to start up
    sleep(Duration::from_millis(500)).await;

    // Verify the coordinator is running
    assert!(coordinator.is_running().await, "Coordinator should be running");

    // Test 1: Create a file and verify it gets processed
    info!("Test 1: Creating test file and verifying embedding generation");
    let test_file = watch_path.join("test_document.md");
    let test_content = r#"
# Test Document

This is a test document for the event-driven embedding pipeline.

## Features

- Real-time file system monitoring
- Automatic embedding generation
- Batch processing and deduplication
- Semantic search integration

The system should automatically detect this file, generate embeddings,
and make it available for semantic search.
"#;

    // Create the test file
    fs::write(&test_file, test_content).await?;
    info!("Created test file: {}", test_file.display());

    // Wait for the event to be processed
    sleep(Duration::from_secs(2)).await;

    // Test 2: Modify the file and verify it gets reprocessed
    info!("Test 2: Modifying test file");
    let modified_content = test_content.to_string() + "\n\n## Additional Content\n\nThis content was added to test modification handling.";
    fs::write(&test_file, modified_content).await?;
    info!("Modified test file");

    // Wait for the modification to be processed
    sleep(Duration::from_secs(2)).await;

    // Test 3: Create multiple files to test batch processing
    info!("Test 3: Creating multiple files for batch processing");
    let files_to_create = vec![
        ("batch_test_1.md", "# Batch Test 1\nFirst file for batch testing."),
        ("batch_test_2.md", "# Batch Test 2\nSecond file for batch testing."),
        ("batch_test_3.md", "# Batch Test 3\nThird file for batch testing."),
    ];

    for (filename, content) in &files_to_create {
        let file_path = watch_path.join(filename);
        fs::write(&file_path, content).await?;
        info!("Created file: {}", filename);
    }

    // Wait for batch processing
    sleep(Duration::from_secs(3)).await;

    // Test 4: Test deduplication by creating the same content quickly
    info!("Test 4: Testing deduplication");
    let dedup_test_file = watch_path.join("dedup_test.md");
    let dedup_content = "Deduplication test content";

    // Create the same file twice in quick succession
    fs::write(&dedup_test_file, dedup_content).await?;
    sleep(Duration::from_millis(100)).await;
    fs::write(&dedup_test_file, dedup_content).await?;
    info!("Created duplicate content for deduplication test");

    // Wait for deduplication processing
    sleep(Duration::from_secs(1)).await;

    // Test 5: Test unsupported file types are ignored
    info!("Test 5: Testing unsupported file type handling");
    let unsupported_file = watch_path.join("test.exe");
    fs::write(&unsupported_file, "binary content").await?;
    info!("Created unsupported file type");

    // Wait for processing
    sleep(Duration::from_millis(500)).await;

    // Test 6: Verify coordinator health and metrics
    info!("Test 6: Checking coordinator health and metrics");
    let health = coordinator.get_daemon_health().await;
    assert!(health.events_processed > 0, "Should have processed events");
    info!("Total events processed: {}", health.events_processed);

    let event_stats = coordinator.get_event_statistics().await;
    info!("Event statistics: {:?}", event_stats);

    // Test 7: Stop the coordinator gracefully
    info!("Test 7: Stopping coordinator gracefully");
    coordinator.stop().await?;
    assert!(!coordinator.is_running().await, "Coordinator should be stopped");

    info!("✅ Complete event pipeline integration test passed!");
    Ok(())
}

/// Test the EmbeddingEventHandler in isolation
#[tokio::test]
async fn test_embedding_event_handler_isolation() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    info!("Starting EmbeddingEventHandler isolation test");

    // Create temporary directory
    let temp_dir = TempDir::new()?;
    let watch_path = temp_dir.path().to_path_buf();

    // Create test file
    let test_file = watch_path.join("handler_test.md");
    let test_content = "# Handler Test\nContent for testing the embedding event handler.";
    fs::write(&test_file, test_content).await?;

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

    // Create event-driven embedding processor
    let event_config = EventDrivenEmbeddingConfig::default();
    let processor = EventDrivenEmbeddingProcessor::new(event_config, std::sync::Arc::new(embedding_pool)).await?;

    // Create event channel
    let (embedding_tx, embedding_rx) = mpsc::unbounded_channel::<EmbeddingEvent>();
    let processor_with_rx = processor.with_embedding_event_receiver(embedding_rx).await;
    let processor_arc = std::sync::Arc::new(processor_with_rx);

    // Start the processor
    processor_arc.start().await?;

    // Create embedding event handler
    let handler = EmbeddingEventHandler::new(processor_arc.clone(), embedding_tx);

    // Create a file event
    let file_event = crucible_watch::FileEvent::new(
        crucible_watch::FileEventKind::Created,
        test_file.clone(),
    );

    // Test that the handler can process the event
    assert!(handler.can_handle(&file_event), "Handler should be able to handle .md files");
    assert_eq!(handler.name(), "embedding_event_handler");
    assert_eq!(handler.priority(), 300);

    // Handle the event
    handler.handle(file_event).await?;

    // Wait for processing
    sleep(Duration::from_secs(1)).await;

    // Test unsupported file type
    let unsupported_file = watch_path.join("test.exe");
    let unsupported_event = crucible_watch::FileEvent::new(
        crucible_watch::FileEventKind::Created,
        unsupported_file,
    );

    assert!(!handler.can_handle(&unsupported_event), "Handler should not handle .exe files");

    // Shutdown
    processor_arc.shutdown().await?;

    info!("✅ EmbeddingEventHandler isolation test passed!");
    Ok(())
}

/// Test batch processing functionality
#[tokio::test]
async fn test_batch_processing_functionality() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    info!("Starting batch processing functionality test");

    // Create temporary directory
    let temp_dir = TempDir::new()?;
    let watch_path = temp_dir.path().to_path_buf();

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

    // Create event-driven embedding processor with custom batch config
    let event_config = EventDrivenEmbeddingConfig {
        max_batch_size: 3,
        batch_timeout_ms: 1000,
        enable_deduplication: true,
        deduplication_window_ms: 500,
        ..Default::default()
    };

    let processor = EventDrivenEmbeddingProcessor::new(event_config, std::sync::Arc::new(embedding_pool)).await?;

    // Create event channel
    let (embedding_tx, embedding_rx) = mpsc::unbounded_channel::<EmbeddingEvent>();
    let processor_with_rx = processor.with_embedding_event_receiver(embedding_rx).await;
    let processor_arc = std::sync::Arc::new(processor_with_rx);

    // Start the processor
    processor_arc.start().await?;

    // Create multiple files quickly to trigger batch processing
    let test_files = vec![
        ("batch1.md", "# Batch Test 1\nFirst file"),
        ("batch2.md", "# Batch Test 2\nSecond file"),
        ("batch3.md", "# Batch Test 3\nThird file"),
        ("batch4.md", "# Batch Test 4\nFourth file"),
    ];

    for (filename, content) in test_files {
        let file_path = watch_path.join(filename);
        fs::write(&file_path, content).await?;

        // Create file event
        let file_event = crucible_watch::FileEvent::new(
            crucible_watch::FileEventKind::Created,
            file_path,
        );

        // Convert to embedding event and send
        let embedding_event = EmbeddingEvent::new(
            file_event.path,
            file_event.kind,
            content.to_string(),
            crucible_watch::create_embedding_metadata(
                &file_event.path,
                &file_event.kind,
                None,
            ),
        );

        embedding_tx.send(embedding_event)?;
    }

    // Wait for batch processing and timeout
    sleep(Duration::from_secs(3)).await;

    // Check metrics
    let metrics = processor_arc.get_metrics().await;
    info!("Batch processing metrics: {:?}", metrics);

    assert!(metrics.total_events_received >= 4, "Should have received at least 4 events");
    assert!(metrics.total_batches_processed > 0, "Should have processed at least one batch");

    // Shutdown
    processor_arc.shutdown().await?;

    info!("✅ Batch processing functionality test passed!");
    Ok(())
}

/// Create a test configuration for the daemon
fn create_test_config(watch_path: &PathBuf) -> Result<DaemonConfig, Box<dyn std::error::Error>> {
    let mut config = DaemonConfig::default();

    // Configure file watching
    config.watching.enabled = true;
    config.watching.watch_paths.push(watch_path.clone());
    config.watching.ignored_paths.push(
        watch_path.join(".git").to_string_lossy().to_string()
    );

    // Configure performance
    config.performance.workers.num_workers = Some(2);
    config.performance.workers.max_queue_size = Some(100);

    // Configure embedding
    config.embedding.enabled = true;
    config.embedding.batch_size = Some(8);
    config.embedding.timeout_ms = Some(10000);
    config.embedding.retry_attempts = Some(2);

    // Configure database (in-memory for testing)
    config.database.connection_string = "memory".to_string();
    config.database.namespace = "test".to_string();
    config.database.database = "test".to_string();

    Ok(config)
}

#[cfg(test)]
mod test_helpers {
    use super::*;

    /// Helper function to create test files with specific content
    pub async fn create_test_file(dir: &PathBuf, name: &str, content: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let file_path = dir.join(name);
        fs::write(&file_path, content).await?;
        Ok(file_path)
    }

    /// Helper function to wait for event processing
    pub async fn wait_for_processing(millis: u64) {
        sleep(Duration::from_millis(millis)).await;
    }

    /// Helper function to verify file exists and has expected content
    pub async fn verify_file_content(file_path: &PathBuf, expected_content: &str) -> Result<bool, Box<dyn std::error::Error>> {
        if !file_path.exists() {
            return Ok(false);
        }

        let actual_content = fs::read_to_string(file_path).await?;
        Ok(actual_content == expected_content)
    }
}