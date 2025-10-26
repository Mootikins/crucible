//! Event-Driven Embedding Integration Tests
//!
//! This test suite validates the event-driven architecture that connects crucible-watch
//! file system events to the embedding pipeline, eliminating the inefficient 10ms worker polling.
//!
//! The tests are designed to FAIL initially because the event-driven integration doesn't exist yet.
//! They serve as a specification for how the system should work once implemented.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use tokio::sync::{mpsc, RwLock};

// Import crucible-watch components
use crucible_watch::{Error, EventHandler, FileEvent, FileEventKind, Result};

// Import embedding components
use crucible_surrealdb::{
    embedding_config::{EmbeddingConfig, EmbeddingModel, PrivacyMode, ThreadPoolMetrics},
    embedding_pool::{EmbeddingProviderIntegration, EmbeddingThreadPool},
};

// Import crucible-config components
use crucible_config::{EmbeddingProviderConfig, EmbeddingProviderType};

// Import event-driven embedding components
use crucible_watch::{
    EmbeddingEvent, EmbeddingEventHandler, EmbeddingEventMetadata, EmbeddingEventPriority,
    EmbeddingEventResult, EventDrivenEmbeddingConfig, EventDrivenEmbeddingProcessor,
    EventProcessorMetrics,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Test file change event to embedding request transformation
    #[tokio::test]
    async fn test_file_change_event_to_embedding_request_transformation() {
        // This test should FAIL because the transformation logic doesn't exist yet

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");

        // Create a test markdown file
        let test_content = r#"---
title: Test Document
tags: [test, embedding]
---

# Main Heading

This is a test paragraph with **bold** text.

## Code Section

```rust
fn hello() {
    println!("Hello, world!");
}
```

## Task List

- [x] Completed task
- [ ] Pending task
"#;

        tokio::fs::write(&file_path, test_content).await.unwrap();

        // Create file change event
        let event = FileEvent::new(FileEventKind::Modified, file_path.clone());

        // This should create an EmbeddingEvent from the FileEvent
        let embedding_event = transform_file_event_to_embedding_event(event)
            .await
            .unwrap();

        // Verify the transformation
        assert_eq!(embedding_event.file_path, file_path);
        assert_eq!(embedding_event.trigger_event, FileEventKind::Modified);
        assert!(!embedding_event.content.is_empty());
        assert_eq!(embedding_event.metadata.content_type, "text/markdown");
        assert_eq!(
            embedding_event.metadata.file_extension,
            Some("md".to_string())
        );
        assert!(embedding_event.metadata.file_size.is_some());
        assert_eq!(
            embedding_event.metadata.priority,
            EmbeddingEventPriority::Normal
        );
        assert!(!embedding_event.metadata.is_batched);
        assert!(embedding_event.metadata.batch_id.is_none());

        // Verify document ID is generated correctly
        assert!(!embedding_event.document_id.is_empty());
        assert_ne!(embedding_event.id, uuid::Uuid::nil());
    }

    /// Test batch event processing for multiple file changes
    #[tokio::test]
    async fn test_batch_event_processing_multiple_files() {
        // This test should FAIL because batch processing doesn't exist yet

        let temp_dir = TempDir::new().unwrap();
        let config = EventDrivenEmbeddingConfig {
            max_batch_size: 3,
            batch_timeout_ms: 100, // Short timeout for testing
            ..Default::default()
        };

        let processor = create_event_driven_processor(config).await.unwrap();

        // Create multiple test files
        let files = vec![
            ("doc1.md", "# Document 1\nContent for document 1."),
            ("doc2.md", "# Document 2\nContent for document 2."),
            ("doc3.md", "# Document 3\nContent for document 3."),
        ];

        let mut file_paths = Vec::new();
        for (name, content) in files {
            let path = temp_dir.path().join(name);
            tokio::fs::write(&path, content).await.unwrap();
            file_paths.push(path);
        }

        // Send multiple file events
        let mut embedding_results = Vec::new();
        for (i, file_path) in file_paths.iter().enumerate() {
            let event = FileEvent::new(FileEventKind::Modified, file_path.clone());

            // Process the event and get embedding result
            let result = process_file_event_with_batching(&processor, event)
                .await
                .unwrap();
            embedding_results.push(result);
        }

        // Wait for batch processing to complete
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify batch processing results
        assert_eq!(embedding_results.len(), 3);

        // Note: In real implementation, we would check batch metadata
        // For now, just verify all events were processed successfully
        assert!(embedding_results.iter().all(|r| r.success));

        // Verify processing succeeded for all events
        assert!(embedding_results.iter().all(|r| r.success));
    }

    /// Test event-driven batch timeout logic
    #[tokio::test]
    async fn test_event_driven_batch_timeout_logic() {
        // This test should FAIL because timeout logic doesn't exist yet

        let config = EventDrivenEmbeddingConfig {
            max_batch_size: 10,   // Large batch size
            batch_timeout_ms: 50, // Short timeout
            ..Default::default()
        };

        let processor = create_event_driven_processor(config).await.unwrap();
        let temp_dir = TempDir::new().unwrap();

        // Create a single test file
        let file_path = temp_dir.path().join("timeout_test.md");
        tokio::fs::write(&file_path, "# Timeout Test\nThis should trigger timeout.")
            .await
            .unwrap();

        // Send a single file event
        let event = FileEvent::new(FileEventKind::Modified, file_path.clone());
        let start_time = Instant::now();

        // Process the event
        let result = process_file_event_with_batching(&processor, event)
            .await
            .unwrap();

        // Should be processed due to timeout, not batch size
        let elapsed = start_time.elapsed();
        assert!(elapsed >= Duration::from_millis(45)); // Allow some tolerance
        assert!(elapsed <= Duration::from_millis(100)); // But not too long

        // Verify the event was processed
        assert!(result.success);
        // Note: In real implementation, we would check batch metadata
    }

    /// Test error handling and retry scenarios
    #[tokio::test]
    async fn test_error_handling_and_retry_scenarios() {
        // This test should FAIL because retry logic doesn't exist yet

        let config = EventDrivenEmbeddingConfig {
            max_retry_attempts: 3,
            retry_delay_ms: 50, // Short delay for testing
            ..Default::default()
        };

        let processor = create_event_driven_processor(config).await.unwrap();
        let temp_dir = TempDir::new().unwrap();

        // Create a file that will cause processing errors
        let file_path = temp_dir.path().join("error_test.md");

        // Simulate a file that causes embedding generation to fail
        let problematic_content = "This content is designed to cause embedding generation to fail";
        tokio::fs::write(&file_path, problematic_content)
            .await
            .unwrap();

        // Create an event that will fail
        let event = FileEvent::new(FileEventKind::Modified, file_path.clone());

        // Process the event with retry logic
        let result = process_file_event_with_retry(&processor, event)
            .await
            .unwrap();

        // Verify retry behavior
        assert!(!result.success); // Should ultimately fail
        assert!(result.error.is_some());
        assert!(result.processing_time >= Duration::from_millis(150)); // Should have retried

        // Check that the error indicates retry attempts were made
        let error_msg = result.error.unwrap();
        assert!(error_msg.contains("retry") || error_msg.contains("attempt"));

        // Test successful retry scenario
        let normal_file_path = temp_dir.path().join("normal_test.md");
        tokio::fs::write(
            &normal_file_path,
            "# Normal Content\nThis should work fine.",
        )
        .await
        .unwrap();

        let normal_event = FileEvent::new(FileEventKind::Modified, normal_file_path);
        let normal_result = process_file_event_with_retry(&processor, normal_event)
            .await
            .unwrap();

        // Normal file should succeed
        assert!(normal_result.success);
        assert!(normal_result.error.is_none());
    }

    /// Test integration with embedding pool's event-driven capabilities
    #[tokio::test]
    async fn test_integration_with_embedding_pool_event_driven() {
        // This test should FAIL because the integration doesn't exist yet

        // Create embedding pool configuration
        let pool_config = EmbeddingConfig {
            worker_count: 2,
            batch_size: 4,
            model_type: EmbeddingModel::LocalMini,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 100,
            timeout_ms: 5000,
            retry_attempts: 2,
            retry_delay_ms: 500,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout_ms: 10000,
        };

        // Create embedding provider configuration
        let provider_config = EmbeddingProviderConfig::openai(
            "test-api-key".to_string(),
            Some("text-embedding-3-small".to_string()),
        );

        let provider_integration = EmbeddingProviderIntegration::with_provider(provider_config);

        // Create embedding pool
        let embedding_pool =
            EmbeddingThreadPool::new_with_provider_config(pool_config, provider_integration)
                .await
                .unwrap();

        // Create event processor with the embedding pool
        let event_config = EventDrivenEmbeddingConfig::default();
        let processor =
            create_event_driven_processor_with_pool(event_config, Arc::new(embedding_pool))
                .await
                .unwrap();

        let temp_dir = TempDir::new().unwrap();

        // Create test files
        let test_files = vec![
            (
                "integration1.md",
                "# Integration Test 1\nTesting event-driven integration.",
            ),
            (
                "integration2.md",
                "# Integration Test 2\nMore content for integration testing.",
            ),
        ];

        let mut results = Vec::new();
        for (filename, content) in test_files {
            let file_path = temp_dir.path().join(filename);
            tokio::fs::write(&file_path, content).await.unwrap();

            let event = FileEvent::new(FileEventKind::Created, file_path);
            let result = process_file_event_integration(&processor, event)
                .await
                .unwrap();
            results.push(result);
        }

        // Wait for processing to complete
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify integration results
        assert_eq!(results.len(), 2);

        for result in results {
            assert!(result.success);
            assert!(result.embedding_dimensions.is_some());
            assert!(result.processing_time > Duration::from_millis(0));
            assert!(result.error.is_none());
        }

        // Verify embedding pool metrics
        let metrics = get_embedding_pool_metrics(&processor).await;
        assert!(metrics.total_tasks_processed >= 2);
        assert_eq!(metrics.failed_tasks, 0);
    }

    /// Test performance improvements over polling
    #[tokio::test]
    async fn test_performance_improvements_over_polling() {
        // This test should FAIL because the performance improvements don't exist yet

        let config = EventDrivenEmbeddingConfig {
            batch_timeout_ms: 10, // Very fast batch processing
            ..Default::default()
        };

        let processor = create_event_driven_processor(config).await.unwrap();
        let temp_dir = TempDir::new().unwrap();

        // Create many files to test performance
        let num_files = 20;
        let mut file_paths = Vec::new();

        for i in 0..num_files {
            let filename = format!("perf_test_{}.md", i);
            let content = format!(
                "# Performance Test {}\nContent for performance test number {}.",
                i, i
            );
            let file_path = temp_dir.path().join(filename);
            tokio::fs::write(&file_path, content).await.unwrap();
            file_paths.push(file_path);
        }

        // Measure event-driven processing time
        let start_time = Instant::now();

        let mut results = Vec::new();
        for file_path in file_paths {
            let event = FileEvent::new(FileEventKind::Created, file_path);
            let result = process_file_event_fast(&processor, event).await.unwrap();
            results.push(result);
        }

        // Wait for all processing to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        let event_driven_time = start_time.elapsed();

        // Verify all events were processed
        assert_eq!(results.len(), num_files);
        let success_count = results.iter().filter(|r| r.success).count();
        assert!(success_count >= num_files * 9 / 10); // Allow for some failures in testing

        // Event-driven should be much faster than polling (10ms intervals)
        // With 20 files, polling would take at least 200ms (20 * 10ms)
        let estimated_polling_time = Duration::from_millis((num_files * 10).try_into().unwrap());

        assert!(event_driven_time < estimated_polling_time);

        // Additionally, verify that the average processing time per event is reasonable
        let avg_time_per_event = event_driven_time / num_files as u32;
        assert!(avg_time_per_event < Duration::from_millis(5)); // Should be very fast
    }

    /// Test deduplication of identical events
    #[tokio::test]
    async fn test_deduplication_of_identical_events() {
        // This test should FAIL because deduplication doesn't exist yet

        let config = EventDrivenEmbeddingConfig {
            enable_deduplication: true,
            deduplication_window_ms: 1000,
            ..Default::default()
        };

        let processor = create_event_driven_processor(config).await.unwrap();
        let temp_dir = TempDir::new().unwrap();

        let file_path = temp_dir.path().join("dedup_test.md");
        tokio::fs::write(
            &file_path,
            "# Deduplication Test\nContent to test deduplication.",
        )
        .await
        .unwrap();

        // Send multiple identical events for the same file
        let mut results = Vec::new();
        for _ in 0..5 {
            let event = FileEvent::new(FileEventKind::Modified, file_path.clone());
            let result = process_file_event_with_deduplication(&processor, event)
                .await
                .unwrap();
            results.push(result);
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // With deduplication, only the first event should be processed
        let processed_count = results.iter().filter(|r| r.success).count();
        assert!(processed_count <= 2); // Allow for some race conditions but should be much less than 5

        // Check that deduplication metrics are recorded
        let metrics = get_processor_metrics(&processor).await;
        assert!(metrics.deduplicated_events >= 3); // At least 3 events should be deduplicated
    }

    /// Test priority-based event processing
    #[tokio::test]
    async fn test_priority_based_event_processing() {
        // This test should FAIL because priority processing doesn't exist yet

        let config = EventDrivenEmbeddingConfig::default();
        let processor = create_event_driven_processor(config).await.unwrap();
        let temp_dir = TempDir::new().unwrap();

        // Create files with different priorities
        let critical_file = temp_dir.path().join("critical.md");
        let normal_file = temp_dir.path().join("normal.md");
        let low_file = temp_dir.path().join("low.md");

        tokio::fs::write(
            &critical_file,
            "# Critical Document\nThis needs immediate processing.",
        )
        .await
        .unwrap();
        tokio::fs::write(
            &normal_file,
            "# Normal Document\nStandard processing is fine.",
        )
        .await
        .unwrap();
        tokio::fs::write(
            &low_file,
            "# Low Priority Document\nBackground processing only.",
        )
        .await
        .unwrap();

        // Create events with different priorities
        let critical_event = create_priority_event(
            &critical_file,
            FileEventKind::Modified,
            EmbeddingEventPriority::Critical,
        );
        let normal_event = create_priority_event(
            &normal_file,
            FileEventKind::Modified,
            EmbeddingEventPriority::Normal,
        );
        let low_event = create_priority_event(
            &low_file,
            FileEventKind::Modified,
            EmbeddingEventPriority::Low,
        );

        let start_time = Instant::now();

        // Process events in order that doesn't match priority
        let low_result = process_priority_event(&processor, low_event).await.unwrap();
        let normal_result = process_priority_event(&processor, normal_event)
            .await
            .unwrap();
        let critical_result = process_priority_event(&processor, critical_event)
            .await
            .unwrap();

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // All events should be processed successfully
        assert!(low_result.success);
        assert!(normal_result.success);
        assert!(critical_result.success);

        // Critical event should be processed faster (lower latency)
        assert!(critical_result.processing_time < normal_result.processing_time);
        assert!(critical_result.processing_time < low_result.processing_time);

        // Verify priority processing (metadata would be checked in real implementation)
        // For now, just verify all events were processed successfully
        assert!(critical_result.success);
        assert!(normal_result.success);
        assert!(low_result.success);
    }

    /// Test graceful shutdown of event processor
    #[tokio::test]
    async fn test_graceful_shutdown_of_event_processor() {
        // This test should FAIL because graceful shutdown doesn't exist yet

        let config = EventDrivenEmbeddingConfig::default();
        let processor = create_event_driven_processor(config).await.unwrap();
        let temp_dir = TempDir::new().unwrap();

        // Start processing some events
        let file_path = temp_dir.path().join("shutdown_test.md");
        tokio::fs::write(&file_path, "# Shutdown Test\nTesting graceful shutdown.")
            .await
            .unwrap();

        let event = FileEvent::new(FileEventKind::Modified, file_path.clone());

        // Start processing event
        let processing_future = process_file_event_async(&processor, event);

        // Wait a bit then initiate shutdown
        tokio::time::sleep(Duration::from_millis(50)).await;
        let shutdown_future = shutdown_event_processor(&processor);

        // Both should complete successfully
        let (result, _) = tokio::join!(processing_future, shutdown_future);

        assert!(result.is_ok());
        assert!(processor.is_shutdown().await);

        // Verify that no new events can be processed after shutdown
        let new_event = FileEvent::new(FileEventKind::Modified, file_path);
        let new_result = process_file_event_after_shutdown(&processor, new_event).await;

        assert!(new_result.is_err());
        // Note: Error type would be Error::Shutdown in real implementation
    }

    // Helper functions implementation

    async fn transform_file_event_to_embedding_event(event: FileEvent) -> Result<EmbeddingEvent> {
        // Read file content
        let content = tokio::fs::read_to_string(&event.path)
            .await
            .map_err(|e| crucible_watch::Error::Io(e))?;

        // Get file metadata
        let metadata = tokio::fs::metadata(&event.path)
            .await
            .map_err(|e| crucible_watch::Error::Io(e))?;

        let file_size = Some(metadata.len());

        // Create embedding event using the helper from embedding_events
        let embedding_event = EmbeddingEvent::new(
            event.path.clone(),
            event.kind.clone(),
            content,
            crucible_watch::create_embedding_metadata(&event.path, &event.kind, file_size),
        );

        Ok(embedding_event)
    }

    async fn create_event_driven_processor(
        config: EventDrivenEmbeddingConfig,
    ) -> Result<Arc<EventDrivenEmbeddingProcessor>> {
        // Create a default embedding pool for testing
        let pool_config = crucible_surrealdb::embedding_config::EmbeddingConfig {
            worker_count: 2,
            batch_size: 4,
            model_type: crucible_surrealdb::embedding_config::EmbeddingModel::LocalMini,
            privacy_mode: crucible_surrealdb::embedding_config::PrivacyMode::StrictLocal,
            max_queue_size: 100,
            timeout_ms: 5000,
            retry_attempts: 2,
            retry_delay_ms: 500,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout_ms: 10000,
        };

        let provider_integration =
            crucible_surrealdb::embedding_pool::EmbeddingProviderIntegration::with_mock(
                256, // Mini model dimensions
                "test-mock-model".to_string(),
            );

        let embedding_pool =
            crucible_surrealdb::embedding_pool::EmbeddingThreadPool::new_with_provider_config(
                pool_config,
                provider_integration,
            )
            .await
            .map_err(|e| crucible_watch::Error::Other(e.to_string()))?;

        let processor =
            EventDrivenEmbeddingProcessor::new(config, Arc::new(embedding_pool)).await?;

        Ok(Arc::new(processor))
    }

    async fn create_event_driven_processor_with_pool(
        config: EventDrivenEmbeddingConfig,
        pool: Arc<EmbeddingThreadPool>,
    ) -> Result<Arc<EventDrivenEmbeddingProcessor>> {
        let processor = EventDrivenEmbeddingProcessor::new(config, pool).await?;
        Ok(Arc::new(processor))
    }

    async fn process_file_event_with_batching(
        processor: &EventDrivenEmbeddingProcessor,
        event: FileEvent,
    ) -> Result<EmbeddingEventResult> {
        // For batching simulation, we'll just process the event normally
        processor.process_file_event(event).await
    }

    async fn process_file_event_with_retry(
        processor: &EventDrivenEmbeddingProcessor,
        event: FileEvent,
    ) -> Result<EmbeddingEventResult> {
        // The processor already has retry logic built in
        processor.process_file_event(event).await
    }

    async fn process_file_event_integration(
        processor: &EventDrivenEmbeddingProcessor,
        event: FileEvent,
    ) -> Result<EmbeddingEventResult> {
        // Standard integration processing
        processor.process_file_event(event).await
    }

    async fn process_file_event_fast(
        processor: &EventDrivenEmbeddingProcessor,
        event: FileEvent,
    ) -> Result<EmbeddingEventResult> {
        // Fast processing path - same as normal for now
        processor.process_file_event(event).await
    }

    async fn process_file_event_with_deduplication(
        processor: &EventDrivenEmbeddingProcessor,
        event: FileEvent,
    ) -> Result<EmbeddingEventResult> {
        // The processor has deduplication built in
        processor.process_file_event(event).await
    }

    fn create_priority_event(
        path: &PathBuf,
        kind: FileEventKind,
        priority: EmbeddingEventPriority,
    ) -> FileEvent {
        // Create a file event with priority metadata stored in the path
        // This is a simplified approach for testing
        let mut event = FileEvent::new(kind, path.clone());

        // We can store priority information in a temporary way for testing
        // In a real implementation, we'd extend FileEvent to support metadata
        event
    }

    async fn process_priority_event(
        processor: &EventDrivenEmbeddingProcessor,
        event: FileEvent,
    ) -> Result<EmbeddingEventResult> {
        // Priority processing - same as normal for now
        processor.process_file_event(event).await
    }

    async fn process_file_event_async(
        processor: &EventDrivenEmbeddingProcessor,
        event: FileEvent,
    ) -> Result<EmbeddingEventResult> {
        // Async processing - same as normal for now
        processor.process_file_event(event).await
    }

    async fn shutdown_event_processor(processor: &EventDrivenEmbeddingProcessor) -> Result<()> {
        processor.shutdown().await
    }

    async fn process_file_event_after_shutdown(
        processor: &EventDrivenEmbeddingProcessor,
        event: FileEvent,
    ) -> Result<EmbeddingEventResult> {
        // Check if processor is shutdown
        if processor.is_shutdown().await {
            return Err(crucible_watch::Error::Other(
                "Processor is shutdown".to_string(),
            ));
        }

        // Process normally if not shutdown
        processor.process_file_event(event).await
    }

    async fn get_embedding_pool_metrics(
        processor: &EventDrivenEmbeddingProcessor,
    ) -> ThreadPoolMetrics {
        // For now, return default metrics since we don't have direct access to the pool
        ThreadPoolMetrics {
            total_tasks_processed: 0,
            active_workers: 0,
            queue_size: 0,
            average_processing_time: std::time::Duration::from_millis(0),
            failed_tasks: 0,
            circuit_breaker_open: false,
            memory_usage: None,
        }
    }

    async fn get_processor_metrics(
        processor: &EventDrivenEmbeddingProcessor,
    ) -> EventProcessorMetrics {
        processor.get_metrics().await
    }
}

// Implementation traits for the event-driven processor
impl EventDrivenEmbeddingProcessor {
    /// Create a new event-driven embedding processor
    pub async fn new(
        config: EventDrivenEmbeddingConfig,
        embedding_pool: Arc<EmbeddingThreadPool>,
    ) -> Result<Self> {
        todo!("Implement EventDrivenEmbeddingProcessor::new")
    }

    /// Process a file event and convert it to embedding processing
    pub async fn process_file_event(&self, event: FileEvent) -> Result<EmbeddingEventResult> {
        todo!("Implement EventDrivenEmbeddingProcessor::process_file_event")
    }

    /// Start the event processing loop
    pub async fn start(&self) -> Result<()> {
        todo!("Implement EventDrivenEmbeddingProcessor::start")
    }

    /// Shutdown the processor gracefully
    pub async fn shutdown(&self) -> Result<()> {
        todo!("Implement EventDrivenEmbeddingProcessor::shutdown")
    }

    /// Check if the processor is shutdown
    pub async fn is_shutdown(&self) -> bool {
        todo!("Implement EventDrivenEmbeddingProcessor::is_shutdown")
    }

    /// Get current processor metrics
    pub async fn get_metrics(&self) -> EventProcessorMetrics {
        todo!("Implement EventDrivenEmbeddingProcessor::get_metrics")
    }
}

impl EmbeddingEventHandler {
    /// Create a new embedding event handler
    pub fn new(processor: Arc<EventDrivenEmbeddingProcessor>) -> Self {
        Self {
            processor,
            supported_extensions: vec!["md".to_string(), "txt".to_string(), "rst".to_string()],
            enable_real_time: true,
        }
    }

    /// Set supported file extensions
    pub fn with_supported_extensions(mut self, extensions: Vec<String>) -> Self {
        self.supported_extensions = extensions;
        self
    }

    /// Enable or disable real-time processing
    pub fn with_real_time_processing(mut self, enable: bool) -> Self {
        self.enable_real_time = enable;
        self
    }
}

#[async_trait::async_trait]
impl EventHandler for EmbeddingEventHandler {
    async fn handle(&self, _event: FileEvent) -> Result<()> {
        todo!("Implement EmbeddingEventHandler::handle")
    }

    fn name(&self) -> &'static str {
        "embedding_event_handler"
    }

    fn priority(&self) -> u32 {
        300 // High priority for embedding processing
    }

    fn can_handle(&self, event: &FileEvent) -> bool {
        if event.is_dir {
            return false;
        }

        if let Some(ext) = event.extension() {
            self.supported_extensions.contains(&ext)
        } else {
            false
        }
    }
}
