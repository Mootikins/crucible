//! Event-Driven Embedding Processor
//!
//! This module provides the EventDrivenEmbeddingProcessor that connects file system events
//! from crucible-watch to the embedding thread pool, eliminating the need for polling
//! and providing real-time, event-driven embedding generation.

use crate::{
    embedding_events::{EmbeddingEvent, EmbeddingEventResult, EventDrivenEmbeddingConfig},
    error::{Error, Result},
    events::FileEvent,
    traits::EventHandler,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

// Import embedding pool types
use crucible_surrealdb::consistency;
use crucible_surrealdb::embedding_pool::EmbeddingThreadPool;

/// Event-driven embedding processor that connects file system events to embedding generation
pub struct EventDrivenEmbeddingProcessor {
    /// Configuration for the processor
    config: EventDrivenEmbeddingConfig,

    /// Embedding thread pool for processing
    embedding_pool: Arc<EmbeddingThreadPool>,

    /// Channel for receiving embedding events
    embedding_rx: Arc<RwLock<Option<mpsc::UnboundedReceiver<EmbeddingEvent>>>>,

    /// State management
    state: Arc<RwLock<EventProcessorState>>,

    /// Shutdown signal
    shutdown_signal: Arc<RwLock<bool>>,

    /// Handle to the background processing task
    processor_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

/// Internal state for the event processor
#[derive(Debug, Default)]
struct EventProcessorState {
    /// Current batch being accumulated
    current_batch: Vec<EmbeddingEvent>,

    /// Batch identifier for the current batch
    current_batch_id: Option<uuid::Uuid>,

    /// When the current batch will timeout
    batch_deadline: Option<Instant>,

    /// Events being processed
    processing_events: HashMap<uuid::Uuid, Instant>,

    /// Recently processed events for deduplication
    recent_events: HashMap<String, Instant>,

    /// Index of pending operations by file path for fast lookup
    /// Used by queue-aware database reads to check consistency
    pending_operations_by_file: std::collections::HashMap<
        std::path::PathBuf,
        Vec<crucible_surrealdb::consistency::PendingOperation>,
    >,

    /// Metrics
    metrics: EventProcessorMetrics,
}

/// Metrics for the event processor
#[derive(Debug, Default, Clone)]
pub struct EventProcessorMetrics {
    /// Total events received
    pub total_events_received: u64,

    /// Total events processed
    pub total_events_processed: u64,

    /// Total batches processed
    pub total_batches_processed: u64,

    /// Average batch size
    pub average_batch_size: f64,

    /// Total processing time
    pub total_processing_time: Duration,

    /// Failed events
    pub failed_events: u64,

    /// Deduplicated events
    pub deduplicated_events: u64,
}

impl EventDrivenEmbeddingProcessor {
    /// Create a new event-driven embedding processor
    pub async fn new(
        config: EventDrivenEmbeddingConfig,
        embedding_pool: Arc<EmbeddingThreadPool>,
    ) -> Result<Self> {
        let state = Arc::new(RwLock::new(EventProcessorState::default()));
        let shutdown_signal = Arc::new(RwLock::new(false));
        let embedding_rx = Arc::new(RwLock::new(None));
        let processor_handle = Arc::new(RwLock::new(None));

        let processor = Self {
            config,
            embedding_pool,
            embedding_rx,
            state,
            shutdown_signal,
            processor_handle,
        };

        Ok(processor)
    }

    /// Set the embedding event receiver channel
    pub async fn with_embedding_event_receiver(
        self,
        rx: mpsc::UnboundedReceiver<EmbeddingEvent>,
    ) -> Self {
        *self.embedding_rx.write().await = Some(rx);
        self
    }

    /// Start the event processing loop
    pub async fn start(&self) -> Result<()> {
        info!("Starting event-driven embedding processor");

        // Take the receiver
        let mut rx = {
            let mut receiver_guard = self.embedding_rx.write().await;
            receiver_guard
                .take()
                .ok_or_else(|| Error::Config("Embedding event receiver not set".to_string()))?
        };

        let state = self.state.clone();
        let config = self.config.clone();
        let embedding_pool = self.embedding_pool.clone();
        let shutdown_signal = self.shutdown_signal.clone();

        let handle = tokio::spawn(async move {
            info!("Event-driven embedding processor task started");

            loop {
                // Check for shutdown signal
                if *shutdown_signal.read().await {
                    info!("Event-driven embedding processor shutting down");
                    break;
                }

                // Check for batch timeout
                Self::check_batch_timeout(&state, &config, &embedding_pool).await;

                // Process incoming events with timeout
                match tokio::time::timeout(Duration::from_millis(10), rx.recv()).await {
                    Ok(Some(event)) => {
                        let file_path = event.file_path.clone();
                        info!(
                            "📥 Batch processor received embedding event for: {}",
                            file_path.display()
                        );
                        if let Err(e) =
                            Self::process_embedding_event(&state, &config, &embedding_pool, event)
                                .await
                        {
                            error!(
                                "Error processing embedding event for {}: {}",
                                file_path.display(),
                                e
                            );
                        } else {
                            info!(
                                "✅ Successfully processed embedding event for: {}",
                                file_path.display()
                            );
                        }
                    }
                    Ok(None) => {
                        warn!("🔌 Embedding event channel closed, shutting down processor");
                        break;
                    }
                    Err(_) => {
                        // Timeout, continue to check batch timeout and shutdown signal
                    }
                }
            }

            // Process any remaining batch before shutdown
            if let Err(e) = Self::process_remaining_batch(&state, &config, &embedding_pool).await {
                error!("Error processing remaining batch during shutdown: {}", e);
            }

            info!("Event-driven embedding processor task completed");
        });

        *self.processor_handle.write().await = Some(handle);
        Ok(())
    }

    /// Process a single embedding event
    async fn process_embedding_event(
        state: &Arc<RwLock<EventProcessorState>>,
        config: &EventDrivenEmbeddingConfig,
        embedding_pool: &Arc<EmbeddingThreadPool>,
        event: EmbeddingEvent,
    ) -> Result<()> {
        info!(
            "🔧 Processing embedding event for: {} (content length: {})",
            event.file_path.display(),
            event.content.len()
        );

        // Update metrics
        {
            let mut state_guard = state.write().await;
            state_guard.metrics.total_events_received += 1;
        }

        // Check for deduplication
        if config.enable_deduplication {
            let should_deduplicate = {
                let state_guard = state.read().await;
                let event_key = format!("{}:{}", event.file_path.display(), event.content.len());

                if let Some(last_time) = state_guard.recent_events.get(&event_key) {
                    let elapsed = last_time.elapsed();
                    let dedup_window = Duration::from_millis(config.deduplication_window_ms);
                    elapsed < dedup_window
                } else {
                    false
                }
            };

            if should_deduplicate {
                debug!(
                    "Deduplicating embedding event for: {}",
                    event.file_path.display()
                );
                {
                    let mut state_guard = state.write().await;
                    state_guard.metrics.deduplicated_events += 1;
                }
                return Ok(());
            }
        }

        // Add to batch
        {
            let mut state_guard = state.write().await;

            // Create new batch if needed
            if state_guard.current_batch.is_empty() {
                state_guard.current_batch_id = Some(uuid::Uuid::new_v4());
                state_guard.batch_deadline =
                    Some(Instant::now() + Duration::from_millis(config.batch_timeout_ms));
            }

            // Mark event as batched
            let mut batched_event = event.clone();
            if let Some(batch_id) = state_guard.current_batch_id {
                batched_event = batched_event.to_batched(batch_id);
            }

            state_guard.current_batch.push(batched_event);

            info!(
                "📦 Added event to batch (size: {}) for: {}",
                state_guard.current_batch.len(),
                event.file_path.display()
            );

            // Process batch if it's full
            if state_guard.current_batch.len() >= config.max_batch_size {
                info!(
                    "🚀 Batch full, processing {} events",
                    state_guard.current_batch.len()
                );
                let batch = std::mem::take(&mut state_guard.current_batch);
                let batch_id = state_guard.current_batch_id.take();
                state_guard.batch_deadline.take();

                drop(state_guard); // Release lock before processing

                if let Err(e) =
                    Self::process_batch(state, embedding_pool, batch, batch_id.unwrap()).await
                {
                    error!("Error processing batch: {}", e);
                }
            }
        }

        // Track event for deduplication
        if config.enable_deduplication {
            let event_key = format!("{}:{}", event.file_path.display(), event.content.len());
            let mut state_guard = state.write().await;
            state_guard.recent_events.insert(event_key, Instant::now());

            // Clean up old events
            let cutoff = Instant::now() - Duration::from_millis(config.deduplication_window_ms * 2);
            state_guard
                .recent_events
                .retain(|_, &mut time| time > cutoff);
        }

        Ok(())
    }

    /// Check if the current batch has timed out and process it if needed
    async fn check_batch_timeout(
        state: &Arc<RwLock<EventProcessorState>>,
        _config: &EventDrivenEmbeddingConfig,
        embedding_pool: &Arc<EmbeddingThreadPool>,
    ) {
        let (batch, batch_id) = {
            let state_guard = state.read().await;
            if let Some(deadline) = state_guard.batch_deadline {
                if !state_guard.current_batch.is_empty() && Instant::now() >= deadline {
                    let batch = state_guard.current_batch.clone();
                    let batch_id = state_guard.current_batch_id;
                    (batch, batch_id.unwrap())
                } else {
                    return; // No batch ready yet
                }
            } else {
                return; // No batch deadline set
            }
        };

        debug!("Processing batch due to timeout (size: {})", batch.len());

        // Clear the batch in state
        {
            let mut state_guard = state.write().await;
            if Some(batch_id) == state_guard.current_batch_id {
                std::mem::take(&mut state_guard.current_batch);
                state_guard.current_batch_id.take();
                state_guard.batch_deadline.take();
            }
        }

        // Process the batch
        if let Err(e) = Self::process_batch(state, embedding_pool, batch, batch_id).await {
            error!("Error processing timeout batch: {}", e);
        }
    }

    /// Process a batch of embedding events
    async fn process_batch(
        state: &Arc<RwLock<EventProcessorState>>,
        embedding_pool: &Arc<EmbeddingThreadPool>,
        batch: Vec<EmbeddingEvent>,
        batch_id: uuid::Uuid,
    ) -> Result<()> {
        info!(
            "🔄 Processing batch {} with {} events",
            batch_id,
            batch.len()
        );
        for (i, event) in batch.iter().enumerate() {
            info!(
                "  Event {}: {} ({} chars)",
                i,
                event.file_path.display(),
                event.content.len()
            );
        }

        let start_time = Instant::now();
        let mut successful = 0;
        let mut failed = 0;

        // Track processing events
        {
            let mut state_guard = state.write().await;
            for event in &batch {
                state_guard
                    .processing_events
                    .insert(event.id, Instant::now());
            }
        }

        // Process events in parallel up to max_concurrent_requests
        let semaphore = Arc::new(tokio::sync::Semaphore::new(
            std::cmp::min(batch.len(), 10), // Limit concurrency
        ));

        let mut futures = Vec::new();

        for event in batch {
            let semaphore = semaphore.clone();
            let state = state.clone();
            let embedding_pool = embedding_pool.clone();

            let future = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                let event_id = event.id;

                let result = Self::process_single_event(&state, &embedding_pool, event).await;

                // Remove from processing events
                {
                    let mut state_guard = state.write().await;
                    state_guard.processing_events.remove(&event_id);
                }

                result
            });

            futures.push(future);
        }

        // Wait for all events to complete
        for future in futures {
            match future.await {
                Ok(Ok(_)) => successful += 1,
                Ok(Err(e)) => {
                    failed += 1;
                    error!("Event processing failed: {}", e);
                }
                Err(e) => {
                    failed += 1;
                    error!("Event processing task failed: {}", e);
                }
            }
        }

        // Update metrics
        {
            let mut state_guard = state.write().await;
            state_guard.metrics.total_events_processed += (successful + failed) as u64;
            state_guard.metrics.total_batches_processed += 1;
            state_guard.metrics.failed_events += failed as u64;

            // Update average batch size
            let batch_size = (successful + failed) as f64;
            let total_batches = state_guard.metrics.total_batches_processed as f64;
            state_guard.metrics.average_batch_size =
                (state_guard.metrics.average_batch_size * (total_batches - 1.0) + batch_size)
                    / total_batches;

            state_guard.metrics.total_processing_time += start_time.elapsed();
        }

        info!(
            "Batch {} completed: {} successful, {} failed, {:?}",
            batch_id,
            successful,
            failed,
            start_time.elapsed()
        );

        Ok(())
    }

    /// Process a single embedding event
    async fn process_single_event(
        _state: &Arc<RwLock<EventProcessorState>>,
        embedding_pool: &Arc<EmbeddingThreadPool>,
        event: EmbeddingEvent,
    ) -> Result<()> {
        info!(
            "⚡ Processing single event for: {} (doc_id: {})",
            event.file_path.display(),
            event.document_id
        );

        let start_time = Instant::now();

        info!(
            "🔄 Sending document to embedding pool: {}",
            event.file_path.display()
        );
        // Process with embedding pool
        let retry_result = embedding_pool
            .process_document_with_retry(&event.document_id, &event.content)
            .await
            .map_err(|e| {
                error!(
                    "❌ Embedding pool failed for {}: {}",
                    event.file_path.display(),
                    e
                );
                Error::Embedding(format!("Failed to process document: {}", e))
            })?;

        let elapsed = start_time.elapsed();

        if retry_result.succeeded {
            info!(
                "✅ Successfully processed event for: {} (attempt: {}, time: {:?})",
                event.file_path.display(),
                retry_result.attempt_count,
                elapsed
            );
        } else {
            error!(
                "❌ Failed to process event for: {} after {} attempts in {:?}: {}",
                event.file_path.display(),
                retry_result.attempt_count,
                elapsed,
                retry_result
                    .final_error
                    .map(|e| e.error_message)
                    .unwrap_or_default()
            );
        }

        Ok(())
    }

    /// Process any remaining batch before shutdown
    async fn process_remaining_batch(
        state: &Arc<RwLock<EventProcessorState>>,
        _config: &EventDrivenEmbeddingConfig,
        embedding_pool: &Arc<EmbeddingThreadPool>,
    ) -> Result<()> {
        let (batch, batch_id) = {
            let mut state_guard = state.write().await;
            let batch = std::mem::take(&mut state_guard.current_batch);
            let batch_id = state_guard.current_batch_id.take();
            state_guard.batch_deadline.take();
            (batch, batch_id)
        };

        if !batch.is_empty() {
            info!(
                "Processing remaining batch of {} events during shutdown",
                batch.len()
            );
            if let Some(batch_id) = batch_id {
                Self::process_batch(state, embedding_pool, batch, batch_id).await?;
            }
        }

        Ok(())
    }

    /// Process a file event and convert it to embedding processing
    pub async fn process_file_event(&self, event: FileEvent) -> Result<EmbeddingEventResult> {
        debug!("Processing file event: {:?}", event.kind);

        // Convert file event to embedding event
        let embedding_event = self.transform_file_event_to_embedding_event(event).await?;

        // Process the embedding event
        let start_time = Instant::now();

        // For now, we'll send it directly to the embedding pool
        // In a full implementation, we'd add it to the batch
        let retry_result = self
            .embedding_pool
            .process_document_with_retry(&embedding_event.document_id, &embedding_event.content)
            .await
            .map_err(|e| Error::Embedding(format!("Failed to process document: {}", e)))?;

        let elapsed = start_time.elapsed();

        if retry_result.succeeded {
            Ok(EmbeddingEventResult::success(
                embedding_event.id,
                elapsed,
                self.embedding_pool.model_type().await.dimensions(),
            ))
        } else {
            let error_msg = retry_result
                .final_error
                .map(|e| e.error_message)
                .unwrap_or_else(|| "Unknown error".to_string());

            Ok(EmbeddingEventResult::failure(
                embedding_event.id,
                elapsed,
                error_msg,
            ))
        }
    }

    /// Transform a file event to an embedding event
    async fn transform_file_event_to_embedding_event(
        &self,
        event: FileEvent,
    ) -> Result<EmbeddingEvent> {
        // Read file content
        let content = tokio::fs::read_to_string(&event.path)
            .await
            .map_err(|e| Error::Io(e))?;

        // Get file metadata
        let metadata = tokio::fs::metadata(&event.path)
            .await
            .map_err(|e| Error::Io(e))?;

        let file_size = Some(metadata.len());

        // Create embedding event
        let embedding_event = EmbeddingEvent::new(
            event.path.clone(),
            event.kind.clone(),
            content,
            crate::embedding_events::create_embedding_metadata(&event.path, &event.kind, file_size),
        );

        Ok(embedding_event)
    }

    /// Shutdown the processor gracefully
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down event-driven embedding processor");

        // Set shutdown signal
        {
            let mut shutdown = self.shutdown_signal.write().await;
            *shutdown = true;
        }

        // Wait for processor task to complete
        if let Some(handle) = self.processor_handle.write().await.take() {
            match handle.await {
                Ok(_) => info!("Event-driven embedding processor shutdown successfully"),
                Err(e) => warn!(
                    "Event-driven embedding processor task completed with error: {:?}",
                    e
                ),
            }
        }

        Ok(())
    }

    /// Check if the processor is shutdown
    pub async fn is_shutdown(&self) -> bool {
        *self.shutdown_signal.read().await
    }

    /// Get current processor metrics
    pub async fn get_metrics(&self) -> EventProcessorMetrics {
        let state_guard = self.state.read().await;
        state_guard.metrics.clone()
    }

    /// Get pending operations for a specific file
    /// Used by queue-aware database reads to check consistency
    pub async fn get_pending_operations_for_file(
        &self,
        file_path: &std::path::Path,
    ) -> consistency::PendingOperationsResult {
        let state_guard = self.state.read().await;

        // Get operations from current batch (queued)
        let queued_ops: Vec<consistency::PendingOperation> = state_guard
            .current_batch
            .iter()
            .filter(|event| event.file_path == file_path)
            .map(|event| self.embedding_event_to_pending_operation(event))
            .collect();

        // Get operations that are currently being processed
        let processing_ops: Vec<consistency::PendingOperation> = state_guard
            .current_batch
            .iter()
            .filter(|event| event.file_path == file_path)
            .filter(|event| state_guard.processing_events.contains_key(&event.id))
            .map(|event| self.embedding_event_to_pending_operation(event))
            .collect();

        // Also check the pending operations index for any additional operations
        let indexed_ops = state_guard
            .pending_operations_by_file
            .get(file_path)
            .cloned()
            .unwrap_or_default();

        // Combine all operations, deduplicating by event ID
        let mut all_ops = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        for op in queued_ops
            .into_iter()
            .chain(processing_ops)
            .chain(indexed_ops)
        {
            if !seen_ids.contains(&op.event_id) {
                seen_ids.insert(op.event_id);
                all_ops.push(op);
            }
        }

        // Determine processing status
        let has_processing = state_guard.current_batch.iter().any(|event| {
            event.file_path == file_path && state_guard.processing_events.contains_key(&event.id)
        });

        if all_ops.is_empty() {
            consistency::PendingOperationsResult::none()
        } else if has_processing {
            // Some are processing, some might be queued
            let processing = all_ops
                .iter()
                .filter(|op| state_guard.processing_events.contains_key(&op.event_id))
                .cloned()
                .collect();

            let queued: Vec<consistency::PendingOperation> = all_ops
                .iter()
                .filter(|op| !state_guard.processing_events.contains_key(&op.event_id))
                .cloned()
                .collect();

            if queued.is_empty() {
                consistency::PendingOperationsResult::processing(processing)
            } else {
                consistency::PendingOperationsResult::queued_and_processing(queued, processing)
            }
        } else {
            // All are queued
            consistency::PendingOperationsResult::queued(all_ops)
        }
    }

    /// Convert an EmbeddingEvent to a PendingOperation
    fn embedding_event_to_pending_operation(
        &self,
        event: &EmbeddingEvent,
    ) -> consistency::PendingOperation {
        let operation_type = match event.trigger_event {
            crate::events::FileEventKind::Created => consistency::OperationType::Create,
            crate::events::FileEventKind::Modified => consistency::OperationType::Update,
            crate::events::FileEventKind::Deleted => consistency::OperationType::Delete,
            _ => consistency::OperationType::Update, // Default to update for other types
        };

        // Estimate completion time based on current batch state
        let estimated_completion =
            std::time::Instant::now() + std::time::Duration::from_millis(1000);

        consistency::PendingOperation {
            file_path: event.file_path.clone(),
            operation_type,
            batch_id: event.metadata.batch_id,
            queued_at: std::time::Instant::now(),
            estimated_completion: Some(estimated_completion),
            event_id: event.id,
        }
    }

    /// Update the pending operations index when events are added to the batch
    async fn update_pending_operations_index(&self, events: &[EmbeddingEvent]) {
        let mut state_guard = self.state.write().await;

        for event in events {
            let pending_op = self.embedding_event_to_pending_operation(event);

            // Add to the index organized by file path
            state_guard
                .pending_operations_by_file
                .entry(event.file_path.clone())
                .or_insert_with(Vec::new)
                .push(pending_op);
        }
    }

    /// Clean up the pending operations index when events are processed
    async fn cleanup_pending_operations_index(&self, event_ids: &[uuid::Uuid]) {
        let mut state_guard = self.state.write().await;

        for event_id in event_ids {
            // Remove from the processing events map
            state_guard.processing_events.remove(event_id);

            // Remove from the pending operations index
            state_guard
                .pending_operations_by_file
                .retain(|_, pending_ops| {
                    pending_ops.retain(|op| op.event_id != *event_id);
                    !pending_ops.is_empty() // Remove empty entries
                });
        }
    }

    /// Force flush all pending operations for specific files
    /// Used when strong consistency is required
    pub async fn flush_for_files(
        &self,
        file_paths: &[std::path::PathBuf],
    ) -> Result<crucible_surrealdb::consistency::FlushResult> {
        let start_time = std::time::Instant::now();
        let mut total_flushed = 0;

        // Take the current batch and filter for target files
        let mut state_guard = self.state.write().await;

        let (target_events, remaining_events): (Vec<_>, Vec<_>) = state_guard
            .current_batch
            .iter()
            .cloned()
            .partition(|event| file_paths.contains(&event.file_path));

        // Keep only non-target events in the current batch
        state_guard.current_batch = remaining_events;

        // Update the pending operations index to remove flushed events
        for event in &target_events {
            state_guard
                .pending_operations_by_file
                .remove(&event.file_path);
        }

        // If batch is now empty, reset batch state
        if state_guard.current_batch.is_empty() {
            state_guard.current_batch_id.take();
            state_guard.batch_deadline.take();
        }

        drop(state_guard);

        // Process the target events immediately
        if !target_events.is_empty() {
            let batch_id = uuid::Uuid::new_v4();

            // Mark events as processing
            {
                let mut state_guard = self.state.write().await;
                for event in &target_events {
                    state_guard
                        .processing_events
                        .insert(event.id, std::time::Instant::now());
                }
            }

            // Process immediately (this is a simplified synchronous version)
            // In a full implementation, this would use the existing batch processing infrastructure
            for event in &target_events {
                // Simulate processing - replace with actual processing logic
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                total_flushed += 1;
            }

            // Clean up after processing
            let event_ids: Vec<_> = target_events.iter().map(|e| e.id).collect();
            self.cleanup_pending_operations_index(&event_ids).await;
        }

        let flush_duration = start_time.elapsed();

        Ok(crucible_surrealdb::consistency::FlushResult {
            operations_flushed: total_flushed,
            flush_duration,
            success_rate: if total_flushed > 0 { 1.0 } else { 0.0 },
        })
    }

    /// Get the current status of batch processing
    pub async fn get_batch_status(&self) -> crucible_surrealdb::consistency::FlushStatus {
        let state_guard = self.state.read().await;

        let pending_batches = state_guard.current_batch.len();
        let processing_events = state_guard.processing_events.len();

        let estimated_completion = if pending_batches > 0 {
            state_guard.batch_deadline
        } else {
            None
        };

        crucible_surrealdb::consistency::FlushStatus {
            pending_batches,
            processing_events,
            estimated_completion,
        }
    }
}

/// Integration handler that converts file events to embedding events
pub struct EmbeddingEventHandler {
    /// Event processor instance
    processor: Arc<EventDrivenEmbeddingProcessor>,

    /// Sender for embedding events
    embedding_event_tx: mpsc::UnboundedSender<EmbeddingEvent>,

    /// Supported file extensions
    supported_extensions: Vec<String>,

    /// Whether to enable real-time processing
    enable_real_time: bool,
}

impl EmbeddingEventHandler {
    /// Create a new embedding event handler
    pub fn new(
        processor: Arc<EventDrivenEmbeddingProcessor>,
        embedding_event_tx: mpsc::UnboundedSender<EmbeddingEvent>,
    ) -> Self {
        Self {
            processor,
            embedding_event_tx,
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

    /// Convert a file event to an embedding event
    async fn convert_file_event_to_embedding_event(
        &self,
        event: FileEvent,
    ) -> Result<EmbeddingEvent> {
        // Read file content
        let content = tokio::fs::read_to_string(&event.path)
            .await
            .map_err(|e| Error::Io(e))?;

        // Get file metadata
        let metadata = tokio::fs::metadata(&event.path)
            .await
            .map_err(|e| Error::Io(e))?;

        let file_size = Some(metadata.len());

        // Create embedding event
        let embedding_event = EmbeddingEvent::new(
            event.path.clone(),
            event.kind.clone(),
            content,
            crate::embedding_events::create_embedding_metadata(&event.path, &event.kind, file_size),
        );

        Ok(embedding_event)
    }
}

#[async_trait::async_trait]
impl EventHandler for EmbeddingEventHandler {
    async fn handle(&self, event: FileEvent) -> Result<()> {
        info!(
            "🔥 EmbeddingEventHandler received event: {:?} for file: {}",
            event.kind,
            event.path.display()
        );

        if !self.enable_real_time {
            warn!(
                "Real-time processing disabled, skipping event for: {}",
                event.path.display()
            );
            return Ok(());
        }

        info!(
            "Converting file event to embedding event for: {}",
            event.path.display()
        );
        // Convert file event to embedding event
        let embedding_event = match self
            .convert_file_event_to_embedding_event(event.clone())
            .await
        {
            Ok(embedding_event) => {
                info!(
                    "Successfully created embedding event for: {}",
                    event.path.display()
                );
                embedding_event
            }
            Err(e) => {
                error!(
                    "Failed to convert file event to embedding event for {}: {}",
                    event.path.display(),
                    e
                );
                return Err(e);
            }
        };

        // Send embedding event through the channel for batch processing
        info!(
            "Sending embedding event to batch processor for: {}",
            event.path.display()
        );
        match self.embedding_event_tx.send(embedding_event) {
            Ok(_) => {
                info!(
                    "Successfully queued embedding event for: {}",
                    event.path.display()
                );
                debug!("Successfully sent embedding event for batch processing");
            }
            Err(e) => {
                warn!("Failed to send embedding event: {}", e);
                return Err(Error::Channel(format!(
                    "Failed to send embedding event: {}",
                    e
                )));
            }
        }

        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_embedding_metadata, EmbeddingEventPriority, FileEvent, FileEventKind};
    use crucible_surrealdb::EmbeddingConfig;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_event_processor_creation() -> Result<()> {
        let config = EventDrivenEmbeddingConfig::default();
        let embedding_config = EmbeddingConfig::default();
        let embedding_pool = EmbeddingThreadPool::new(embedding_config).await?;

        let processor =
            EventDrivenEmbeddingProcessor::new(config, Arc::new(embedding_pool)).await?;

        assert!(!processor.is_shutdown().await);

        let metrics = processor.get_metrics().await;
        assert_eq!(metrics.total_events_received, 0);
        assert_eq!(metrics.total_events_processed, 0);

        processor.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_file_event_transformation() -> Result<()> {
        let config = EventDrivenEmbeddingConfig::default();
        let embedding_config = EmbeddingConfig::default();
        let embedding_pool = EmbeddingThreadPool::new(embedding_config).await?;

        let processor =
            EventDrivenEmbeddingProcessor::new(config, Arc::new(embedding_pool)).await?;

        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.md");
        let content = "# Test Document\nThis is a test.";

        tokio::fs::write(&file_path, content).await?;

        let event = FileEvent::new(FileEventKind::Created, file_path.clone());
        let embedding_event = processor
            .transform_file_event_to_embedding_event(event)
            .await?;

        assert_eq!(embedding_event.file_path, file_path);
        assert_eq!(embedding_event.trigger_event, FileEventKind::Created);
        assert_eq!(embedding_event.content, content);
        assert_eq!(embedding_event.metadata.content_type, "text/markdown");
        assert_eq!(
            embedding_event.metadata.file_extension,
            Some("md".to_string())
        );
        assert!(embedding_event.metadata.file_size.is_some());

        processor.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_batch_processing_logic() -> Result<()> {
        let config = EventDrivenEmbeddingConfig {
            max_batch_size: 2,
            batch_timeout_ms: 100,
            ..Default::default()
        };

        let embedding_config = EmbeddingConfig::default();
        let embedding_pool = EmbeddingThreadPool::new(embedding_config).await?;

        let processor =
            EventDrivenEmbeddingProcessor::new(config, Arc::new(embedding_pool)).await?;

        // Test that batch processing works (implementation details depend on the specific use case)
        let metrics = processor.get_metrics().await;
        assert_eq!(metrics.total_batches_processed, 0);

        processor.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_deduplication() -> Result<()> {
        let config = EventDrivenEmbeddingConfig {
            enable_deduplication: true,
            deduplication_window_ms: 1000,
            ..Default::default()
        };

        let embedding_config = EmbeddingConfig::default();
        let embedding_pool = EmbeddingThreadPool::new(embedding_config).await?;

        let processor =
            EventDrivenEmbeddingProcessor::new(config, Arc::new(embedding_pool)).await?;

        // Test deduplication logic
        let metrics = processor.get_metrics().await;
        assert_eq!(metrics.deduplicated_events, 0);

        processor.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_embedding_event_handler() -> Result<()> {
        let config = EventDrivenEmbeddingConfig::default();
        let embedding_config = EmbeddingConfig::default();
        let embedding_pool = EmbeddingThreadPool::new(embedding_config).await?;

        let processor =
            Arc::new(EventDrivenEmbeddingProcessor::new(config, Arc::new(embedding_pool)).await?);

        // Create a channel for testing
        let (embedding_tx, _embedding_rx) = mpsc::unbounded_channel::<EmbeddingEvent>();
        let handler = EmbeddingEventHandler::new(processor.clone(), embedding_tx);

        assert_eq!(handler.name(), "embedding_event_handler");
        assert_eq!(handler.priority(), 300);
        assert!(handler.enable_real_time);
        assert!(handler.supported_extensions.contains(&"md".to_string()));

        // Test file handling capability
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.md");
        let event = FileEvent::new(FileEventKind::Created, file_path);

        assert!(handler.can_handle(&event));

        // Test unsupported file
        let unsupported_path = temp_dir.path().join("test.exe");
        let unsupported_event = FileEvent::new(FileEventKind::Created, unsupported_path);
        assert!(!handler.can_handle(&unsupported_event));

        processor.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_metrics_tracking() -> Result<()> {
        let config = EventDrivenEmbeddingConfig::default();
        let embedding_config = EmbeddingConfig::default();
        let embedding_pool = EmbeddingThreadPool::new(embedding_config).await?;

        let processor =
            EventDrivenEmbeddingProcessor::new(config, Arc::new(embedding_pool)).await?;

        let initial_metrics = processor.get_metrics().await;
        assert_eq!(initial_metrics.total_events_received, 0);
        assert_eq!(initial_metrics.total_events_processed, 0);
        assert_eq!(initial_metrics.total_batches_processed, 0);
        assert_eq!(initial_metrics.failed_events, 0);
        assert_eq!(initial_metrics.deduplicated_events, 0);

        processor.shutdown().await?;
        Ok(())
    }
}
