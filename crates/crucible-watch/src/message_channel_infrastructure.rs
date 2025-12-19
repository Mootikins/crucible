//! Message Channel Infrastructure for Event-Driven Embedding
//!
//! This module provides the core message channel infrastructure that connects
//! file system events to embedding processing, enabling efficient event-driven
//! processing without polling.
//!
//! # Deprecation Notice
//!
//! This entire module is deprecated in favor of the `SessionEvent` event bus architecture.
//! The `MessageChannelInfrastructure` used tokio broadcast/mpsc channels for embedding events,
//! but this functionality is now handled by the unified `EventBus` from `crucible_rune::event_bus`
//! with `SessionEvent` variants from `crucible_core::events`.
//!
//! ## Migration
//!
//! Instead of using `MessageChannelInfrastructure` to manage embedding event channels:
//!
//! 1. Use `EventBus::emit_session()` to emit `SessionEvent::EmbeddingRequested`
//! 2. Register handlers with `EventBus::register()` to process embedding events
//! 3. Emit `SessionEvent::EmbeddingGenerated` or `SessionEvent::EmbeddingBatchComplete` on completion
//!
//! ```ignore
//! // Old approach:
//! let mut infra = MessageChannelInfrastructure::new(config)?;
//! infra.start().await?;
//! infra.send_embedding_event(event).await?;
//!
//! // New approach:
//! use crucible_core::events::{SessionEvent, Priority, EventEmitter};
//! use crucible_rune::event_bus::EventBus;
//!
//! let bus = EventBus::new();
//! bus.emit_session(SessionEvent::EmbeddingRequested {
//!     entity_id: "note:path/to/note.md".into(),
//!     block_ids: vec!["block_abc123".into()],
//!     priority: Priority::Normal,
//! });
//! ```
//!
//! The event bus provides:
//! - Unified event handling across all Crucible components
//! - Handler priority and fail-open semantics
//! - Rune scripting integration for custom handlers
//! - Better integration with storage and watch systems

#[allow(deprecated)]
use crate::{
    embedding_events::{EmbeddingEvent, EmbeddingEventResult, EventDrivenEmbeddingConfig},
    error::Result,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Message channel infrastructure for event-driven embedding.
///
/// # Deprecation
///
/// This struct is deprecated. Use the `EventBus` from `crucible_rune::event_bus` with
/// `SessionEvent` variants from `crucible_core::events` instead. The event bus provides
/// unified event handling across all Crucible components.
///
/// See the module-level documentation for migration guidance.
#[deprecated(
    since = "0.1.0",
    note = "Use EventBus with SessionEvent from crucible_core::events instead"
)]
#[allow(deprecated)]
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
#[allow(deprecated)]
#[derive(Debug, Default)]
struct ChannelState {
    /// Current batch being accumulated
    current_batch: Vec<EmbeddingEvent>,

    /// Batch identifier for the current batch
    current_batch_id: Option<uuid::Uuid>,

    /// When the current batch will timeout
    batch_deadline: Option<Instant>,

    /// Events being processed
    #[allow(dead_code)] // Reserved for tracking in-flight events
    processing_events: HashMap<uuid::Uuid, Instant>,

    /// Recently processed events for deduplication
    recent_events: HashMap<String, Instant>,

    /// Channel metrics
    metrics: ChannelMetrics,
}

/// Metrics for the channel infrastructure.
///
/// # Deprecation
///
/// This struct is deprecated along with `MessageChannelInfrastructure`. Event bus
/// handlers should track their own metrics through the tracing/metrics system.
#[deprecated(
    since = "0.1.0",
    note = "Use EventBus with SessionEvent from crucible_core::events instead"
)]
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

#[allow(deprecated)]
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

            if let Some(_batch_id) = batch_id {
                // Send batch completion results
                for event in &batch {
                    let result = EmbeddingEventResult::success(
                        event.id,
                        Duration::from_millis(50), // Simulated processing time
                        256,                       // Mock dimensions
                    );

                    if embedding_result_tx.send(result).is_err() {
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

                if embedding_result_tx.send(result).is_err() {
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

            if self.embedding_result_tx.send(result).is_err() {
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
#[allow(deprecated)]
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
