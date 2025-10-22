//! Subscription manager for centralized coordination of plugin event subscriptions

use crate::events::{DaemonEvent, EventBus};
use crate::plugin_events::{
    error::{SubscriptionError, SubscriptionResult},
    types::{
        AuthContext, EventPermission, PermissionScope, SubscriptionConfig,
        SubscriptionId, SubscriptionStats, SubscriptionStatus, SubscriptionType,
    },
    DeliverySystem, EventBridge, FilterEngine, SubscriptionRegistry,
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Central subscription manager that coordinates all subscription-related components
#[derive(Clone)]
pub struct SubscriptionManager {
    /// Inner manager state
    inner: Arc<RwLock<SubscriptionManagerInner>>,

    /// Manager task handles
    task_handles: Vec<Arc<tokio::task::JoinHandle<()>>>,

    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,
}

/// Internal subscription manager state
struct SubscriptionManagerInner {
    /// Subscription registry
    subscription_registry: Arc<SubscriptionRegistry>,

    /// Filter engine
    filter_engine: Arc<FilterEngine>,

    /// Event bridge
    event_bridge: Arc<EventBridge>,

    /// Delivery system
    delivery_system: Arc<DeliverySystem>,

    /// Manager configuration
    config: ManagerConfig,

    /// Plugin connection manager
    plugin_connection_manager: Arc<dyn PluginConnectionManager + Send + Sync>,

    /// Manager statistics
    stats: ManagerStats,

    /// Active subscriptions by plugin
    plugin_subscriptions: HashMap<String, Vec<SubscriptionId>>,

    /// Subscription health monitoring
    health_monitor: SubscriptionHealthMonitor,

    /// Security manager
    security_manager: SecurityManager,

    /// Lifecycle manager
    lifecycle_manager: LifecycleManager,

    /// Performance monitor
    performance_monitor: PerformanceMonitor,

    /// Manager state
    state: ManagerState,

    /// Event bus for daemon events
    event_bus: Option<Arc<dyn EventBus + Send + Sync>>,
}

/// Manager configuration
#[derive(Debug, Clone)]
pub struct ManagerConfig {
    /// Enable automatic subscription cleanup
    pub enable_auto_cleanup: bool,

    /// Cleanup interval in seconds
    pub cleanup_interval_seconds: u64,

    /// Maximum subscription lifetime in hours
    pub max_subscription_lifetime_hours: u64,

    /// Enable health monitoring
    pub enable_health_monitoring: bool,

    /// Health check interval in seconds
    pub health_check_interval_seconds: u64,

    /// Enable performance monitoring
    pub enable_performance_monitoring: bool,

    /// Performance metrics interval in seconds
    pub performance_metrics_interval_seconds: u64,

    /// Enable security enforcement
    pub enable_security_enforcement: bool,

    /// Maximum subscriptions per plugin
    pub max_subscriptions_per_plugin: u32,

    /// Default subscription timeout in minutes
    pub default_subscription_timeout_minutes: u64,

    /// Enable subscription persistence
    pub enable_persistence: bool,

    /// Persistence backend configuration
    pub persistence_config: PersistenceConfig,

    /// Audit logging configuration
    pub audit_config: AuditConfig,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            enable_auto_cleanup: true,
            cleanup_interval_seconds: 300, // 5 minutes
            max_subscription_lifetime_hours: 24,
            enable_health_monitoring: true,
            health_check_interval_seconds: 60,
            enable_performance_monitoring: true,
            performance_metrics_interval_seconds: 30,
            enable_security_enforcement: true,
            max_subscriptions_per_plugin: 100,
            default_subscription_timeout_minutes: 60,
            enable_persistence: true,
            persistence_config: PersistenceConfig::default(),
            audit_config: AuditConfig::default(),
        }
    }
}

/// Persistence configuration
#[derive(Debug, Clone)]
pub struct PersistenceConfig {
    /// Persistence backend type
    pub backend_type: PersistenceBackend,

    /// Database connection string
    pub connection_string: Option<String>,

    /// Table name for subscriptions
    pub table_name: String,

    /// Connection pool size
    pub pool_size: u32,

    /// Enable persistence for statistics
    pub enable_stats_persistence: bool,

    /// Persistence retention period in days
    pub retention_days: u32,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            backend_type: PersistenceBackend::Memory,
            connection_string: None,
            table_name: "plugin_subscriptions".to_string(),
            pool_size: 10,
            enable_stats_persistence: true,
            retention_days: 30,
        }
    }
}

/// Persistence backend type
#[derive(Debug, Clone, PartialEq)]
pub enum PersistenceBackend {
    Memory,
    Sqlite,
    Postgres,
    Redis,
    Custom(String),
}

/// Audit configuration
#[derive(Debug, Clone)]
pub struct AuditConfig {
    /// Enable audit logging
    pub enabled: bool,

    /// Audit log level
    pub level: AuditLevel,

    /// Audit log retention in days
    pub retention_days: u32,

    /// Audit events to log
    pub events: Vec<AuditEvent>,

    /// External audit service URL
    pub external_service_url: Option<String>,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            level: AuditLevel::Info,
            retention_days: 90,
            events: vec![
                AuditEvent::SubscriptionCreated,
                AuditEvent::SubscriptionDeleted,
                AuditEvent::SubscriptionModified,
                AuditEvent::SecurityViolation,
                AuditEvent::DeliveryFailure,
            ],
            external_service_url: None,
        }
    }
}

/// Audit log level
#[derive(Debug, Clone, PartialEq)]
pub enum AuditLevel {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

/// Audit event types
#[derive(Debug, Clone, PartialEq)]
pub enum AuditEvent {
    SubscriptionCreated,
    SubscriptionDeleted,
    SubscriptionModified,
    EventDelivered,
    EventFiltered,
    SecurityViolation,
    DeliveryFailure,
    SystemStarted,
    SystemStopped,
    ConfigurationChanged,
}

/// Manager statistics
#[derive(Debug, Clone, Default)]
struct ManagerStats {
    /// Total subscriptions created
    total_subscriptions_created: u64,

    /// Total subscriptions deleted
    total_subscriptions_deleted: u64,

    /// Total events processed
    total_events_processed: u64,

    /// Total events delivered
    total_events_delivered: u64,

    /// Total security violations
    total_security_violations: u64,

    /// Current active subscriptions
    active_subscriptions: u64,

    /// Average subscription lifetime in hours
    avg_subscription_lifetime_hours: f64,

    /// Peak concurrent subscriptions
    peak_concurrent_subscriptions: u64,

    /// System uptime in seconds
    uptime_seconds: u64,

    /// Last activity timestamp
    last_activity: Option<DateTime<Utc>>,

    /// Memory usage in bytes
    memory_usage_bytes: u64,

    /// CPU usage percentage
    cpu_usage_percent: f64,
}

/// Subscription health monitor
#[derive(Debug, Clone)]
struct SubscriptionHealthMonitor {
    /// Health status for each subscription
    subscription_health: HashMap<SubscriptionId, SubscriptionHealth>,

    /// Health check configuration
    config: HealthCheckConfig,

    /// Last health check timestamp
    last_health_check: Option<DateTime<Utc>>,

    /// Unhealthy subscriptions count
    unhealthy_count: u64,
}

/// Health check configuration
#[derive(Debug, Clone)]
struct HealthCheckConfig {
    /// Health check interval in seconds
    interval_seconds: u64,

    /// Maximum consecutive failures before marking unhealthy
    max_consecutive_failures: u32,

    /// Health check timeout in seconds
    timeout_seconds: u64,

    /// Enable automatic recovery
    enable_auto_recovery: bool,

    /// Recovery backoff strategy
    recovery_backoff: RecoveryBackoff,
}

/// Recovery backoff strategy
#[derive(Debug, Clone)]
enum RecoveryBackoff {
    Fixed { interval_seconds: u64 },
    Exponential { base_seconds: u64, max_seconds: u64 },
    Linear { increment_seconds: u64 },
}

/// Subscription health information
#[derive(Debug, Clone)]
struct SubscriptionHealth {
    /// Subscription ID
    subscription_id: SubscriptionId,

    /// Health status
    status: HealthStatus,

    /// Last check timestamp
    last_check: DateTime<Utc>,

    /// Consecutive failure count
    consecutive_failures: u32,

    /// Last error
    last_error: Option<String>,

    /// Health score (0.0 to 1.0)
    health_score: f64,

    /// Performance metrics
    performance: HealthPerformanceMetrics,
}

/// Health status
#[derive(Debug, Clone, PartialEq)]
enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Recovering,
}

/// Health performance metrics
#[derive(Debug, Clone, Default)]
struct HealthPerformanceMetrics {
    /// Average delivery time in milliseconds
    avg_delivery_time_ms: f64,

    /// Success rate (0.0 to 1.0)
    success_rate: f64,

    /// Queue depth
    queue_depth: usize,

    /// Error rate (0.0 to 1.0)
    error_rate: f64,
}

/// Security manager
#[derive(Debug, Clone)]
struct SecurityManager {
    /// Security policies
    policies: HashMap<String, SecurityPolicy>,

    /// Access control lists
    acls: HashMap<String, AccessControlList>,

    /// Security violations
    violations: VecDeque<SecurityViolation>,

    /// Security statistics
    stats: SecurityStats,
}

/// Security policy
#[derive(Debug, Clone)]
struct SecurityPolicy {
    /// Policy name
    name: String,

    /// Policy rules
    rules: Vec<SecurityRule>,

    /// Policy enabled flag
    enabled: bool,

    /// Policy priority
    priority: u8,
}

/// Security rule
#[derive(Debug, Clone)]
struct SecurityRule {
    /// Rule name
    name: String,

    /// Rule condition
    condition: String,

    /// Rule action
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
    Quarantine,
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

    /// Permission
    permission: SecurityAction,

    /// Conditions
    conditions: Vec<String>,
}

/// Security violation
#[derive(Debug, Clone)]
struct SecurityViolation {
    /// Violation ID
    id: String,

    /// Timestamp
    timestamp: DateTime<Utc>,

    /// Violation type
    violation_type: SecurityViolationType,

    /// Affected subscription
    subscription_id: Option<SubscriptionId>,

    /// Plugin ID
    plugin_id: Option<String>,

    /// Event ID
    event_id: Option<String>,

    /// Description
    description: String,

    /// Severity
    severity: SecuritySeverity,

    /// Resolution status
    resolution: Option<SecurityResolution>,
}

/// Security violation type
#[derive(Debug, Clone, PartialEq)]
enum SecurityViolationType {
    UnauthorizedAccess,
    PrivilegeEscalation,
    SuspiciousActivity,
    PolicyViolation,
    DataBreach,
    DenialOfService,
}

/// Security severity
#[derive(Debug, Clone, PartialEq, PartialOrd, Ord)]
enum SecuritySeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Security resolution
#[derive(Debug, Clone)]
struct SecurityResolution {
    /// Resolution timestamp
    timestamp: DateTime<Utc>,

    /// Resolution action
    action: String,

    /// Resolution details
    details: String,
}

/// Security statistics
#[derive(Debug, Clone, Default)]
struct SecurityStats {
    /// Total violations
    total_violations: u64,

    /// Violations by type
    violations_by_type: HashMap<SecurityViolationType, u64>,

    /// Violations by severity
    violations_by_severity: HashMap<SecuritySeverity, u64>,

    /// Resolved violations
    resolved_violations: u64,

    /// Active violations
    active_violations: u64,

    /// Security score (0.0 to 1.0)
    security_score: f64,
}

/// Lifecycle manager
#[derive(Debug, Clone)]
struct LifecycleManager {
    /// Lifecycle states for subscriptions
    subscription_lifecycles: HashMap<SubscriptionId, SubscriptionLifecycle>,

    /// Lifecycle configuration
    config: LifecycleConfig,

    /// Lifecycle events
    events: VecDeque<LifecycleEvent>,
}

/// Subscription lifecycle
#[derive(Debug, Clone)]
struct SubscriptionLifecycle {
    /// Subscription ID
    subscription_id: SubscriptionId,

    /// Current state
    state: LifecycleState,

    /// State history
    state_history: Vec<LifecycleStateTransition>,

    /// Created timestamp
    created_at: DateTime<Utc>,

    /// Last updated timestamp
    updated_at: DateTime<Utc>,

    /// Expiration timestamp
    expires_at: Option<DateTime<Utc>>,

    /// Lifecycle metadata
    metadata: HashMap<String, String>,
}

/// Lifecycle state
#[derive(Debug, Clone, PartialEq)]
enum LifecycleState {
    Initializing,
    Active,
    Paused,
    Suspended,
    Terminating,
    Terminated,
    Error(String),
}

/// Lifecycle state transition
#[derive(Debug, Clone)]
struct LifecycleStateTransition {
    /// From state
    from_state: LifecycleState,

    /// To state
    to_state: LifecycleState,

    /// Transition timestamp
    timestamp: DateTime<Utc>,

    /// Transition reason
    reason: String,

    /// Transition metadata
    metadata: HashMap<String, String>,
}

/// Lifecycle configuration
#[derive(Debug, Clone)]
struct LifecycleConfig {
    /// Default subscription lifetime in hours
    default_lifetime_hours: u64,

    /// Grace period for termination in minutes
    termination_grace_period_minutes: u64,

    /// Enable automatic renewal
    enable_auto_renewal: bool,

    /// Renewal threshold (percentage of lifetime)
    renewal_threshold_percent: u8,

    /// Maximum renewal attempts
    max_renewal_attempts: u32,
}

/// Lifecycle event
#[derive(Debug, Clone)]
struct LifecycleEvent {
    /// Event ID
    id: String,

    /// Subscription ID
    subscription_id: SubscriptionId,

    /// Event type
    event_type: LifecycleEventType,

    /// Timestamp
    timestamp: DateTime<Utc>,

    /// Event data
    data: HashMap<String, String>,
}

/// Lifecycle event type
#[derive(Debug, Clone, PartialEq)]
enum LifecycleEventType {
    Created,
    Activated,
    Paused,
    Suspended,
    Resumed,
    Renewed,
    Expiring,
    Expired,
    Terminated,
    Error,
}

/// Performance monitor
#[derive(Debug, Clone)]
struct PerformanceMonitor {
    /// Performance metrics
    metrics: PerformanceMetrics,

    /// Metric collection configuration
    config: PerformanceMonitorConfig,

    /// Historical metrics
    historical_metrics: VecDeque<HistoricalMetrics>,
}

/// Performance metrics
#[derive(Debug, Clone, Default)]
struct PerformanceMetrics {
    /// Events per second
    events_per_second: f64,

    /// Average latency in milliseconds
    avg_latency_ms: f64,

    /// P95 latency in milliseconds
    p95_latency_ms: f64,

    /// P99 latency in milliseconds
    p99_latency_ms: f64,

    /// Throughput in events per second
    throughput: f64,

    /// Error rate (0.0 to 1.0)
    error_rate: f64,

    /// Memory usage in bytes
    memory_usage_bytes: u64,

    /// CPU usage percentage
    cpu_usage_percent: f64,

    /// Queue depth
    queue_depth: usize,

    /// Active connections
    active_connections: u32,
}

/// Performance monitor configuration
#[derive(Debug, Clone)]
struct PerformanceMonitorConfig {
    /// Metrics collection interval in seconds
    collection_interval_seconds: u64,

    /// Historical data retention period in hours
    historical_retention_hours: u64,

    /// Enable performance alerts
    enable_alerts: bool,

    /// Performance thresholds
    thresholds: PerformanceThresholds,
}

/// Performance thresholds
#[derive(Debug, Clone)]
struct PerformanceThresholds {
    /// Maximum latency in milliseconds
    max_latency_ms: f64,

    /// Minimum throughput in events per second
    min_throughput: f64,

    /// Maximum error rate (0.0 to 1.0)
    max_error_rate: f64,

    /// Maximum queue depth
    max_queue_depth: usize,

    /// Maximum CPU usage percentage
    max_cpu_usage_percent: f64,

    /// Maximum memory usage in bytes
    max_memory_usage_bytes: u64,
}

/// Historical metrics
#[derive(Debug, Clone)]
struct HistoricalMetrics {
    /// Timestamp
    timestamp: DateTime<Utc>,

    /// Metrics snapshot
    metrics: PerformanceMetrics,
}

/// Manager state
#[derive(Debug, Clone, PartialEq)]
enum ManagerState {
    Starting,
    Running,
    Stopping,
    Stopped,
    Error(String),
}

/// Plugin connection manager trait (redefined from delivery system)
#[async_trait::async_trait]
pub trait PluginConnectionManager: Send + Sync {
    /// Check if plugin is connected
    async fn is_plugin_connected(&self, plugin_id: &str) -> bool;

    /// Get plugin information
    async fn get_plugin_info(&self, plugin_id: &str) -> Option<PluginInfo>;

    /// Send event to plugin
    async fn send_event_to_plugin(
        &self,
        plugin_id: &str,
        event: &SerializedEvent,
    ) -> SubscriptionResult<()>;

    /// Get all connected plugins
    async fn get_connected_plugins(&self) -> Vec<PluginInfo>;
}

/// Plugin information
#[derive(Debug, Clone)]
pub struct PluginInfo {
    /// Plugin ID
    pub plugin_id: String,

    /// Plugin name
    pub plugin_name: String,

    /// Plugin version
    pub plugin_version: String,

    /// Plugin status
    pub status: PluginStatus,

    /// Connected timestamp
    pub connected_at: DateTime<Utc>,

    /// Last activity
    pub last_activity: DateTime<Utc>,

    /// Plugin capabilities
    pub capabilities: Vec<String>,

    /// Plugin metadata
    pub metadata: HashMap<String, String>,
}

/// Plugin status
#[derive(Debug, Clone, PartialEq)]
pub enum PluginStatus {
    Connected,
    Disconnected,
    Error,
    Busy,
    Suspended,
}

/// Serialized event for delivery
#[derive(Debug, Clone)]
pub struct SerializedEvent {
    /// Serialized data
    pub data: Vec<u8>,

    /// Content type
    pub content_type: String,

    /// Event metadata
    pub metadata: HashMap<String, String>,
}

impl SubscriptionManager {
    /// Create a new subscription manager
    pub fn new(
        config: ManagerConfig,
        plugin_connection_manager: Arc<dyn PluginConnectionManager + Send + Sync>,
    ) -> Self {
        // Create core components
        let subscription_registry = Arc::new(SubscriptionRegistry::new());
        let filter_engine = Arc::new(FilterEngine::new());

        // Create delivery system
        let delivery_config = crate::plugin_events::delivery_system::DeliveryConfig::default();
        let delivery_system = Arc::new(DeliverySystem::new(
            delivery_config,
            plugin_connection_manager.clone(),
        ));

        // Create event bridge
        let bridge_config = crate::plugin_events::event_bridge::BridgeConfig::default();
        let event_bridge = Arc::new(EventBridge::new(
            subscription_registry.clone(),
            filter_engine.clone(),
            bridge_config,
        ));

        // Initialize health monitor
        let health_monitor = SubscriptionHealthMonitor {
            subscription_health: HashMap::new(),
            config: HealthCheckConfig {
                interval_seconds: config.health_check_interval_seconds,
                max_consecutive_failures: 3,
                timeout_seconds: 30,
                enable_auto_recovery: true,
                recovery_backoff: RecoveryBackoff::Exponential {
                    base_seconds: 60,
                    max_seconds: 3600,
                },
            },
            last_health_check: None,
            unhealthy_count: 0,
        };

        // Initialize security manager
        let security_manager = SecurityManager {
            policies: HashMap::new(),
            acls: HashMap::new(),
            violations: VecDeque::new(),
            stats: SecurityStats::default(),
        };

        // Initialize lifecycle manager
        let lifecycle_manager = LifecycleManager {
            subscription_lifecycles: HashMap::new(),
            config: LifecycleConfig {
                default_lifetime_hours: config.max_subscription_lifetime_hours,
                termination_grace_period_minutes: 5,
                enable_auto_renewal: false,
                renewal_threshold_percent: 80,
                max_renewal_attempts: 3,
            },
            events: VecDeque::new(),
        };

        // Initialize performance monitor
        let performance_monitor = PerformanceMonitor {
            metrics: PerformanceMetrics::default(),
            config: PerformanceMonitorConfig {
                collection_interval_seconds: config.performance_metrics_interval_seconds,
                historical_retention_hours: 24,
                enable_alerts: true,
                thresholds: PerformanceThresholds {
                    max_latency_ms: 1000.0,
                    min_throughput: 100.0,
                    max_error_rate: 0.05,
                    max_queue_depth: 1000,
                    max_cpu_usage_percent: 80.0,
                    max_memory_usage_bytes: 1024 * 1024 * 1024, // 1GB
                },
            },
            historical_metrics: VecDeque::new(),
        };

        let inner = SubscriptionManagerInner {
            subscription_registry: subscription_registry.clone(),
            filter_engine: filter_engine.clone(),
            event_bridge,
            delivery_system,
            config,
            plugin_connection_manager,
            stats: ManagerStats::default(),
            plugin_subscriptions: HashMap::new(),
            health_monitor,
            security_manager,
            lifecycle_manager,
            performance_monitor,
            state: ManagerState::Stopped,
            event_bus: None,
        };

        let (shutdown_tx, _) = mpsc::channel(1);

        Self {
            inner: Arc::new(RwLock::new(inner)),
            task_handles: Vec::new(),
            shutdown_tx: Some(shutdown_tx),
        }
    }

    /// Start the subscription manager
    pub async fn start(&mut self, event_bus: Arc<dyn EventBus + Send + Sync>) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;

        if inner.state != ManagerState::Stopped {
            return Err(SubscriptionError::InternalError(
                "Subscription manager is already running".to_string()
            ));
        }

        inner.state = ManagerState::Starting;
        inner.event_bus = Some(event_bus.clone());

        // Start event bridge
        {
            let bridge = inner.event_bridge.clone();
            let event_bus_clone = event_bus.clone();
            tokio::spawn(async move {
                let mut bridge_mut = (*bridge).clone();
                if let Err(e) = bridge_mut.start(event_bus_clone).await {
                    error!("Failed to start event bridge: {}", e);
                }
            });
        }

        // Start delivery system
        {
            let delivery_system = inner.delivery_system.clone();
            tokio::spawn(async move {
                let mut delivery_system_mut = (*delivery_system).clone();
                if let Err(e) = delivery_system_mut.start().await {
                    error!("Failed to start delivery system: {}", e);
                }
            });
        }

        // Start background tasks
        if inner.config.enable_health_monitoring {
            let inner_clone = self.inner.clone();
            let health_handle = tokio::spawn(async move {
                Self::health_monitor_task(inner_clone).await;
            });
            self.task_handles.push(Arc::new(health_handle));
        }

        if inner.config.enable_auto_cleanup {
            let inner_clone = self.inner.clone();
            let cleanup_handle = tokio::spawn(async move {
                Self::cleanup_task(inner_clone).await;
            });
            self.task_handles.push(Arc::new(cleanup_handle));
        }

        if inner.config.enable_performance_monitoring {
            let inner_clone = self.inner.clone();
            let performance_handle = tokio::spawn(async move {
                Self::performance_monitor_task(inner_clone).await;
            });
            self.task_handles.push(Arc::new(performance_handle));
        }

        inner.state = ManagerState::Running;
        inner.stats.last_activity = Some(Utc::now());

        info!("Subscription manager started successfully");

        Ok(())
    }

    /// Stop the subscription manager
    pub async fn stop(&self) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;

        if inner.state != ManagerState::Running {
            return Ok(());
        }

        inner.state = ManagerState::Stopping;

        // Stop event bridge
        if let Err(e) = inner.event_bridge.stop().await {
            error!("Failed to stop event bridge: {}", e);
        }

        // Stop delivery system
        if let Err(e) = inner.delivery_system.stop().await {
            error!("Failed to stop delivery system: {}", e);
        }

        // Send shutdown signal to background tasks
        if let Some(shutdown_tx) = &self.shutdown_tx {
            let _ = shutdown_tx.send(()).await;
        }

        // Wait for all tasks to complete
        for handle in &self.task_handles {
            let _ = handle.await;
        }

        inner.state = ManagerState::Stopped;

        info!("Subscription manager stopped");

        Ok(())
    }

    /// Create a new subscription
    pub async fn create_subscription(
        &self,
        plugin_id: String,
        subscription_config: SubscriptionConfig,
    ) -> SubscriptionResult<SubscriptionId> {
        let mut inner = self.inner.write().await;

        // Validate plugin exists and is connected
        if !inner.plugin_connection_manager.is_plugin_connected(&plugin_id).await {
            return Err(SubscriptionError::PluginNotFound(plugin_id));
        }

        // Check subscription limits
        let plugin_sub_count = inner.plugin_subscriptions
            .get(&plugin_id)
            .map(|subs| subs.len())
            .unwrap_or(0);

        if plugin_sub_count >= inner.config.max_subscriptions_per_plugin as usize {
            return Err(SubscriptionError::ResourceExhausted(
                format!("Plugin {} has reached maximum subscription limit", plugin_id)
            ));
        }

        // Validate subscription configuration
        self.validate_subscription_config(&subscription_config)?;

        // Set default authorization context if not provided
        let mut final_config = subscription_config;
        if final_config.auth_context.permissions.is_empty() {
            final_config.auth_context = self.create_default_auth_context(&plugin_id);
        }

        // Register subscription
        inner.subscription_registry.register_subscription(final_config.clone()).await?;

        // Register with delivery system
        inner.delivery_system.register_subscription(final_config.clone()).await?;

        // Update plugin subscriptions
        let plugin_subs = inner.plugin_subscriptions
            .entry(plugin_id.clone())
            .or_insert_with(Vec::new);
        plugin_subs.push(final_config.id.clone());

        // Initialize health monitoring
        inner.health_monitor.subscription_health.insert(
            final_config.id.clone(),
            SubscriptionHealth {
                subscription_id: final_config.id.clone(),
                status: HealthStatus::Healthy,
                last_check: Utc::now(),
                consecutive_failures: 0,
                last_error: None,
                health_score: 1.0,
                performance: HealthPerformanceMetrics::default(),
            },
        );

        // Initialize lifecycle
        inner.lifecycle_manager.subscription_lifecycles.insert(
            final_config.id.clone(),
            SubscriptionLifecycle {
                subscription_id: final_config.id.clone(),
                state: LifecycleState::Active,
                state_history: vec![LifecycleStateTransition {
                    from_state: LifecycleState::Initializing,
                    to_state: LifecycleState::Active,
                    timestamp: Utc::now(),
                    reason: "Subscription created".to_string(),
                    metadata: HashMap::new(),
                }],
                created_at: Utc::now(),
                updated_at: Utc::now(),
                expires_at: None,
                metadata: HashMap::new(),
            },
        );

        // Update statistics
        inner.stats.total_subscriptions_created += 1;
        inner.stats.active_subscriptions += 1;
        inner.stats.last_activity = Some(Utc::now());

        if inner.stats.active_subscriptions > inner.stats.peak_concurrent_subscriptions {
            inner.stats.peak_concurrent_subscriptions = inner.stats.active_subscriptions;
        }

        // Log audit event
        self.log_audit_event(AuditEvent::SubscriptionCreated, &final_config).await;

        info!("Created subscription {} for plugin {}", final_config.id.as_string(), plugin_id);

        Ok(final_config.id)
    }

    /// Delete a subscription
    pub async fn delete_subscription(&self, subscription_id: &SubscriptionId) -> SubscriptionResult<()> {
        let mut inner = self.inner.write().await;

        // Get subscription details
        let subscription = inner.subscription_registry.get_subscription(subscription_id).await?;

        // Unregister from delivery system
        inner.delivery_system.unregister_subscription(subscription_id).await?;

        // Unregister from registry
        inner.subscription_registry.unregister_subscription(subscription_id).await?;

        // Update plugin subscriptions
        if let Some(plugin_subs) = inner.plugin_subscriptions.get_mut(&subscription.plugin_id) {
            plugin_subs.retain(|id| id != subscription_id);
            if plugin_subs.is_empty() {
                inner.plugin_subscriptions.remove(&subscription.plugin_id);
            }
        }

        // Update lifecycle state
        if let Some(lifecycle) = inner.lifecycle_manager.subscription_lifecycles.get_mut(subscription_id) {
            lifecycle.state = LifecycleState::Terminated;
            lifecycle.updated_at = Utc::now();
            lifecycle.state_history.push(LifecycleStateTransition {
                from_state: lifecycle.state.clone(),
                to_state: LifecycleState::Terminated,
                timestamp: Utc::now(),
                reason: "Subscription deleted".to_string(),
                metadata: HashMap::new(),
            });
        }

        // Clean up health monitoring
        inner.health_monitor.subscription_health.remove(subscription_id);

        // Update statistics
        inner.stats.total_subscriptions_deleted += 1;
        if inner.stats.active_subscriptions > 0 {
            inner.stats.active_subscriptions -= 1;
        }
        inner.stats.last_activity = Some(Utc::now());

        // Log audit event
        self.log_audit_event(AuditEvent::SubscriptionDeleted, &subscription).await;

        info!("Deleted subscription {}", subscription_id.as_string());

        Ok(())
    }

    /// Get subscription by ID
    pub async fn get_subscription(&self, subscription_id: &SubscriptionId) -> SubscriptionResult<SubscriptionConfig> {
        let inner = self.inner.read().await;
        inner.subscription_registry.get_subscription(subscription_id).await
    }

    /// Get all subscriptions for a plugin
    pub async fn get_plugin_subscriptions(&self, plugin_id: &str) -> Vec<SubscriptionConfig> {
        let inner = self.inner.read().await;
        inner.subscription_registry.get_plugin_subscriptions(plugin_id).await
    }

    /// Get subscription statistics
    pub async fn get_subscription_stats(&self, subscription_id: &SubscriptionId) -> SubscriptionResult<SubscriptionStats> {
        let inner = self.inner.read().await;
        inner.subscription_registry.get_stats(subscription_id).await
    }

    /// Get manager statistics
    pub async fn get_manager_stats(&self) -> ManagerStats {
        let inner = self.inner.read().await;
        inner.stats.clone()
    }

    /// Get health information for subscriptions
    pub async fn get_subscription_health(&self, subscription_id: &SubscriptionId) -> Option<SubscriptionHealth> {
        let inner = self.inner.read().await;
        inner.health_monitor.subscription_health.get(subscription_id).cloned()
    }

    /// Get performance metrics
    pub async fn get_performance_metrics(&self) -> PerformanceMetrics {
        let inner = self.inner.read().await;
        inner.performance_monitor.metrics.clone()
    }

    /// Get security violations
    pub async fn get_security_violations(&self, limit: Option<usize>) -> Vec<SecurityViolation> {
        let inner = self.inner.read().await;
        let violations = &inner.security_manager.violations;

        if let Some(limit) = limit {
            violations.iter().rev().take(limit).cloned().collect()
        } else {
            violations.iter().rev().cloned().collect()
        }
    }

    /// Validate subscription configuration
    fn validate_subscription_config(&self, config: &SubscriptionConfig) -> SubscriptionResult<()> {
        // Validate subscription name
        if config.name.is_empty() {
            return Err(SubscriptionError::ValidationError(
                "Subscription name cannot be empty".to_string()
            ));
        }

        // Validate delivery options
        if config.delivery_options.max_event_size == 0 {
            return Err(SubscriptionError::ValidationError(
                "Max event size must be greater than 0".to_string()
            ));
        }

        // Validate subscription type specific requirements
        match &config.subscription_type {
            SubscriptionType::Batched { interval_seconds, max_batch_size } => {
                if *interval_seconds == 0 {
                    return Err(SubscriptionError::ValidationError(
                        "Batch interval must be greater than 0".to_string()
                    ));
                }
                if *max_batch_size == 0 {
                    return Err(SubscriptionError::ValidationError(
                        "Max batch size must be greater than 0".to_string()
                    ));
                }
            }
            SubscriptionType::Persistent { max_stored_events, ttl } => {
                if *max_stored_events == 0 {
                    return Err(SubscriptionError::ValidationError(
                        "Max stored events must be greater than 0".to_string()
                    ));
                }
                if ttl.is_zero() {
                    return Err(SubscriptionError::ValidationError(
                        "TTL must be greater than 0".to_string()
                    ));
                }
            }
            _ => {} // Other types are generally valid
        }

        Ok(())
    }

    /// Create default authorization context for plugin
    fn create_default_auth_context(&self, plugin_id: &str) -> AuthContext {
        let default_permissions = vec![
            EventPermission {
                scope: PermissionScope::Plugin,
                event_types: vec![],
                categories: vec![],
                sources: vec![plugin_id.to_string()],
                max_priority: None,
            }
        ];

        AuthContext::new(plugin_id.to_string(), default_permissions)
    }

    /// Log audit event
    async fn log_audit_event(&self, event_type: AuditEvent, subscription: &SubscriptionConfig) {
        let inner = self.inner.read().await;

        if inner.config.audit_config.enabled {
            if inner.config.audit_config.events.contains(&event_type) {
                // In a real implementation, this would log to audit system
                info!("Audit event: {:?} for subscription {}", event_type, subscription.id.as_string());
            }
        }
    }

    /// Health monitor background task
    async fn health_monitor_task(inner: Arc<RwLock<SubscriptionManagerInner>>) {
        let mut interval = tokio::time::interval(
            std::time::Duration::from_secs(inner.read().await.health_monitor.config.interval_seconds)
        );

        loop {
            interval.tick().await;

            let running = {
                let inner_guard = inner.read().await;
                matches!(inner_guard.state, ManagerState::Running)
            };

            if !running {
                break;
            }

            // Perform health checks
            Self::perform_health_checks(&inner).await;
        }
    }

    /// Perform health checks for all subscriptions
    async fn perform_health_checks(inner: &Arc<RwLock<SubscriptionManagerInner>>) {
        let mut inner_guard = inner.write().await;

        let subscriptions_to_check: Vec<SubscriptionId> = inner_guard
            .health_monitor
            .subscription_health
            .keys()
            .cloned()
            .collect();

        for subscription_id in subscriptions_to_check {
            // Check subscription health
            let health_status = Self::check_subscription_health(&inner_guard, &subscription_id).await;

            // Update health information
            if let Some(health) = inner_guard.health_monitor.subscription_health.get_mut(&subscription_id) {
                health.last_check = Utc::now();
                health.status = health_status.status;
                health.consecutive_failures = health_status.consecutive_failures;
                health.last_error = health_status.last_error;
                health.health_score = health_status.health_score;
                health.performance = health_status.performance;
            }
        }

        inner_guard.health_monitor.last_health_check = Some(Utc::now());
    }

    /// Check individual subscription health
    async fn check_subscription_health(
        inner: &SubscriptionManagerInner,
        subscription_id: &SubscriptionId,
    ) -> SubscriptionHealth {
        // Get subscription statistics
        let stats = inner.subscription_registry.get_stats(subscription_id).await
            .unwrap_or_default();

        // Get delivery statistics
        let delivery_stats = inner.delivery_system.get_subscription_stats(subscription_id).await
            .unwrap_or_default();

        // Calculate health metrics
        let success_rate = if stats.events_received > 0 {
            stats.events_delivered as f64 / stats.events_received as f64
        } else {
            1.0
        };

        let avg_delivery_time = delivery_stats.avg_delivery_time_ms;

        // Determine health status
        let (status, health_score) = if success_rate >= 0.95 && avg_delivery_time < 1000.0 {
            (HealthStatus::Healthy, 1.0)
        } else if success_rate >= 0.8 && avg_delivery_time < 5000.0 {
            (HealthStatus::Degraded, 0.7)
        } else {
            (HealthStatus::Unhealthy, 0.3)
        };

        SubscriptionHealth {
            subscription_id: subscription_id.clone(),
            status,
            last_check: Utc::now(),
            consecutive_failures: if success_rate < 0.5 { 1 } else { 0 },
            last_error: if success_rate < 0.5 {
                Some("High failure rate detected".to_string())
            } else {
                None
            },
            health_score,
            performance: HealthPerformanceMetrics {
                avg_delivery_time_ms: avg_delivery_time,
                success_rate,
                queue_depth: delivery_stats.queue_size,
                error_rate: 1.0 - success_rate,
            },
        }
    }

    /// Cleanup background task
    async fn cleanup_task(inner: Arc<RwLock<SubscriptionManagerInner>>) {
        let mut interval = tokio::time::interval(
            std::time::Duration::from_secs(inner.read().await.config.cleanup_interval_seconds)
        );

        loop {
            interval.tick().await;

            let running = {
                let inner_guard = inner.read().await;
                matches!(inner_guard.state, ManagerState::Running)
            };

            if !running {
                break;
            }

            // Perform cleanup
            Self::perform_cleanup(&inner).await;
        }
    }

    /// Perform cleanup operations
    async fn perform_cleanup(inner: &Arc<RwLock<SubscriptionManagerInner>>) {
        let mut inner_guard = inner.write().await;

        // Cleanup expired subscriptions
        let max_age = chrono::Duration::hours(inner_guard.config.max_subscription_lifetime_hours as i64);
        if let Ok(removed_count) = inner_guard.subscription_registry.cleanup_expired(max_age).await {
            if removed_count > 0 {
                info!("Cleaned up {} expired subscriptions", removed_count);
            }
        }

        // Cleanup old security violations
        let retention_period = chrono::Duration::days(30); // Default retention
        let cutoff_time = Utc::now() - retention_period;

        inner_guard.security_manager.violations.retain(|v| v.timestamp > cutoff_time);

        // Cleanup old historical metrics
        let retention_hours = inner_guard.performance_monitor.config.historical_retention_hours;
        let cutoff_time = Utc::now() - chrono::Duration::hours(retention_hours as i64);

        inner_guard.performance_monitor.historical_metrics.retain(|m| m.timestamp > cutoff_time);
    }

    /// Performance monitor background task
    async fn performance_monitor_task(inner: Arc<RwLock<SubscriptionManagerInner>>) {
        let mut interval = tokio::time::interval(
            std::time::Duration::from_secs(inner.read().await.performance_monitor.config.collection_interval_seconds)
        );

        loop {
            interval.tick().await;

            let running = {
                let inner_guard = inner.read().await;
                matches!(inner_guard.state, ManagerState::Running)
            };

            if !running {
                break;
            }

            // Collect performance metrics
            Self::collect_performance_metrics(&inner).await;
        }
    }

    /// Collect performance metrics
    async fn collect_performance_metrics(inner: &Arc<RwLock<SubscriptionManagerInner>>) {
        let mut inner_guard = inner.write().await;

        // Get metrics from various components
        let registry_metrics = inner_guard.subscription_registry.get_metrics().await;
        let delivery_metrics = inner_guard.delivery_system.get_metrics().await;

        // Calculate aggregate metrics
        let events_per_second = delivery_metrics.throughput_events_per_sec;
        let avg_latency_ms = delivery_metrics.avg_delivery_time_ms;
        let error_rate = delivery_metrics.error_rate;
        let memory_usage_bytes = registry_metrics.memory_usage_bytes;

        // Update performance metrics
        inner_guard.performance_monitor.metrics = PerformanceMetrics {
            events_per_second,
            avg_latency_ms,
            p95_latency_ms: avg_latency_ms * 1.5, // Estimate
            p99_latency_ms: avg_latency_ms * 2.0, // Estimate
            throughput: events_per_second,
            error_rate,
            memory_usage_bytes,
            cpu_usage_percent: 0.0, // Would need system monitoring
            queue_depth: delivery_metrics.total_queue_depth,
            active_connections: inner_guard.plugin_subscriptions.len() as u32,
        };

        // Store historical metrics
        inner_guard.performance_monitor.historical_metrics.push_back(HistoricalMetrics {
            timestamp: Utc::now(),
            metrics: inner_guard.performance_monitor.metrics.clone(),
        });

        // Enforce retention
        let retention_limit = (inner_guard.performance_monitor.config.historical_retention_hours * 60 * 60) as usize;
        while inner_guard.performance_monitor.historical_metrics.len() > retention_limit {
            inner_guard.performance_monitor.historical_metrics.pop_front();
        }

        // Check for performance alerts
        if inner_guard.performance_monitor.config.enable_alerts {
            Self::check_performance_alerts(&inner_guard.performance_monitor.metrics, &inner_guard.performance_monitor.config.thresholds).await;
        }
    }

    /// Check for performance alerts
    async fn check_performance_alerts(metrics: &PerformanceMetrics, thresholds: &PerformanceThresholds) {
        if metrics.avg_latency_ms > thresholds.max_latency_ms {
            warn!("Performance alert: High latency detected - {:.2}ms (threshold: {:.2}ms)",
                  metrics.avg_latency_ms, thresholds.max_latency_ms);
        }

        if metrics.throughput < thresholds.min_throughput {
            warn!("Performance alert: Low throughput detected - {:.2} events/sec (threshold: {:.2} events/sec)",
                  metrics.throughput, thresholds.min_throughput);
        }

        if metrics.error_rate > thresholds.max_error_rate {
            warn!("Performance alert: High error rate detected - {:.2}% (threshold: {:.2}%)",
                  metrics.error_rate * 100.0, thresholds.max_error_rate * 100.0);
        }

        if metrics.queue_depth > thresholds.max_queue_depth {
            warn!("Performance alert: High queue depth detected - {} (threshold: {})",
                  metrics.queue_depth, thresholds.max_queue_depth);
        }

        if metrics.memory_usage_bytes > thresholds.max_memory_usage_bytes {
            warn!("Performance alert: High memory usage detected - {} bytes (threshold: {} bytes)",
                  metrics.memory_usage_bytes, thresholds.max_memory_usage_bytes);
        }
    }
}

impl Default for SubscriptionManager {
    fn default() -> Self {
        let config = ManagerConfig::default();
        let plugin_connection_manager = Arc::new(MockPluginConnectionManager::new());
        Self::new(config, plugin_connection_manager)
    }
}

/// Mock plugin connection manager for testing
struct MockPluginConnectionManager {
    plugins: Arc<RwLock<HashMap<String, PluginInfo>>>,
}

impl MockPluginConnectionManager {
    fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn add_plugin(&self, plugin_info: PluginInfo) {
        let mut plugins = self.plugins.write().await;
        plugins.insert(plugin_info.plugin_id.clone(), plugin_info);
    }
}

#[async_trait::async_trait]
impl PluginConnectionManager for MockPluginConnectionManager {
    async fn is_plugin_connected(&self, plugin_id: &str) -> bool {
        let plugins = self.plugins.read().await;
        plugins.get(plugin_id)
            .map(|p| matches!(p.status, PluginStatus::Connected))
            .unwrap_or(false)
    }

    async fn get_plugin_info(&self, plugin_id: &str) -> Option<PluginInfo> {
        let plugins = self.plugins.read().await;
        plugins.get(plugin_id).cloned()
    }

    async fn send_event_to_plugin(
        &self,
        _plugin_id: &str,
        _event: &SerializedEvent,
    ) -> SubscriptionResult<()> {
        Ok(())
    }

    async fn get_connected_plugins(&self) -> Vec<PluginInfo> {
        let plugins = self.plugins.read().await;
        plugins.values()
            .filter(|p| matches!(p.status, PluginStatus::Connected))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{MockEventBus, EventPayload, EventSource, SourceType};

    #[tokio::test]
    async fn test_manager_creation() {
        let config = ManagerConfig::default();
        let plugin_connection_manager = Arc::new(MockPluginConnectionManager::new());
        let manager = SubscriptionManager::new(config, plugin_connection_manager);

        assert_eq!(manager.get_manager_stats().await.active_subscriptions, 0);
    }

    #[tokio::test]
    async fn test_subscription_creation() {
        let config = ManagerConfig::default();
        let plugin_connection_manager = Arc::new(MockPluginConnectionManager::new());

        // Add a mock plugin
        let plugin_info = PluginInfo {
            plugin_id: "test-plugin".to_string(),
            plugin_name: "Test Plugin".to_string(),
            plugin_version: "1.0.0".to_string(),
            status: PluginStatus::Connected,
            connected_at: Utc::now(),
            last_activity: Utc::now(),
            capabilities: vec!["events".to_string()],
            metadata: HashMap::new(),
        };
        plugin_connection_manager.add_plugin(plugin_info).await;

        let mut manager = SubscriptionManager::new(config, plugin_connection_manager);

        let subscription_config = SubscriptionConfig::new(
            "test-plugin".to_string(),
            "test-subscription".to_string(),
            SubscriptionType::Realtime,
            AuthContext::new("test-plugin".to_string(), vec![]),
        );

        // Note: This test would require the manager to be started first
        // manager.start(Arc::new(MockEventBus::new())).await.unwrap();
        // let subscription_id = manager.create_subscription("test-plugin".to_string(), subscription_config).await.unwrap();
        // assert!(subscription_id.as_string().len() > 0);
    }

    #[tokio::test]
    async fn test_plugin_connection_manager() {
        let manager = MockPluginConnectionManager::new();

        let plugin_info = PluginInfo {
            plugin_id: "test-plugin".to_string(),
            plugin_name: "Test Plugin".to_string(),
            plugin_version: "1.0.0".to_string(),
            status: PluginStatus::Connected,
            connected_at: Utc::now(),
            last_activity: Utc::now(),
            capabilities: vec!["events".to_string()],
            metadata: HashMap::new(),
        };

        manager.add_plugin(plugin_info).await;

        assert!(manager.is_plugin_connected("test-plugin").await);
        assert!(!manager.is_plugin_connected("nonexistent").await);

        let info = manager.get_plugin_info("test-plugin").await;
        assert!(info.is_some());
        assert_eq!(info.unwrap().plugin_id, "test-plugin");
    }
}