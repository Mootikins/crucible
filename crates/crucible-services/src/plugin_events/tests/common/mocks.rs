//! Mock implementations for testing plugin event subscription system

use super::*;
use crate::plugin_events::types::*;
use crate::plugin_events::error::*;
use crate::plugin_events::subscription_api::*;
use crate::events::{DaemonEvent, EventBus, EventFilter, EventPriority, EventType};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use uuid::Uuid;

/// Mock event bus for testing
#[derive(Debug)]
pub struct MockEventBus {
    /// Published events
    pub events: Arc<RwLock<Vec<DaemonEvent>>>,

    /// Event listeners
    pub listeners: Arc<RwLock<HashMap<String, EventListener>>>,

    /// Whether to simulate failures
    pub simulate_failures: Arc<RwLock<bool>>,

    /// Events to drop (simulate loss)
    pub drop_events: Arc<RwLock<Vec<Uuid>>>,
}

#[derive(Debug, Clone)]
struct EventListener {
    id: String,
    filter: Option<EventFilter>,
    callback: Arc<dyn Fn(DaemonEvent) + Send + Sync>,
}

impl MockEventBus {
    /// Create new mock event bus
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
            listeners: Arc::new(RwLock::new(HashMap::new())),
            simulate_failures: Arc::new(RwLock::new(false)),
            drop_events: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Create mock event bus with failure simulation
    pub fn with_failures() -> Self {
        let mut bus = Self::new();
        *bus.simulate_failures.write().unwrap() = true;
        bus
    }

    /// Get number of published events
    pub async fn event_count(&self) -> usize {
        self.events.read().await.len()
    }

    /// Clear all events
    pub async fn clear(&self) {
        self.events.write().await.clear();
        self.drop_events.write().await.clear();
    }

    /// Add event to drop list
    pub async fn drop_event(&self, event_id: Uuid) {
        self.drop_events.write().await.push(event_id);
    }

    /// Simulate event delivery to listeners
    async fn notify_listeners(&self, event: &DaemonEvent) {
        let listeners = self.listeners.read().await.clone();
        for listener in listeners.values() {
            // Apply filter if present
            if let Some(filter) = &listener.filter {
                if !filter.matches(event) {
                    continue;
                }
            }

            // Call listener callback
            (listener.callback)(event.clone());
        }
    }
}

#[async_trait::async_trait]
impl EventBus for MockEventBus {
    async fn publish(&self, event: DaemonEvent) -> crate::events::EventResult<()> {
        // Check for simulated failures
        if *self.simulate_failures.read().await {
            // Randomly fail 10% of events
            if rand::random::<f32>() < 0.1 {
                return Err(crate::events::EventError::PublishError(
                    "Simulated publish failure".to_string()
                ));
            }
        }

        // Check if event should be dropped
        if self.drop_events.read().await.contains(&event.id) {
            return Ok(()); // Silently drop
        }

        // Store event
        self.events.write().await.push(event.clone());

        // Notify listeners
        self.notify_listeners(&event).await;

        Ok(())
    }

    async fn subscribe(
        &self,
        filter: Option<EventFilter>,
        callback: Arc<dyn Fn(DaemonEvent) + Send + Sync>,
    ) -> crate::events::EventResult<String> {
        let listener_id = Uuid::new_v4().to_string();
        let listener = EventListener {
            id: listener_id.clone(),
            filter,
            callback,
        };

        self.listeners.write().await.insert(listener_id.clone(), listener);
        Ok(listener_id)
    }

    async fn unsubscribe(&self, subscription_id: &str) -> crate::events::EventResult<()> {
        self.listeners.write().await.remove(subscription_id);
        Ok(())
    }
}

/// Mock plugin connection manager
#[derive(Debug)]
pub struct MockPluginConnectionManager {
    /// Connected plugins
    pub plugins: Arc<RwLock<HashMap<String, PluginInfo>>>,

    /// Event delivery tracking
    pub delivered_events: Arc<RwLock<HashMap<String, Vec<SerializedEvent>>>>,

    /// Simulate connection failures
    pub connection_failures: Arc<RwLock<HashMap<String, bool>>>,

    /// Simulate delivery failures
    pub delivery_failures: Arc<RwLock<HashMap<String, bool>>>,

    /// Event processing delay
    pub processing_delay: Arc<RwLock<std::time::Duration>>,
}

impl MockPluginConnectionManager {
    /// Create new mock plugin connection manager
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            delivered_events: Arc::new(RwLock::new(HashMap::new())),
            connection_failures: Arc::new(RwLock::new(HashMap::new())),
            delivery_failures: Arc::new(RwLock::new(HashMap::new())),
            processing_delay: Arc::new(RwLock::new(std::time::Duration::from_millis(0))),
        }
    }

    /// Register a plugin
    pub async fn register_plugin(&self, plugin_info: PluginInfo) {
        self.plugins.write().await.insert(plugin_info.plugin_id.clone(), plugin_info);
    }

    /// Unregister a plugin
    pub async fn unregister_plugin(&self, plugin_id: &str) {
        self.plugins.write().await.remove(plugin_id);
        self.delivered_events.write().await.remove(plugin_id);
    }

    /// Simulate connection failure for plugin
    pub async fn set_connection_failure(&self, plugin_id: &str, should_fail: bool) {
        self.connection_failures.write().await.insert(plugin_id.to_string(), should_fail);
    }

    /// Simulate delivery failure for plugin
    pub async fn set_delivery_failure(&self, plugin_id: &str, should_fail: bool) {
        self.delivery_failures.write().await.insert(plugin_id.to_string(), should_fail);
    }

    /// Set processing delay
    pub async fn set_processing_delay(&self, delay: std::time::Duration) {
        *self.processing_delay.write().await = delay;
    }

    /// Get number of events delivered to plugin
    pub async fn delivered_count(&self, plugin_id: &str) -> usize {
        self.delivered_events
            .read()
            .await
            .get(plugin_id)
            .map(|events| events.len())
            .unwrap_or(0)
    }

    /// Clear all delivered events
    pub async fn clear_delivered_events(&self) {
        self.delivered_events.write().await.clear();
    }
}

#[async_trait::async_trait]
impl PluginConnectionManager for MockPluginConnectionManager {
    async fn is_plugin_connected(&self, plugin_id: &str) -> bool {
        // Check for simulated connection failure
        if let Some(should_fail) = self.connection_failures.read().await.get(plugin_id) {
            if *should_fail {
                return false;
            }
        }

        self.plugins.read().await.contains_key(plugin_id)
    }

    async fn get_plugin_info(&self, plugin_id: &str) -> Option<PluginInfo> {
        self.plugins.read().await.get(plugin_id).cloned()
    }

    async fn send_event_to_plugin(
        &self,
        plugin_id: &str,
        event: &SerializedEvent,
    ) -> SubscriptionResult<()> {
        // Check if plugin is connected
        if !self.is_plugin_connected(plugin_id).await {
            return Err(SubscriptionError::PluginNotFound(plugin_id.to_string()));
        }

        // Check for simulated delivery failure
        if let Some(should_fail) = self.delivery_failures.read().await.get(plugin_id) {
            if *should_fail {
                return Err(SubscriptionError::DeliveryError(
                    "Simulated delivery failure".to_string()
                ));
            }
        }

        // Add processing delay
        let delay = *self.processing_delay.read().await;
        if delay > std::time::Duration::from_millis(0) {
            tokio::time::sleep(delay).await;
        }

        // Store event as delivered
        let mut delivered = self.delivered_events.write().await;
        let events = delivered.entry(plugin_id.to_string()).or_insert_with(Vec::new);
        events.push(event.clone());

        Ok(())
    }

    async fn get_connected_plugins(&self) -> Vec<PluginInfo> {
        self.plugins
            .read()
            .await
            .values()
            .filter(|p| self.is_plugin_connected(&p.plugin_id).await)
            .cloned()
            .collect()
    }
}

/// Mock subscription registry for testing
#[derive(Debug)]
pub struct MockSubscriptionRegistry {
    /// Subscriptions by ID
    pub subscriptions: Arc<RwLock<HashMap<SubscriptionId, SubscriptionConfig>>>,

    /// Subscriptions by plugin ID
    pub plugin_subscriptions: Arc<RwLock<HashMap<String, Vec<SubscriptionId>>>>,

    /// Subscription events tracking
    pub subscription_events: Arc<RwLock<HashMap<SubscriptionId, Vec<DaemonEvent>>>>,

    /// Simulate registry failures
    pub simulate_failures: Arc<RwLock<bool>>,
}

impl MockSubscriptionRegistry {
    /// Create new mock registry
    pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            plugin_subscriptions: Arc::new(RwLock::new(HashMap::new())),
            subscription_events: Arc::new(RwLock::new(HashMap::new())),
            simulate_failures: Arc::new(RwLock::new(false)),
        }
    }

    /// Create mock registry with failures
    pub fn with_failures() -> Self {
        let mut registry = Self::new();
        *registry.simulate_failures.write().unwrap() = true;
        registry
    }

    /// Add subscription directly (for testing)
    pub async fn add_subscription(&self, subscription: SubscriptionConfig) {
        let subscription_id = subscription.id.clone();
        let plugin_id = subscription.plugin_id.clone();

        // Add to main storage
        self.subscriptions.write().await.insert(subscription_id.clone(), subscription);

        // Add to plugin index
        let mut plugin_subs = self.plugin_subscriptions.write().await;
        plugin_subs.entry(plugin_id).or_insert_with(Vec::new).push(subscription_id);
    }

    /// Get subscription events
    pub async fn get_subscription_events(&self, subscription_id: &SubscriptionId) -> Vec<DaemonEvent> {
        self.subscription_events
            .read()
            .await
            .get(subscription_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Clear all subscriptions
    pub async fn clear(&self) {
        self.subscriptions.write().await.clear();
        self.plugin_subscriptions.write().await.clear();
        self.subscription_events.write().await.clear();
    }
}

/// Mock filter engine for testing
#[derive(Debug)]
pub struct MockFilterEngine {
    /// Compiled filters
    pub filters: Arc<RwLock<HashMap<String, CompiledFilter>>>,

    /// Filter evaluation results (for testing)
    pub evaluation_results: Arc<RwLock<HashMap<String, bool>>>,

    /// Simulate filter failures
    pub simulate_failures: Arc<RwLock<bool>>,
}

#[derive(Debug, Clone)]
struct CompiledFilter {
    expression: String,
    compilation_time: std::time::Duration,
}

impl MockFilterEngine {
    /// Create new mock filter engine
    pub fn new() -> Self {
        Self {
            filters: Arc::new(RwLock::new(HashMap::new())),
            evaluation_results: Arc::new(RwLock::new(HashMap::new())),
            simulate_failures: Arc::new(RwLock::new(false)),
        }
    }

    /// Set filter evaluation result
    pub async fn set_evaluation_result(&self, filter_id: &str, result: bool) {
        self.evaluation_results.write().await.insert(filter_id.to_string(), result);
    }

    /// Get compiled filter
    pub async fn get_compiled_filter(&self, filter_id: &str) -> Option<CompiledFilter> {
        self.filters.read().await.get(filter_id).cloned()
    }
}

impl MockFilterEngine {
    /// Mock filter compilation
    pub async fn compile_filter(&self, expression: &str) -> SubscriptionResult<String> {
        // Check for simulated failures
        if *self.simulate_failures.read().await {
            return Err(SubscriptionError::FilteringError(
                "Simulated filter compilation failure".to_string()
            ));
        }

        let filter_id = format!("filter-{}", Uuid::new_v4());
        let compiled = CompiledFilter {
            expression: expression.to_string(),
            compilation_time: std::time::Duration::from_millis(1),
        };

        self.filters.write().await.insert(filter_id.clone(), compiled);
        Ok(filter_id)
    }

    /// Mock filter evaluation
    pub async fn evaluate_filter(&self, filter_id: &str, event: &DaemonEvent) -> SubscriptionResult<bool> {
        // Check for simulated failures
        if *self.simulate_failures.read().await {
            return Err(SubscriptionError::FilteringError(
                "Simulated filter evaluation failure".to_string()
            ));
        }

        // Return predetermined result if set
        if let Some(result) = self.evaluation_results.read().await.get(filter_id) {
            return Ok(*result);
        }

        // Default behavior: accept all events
        Ok(true)
    }
}

/// Mock delivery system for testing
#[derive(Debug)]
pub struct MockDeliverySystem {
    /// Delivery tracking
    pub deliveries: Arc<RwLock<Vec<EventDelivery>>>,

    /// Delivery queue simulation
    pub delivery_queue: Arc<RwLock<Vec<QueuedEvent>>>,

    /// Simulate delivery failures
    pub delivery_failures: Arc<RwLock<HashMap<SubscriptionId, bool>>>,

    /// Simulate delivery delays
    pub delivery_delays: Arc<RwLock<HashMap<SubscriptionId, std::time::Duration>>>,

    /// Delivery statistics
    pub stats: Arc<RwLock<DeliveryStats>>,
}

#[derive(Debug, Clone)]
struct QueuedEvent {
    event: DaemonEvent,
    subscription_id: SubscriptionId,
    queued_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Default)]
struct DeliveryStats {
    total_delivered: u64,
    total_failed: u64,
    total_queued: u64,
}

impl MockDeliverySystem {
    /// Create new mock delivery system
    pub fn new() -> Self {
        Self {
            deliveries: Arc::new(RwLock::new(Vec::new())),
            delivery_queue: Arc::new(RwLock::new(Vec::new())),
            delivery_failures: Arc::new(RwLock::new(HashMap::new())),
            delivery_delays: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(DeliveryStats::default())),
        }
    }

    /// Set delivery failure for subscription
    pub async fn set_delivery_failure(&self, subscription_id: &SubscriptionId, should_fail: bool) {
        self.delivery_failures.write().await.insert(subscription_id.clone(), should_fail);
    }

    /// Set delivery delay for subscription
    pub async fn set_delivery_delay(&self, subscription_id: &SubscriptionId, delay: std::time::Duration) {
        self.delivery_delays.write().await.insert(subscription_id.clone(), delay);
    }

    /// Get delivery count for subscription
    pub async fn get_delivery_count(&self, subscription_id: &SubscriptionId) -> usize {
        self.deliveries
            .read()
            .await
            .iter()
            .filter(|d| d.subscription_id == *subscription_id)
            .count()
    }

    /// Get delivery statistics
    pub async fn get_stats(&self) -> DeliveryStats {
        self.stats.read().await.clone()
    }

    /// Clear all deliveries
    pub async fn clear(&self) {
        self.deliveries.write().await.clear();
        self.delivery_queue.write().await.clear();
        *self.stats.write().await = DeliveryStats::default();
    }
}

/// Test utilities for working with mocks
pub mod mock_utils {
    use super::*;

    /// Create test plugin info
    pub fn create_test_plugin_info(plugin_id: &str, status: PluginStatus) -> PluginInfo {
        PluginInfo {
            plugin_id: plugin_id.to_string(),
            plugin_name: format!("Test Plugin {}", plugin_id),
            plugin_version: "1.0.0".to_string(),
            status,
            connected_at: Utc::now(),
            last_activity: Utc::now(),
            capabilities: vec!["events".to_string(), "subscriptions".to_string()],
            metadata: HashMap::new(),
        }
    }

    /// Create serialized event from daemon event
    pub fn create_serialized_event(event: &DaemonEvent) -> SerializedEvent {
        SerializedEvent {
            id: event.id,
            timestamp: event.timestamp,
            event_type: format!("{:?}", event.event_type),
            source_id: event.source.id.clone(),
            priority: format!("{:?}", event.priority),
            data: serde_json::to_value(event).unwrap_or_default(),
            metadata: event.metadata.clone(),
        }
    }

    /// Create test event delivery
    pub fn create_test_delivery(
        subscription_id: SubscriptionId,
        event: &DaemonEvent,
        status: DeliveryStatus,
    ) -> EventDelivery {
        EventDelivery {
            event_id: event.id,
            subscription_id,
            status,
            attempts: vec![],
            metadata: HashMap::new(),
        }
    }
}