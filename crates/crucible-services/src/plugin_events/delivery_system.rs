//! Event delivery system for reliable event delivery to plugins

use crate::events::DaemonEvent;
use crate::plugin_events::{
    error::{SubscriptionError, SubscriptionResult},
    types::{
        DeliveryAttempt, DeliveryOptions, DeliveryResult, DeliveryStatus,
        EventDelivery, SubscriptionConfig, SubscriptionId, SubscriptionType,
        BackpressureHandling, EventOrdering, RetryBackoff,
    },
};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Semaphore};
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, error, info, warn};

/// Event delivery system for reliable plugin event delivery
#[derive(Clone)]
pub struct DeliverySystem {
    /// Inner delivery system state
    inner: Arc<RwLock<DeliverySystemInner>>,

    /// Delivery task handle
    delivery_handle: Option<Arc<tokio::task::JoinHandle<()>>>,

    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,
}

/// Internal delivery system state
struct DeliverySystemInner {
    /// Active delivery queues by subscription ID
    delivery_queues: HashMap<SubscriptionId, DeliveryQueue>,

    /// In-flight deliveries by event ID
    in_flight_deliveries: HashMap<String, InFlightDelivery>,

    /// Delivery statistics
    delivery_stats: HashMap<SubscriptionId, DeliveryStats>,

    /// Delivery configuration
    config: DeliveryConfig,

    /// System metrics
    metrics: DeliveryMetrics,

    /// Event serializer
    serializer: Arc<EventSerializer>,

    /// Compression handler
    compression: Arc<CompressionHandler>,

    /// Encryption handler
    encryption: Arc<EncryptionHandler>,

    /// Plugin connection manager
    connection_manager: Arc<dyn PluginConnectionManager + Send + Sync>,

    /// Running state
    running: bool,
}

/// Delivery queue for a subscription
#[derive(Debug)]
struct DeliveryQueue {
    /// Queue ID (subscription ID)
    subscription_id: SubscriptionId,

    /// Subscription configuration
    subscription_config: SubscriptionConfig,

    /// Event queue
    events: VecDeque<QueuedEvent>,

    /// Queue capacity
    capacity: usize,

    /// Current queue size
    size: usize,

    /// Ordering mode
    ordering: EventOrdering,

    /// Backpressure strategy
    backpressure: BackpressureHandling,

    /// Batch configuration for batched subscriptions
    batch_config: Option<BatchConfig>,

    /// Last batch flush time
    last_batch_flush: Option<DateTime<Utc>>,

    /// Priority queue for priority ordering
    priority_queue: Option<std::collections::BinaryHeap<PriorityQueuedEvent>>,
}

/// Queued event waiting for delivery
#[derive(Debug, Clone)]
struct QueuedEvent {
    /// Event to deliver
    event: DaemonEvent,

    /// Queue timestamp
    queued_at: DateTime<Utc>,

    /// Delivery attempt count
    attempts: u32,

    /// Next retry timestamp
    next_retry_at: Option<DateTime<Utc>>,

    /// Delivery priority
    priority: u8,

    /// Sequence number for ordering
    sequence: u64,
}

/// Priority queued event for priority ordering
#[derive(Debug, Clone)]
struct PriorityQueuedEvent {
    /// Queued event
    event: QueuedEvent,

    /// Priority (lower is higher priority)
    priority: u8,

    /// Sequence for FIFO within same priority
    sequence: u64,
}

impl PartialEq for PriorityQueuedEvent {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.sequence == other.sequence
    }
}

impl Eq for PriorityQueuedEvent {}

impl PartialOrd for PriorityQueuedEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriorityQueuedEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse order for min-heap behavior (lower priority = higher precedence)
        other.priority.cmp(&self.priority)
            .then_with(|| self.sequence.cmp(&other.sequence))
    }
}

/// Batch configuration for batched delivery
#[derive(Debug, Clone)]
struct BatchConfig {
    /// Maximum batch size
    max_size: usize,

    /// Maximum batch wait time in milliseconds
    max_wait_ms: u64,

    /// Current batch
    current_batch: Vec<QueuedEvent>,

    /// Batch creation time
    batch_created_at: DateTime<Utc>,
}

/// In-flight delivery tracking
#[derive(Debug, Clone)]
struct InFlightDelivery {
    /// Delivery information
    delivery: EventDelivery,

    /// Delivery timeout
    timeout: Instant,

    /// Delivery channel for response
    response_tx: mpsc::Sender<DeliveryResponse>,
}

/// Delivery response
#[derive(Debug, Clone)]
struct DeliveryResponse {
    /// Event ID
    event_id: String,

    /// Subscription ID
    subscription_id: SubscriptionId,

    /// Delivery result
    result: DeliveryResult,

    /// Delivery duration
    duration_ms: u64,

    /// Timestamp
    timestamp: DateTime<Utc>,
}

/// Delivery statistics
#[derive(Debug, Clone, Default)]
struct DeliveryStats {
    /// Total events queued
    total_queued: u64,

    /// Total events delivered
    total_delivered: u64,

    /// Total events failed
    total_failed: u64,

    /// Total events retried
    total_retries: u64,

    /// Average delivery time in milliseconds
    avg_delivery_time_ms: f64,

    /// Current queue size
    queue_size: usize,

    /// Peak queue size
    peak_queue_size: usize,

    /// Last activity timestamp
    last_activity: Option<DateTime<Utc>>,

    /// Success rate (0.0 to 1.0)
    success_rate: f64,
}

/// Delivery system configuration
#[derive(Debug, Clone)]
pub struct DeliveryConfig {
    /// Maximum concurrent deliveries
    pub max_concurrent_deliveries: usize,

    /// Default delivery timeout in seconds
    pub default_timeout_seconds: u64,

    /// Maximum queue size per subscription
    pub max_queue_size: usize,

    /// Enable delivery persistence
    pub enable_persistence: bool,

    /// Persistence retention period in hours
    pub persistence_retention_hours: u64,

    /// Enable delivery acknowledgments
    pub enable_acknowledgments: bool,

    /// Acknowledgment timeout in seconds
    pub ack_timeout_seconds: u64,

    /// Delivery worker count
    pub worker_count: usize,

    /// Enable metrics collection
    pub enable_metrics: bool,

    /// Metrics collection interval in seconds
    pub metrics_interval_seconds: u64,

    /// Enable compression
    pub enable_compression: bool,

    /// Compression threshold in bytes
    pub compression_threshold: usize,

    /// Enable encryption
    pub enable_encryption: bool,

    /// Encryption key (simplified - in production use proper key management)
    pub encryption_key: Option<String>,
}

impl Default for DeliveryConfig {
    fn default() -> Self {
        Self {
            max_concurrent_deliveries: 100,
            default_timeout_seconds: 30,
            max_queue_size: 10000,
            enable_persistence: true,
            persistence_retention_hours: 24,
            enable_acknowledgments: true,
            ack_timeout_seconds: 10,
            worker_count: 4,
            enable_metrics: true,
            metrics_interval_seconds: 60,
            enable_compression: false,
            compression_threshold: 1024,
            enable_encryption: false,
            encryption_key: None,
        }
    }
}

/// Delivery system metrics
#[derive(Debug, Clone, Default)]
pub struct DeliveryMetrics {
    /// Total subscriptions managed
    pub total_subscriptions: u64,

    /// Active delivery queues
    pub active_queues: u64,

    /// Total events processed
    pub total_events_processed: u64,

    /// Total deliveries completed
    pub total_deliveries_completed: u64,

    /// Total deliveries failed
    pub total_deliveries_failed: u64,

    /// Average delivery time across all subscriptions
    pub avg_delivery_time_ms: f64,

    /// System throughput (events per second)
    pub throughput_events_per_sec: f64,

    /// Error rate (0.0 to 1.0)
    pub error_rate: f64,

    /// Memory usage in bytes
    pub memory_usage_bytes: u64,

    /// Queue depth across all subscriptions
    pub total_queue_depth: usize,

    /// Last updated timestamp
    pub last_updated: DateTime<Utc>,
}

/// Event serializer for different formats
struct EventSerializer {
    /// Serialization format
    format: SerializationFormat,
}

/// Serialization format
#[derive(Debug, Clone)]
enum SerializationFormat {
    Json,
    MessagePack,
    Protobuf,
    Custom(String),
}

/// Compression handler for event compression
struct CompressionHandler {
    /// Compression algorithm
    algorithm: CompressionAlgorithm,

    /// Compression threshold
    threshold: usize,
}

/// Compression algorithm
#[derive(Debug, Clone)]
enum CompressionAlgorithm {
    Gzip,
    Lz4,
    Zstd,
    None,
}

/// Encryption handler for event encryption
struct EncryptionHandler {
    /// Encryption algorithm
    algorithm: EncryptionAlgorithm,

    /// Encryption key
    key: Option<String>,
}

/// Encryption algorithm
#[derive(Debug, Clone)]
enum EncryptionAlgorithm {
    Aes256Gcm,
    ChaCha20Poly1305,
    None,
}

/// Plugin connection manager trait
#[async_trait::async_trait]
pub trait PluginConnectionManager: Send + Sync {
    /// Check if plugin is connected
    async fn is_plugin_connected(&self, plugin_id: &str) -> bool;

    /// Deliver event to plugin
    async fn deliver_event_to_plugin(
        &self,
        plugin_id: &str,
        event: &SerializedEvent,
    ) -> SubscriptionResult<DeliveryResult>;

    /// Get plugin connection health
    async fn get_plugin_health(&self, plugin_id: &str) -> Option<PluginHealth>;
}

/// Serialized event for delivery
#[derive(Debug, Clone)]
pub struct SerializedEvent {
    /// Serialized data
    pub data: Vec<u8>,

    /// Content type
    pub content_type: String,

    /// Encoding
    pub encoding: String,

    /// Compression algorithm (if any)
    pub compression: Option<String>,

    /// Encryption algorithm (if any)
    pub encryption: Option<String>,

    /// Event metadata
    pub metadata: HashMap<String, String>,
}

/// Plugin health information
#[derive(Debug, Clone)]
pub struct PluginHealth {
    /// Plugin status
    pub status: PluginStatus,

    /// Last activity
    pub last_activity: DateTime<Utc>,

    /// Response time in milliseconds
    pub response_time_ms: u64,

    /// Error count
    pub error_count: u64,

    /// Additional health details
    pub details: HashMap<String, String>,
}

/// Plugin status
#[derive(Debug, Clone, PartialEq)]
pub enum PluginStatus {
    Connected,
    Disconnected,
    Error,
    Busy,
}

impl DeliverySystem {
    /// Create a new delivery system
    pub fn new(
        config: DeliveryConfig,
        connection_manager: Arc<dyn PluginConnectionManager + Send + Sync>,
    ) -> Self {
        let inner = DeliverySystemInner {
            delivery_queues: HashMap::new(),
            in_flight_deliveries: HashMap::new(),
            delivery_stats: HashMap::new(),
            config,
            metrics: DeliveryMetrics::default(),
            serializer: Arc::new(EventSerializer {
                format: SerializationFormat::Json,
            }),
            compression: Arc::new(CompressionHandler {
                algorithm: CompressionAlgorithm::None,
                threshold: 1024,
            }),
            encryption: Arc::new(EncryptionHandler {
                algorithm: EncryptionAlgorithm::None,
                key: None,
            }),
            connection_manager,
            running: false,
        };

        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        let delivery_system = Self {
            inner: Arc::new(RwLock::new(inner)),
            delivery_handle: None,
            shutdown_tx: Some(shutdown_tx),
        };

        delivery_system
    }

    /// Start the delivery system
    pub async fn start(&mut self) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;

        if inner.running {
            return Ok(());
        }

        inner.running = true;

        // Start delivery worker tasks
        let system_inner = self.inner.clone();
        let shutdown_rx = async {
            // This will be replaced with actual shutdown signal handling
            let (_tx, mut rx) = mpsc::channel::<()>(1);
            let _ = rx.recv().await;
        };

        // Start delivery workers
        for worker_id in 0..inner.config.worker_count {
            let system_inner_clone = system_inner.clone();
            let worker_id = worker_id;

            tokio::spawn(async move {
                Self::delivery_worker(system_inner_clone, worker_id).await;
            });
        }

        // Start metrics collection
        if inner.config.enable_metrics {
            let system_inner_clone = system_inner.clone();
            tokio::spawn(async move {
                Self::metrics_collector(system_inner_clone).await;
            });
        }

        info!("Delivery system started with {} workers", inner.config.worker_count);

        Ok(())
    }

    /// Stop the delivery system
    pub async fn stop(&self) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;

        if !inner.running {
            return Ok(());
        }

        inner.running = false;

        // Send shutdown signal
        if let Some(shutdown_tx) = &self.shutdown_tx {
            let _ = shutdown_tx.send(()).await;
        }

        // Wait for delivery tasks to complete
        if let Some(handle) = &self.delivery_handle {
            let _ = handle.await;
        }

        info!("Delivery system stopped");

        Ok(())
    }

    /// Register a subscription for event delivery
    pub async fn register_subscription(&self, subscription: SubscriptionConfig) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;

        // Create delivery queue for subscription
        let queue = DeliveryQueue {
            subscription_id: subscription.id.clone(),
            subscription_config: subscription.clone(),
            events: VecDeque::new(),
            capacity: subscription.delivery_options.max_event_size,
            size: 0,
            ordering: subscription.delivery_options.ordering.clone(),
            backpressure: subscription.delivery_options.backpressure_handling.clone(),
            batch_config: match &subscription.subscription_type {
                SubscriptionType::Batched { interval_seconds, max_batch_size } => {
                    Some(BatchConfig {
                        max_size: *max_batch_size,
                        max_wait_ms: interval_seconds * 1000,
                        current_batch: Vec::new(),
                        batch_created_at: Utc::now(),
                    })
                }
                _ => None,
            },
            last_batch_flush: None,
            priority_queue: if subscription.delivery_options.ordering == EventOrdering::Priority {
                Some(std::collections::BinaryHeap::new())
            } else {
                None
            },
        };

        inner.delivery_queues.insert(subscription.id.clone(), queue);
        inner.delivery_stats.insert(subscription.id.clone(), DeliveryStats::default());
        inner.metrics.total_subscriptions += 1;
        inner.metrics.active_queues += 1;

        info!("Registered delivery queue for subscription {}", subscription.id.as_string());

        Ok(())
    }

    /// Unregister a subscription
    pub async fn unregister_subscription(&self, subscription_id: &SubscriptionId) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;

        inner.delivery_queues.remove(subscription_id);
        inner.delivery_stats.remove(subscription_id);

        if inner.metrics.active_queues > 0 {
            inner.metrics.active_queues -= 1;
        }

        info!("Unregistered delivery queue for subscription {}", subscription_id.as_string());

        Ok(())
    }

    /// Queue an event for delivery to a subscription
    pub async fn queue_event(
        &self,
        subscription_id: &SubscriptionId,
        event: DaemonEvent,
    ) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;

        let queue = inner.delivery_queues
            .get_mut(subscription_id)
            .ok_or_else(|| SubscriptionError::SubscriptionNotFound(
                subscription_id.as_string()
            ))?;

        // Check queue capacity and handle backpressure
        if queue.size >= queue.capacity {
            self.handle_backpressure(&mut inner, queue, &event).await?;
        }

        // Create queued event
        let queued_event = QueuedEvent {
            event,
            queued_at: Utc::now(),
            attempts: 0,
            next_retry_at: None,
            priority: 0, // Will be set based on event priority
            sequence: inner.metrics.total_events_processed,
        };

        // Add to queue based on ordering
        match &queue.ordering {
            EventOrdering::Priority => {
                if let Some(priority_queue) = &mut queue.priority_queue {
                    priority_queue.push(PriorityQueuedEvent {
                        event: queued_event.clone(),
                        priority: queued_event.event.priority.value(),
                        sequence: queued_event.sequence,
                    });
                }
            }
            _ => {
                queue.events.push_back(queued_event.clone());
            }
        }

        queue.size += 1;
        inner.metrics.total_events_processed += 1;

        // Update statistics
        if let Some(stats) = inner.delivery_stats.get_mut(subscription_id) {
            stats.total_queued += 1;
            stats.queue_size = queue.size;
            stats.last_activity = Some(Utc::now());

            if queue.size > stats.peak_queue_size {
                stats.peak_queue_size = queue.size;
            }
        }

        // Handle batched subscriptions
        if let Some(batch_config) = &mut queue.batch_config {
            batch_config.current_batch.push(queued_event);

            // Check if batch should be flushed
            let should_flush = batch_config.current_batch.len() >= batch_config.max_size ||
                (Utc::now() - batch_config.batch_created_at).num_milliseconds() as u64 >= batch_config.max_wait_ms;

            if should_flush {
                self.flush_batch(&mut inner, subscription_id).await?;
            }
        }

        debug!("Queued event for subscription {} (queue size: {})",
               subscription_id.as_string(), queue.size);

        Ok(())
    }

    /// Handle backpressure when queue is full
    async fn handle_backpressure(
        &self,
        inner: &mut DeliverySystemInner,
        queue: &mut DeliveryQueue,
        event: &DaemonEvent,
    ) -> SubscriptionResult<()> {
        match &queue.backpressure {
            BackpressureHandling::Buffer { max_size } => {
                if queue.size >= *max_size {
                    return Err(SubscriptionError::BackpressureError(
                        "Queue buffer overflow".to_string()
                    ));
                }
            }

            BackpressureHandling::DropOldest { max_size } => {
                if queue.size >= *max_size {
                    if let Some(oldest) = queue.events.pop_front() {
                        queue.size -= 1;
                        warn!("Dropped oldest event for subscription {} due to backpressure",
                              queue.subscription_id.as_string());
                    }
                }
            }

            BackpressureHandling::DropNewest => {
                warn!("Dropped newest event for subscription {} due to backpressure",
                      queue.subscription_id.as_string());
                return Err(SubscriptionError::BackpressureError(
                    "Event dropped due to backpressure".to_string()
                ));
            }

            BackpressureHandling::ApplyBackpressure => {
                // In a real implementation, this would signal the event source
                // to slow down production
                warn!("Applying backpressure for subscription {}",
                      queue.subscription_id.as_string());
            }

            BackpressureHandling::Custom { handler_name } => {
                warn!("Custom backpressure handler '{}' not implemented", handler_name);
            }
        }

        Ok(())
    }

    /// Flush batch for batched subscriptions
    async fn flush_batch(
        &self,
        inner: &mut DeliverySystemInner,
        subscription_id: &SubscriptionId,
    ) -> SubscriptionResult<()> {
        let queue = inner.delivery_queues
            .get_mut(subscription_id)
            .ok_or_else(|| SubscriptionError::SubscriptionNotFound(
                subscription_id.as_string()
            ))?;

        if let Some(batch_config) = &mut queue.batch_config {
            if !batch_config.current_batch.is_empty() {
                // Process batch as a single delivery
                let batch_events: Vec<DaemonEvent> = batch_config.current_batch
                    .iter()
                    .map(|qe| qe.event.clone())
                    .collect();

                // Create a batch event for delivery
                let batch_event = self.create_batch_event(&batch_events)?;

                // Queue batch event
                let queued_batch = QueuedEvent {
                    event: batch_event,
                    queued_at: Utc::now(),
                    attempts: 0,
                    next_retry_at: None,
                    priority: 0,
                    sequence: inner.metrics.total_events_processed,
                };

                queue.events.push_back(queued_batch);
                queue.size += 1;

                // Clear batch
                batch_config.current_batch.clear();
                batch_config.batch_created_at = Utc::now();
                queue.last_batch_flush = Some(Utc::now());

                debug!("Flushed batch of {} events for subscription {}",
                       batch_events.len(), subscription_id.as_string());
            }
        }

        Ok(())
    }

    /// Create batch event from multiple events
    fn create_batch_event(&self, events: &[DaemonEvent]) -> SubscriptionResult<DaemonEvent> {
        use crate::events::{EventPayload, EventSource, SourceType};

        let batch_data = serde_json::json!({
            "batch_id": uuid::Uuid::new_v4().to_string(),
            "events": events.iter().map(|e| {
                serde_json::json!({
                    "id": e.id.to_string(),
                    "event_type": serde_json::to_value(&e.event_type).unwrap_or_default(),
                    "priority": e.priority.value(),
                    "source": serde_json::to_value(&e.source).unwrap_or_default(),
                    "created_at": e.created_at.to_rfc3339(),
                    "payload": e.payload.data,
                })
            }).collect::<Vec<_>>(),
            "batch_size": events.len(),
            "batch_created_at": Utc::now().to_rfc3339(),
        });

        let batch_event = DaemonEvent::new(
            crate::events::EventType::Custom("event_batch".to_string()),
            EventSource::new("delivery_system".to_string(), SourceType::System),
            EventPayload::json(batch_data),
        );

        Ok(batch_event)
    }

    /// Delivery worker task
    async fn delivery_worker(inner: Arc<RwLock<DeliverySystemInner>>, worker_id: usize) {
        info!("Delivery worker {} started", worker_id);

        loop {
            // Check if system is still running
            let running = {
                let inner_guard = inner.read().await;
                inner_guard.running
            };

            if !running {
                break;
            }

            // Find next event to deliver
            let (subscription_id, queued_event) = {
                let mut inner_guard = inner.write().await;
                Self::get_next_event(&mut inner_guard)
            };

            if let Some((sub_id, event)) = (subscription_id, queued_event) {
                // Deliver event
                let delivery_result = Self::deliver_event_internal(
                    inner.clone(),
                    &sub_id,
                    &event.event
                ).await;

                // Handle delivery result
                let mut inner_guard = inner.write().await;
                Self::handle_delivery_result(
                    &mut inner_guard,
                    &sub_id,
                    &event,
                    delivery_result
                ).await;
            } else {
                // No events to deliver, wait a bit
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }

        info!("Delivery worker {} stopped", worker_id);
    }

    /// Get next event to deliver from any queue
    fn get_next_event(inner: &mut DeliverySystemInner) -> Option<(SubscriptionId, QueuedEvent)> {
        for (subscription_id, queue) in &mut inner.delivery_queues {
            // Check retry timestamps
            while let Some(event) = queue.events.front() {
                if let Some(retry_at) = event.next_retry_at {
                    if retry_at <= Utc::now() {
                        break; // Ready to retry
                    } else {
                        // Not ready to retry yet, check other queues
                        break;
                    }
                } else {
                    break; // Ready to deliver
                }
            }

            // Get next event based on ordering
            let event = match &queue.ordering {
                EventOrdering::Priority => {
                    queue.priority_queue.as_mut()
                        .and_then(|pq| pq.pop())
                        .map(|pqe| pqe.event)
                }
                _ => queue.events.pop_front(),
            };

            if let Some(event) = event {
                queue.size = queue.size.saturating_sub(1);
                return Some((subscription_id.clone(), event));
            }
        }

        None
    }

    /// Internal event delivery
    async fn deliver_event_internal(
        inner: Arc<RwLock<DeliverySystemInner>>,
        subscription_id: &SubscriptionId,
        event: &DaemonEvent,
    ) -> DeliveryResult {
        let inner_guard = inner.read().await;

        // Get subscription config
        let queue = inner_guard.delivery_queues.get(subscription_id);
        let subscription_config = queue.as_ref()
            .map(|q| &q.subscription_config);

        if let Some(config) = subscription_config {
            // Check if plugin is connected
            if !inner_guard.connection_manager.is_plugin_connected(&config.plugin_id).await {
                return DeliveryResult::Unavailable;
            }

            // Serialize event
            let serialized_event = match inner_guard.serializer.serialize(event) {
                Ok(se) => se,
                Err(e) => {
                    error!("Failed to serialize event: {}", e);
                    return DeliveryResult::Failed { error: format!("Serialization failed: {}", e) };
                }
            };

            // Apply compression if needed
            let serialized_event = match inner_guard.compression.compress(&serialized_event) {
                Ok(se) => se,
                Err(e) => {
                    error!("Failed to compress event: {}", e);
                    return DeliveryResult::Failed { error: format!("Compression failed: {}", e) };
                }
            };

            // Apply encryption if needed
            let serialized_event = match inner_guard.encryption.encrypt(&serialized_event) {
                Ok(se) => se,
                Err(e) => {
                    error!("Failed to encrypt event: {}", e);
                    return DeliveryResult::Failed { error: format!("Encryption failed: {}", e) };
                }
            };

            // Deliver to plugin
            match inner_guard.connection_manager
                .deliver_event_to_plugin(&config.plugin_id, &serialized_event)
                .await {
                Ok(result) => result,
                Err(e) => DeliveryResult::Failed { error: e.to_string() },
            }
        } else {
            DeliveryResult::Failed { error: "Subscription not found".to_string() }
        }
    }

    /// Handle delivery result
    async fn handle_delivery_result(
        inner: &mut DeliverySystemInner,
        subscription_id: &SubscriptionId,
        queued_event: &QueuedEvent,
        result: DeliveryResult,
    ) {
        let now = Utc::now();
        let duration_ms = (now - queued_event.queued_at).num_milliseconds() as u64;

        // Update statistics
        if let Some(stats) = inner.delivery_stats.get_mut(subscription_id) {
            match &result {
                DeliveryResult::Success => {
                    stats.total_delivered += 1;
                    stats.last_activity = Some(now);
                }
                DeliveryResult::Failed { .. } | DeliveryResult::Timeout => {
                    stats.total_failed += 1;
                }
                DeliveryResult::Unavailable | DeliveryResult::Rejected { .. } => {
                    // These might be retried
                }
            }

            // Update average delivery time
            let total_deliveries = stats.total_delivered + stats.total_failed;
            if total_deliveries > 0 {
                stats.avg_delivery_time_ms =
                    (stats.avg_delivery_time_ms * (total_deliveries - 1) as f64 + duration_ms as f64)
                    / total_deliveries as f64;
            }

            // Update success rate
            stats.success_rate = stats.total_delivered as f64 / total_deliveries as f64;
        }

        // Handle retry logic for failed deliveries
        match &result {
            DeliveryResult::Success => {
                inner.metrics.total_deliveries_completed += 1;
                debug!("Event delivered successfully to subscription {}", subscription_id.as_string());
            }

            DeliveryResult::Failed { error } | DeliveryResult::Timeout => {
                inner.metrics.total_deliveries_failed += 1;

                // Check if should retry
                if let Some(queue) = inner.delivery_queues.get_mut(subscription_id) {
                    let max_retries = queue.subscription_config.delivery_options.max_retries;

                    if queued_event.attempts < max_retries {
                        // Calculate retry delay
                        let retry_delay = Self::calculate_retry_delay(
                            &queue.subscription_config.delivery_options.retry_backoff,
                            queued_event.attempts
                        );

                        // Create retry event
                        let mut retry_event = queued_event.clone();
                        retry_event.attempts += 1;
                        retry_event.next_retry_at = Some(now + chrono::Duration::milliseconds(retry_delay as i64));

                        // Re-queue for retry
                        queue.events.push_front(retry_event);
                        queue.size += 1;

                        inner.metrics.total_retries += 1;

                        warn!("Event delivery failed for subscription {}, retry {} in {}ms: {}",
                              subscription_id.as_string(), queued_event.attempts + 1, retry_delay, error);
                    } else {
                        error!("Event delivery failed permanently for subscription {} after {} attempts: {}",
                              subscription_id.as_string(), max_retries, error);
                    }
                }
            }

            DeliveryResult::Unavailable => {
                // Plugin is unavailable, retry later
                if let Some(queue) = inner.delivery_queues.get_mut(subscription_id) {
                    let mut retry_event = queued_event.clone();
                    retry_event.next_retry_at = Some(now + chrono::Duration::seconds(5));
                    queue.events.push_front(retry_event);
                    queue.size += 1;
                }
            }

            DeliveryResult::Rejected { reason } => {
                warn!("Event rejected by subscription {}: {}", subscription_id.as_string(), reason);
            }
        }
    }

    /// Calculate retry delay based on backoff strategy
    fn calculate_retry_delay(backoff: &RetryBackoff, attempt: u32) -> u64 {
        match backoff {
            RetryBackoff::Fixed { delay_ms } => *delay_ms,

            RetryBackoff::Exponential { base_ms, max_ms } => {
                let delay = base_ms * 2_u64.pow(attempt);
                std::cmp::min(delay, *max_ms)
            }

            RetryBackoff::Linear { increment_ms } => {
                increment_ms * (attempt + 1)
            }

            RetryBackoff::Custom { function_name: _ } => {
                // Custom retry functions would be implemented here
                1000 // Default 1 second
            }
        }
    }

    /// Metrics collection task
    async fn metrics_collector(inner: Arc<RwLock<DeliverySystemInner>>) {
        let mut interval = tokio::time::interval(Duration::from_secs(60));

        loop {
            interval.tick().await;

            let running = {
                let inner_guard = inner.read().await;
                inner_guard.running
            };

            if !running {
                break;
            }

            // Collect and update metrics
            let mut inner_guard = inner.write().await;
            Self::update_system_metrics(&mut inner_guard);
        }
    }

    /// Update system metrics
    fn update_system_metrics(inner: &mut DeliverySystemInner) {
        let mut total_queue_depth = 0;
        let mut total_deliveries_completed = 0;
        let mut total_deliveries_failed = 0;
        let mut total_avg_delivery_time = 0.0;
        let mut active_subscriptions = 0;

        for (subscription_id, queue) in &inner.delivery_queues {
            total_queue_depth += queue.size;
            active_subscriptions += 1;

            if let Some(stats) = inner.delivery_stats.get(subscription_id) {
                total_deliveries_completed += stats.total_delivered;
                total_deliveries_failed += stats.total_failed;
                total_avg_delivery_time += stats.avg_delivery_time_ms;
            }
        }

        // Update system metrics
        inner.metrics.total_queue_depth = total_queue_depth;
        inner.metrics.active_queues = active_subscriptions as u64;
        inner.metrics.total_deliveries_completed = total_deliveries_completed;
        inner.metrics.total_deliveries_failed = total_deliveries_failed;

        if active_subscriptions > 0 {
            inner.metrics.avg_delivery_time_ms = total_avg_delivery_time / active_subscriptions as f64;
        }

        let total_deliveries = total_deliveries_completed + total_deliveries_failed;
        if total_deliveries > 0 {
            inner.metrics.error_rate = total_deliveries_failed as f64 / total_deliveries as f64;
        }

        // Calculate throughput (events per second over the last minute)
        inner.metrics.throughput_events_per_sec = inner.metrics.total_events_processed as f64 / 60.0;

        inner.metrics.last_updated = Utc::now();
    }

    /// Get delivery statistics for a subscription
    pub async fn get_subscription_stats(&self, subscription_id: &SubscriptionId) -> Option<DeliveryStats> {
        let inner = self.inner.read().await;
        inner.delivery_stats.get(subscription_id).cloned()
    }

    /// Get system metrics
    pub async fn get_metrics(&self) -> DeliveryMetrics {
        let inner = self.inner.read().await;
        inner.metrics.clone()
    }

    /// Get queue information for all subscriptions
    pub async fn get_queue_info(&self) -> HashMap<SubscriptionId, QueueInfo> {
        let inner = self.inner.read().await;
        inner.delivery_queues
            .iter()
            .map(|(id, queue)| {
                (id.clone(), QueueInfo {
                    subscription_id: id.clone(),
                    queue_size: queue.size,
                    capacity: queue.capacity,
                    ordering: queue.ordering.clone(),
                    backpressure: queue.backpressure.clone(),
                    has_batch: queue.batch_config.is_some(),
                    last_activity: inner.delivery_stats
                        .get(id)
                        .and_then(|s| s.last_activity),
                })
            })
            .collect()
    }
}

/// Queue information for monitoring
#[derive(Debug, Clone)]
pub struct QueueInfo {
    /// Subscription ID
    pub subscription_id: SubscriptionId,

    /// Current queue size
    pub queue_size: usize,

    /// Queue capacity
    pub capacity: usize,

    /// Event ordering
    pub ordering: EventOrdering,

    /// Backpressure handling
    pub backpressure: BackpressureHandling,

    /// Whether queue has batch configuration
    pub has_batch: bool,

    /// Last activity timestamp
    pub last_activity: Option<DateTime<Utc>>,
}

impl EventSerializer {
    /// Serialize an event
    fn serialize(&self, event: &DaemonEvent) -> SubscriptionResult<SerializedEvent> {
        match self.format {
            SerializationFormat::Json => {
                let data = serde_json::to_vec(event)
                    .map_err(|e| SubscriptionError::SerializationError(e))?;

                Ok(SerializedEvent {
                    data,
                    content_type: "application/json".to_string(),
                    encoding: "utf-8".to_string(),
                    compression: None,
                    encryption: None,
                    metadata: HashMap::new(),
                })
            }

            SerializationFormat::MessagePack => {
                // MessagePack serialization would be implemented here
                Err(SubscriptionError::SerializationError(
                    "MessagePack serialization not implemented".to_string()
                ))
            }

            SerializationFormat::Protobuf => {
                // Protobuf serialization would be implemented here
                Err(SubscriptionError::SerializationError(
                    "Protobuf serialization not implemented".to_string()
                ))
            }

            SerializationFormat::Custom(_) => {
                Err(SubscriptionError::SerializationError(
                    "Custom serialization not implemented".to_string()
                ))
            }
        }
    }
}

impl CompressionHandler {
    /// Compress serialized event if needed
    fn compress(&self, event: &SerializedEvent) -> SubscriptionResult<SerializedEvent> {
        if event.data.len() < self.threshold {
            return Ok(event.clone());
        }

        match self.algorithm {
            CompressionAlgorithm::None => Ok(event.clone()),
            CompressionAlgorithm::Gzip => {
                // Gzip compression would be implemented here
                Err(SubscriptionError::SerializationError(
                    "Gzip compression not implemented".to_string()
                ))
            }
            CompressionAlgorithm::Lz4 => {
                // LZ4 compression would be implemented here
                Err(SubscriptionError::SerializationError(
                    "LZ4 compression not implemented".to_string()
                ))
            }
            CompressionAlgorithm::Zstd => {
                // Zstd compression would be implemented here
                Err(SubscriptionError::SerializationError(
                    "Zstd compression not implemented".to_string()
                ))
            }
        }
    }
}

impl EncryptionHandler {
    /// Encrypt serialized event if needed
    fn encrypt(&self, event: &SerializedEvent) -> SubscriptionResult<SerializedEvent> {
        match self.algorithm {
            EncryptionAlgorithm::None => Ok(event.clone()),
            EncryptionAlgorithm::Aes256Gcm => {
                // AES-256-GCM encryption would be implemented here
                Err(SubscriptionError::SecurityError(
                    "AES-256-GCM encryption not implemented".to_string()
                ))
            }
            EncryptionAlgorithm::ChaCha20Poly1305 => {
                // ChaCha20-Poly1305 encryption would be implemented here
                Err(SubscriptionError::SecurityError(
                    "ChaCha20-Poly1305 encryption not implemented".to_string()
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventPayload, EventSource, SourceType};

    // Mock connection manager for testing
    struct MockConnectionManager;

    #[async_trait::async_trait]
    impl PluginConnectionManager for MockConnectionManager {
        async fn is_plugin_connected(&self, _plugin_id: &str) -> bool {
            true
        }

        async fn deliver_event_to_plugin(
            &self,
            _plugin_id: &str,
            _event: &SerializedEvent,
        ) -> SubscriptionResult<DeliveryResult> {
            Ok(DeliveryResult::Success)
        }

        async fn get_plugin_health(&self, _plugin_id: &str) -> Option<PluginHealth> {
            Some(PluginHealth {
                status: PluginStatus::Connected,
                last_activity: Utc::now(),
                response_time_ms: 10,
                error_count: 0,
                details: HashMap::new(),
            })
        }
    }

    #[tokio::test]
    async fn test_subscription_registration() {
        let config = DeliveryConfig::default();
        let connection_manager = Arc::new(MockConnectionManager);
        let delivery_system = DeliverySystem::new(config, connection_manager);

        let subscription = SubscriptionConfig::new(
            "test-plugin".to_string(),
            "test-subscription".to_string(),
            SubscriptionType::Realtime,
            crate::plugin_events::types::AuthContext::new(
                "test-user".to_string(),
                vec![]
            ),
        );

        assert!(delivery_system.register_subscription(subscription).await.is_ok());
    }

    #[tokio::test]
    async fn test_event_queuing() {
        let config = DeliveryConfig::default();
        let connection_manager = Arc::new(MockConnectionManager);
        let delivery_system = DeliverySystem::new(config, connection_manager);

        let subscription = SubscriptionConfig::new(
            "test-plugin".to_string(),
            "test-subscription".to_string(),
            SubscriptionType::Realtime,
            crate::plugin_events::types::AuthContext::new(
                "test-user".to_string(),
                vec![]
            ),
        );

        delivery_system.register_subscription(subscription.clone()).await.unwrap();

        let event = DaemonEvent::new(
            crate::events::EventType::System(
                crate::events::SystemEventType::DaemonStarted { version: "1.0.0".to_string() }
            ),
            EventSource::new("test".to_string(), SourceType::System),
            EventPayload::json(serde_json::json!({})),
        );

        assert!(delivery_system.queue_event(&subscription.id, event).await.is_ok());
    }
}