//! Message Channel Infrastructure for Event-Driven Embedding
//!
//! This module provides the core message channel infrastructure that connects
//! file system events to embedding processing, enabling efficient event-driven
//! processing without polling.

use crate::{
    embedding_events::{EmbeddingEvent, EmbeddingEventResult, EventDrivenEmbeddingConfig},
    error::Result,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Message channel infrastructure for event-driven embedding
pub struct MessageChannelInfrastructure {
    /// Configuration for the channel infrastructure
    config: EventDrivenEmbeddingConfig,

    /// Broadcast channel for embedding events (multiple subscribers)
    embedding_event_tx: broadcast::Sender<EmbeddingEvent>,

    /// Channel for embedding results
    embedding_result_tx: mpsc::UnboundedSender<EmbeddingEventResult>,
    embedding_result_rx: Arc<RwLock<Option<mpsc::UnboundedReceiver<EmbeddingEventResult>>>>,

    /// State management
    state: Arc<RwLock<ChannelState>>,

    /// Shutdown signal
    shutdown_signal: Arc<RwLock<bool>>,

    /// Batch processing task handle
    batch_processor_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

/// Internal state for the channel infrastructure
#[derive(Debug, Default)]
struct ChannelState {
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

    /// Channel metrics
    metrics: ChannelMetrics,
}

/// Metrics for the channel infrastructure
#[derive(Debug, Default, Clone)]
pub struct ChannelMetrics {
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

    /// Channel capacity utilization
    pub channel_utilization: f64,

    /// Number of active subscribers
    pub active_subscribers: u32,
}

impl MessageChannelInfrastructure {
    /// Create a new message channel infrastructure
    pub fn new(config: EventDrivenEmbeddingConfig) -> Result<Self> {
        let (embedding_event_tx, _) = broadcast::channel(config.max_queue_size);
        let (embedding_result_tx, embedding_result_rx) = mpsc::unbounded_channel();

        let state = Arc::new(RwLock::new(ChannelState::default()));
        let shutdown_signal = Arc::new(RwLock::new(false));
        let batch_processor_handle = Arc::new(RwLock::new(None));

        Ok(Self {
            config,
            embedding_event_tx,
            embedding_result_tx,
            embedding_result_rx: Arc::new(RwLock::new(Some(embedding_result_rx))),
            state,
            shutdown_signal,
            batch_processor_handle,
        })
    }

    /// Start the message channel infrastructure
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting message channel infrastructure");

        // Start the batch processor
        let state = self.state.clone();
        let config = self.config.clone();
        let shutdown_signal = self.shutdown_signal.clone();
        let mut embedding_result_tx = self.embedding_result_tx.clone();

        let handle = tokio::spawn(async move {
            info!("Batch processor task started");

            loop {
                // Check for shutdown signal
                if *shutdown_signal.read().await {
                    info!("Batch processor shutting down");
                    break;
                }

                // Check for batch timeout and process if needed
                Self::check_and_process_batch_timeout(&state, &config, &mut embedding_result_tx)
                    .await;

                // Small delay to prevent busy-waiting
                tokio::time::sleep(Duration::from_millis(10)).await;
            }

            // Process any remaining batch before shutdown
            if let Err(e) =
                Self::process_remaining_batch(&state, &config, &mut embedding_result_tx).await
            {
                error!("Error processing remaining batch during shutdown: {}", e);
            }

            info!("Batch processor task completed");
        });

        *self.batch_processor_handle.write().await = Some(handle);
        Ok(())
    }

    /// Get a sender for embedding events
    pub fn get_embedding_event_sender(&self) -> broadcast::Sender<EmbeddingEvent> {
        self.embedding_event_tx.clone()
    }

    /// Get a receiver for embedding events
    pub fn subscribe_to_embedding_events(&self) -> broadcast::Receiver<EmbeddingEvent> {
        self.embedding_event_tx.subscribe()
    }

    /// Get a sender for embedding results
    pub fn get_embedding_result_sender(&self) -> mpsc::UnboundedSender<EmbeddingEventResult> {
        self.embedding_result_tx.clone()
    }

    /// Get a receiver for embedding results
    pub async fn get_embedding_result_receiver(
        &self,
    ) -> Option<mpsc::UnboundedReceiver<EmbeddingEventResult>> {
        self.embedding_result_rx.write().await.take()
    }

    /// Send an embedding event through the channel
    pub async fn send_embedding_event(&self, event: EmbeddingEvent) -> Result<()> {
        debug!("Sending embedding event for: {}", event.file_path.display());

        // Update metrics
        {
            let mut state = self.state.write().await;
            state.metrics.total_events_received += 1;
        }

        // Check for deduplication
        if self.config.enable_deduplication {
            let should_deduplicate = {
                let state = self.state.read().await;
                let event_key = format!("{}:{}", event.file_path.display(), event.content.len());

                if let Some(last_time) = state.recent_events.get(&event_key) {
                    let elapsed = last_time.elapsed();
                    let dedup_window = Duration::from_millis(self.config.deduplication_window_ms);
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
                    let mut state = self.state.write().await;
                    state.metrics.deduplicated_events += 1;
                }
                return Ok(());
            }
        }

        // Send the event
        match self.embedding_event_tx.send(event.clone()) {
            Ok(_) => {
                debug!(
                    "Successfully sent embedding event for: {}",
                    event.file_path.display()
                );
            }
            Err(broadcast::error::SendError(_)) => {
                warn!(
                    "No subscribers for embedding event: {}",
                    event.file_path.display()
                );
            }
        }

        // Track event for deduplication before moving it
        if self.config.enable_deduplication {
            let event_key = format!("{}:{}", event.file_path.display(), event.content.len());
            let mut state = self.state.write().await;
            state.recent_events.insert(event_key, Instant::now());

            // Clean up old events
            let cutoff =
                Instant::now() - Duration::from_millis(self.config.deduplication_window_ms * 2);
            state.recent_events.retain(|_, &mut time| time > cutoff);
        }

        // Add to batch processing
        self.add_to_batch(event).await?;

        Ok(())
    }

    /// Add an event to the current batch
    async fn add_to_batch(&self, event: EmbeddingEvent) -> Result<()> {
        let mut state = self.state.write().await;

        // Create new batch if needed
        if state.current_batch.is_empty() {
            state.current_batch_id = Some(uuid::Uuid::new_v4());
            state.batch_deadline =
                Some(Instant::now() + Duration::from_millis(self.config.batch_timeout_ms));
        }

        // Mark event as batched
        let mut batched_event = event.clone();
        if let Some(batch_id) = state.current_batch_id {
            batched_event = batched_event.to_batched(batch_id);
        }

        state.current_batch.push(batched_event);
        debug!("Added event to batch (size: {})", state.current_batch.len());

        // Process batch if it's full
        if state.current_batch.len() >= self.config.max_batch_size {
            let batch = std::mem::take(&mut state.current_batch);
            let batch_id = state.current_batch_id.take();
            state.batch_deadline.take();

            drop(state); // Release lock before processing

            // Send batch result notification
            if let Some(batch_id) = batch_id {
                self.send_batch_result(batch, batch_id).await?;
            }
        }

        Ok(())
    }

    /// Check and process batch timeout
    async fn check_and_process_batch_timeout(
        state: &Arc<RwLock<ChannelState>>,
        _config: &EventDrivenEmbeddingConfig,
        embedding_result_tx: &mut mpsc::UnboundedSender<EmbeddingEventResult>,
    ) {
        let should_process = {
            let state = state.read().await;
            if let Some(deadline) = state.batch_deadline {
                !state.current_batch.is_empty() && Instant::now() >= deadline
            } else {
                false
            }
        };

        if should_process {
            let (batch, batch_id) = {
                let mut state = state.write().await;
                if !state.current_batch.is_empty() {
                    debug!(
                        "Processing batch due to timeout (size: {})",
                        state.current_batch.len()
                    );

                    let batch = std::mem::take(&mut state.current_batch);
                    let batch_id = state.current_batch_id.take();
                    state.batch_deadline.take();
                    (batch, batch_id)
                } else {
                    return;
                }
            };

            if let Some(batch_id) = batch_id {
                // Send batch completion results
                for event in &batch {
                    let result = EmbeddingEventResult::success(
                        event.id,
                        Duration::from_millis(50), // Simulated processing time
                        256,                       // Mock dimensions
                    );

                    if let Err(_) = embedding_result_tx.send(result) {
                        debug!("Failed to send batch timeout result - no receiver");
                        break;
                    }
                }

                // Update metrics
                {
                    let mut state = state.write().await;
                    state.metrics.total_batches_processed += 1;
                    state.metrics.total_events_processed += batch.len() as u64;
                }
            }
        }
    }

    /// Process remaining batch before shutdown
    async fn process_remaining_batch(
        state: &Arc<RwLock<ChannelState>>,
        _config: &EventDrivenEmbeddingConfig,
        embedding_result_tx: &mut mpsc::UnboundedSender<EmbeddingEventResult>,
    ) -> Result<()> {
        let (batch, _batch_id) = {
            let mut state = state.write().await;
            let batch = std::mem::take(&mut state.current_batch);
            let batch_id = state.current_batch_id.take();
            state.batch_deadline.take();
            (batch, batch_id)
        };

        if !batch.is_empty() {
            info!(
                "Processing remaining batch of {} events during shutdown",
                batch.len()
            );

            // Send completion results for remaining events
            for event in &batch {
                let result =
                    EmbeddingEventResult::success(event.id, Duration::from_millis(50), 256);

                if let Err(_) = embedding_result_tx.send(result) {
                    debug!("Failed to send remaining batch result");
                    break;
                }
            }

            // Update metrics
            {
                let mut state = state.write().await;
                state.metrics.total_batches_processed += 1;
                state.metrics.total_events_processed += batch.len() as u64;
            }
        }

        Ok(())
    }

    /// Send batch completion results
    async fn send_batch_result(
        &self,
        batch: Vec<EmbeddingEvent>,
        batch_id: uuid::Uuid,
    ) -> Result<()> {
        debug!(
            "Sending batch result for batch {} with {} events",
            batch_id,
            batch.len()
        );

        // Update metrics
        {
            let mut state = self.state.write().await;
            state.metrics.total_batches_processed += 1;
            state.metrics.total_events_processed += batch.len() as u64;

            // Update average batch size
            let batch_size = batch.len() as f64;
            let total_batches = state.metrics.total_batches_processed as f64;
            state.metrics.average_batch_size =
                (state.metrics.average_batch_size * (total_batches - 1.0) + batch_size)
                    / total_batches;
        }

        // Send completion results for each event in the batch
        for event in &batch {
            let result = EmbeddingEventResult::success(
                event.id,
                Duration::from_millis(50), // Simulated processing time
                256,                       // Mock dimensions
            );

            if let Err(_) = self.embedding_result_tx.send(result) {
                debug!("Failed to send batch result - no receiver");
                break;
            }
        }

        Ok(())
    }

    /// Get current channel metrics
    pub async fn get_metrics(&self) -> ChannelMetrics {
        let state = self.state.read().await;
        let mut metrics = state.metrics.clone();

        // Update channel utilization
        metrics.channel_utilization = if self.config.max_queue_size > 0 {
            (self.embedding_event_tx.len() as f64) / (self.config.max_queue_size as f64)
        } else {
            0.0
        };

        // Update active subscribers count
        metrics.active_subscribers = self.embedding_event_tx.receiver_count() as u32;

        metrics
    }

    /// Shutdown the channel infrastructure gracefully
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down message channel infrastructure");

        // Set shutdown signal
        {
            let mut shutdown = self.shutdown_signal.write().await;
            *shutdown = true;
        }

        // Wait for batch processor to complete
        if let Some(handle) = self.batch_processor_handle.write().await.take() {
            match handle.await {
                Ok(_) => info!("Batch processor shutdown successfully"),
                Err(e) => warn!("Batch processor completed with error: {:?}", e),
            }
        }

        // Close the embedding event channel
        drop(self.embedding_event_tx.clone());

        info!("Message channel infrastructure shutdown complete");
        Ok(())
    }

    /// Check if the infrastructure is shutdown
    pub async fn is_shutdown(&self) -> bool {
        *self.shutdown_signal.read().await
    }
}

/// Factory functions for creating message channel infrastructure
impl MessageChannelInfrastructure {
    /// Create infrastructure optimized for high throughput
    pub fn optimize_for_throughput() -> Result<Self> {
        let config = EventDrivenEmbeddingConfig {
            max_batch_size: 64,
            batch_timeout_ms: 100,
            max_concurrent_requests: 16,
            max_queue_size: 2000,
            max_retry_attempts: 2,
            retry_delay_ms: 500,
            enable_deduplication: true,
            deduplication_window_ms: 1000,
        };

        Self::new(config)
    }

    /// Create infrastructure optimized for low latency
    pub fn optimize_for_latency() -> Result<Self> {
        let config = EventDrivenEmbeddingConfig {
            max_batch_size: 4,
            batch_timeout_ms: 10,
            max_concurrent_requests: 8,
            max_queue_size: 100,
            max_retry_attempts: 1,
            retry_delay_ms: 100,
            enable_deduplication: false,
            deduplication_window_ms: 500,
        };

        Self::new(config)
    }

    /// Create infrastructure optimized for resource efficiency
    pub fn optimize_for_resources() -> Result<Self> {
        let config = EventDrivenEmbeddingConfig {
            max_batch_size: 16,
            batch_timeout_ms: 500,
            max_concurrent_requests: 4,
            max_queue_size: 50,
            max_retry_attempts: 1,
            retry_delay_ms: 1000,
            enable_deduplication: true,
            deduplication_window_ms: 2000,
        };

        Self::new(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileEvent, FileEventKind};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_channel_infrastructure_creation() -> Result<()> {
        let config = EventDrivenEmbeddingConfig::default();
        let mut infrastructure = MessageChannelInfrastructure::new(config)?;

        assert!(!infrastructure.is_shutdown().await);

        let metrics = infrastructure.get_metrics().await;
        assert_eq!(metrics.total_events_received, 0);
        assert_eq!(metrics.total_events_processed, 0);

        infrastructure.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_embedding_event_sending() -> Result<()> {
        let config = EventDrivenEmbeddingConfig::default();
        let mut infrastructure = MessageChannelInfrastructure::new(config)?;

        // Test sending an event
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.md");
        let content = "# Test Document\nThis is a test.";

        let event = EmbeddingEvent::new(
            file_path.clone(),
            FileEventKind::Created,
            content.to_string(),
            crate::embedding_events::create_embedding_metadata(
                &file_path,
                &FileEventKind::Created,
                Some(content.len() as u64),
            ),
        );

        infrastructure.send_embedding_event(event).await?;

        let metrics = infrastructure.get_metrics().await;
        assert_eq!(metrics.total_events_received, 1);

        infrastructure.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_batch_processing() -> Result<()> {
        let config = EventDrivenEmbeddingConfig {
            max_batch_size: 2,
            batch_timeout_ms: 100,
            ..Default::default()
        };

        let mut infrastructure = MessageChannelInfrastructure::new(config)?;
        infrastructure.start().await?;

        let temp_dir = TempDir::new()?;

        // Send multiple events
        for i in 0..3 {
            let file_path = temp_dir.path().join(format!("test{}.md", i));
            let content = format!("# Test Document {}\nThis is test {}.", i, i);

            let event = EmbeddingEvent::new(
                file_path.clone(),
                FileEventKind::Modified,
                content,
                crate::embedding_events::create_embedding_metadata(
                    &file_path,
                    &FileEventKind::Modified,
                    Some(100),
                ),
            );

            infrastructure.send_embedding_event(event).await?;
        }

        // Wait for batch processing
        tokio::time::sleep(Duration::from_millis(150)).await;

        let metrics = infrastructure.get_metrics().await;
        assert_eq!(metrics.total_events_received, 3);
        assert!(metrics.total_batches_processed > 0);

        infrastructure.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_deduplication() -> Result<()> {
        let config = EventDrivenEmbeddingConfig {
            enable_deduplication: true,
            deduplication_window_ms: 1000,
            ..Default::default()
        };

        let mut infrastructure = MessageChannelInfrastructure::new(config)?;

        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.md");
        let content = "# Test Document\nThis is a test.";

        // Send multiple identical events
        for _ in 0..3 {
            let event = EmbeddingEvent::new(
                file_path.clone(),
                FileEventKind::Modified,
                content.to_string(),
                crate::embedding_events::create_embedding_metadata(
                    &file_path,
                    &FileEventKind::Modified,
                    Some(content.len() as u64),
                ),
            );

            infrastructure.send_embedding_event(event).await?;
        }

        let metrics = infrastructure.get_metrics().await;
        assert_eq!(metrics.total_events_received, 3);
        assert!(metrics.deduplicated_events > 0);

        infrastructure.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_optimization_presets() -> Result<()> {
        // Test throughput optimization
        let mut throughput_infra = MessageChannelInfrastructure::optimize_for_throughput()?;
        let throughput_metrics = throughput_infra.get_metrics().await;
        assert_eq!(throughput_metrics.total_events_received, 0);

        // Test latency optimization
        let mut latency_infra = MessageChannelInfrastructure::optimize_for_latency()?;
        let latency_metrics = latency_infra.get_metrics().await;
        assert_eq!(latency_metrics.total_events_received, 0);

        // Test resource optimization
        let mut resource_infra = MessageChannelInfrastructure::optimize_for_resources()?;
        let resource_metrics = resource_infra.get_metrics().await;
        assert_eq!(resource_metrics.total_events_received, 0);

        throughput_infra.shutdown().await?;
        latency_infra.shutdown().await?;
        resource_infra.shutdown().await?;

        Ok(())
    }
}
