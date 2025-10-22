//! Event bridge between daemon event system and plugin event subscriptions

use crate::events::{DaemonEvent, EventBus, EventHandler};
use crate::plugin_events::{
    error::{SubscriptionError, SubscriptionResult},
    types::{SubscriptionConfig, SubscriptionId, EventPermission, PermissionScope},
    FilterEngine, SubscriptionRegistry,
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Event bridge that connects daemon events to plugin subscriptions
#[derive(Clone)]
pub struct EventBridge {
    /// Inner bridge state
    inner: Arc<RwLock<EventBridgeInner>>,

    /// Event receiver from daemon event bus
    event_rx: Arc<RwLock<Option<mpsc::UnboundedReceiver<DaemonEvent>>>>,

    /// Bridge task handle
    bridge_handle: Option<Arc<tokio::task::JoinHandle<()>>>,

    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,
}

/// Internal bridge state
struct EventBridgeInner {
    /// Subscription registry
    subscription_registry: Arc<SubscriptionRegistry>,

    /// Filter engine
    filter_engine: Arc<FilterEngine>,

    /// Bridge configuration
    config: BridgeConfig,

    /// Event transformation rules
    transformation_rules: HashMap<String, TransformationRule>,

    /// Security context for event access
    security_context: SecurityContext,

    /// Bridge statistics
    stats: BridgeStats,

    /// Event processing buffer
    event_buffer: VecDeque<BufferedEvent>,

    /// Bridge status
    status: BridgeStatus,

    /// Running state
    running: bool,
}

/// Bridge configuration
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// Enable event transformation
    pub enable_transformation: bool,

    /// Maximum buffer size for events
    pub max_buffer_size: usize,

    /// Event processing timeout in milliseconds
    pub processing_timeout_ms: u64,

    /// Enable event deduplication
    pub enable_deduplication: bool,

    /// Deduplication window in seconds
    pub deduplication_window_seconds: u64,

    /// Enable audit logging
    pub enable_audit_logging: bool,

    /// Audit log retention in hours
    pub audit_retention_hours: u64,

    /// Enable metrics collection
    pub enable_metrics: bool,

    /// Metrics collection interval in seconds
    pub metrics_interval_seconds: u64,

    /// Maximum concurrent event processing
    pub max_concurrent_processing: usize,

    /// Enable security filtering
    pub enable_security_filtering: bool,

    /// Event priority handling
    pub enable_priority_handling: bool,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            enable_transformation: true,
            max_buffer_size: 10000,
            processing_timeout_ms: 5000,
            enable_deduplication: true,
            deduplication_window_seconds: 60,
            enable_audit_logging: true,
            audit_retention_hours: 24,
            enable_metrics: true,
            metrics_interval_seconds: 60,
            max_concurrent_processing: 50,
            enable_security_filtering: true,
            enable_priority_handling: true,
        }
    }
}

/// Event transformation rule
#[derive(Debug, Clone)]
struct TransformationRule {
    /// Rule name
    name: String,

    /// Source event type pattern
    source_pattern: String,

    /// Target event type
    target_type: String,

    /// Transformation function name
    transformation_function: String,

    /// Rule priority
    priority: u8,

    /// Rule enabled flag
    enabled: bool,

    /// Rule metadata
    metadata: HashMap<String, String>,
}

/// Security context for event access control
#[derive(Debug, Clone)]
struct SecurityContext {
    /// Default permissions for new subscriptions
    default_permissions: Vec<EventPermission>,

    /// Security policies
    security_policies: HashMap<String, SecurityPolicy>,

    /// Event access control lists
    access_control_lists: HashMap<String, AccessControlList>,

    /// Security audit log
    audit_log: VecDeque<SecurityAuditEntry>,
}

/// Security policy
#[derive(Debug, Clone)]
struct SecurityPolicy {
    /// Policy name
    name: String,

    /// Policy rules
    rules: Vec<SecurityRule>,

    /// Policy priority
    priority: u8,

    /// Policy enabled flag
    enabled: bool,
}

/// Security rule
#[derive(Debug, Clone)]
struct SecurityRule {
    /// Rule name
    name: String,

    /// Rule condition
    condition: String,

    /// Rule action (allow/deny)
    action: SecurityAction,

    /// Rule parameters
    parameters: HashMap<String, String>,
}

/// Security action
#[derive(Debug, Clone, PartialEq)]
enum SecurityAction {
    Allow,
    Deny,
    Log,
    Transform,
}

/// Access control list
#[derive(Debug, Clone)]
struct AccessControlList {
    /// ACL name
    name: String,

    /// ACL entries
    entries: Vec<AclEntry>,

    /// Default action
    default_action: SecurityAction,
}

/// ACL entry
#[derive(Debug, Clone)]
struct AclEntry {
    /// Principal (plugin/user ID)
    principal: String,

    /// Event type pattern
    event_pattern: String,

    /// Permission (allow/deny)
    permission: SecurityAction,

    /// Conditions
    conditions: Vec<String>,
}

/// Security audit entry
#[derive(Debug, Clone)]
struct SecurityAuditEntry {
    /// Timestamp
    timestamp: DateTime<Utc>,

    /// Event ID
    event_id: String,

    /// Subscription ID
    subscription_id: Option<SubscriptionId>,

    /// Plugin ID
    plugin_id: Option<String>,

    /// Action performed
    action: String,

    /// Decision (allowed/denied)
    decision: bool,

    /// Reason for decision
    reason: String,

    /// Additional context
    context: HashMap<String, String>,
}

/// Bridge statistics
#[derive(Debug, Clone, Default)]
struct BridgeStats {
    /// Total events received from daemon
    total_events_received: u64,

    /// Total events processed
    total_events_processed: u64,

    /// Total events delivered to subscriptions
    total_events_delivered: u64,

    /// Total events filtered out
    total_events_filtered: u64,

    /// Total events blocked by security
    total_events_blocked: u64,

    /// Total events transformed
    total_events_transformed: u64,

    /// Average processing time per event in microseconds
    avg_processing_time_us: u64,

    /// Current buffer size
    buffer_size: usize,

    /// Peak buffer size
    peak_buffer_size: usize,

    /// Active subscriptions count
    active_subscriptions: u64,

    /// Security violations count
    security_violations: u64,

    /// Last activity timestamp
    last_activity: Option<DateTime<Utc>>,

    /// Processing rate (events per second)
    processing_rate: f64,
}

/// Buffered event waiting for processing
#[derive(Debug, Clone)]
struct BufferedEvent {
    /// Event data
    event: DaemonEvent,

    /// Received timestamp
    received_at: DateTime<Utc>,

    /// Processing priority
    priority: u8,

    /// Retry count
    retry_count: u32,

    /// Processing state
    state: ProcessingState,
}

/// Event processing state
#[derive(Debug, Clone, PartialEq)]
enum ProcessingState {
    Pending,
    Processing,
    Processed,
    Failed,
    Skipped,
}

/// Bridge status
#[derive(Debug, Clone, PartialEq)]
enum BridgeStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Error(String),
}

impl EventBridge {
    /// Create a new event bridge
    pub fn new(
        subscription_registry: Arc<SubscriptionRegistry>,
        filter_engine: Arc<FilterEngine>,
        config: BridgeConfig,
    ) -> Self {
        let inner = EventBridgeInner {
            subscription_registry,
            filter_engine,
            config,
            transformation_rules: HashMap::new(),
            security_context: SecurityContext {
                default_permissions: vec![
                    EventPermission {
                        scope: PermissionScope::Plugin,
                        event_types: vec![],
                        categories: vec![],
                        sources: vec![],
                        max_priority: None,
                    }
                ],
                security_policies: HashMap::new(),
                access_control_lists: HashMap::new(),
                audit_log: VecDeque::new(),
            },
            stats: BridgeStats::default(),
            event_buffer: VecDeque::new(),
            status: BridgeStatus::Stopped,
            running: false,
        };

        let (shutdown_tx, _) = mpsc::channel(1);

        Self {
            inner: Arc::new(RwLock::new(inner)),
            event_rx: Arc::new(RwLock::new(None)),
            bridge_handle: None,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    /// Start the event bridge
    pub async fn start(&mut self, event_bus: Arc<dyn EventBus + Send + Sync>) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;

        if inner.running {
            return Ok(());
        }

        inner.status = BridgeStatus::Starting;

        // Subscribe to daemon event bus
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let bridge_handler = BridgeEventHandler::new(event_tx);
        event_bus.subscribe(bridge_handler).await
            .map_err(|e| SubscriptionError::EventError(e.into()))?;

        // Store event receiver
        {
            let mut rx_guard = self.event_rx.write().await;
            *rx_guard = Some(event_rx);
        }

        inner.running = true;
        inner.status = BridgeStatus::Running;

        // Start bridge processing task
        let inner_clone = self.inner.clone();
        let event_rx_clone = self.event_rx.clone();
        let bridge_handle = tokio::spawn(async move {
            Self::bridge_processor(inner_clone, event_rx_clone).await;
        });

        self.bridge_handle = Some(Arc::new(bridge_handle));

        info!("Event bridge started successfully");

        Ok(())
    }

    /// Stop the event bridge
    pub async fn stop(&self) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;

        if !inner.running {
            return Ok(());
        }

        inner.status = BridgeStatus::Stopping;
        inner.running = false;

        // Send shutdown signal
        if let Some(shutdown_tx) = &self.shutdown_tx {
            let _ = shutdown_tx.send(()).await;
        }

        // Wait for bridge task to complete
        if let Some(handle) = &self.bridge_handle {
            let _ = handle.await;
        }

        inner.status = BridgeStatus::Stopped;

        info!("Event bridge stopped");

        Ok(())
    }

    /// Add event transformation rule
    pub async fn add_transformation_rule(&self, rule: TransformationRule) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;
        inner.transformation_rules.insert(rule.name.clone(), rule);
        Ok(())
    }

    /// Remove transformation rule
    pub async fn remove_transformation_rule(&self, rule_name: &str) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;
        inner.transformation_rules.remove(rule_name);
        Ok(())
    }

    /// Add security policy
    pub async fn add_security_policy(&self, policy: SecurityPolicy) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;
        inner.security_context.security_policies.insert(policy.name.clone(), policy);
        Ok(())
    }

    /// Add access control list
    pub async fn add_access_control_list(&self, acl: AccessControlList) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;
        inner.security_context.access_control_lists.insert(acl.name.clone(), acl);
        Ok(())
    }

    /// Get bridge statistics
    pub async fn get_stats(&self) -> BridgeStats {
        let inner = self.inner.read().await;
        inner.stats.clone()
    }

    /// Get bridge status
    pub async fn get_status(&self) -> BridgeStatus {
        let inner = self.inner.read().await;
        inner.status.clone()
    }

    /// Get security audit log
    pub async fn get_audit_log(&self, limit: Option<usize>) -> Vec<SecurityAuditEntry> {
        let inner = self.inner.read().await;
        let audit_log = &inner.security_context.audit_log;

        if let Some(limit) = limit {
            audit_log.iter().rev().take(limit).cloned().collect()
        } else {
            audit_log.iter().rev().cloned().collect()
        }
    }

    /// Bridge processor task
    async fn bridge_processor(
        inner: Arc<RwLock<EventBridgeInner>>,
        event_rx: Arc<RwLock<Option<mpsc::UnboundedReceiver<DaemonEvent>>>>,
    ) {
        info!("Event bridge processor started");

        loop {
            // Check if still running
            let running = {
                let inner_guard = inner.read().await;
                inner_guard.running
            };

            if !running {
                break;
            }

            // Receive next event
            let event = {
                let mut rx_guard = event_rx.write().await;
                if let Some(rx) = &mut *rx_guard {
                    match rx.try_recv() {
                        Ok(event) => Some(event),
                        Err(mpsc::error::TryRecvError::Empty) => None,
                        Err(mpsc::error::TryRecvError::Disconnected) => {
                            warn!("Event channel disconnected, stopping bridge processor");
                            break;
                        }
                    }
                } else {
                    None
                }
            };

            if let Some(event) = event {
                // Process event
                Self::process_event(inner.clone(), event).await;
            } else {
                // No events, wait a bit
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        }

        info!("Event bridge processor stopped");
    }

    /// Process a single event
    async fn process_event(inner: Arc<RwLock<EventBridgeInner>>, event: DaemonEvent) {
        let start_time = std::time::Instant::now();

        // Update stats
        {
            let mut inner_guard = inner.write().await;
            inner_guard.stats.total_events_received += 1;
            inner_guard.stats.last_activity = Some(Utc::now());
        }

        // Check event deduplication
        if inner.read().await.config.enable_deduplication {
            if Self::is_duplicate_event(&inner, &event).await {
                debug!("Skipping duplicate event: {}", event.id);
                return;
            }
        }

        // Apply event transformations
        let transformed_event = match Self::apply_transformations(&inner, &event).await {
            Ok(event) => event,
            Err(e) => {
                error!("Event transformation failed: {}", e);
                return;
            }
        };

        // Get matching subscriptions
        let matching_subscriptions = {
            let inner_guard = inner.read().await;
            inner_guard.subscription_registry.get_matching_subscriptions(&transformed_event).await
        };

        // Process each subscription
        for subscription in matching_subscriptions {
            // Security check
            if inner.read().await.config.enable_security_filtering {
                if !Self::check_event_security(&inner, &transformed_event, &subscription).await {
                    continue;
                }
            }

            // Filter check
            let filter_match = Self::check_subscription_filters(&inner, &transformed_event, &subscription).await;

            if filter_match {
                // Queue event for delivery
                if let Err(e) = Self::queue_event_for_delivery(&inner, &subscription, &transformed_event).await {
                    error!("Failed to queue event for delivery: {}", e);
                }
            }
        }

        // Update processing stats
        let processing_time = start_time.elapsed();
        {
            let mut inner_guard = inner.write().await;
            inner_guard.stats.total_events_processed += 1;

            // Update average processing time
            let total_processed = inner_guard.stats.total_events_processed;
            inner_guard.stats.avg_processing_time_us =
                (inner_guard.stats.avg_processing_time_us * (total_processed - 1) +
                 processing_time.as_micros() as u64) / total_processed;

            // Update processing rate
            inner_guard.stats.processing_rate = total_processed as f64 /
                (Utc::now() - inner_guard.stats.last_activity.unwrap_or_else(Utc::now))
                    .num_seconds().max(1) as f64;
        }

        debug!("Processed event {} in {}μs", event.id, processing_time.as_micros());
    }

    /// Check if event is a duplicate
    async fn is_duplicate_event(inner: &Arc<RwLock<EventBridgeInner>>, event: &DaemonEvent) -> bool {
        // This would implement event deduplication logic
        // For now, return false (no deduplication)
        false
    }

    /// Apply event transformations
    async fn apply_transformations(
        inner: &Arc<RwLock<EventBridgeInner>>,
        event: &DaemonEvent,
    ) -> SubscriptionResult<DaemonEvent> {
        let inner_guard = inner.read().await;

        if !inner_guard.config.enable_transformation {
            return Ok(event.clone());
        }

        let mut transformed_event = event.clone();

        // Apply matching transformation rules
        for rule in inner_guard.transformation_rules.values() {
            if rule.enabled && Self::event_matches_pattern(event, &rule.source_pattern) {
                transformed_event = Self::apply_transformation_rule(&transformed_event, rule)?;
                inner_guard.stats.total_events_transformed += 1;
            }
        }

        Ok(transformed_event)
    }

    /// Check if event matches pattern
    fn event_matches_pattern(event: &DaemonEvent, pattern: &str) -> bool {
        // Simple pattern matching - in production, use proper pattern matching
        let event_type_str = match &event.event_type {
            crate::events::EventType::Filesystem(_) => "filesystem",
            crate::events::EventType::Database(_) => "database",
            crate::events::EventType::External(_) => "external",
            crate::events::EventType::Mcp(_) => "mcp",
            crate::events::EventType::Service(_) => "service",
            crate::events::EventType::System(_) => "system",
            crate::events::EventType::Custom(name) => name,
        };

        pattern == "*" || pattern == event_type_str
    }

    /// Apply transformation rule to event
    fn apply_transformation_rule(
        event: &DaemonEvent,
        rule: &TransformationRule,
    ) -> SubscriptionResult<DaemonEvent> {
        // This would implement actual transformation logic
        // For now, just return the event unchanged
        Ok(event.clone())
    }

    /// Check event security
    async fn check_event_security(
        inner: &Arc<RwLock<EventBridgeInner>>,
        event: &DaemonEvent,
        subscription: &SubscriptionConfig,
    ) -> bool {
        let inner_guard = inner.read().await;

        // Check subscription authorization
        if !subscription.auth_context.can_access_event(event) {
            // Log security violation
            Self::log_security_violation(
                &inner_guard.security_context,
                event,
                Some(&subscription.id),
                &subscription.plugin_id,
                "Unauthorized access attempt",
            ).await;

            inner_guard.stats.security_violations += 1;
            return false;
        }

        // Check security policies
        for policy in inner_guard.security_context.security_policies.values() {
            if policy.enabled && Self::evaluate_security_policy(event, subscription, policy) {
                // Policy evaluation determines if access is allowed
                return true;
            }
        }

        // Check access control lists
        for acl in inner_guard.security_context.access_control_lists.values() {
            if Self::check_acl_access(event, subscription, acl) {
                return true;
            }
        }

        true
    }

    /// Evaluate security policy
    fn evaluate_security_policy(
        event: &DaemonEvent,
        subscription: &SubscriptionConfig,
        policy: &SecurityPolicy,
    ) -> bool {
        // This would implement policy evaluation logic
        // For now, allow all access
        true
    }

    /// Check ACL access
    fn check_acl_access(
        event: &DaemonEvent,
        subscription: &SubscriptionConfig,
        acl: &AccessControlList,
    ) -> bool {
        // Check ACL entries
        for entry in &acl.entries {
            if entry.principal == subscription.plugin_id || entry.principal == "*" {
                if Self::event_matches_pattern(event, &entry.event_pattern) {
                    return matches!(entry.permission, SecurityAction::Allow);
                }
            }
        }

        // Use default action
        matches!(acl.default_action, SecurityAction::Allow)
    }

    /// Log security violation
    async fn log_security_violation(
        security_context: &SecurityContext,
        event: &DaemonEvent,
        subscription_id: Option<&SubscriptionId>,
        plugin_id: &str,
        reason: &str,
    ) {
        let audit_entry = SecurityAuditEntry {
            timestamp: Utc::now(),
            event_id: event.id.to_string(),
            subscription_id: subscription_id.cloned(),
            plugin_id: Some(plugin_id.to_string()),
            action: "event_access_denied".to_string(),
            decision: false,
            reason: reason.to_string(),
            context: HashMap::new(),
        };

        // In a real implementation, this would be stored in a persistent audit log
        warn!("Security violation: {} - {}", reason, event.id);
    }

    /// Check subscription filters
    async fn check_subscription_filters(
        inner: &Arc<RwLock<EventBridgeInner>>,
        event: &DaemonEvent,
        subscription: &SubscriptionConfig,
    ) -> bool {
        let inner_guard = inner.read().await;

        // Use the filter engine to check if event matches subscription filters
        if subscription.filters.is_empty() {
            return true; // No filters means accept all
        }

        for filter in &subscription.filters {
            let filter_key = match inner_guard.filter_engine.compile_filter(filter).await {
                Ok(key) => key,
                Err(e) => {
                    error!("Failed to compile filter: {}", e);
                    continue;
                }
            };

            match inner_guard.filter_engine.matches_filter(event, &filter_key).await {
                Ok(matches) => {
                    if matches {
                        return true;
                    }
                }
                Err(e) => {
                    error!("Filter matching failed: {}", e);
                }
            }
        }

        false
    }

    /// Queue event for delivery to subscription
    async fn queue_event_for_delivery(
        inner: &Arc<RwLock<EventBridgeInner>>,
        subscription: &SubscriptionConfig,
        event: &DaemonEvent,
    ) -> SubscriptionResult<()> {
        let inner_guard = inner.read().await;

        // In a real implementation, this would integrate with the delivery system
        // For now, just update stats
        inner_guard.stats.total_events_delivered += 1;

        debug!("Queued event {} for subscription {}", event.id, subscription.id.as_string());

        Ok(())
    }
}

/// Bridge event handler for receiving daemon events
struct BridgeEventHandler {
    /// Event sender to bridge
    event_tx: mpsc::UnboundedSender<DaemonEvent>,
}

impl BridgeEventHandler {
    /// Create new bridge event handler
    fn new(event_tx: mpsc::UnboundedSender<DaemonEvent>) -> Self {
        Self { event_tx }
    }
}

#[async_trait::async_trait]
impl EventHandler for BridgeEventHandler {
    async fn handle_event(&self, event: DaemonEvent) -> crate::events::EventResult<()> {
        if let Err(e) = self.event_tx.send(event) {
            error!("Failed to send event to bridge: {}", e);
            return Err(crate::events::EventError::DeliveryError(
                "Bridge event channel closed".to_string()
            ));
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "plugin_event_bridge"
    }

    fn priority(&self) -> u8 {
        5 // Medium priority
    }
}

impl Default for EventBridge {
    fn default() -> Self {
        let config = BridgeConfig::default();
        let subscription_registry = Arc::new(SubscriptionRegistry::new());
        let filter_engine = Arc::new(FilterEngine::new());
        Self::new(subscription_registry, filter_engine, config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventBus, MockEventBus, EventPayload, EventSource, SourceType};

    #[tokio::test]
    async fn test_bridge_creation() {
        let subscription_registry = Arc::new(SubscriptionRegistry::new());
        let filter_engine = Arc::new(FilterEngine::new());
        let config = BridgeConfig::default();

        let bridge = EventBridge::new(subscription_registry, filter_engine, config);
        assert_eq!(bridge.get_status().await, BridgeStatus::Stopped);
    }

    #[tokio::test]
    async fn test_transformation_rules() {
        let subscription_registry = Arc::new(SubscriptionRegistry::new());
        let filter_engine = Arc::new(FilterEngine::new());
        let config = BridgeConfig::default();

        let bridge = EventBridge::new(subscription_registry, filter_engine, config);

        let rule = TransformationRule {
            name: "test_rule".to_string(),
            source_pattern: "system".to_string(),
            target_type: "transformed_system".to_string(),
            transformation_function: "identity".to_string(),
            priority: 1,
            enabled: true,
            metadata: HashMap::new(),
        };

        assert!(bridge.add_transformation_rule(rule).await.is_ok());
    }

    #[tokio::test]
    async fn test_security_policies() {
        let subscription_registry = Arc::new(SubscriptionRegistry::new());
        let filter_engine = Arc::new(FilterEngine::new());
        let config = BridgeConfig::default();

        let bridge = EventBridge::new(subscription_registry, filter_engine, config);

        let policy = SecurityPolicy {
            name: "test_policy".to_string(),
            rules: vec![],
            priority: 1,
            enabled: true,
        };

        assert!(bridge.add_security_policy(policy).await.is_ok());
    }
}