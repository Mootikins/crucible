//! Core types and data structures for plugin event subscription system

use crate::events::{DaemonEvent, EventFilter, EventPriority, EventType};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

/// Plugin event subscription configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubscriptionConfig {
    /// Unique subscription identifier
    pub id: SubscriptionId,

    /// Plugin identifier
    pub plugin_id: String,

    /// Subscription name/description
    pub name: String,

    /// Event filters for this subscription
    pub filters: Vec<EventFilter>,

    /// Subscription type
    pub subscription_type: SubscriptionType,

    /// Delivery options
    pub delivery_options: DeliveryOptions,

    /// Subscription status
    pub status: SubscriptionStatus,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,

    /// Subscription metadata
    pub metadata: HashMap<String, String>,

    /// Authorization context
    pub auth_context: AuthContext,
}

impl SubscriptionConfig {
    /// Create a new subscription
    pub fn new(
        plugin_id: String,
        name: String,
        subscription_type: SubscriptionType,
        auth_context: AuthContext,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: SubscriptionId::new(),
            plugin_id,
            name,
            filters: Vec::new(),
            subscription_type,
            delivery_options: DeliveryOptions::default(),
            status: SubscriptionStatus::Active,
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
            auth_context,
        }
    }

    /// Add event filter to subscription
    pub fn with_filter(mut self, filter: EventFilter) -> Self {
        self.filters.push(filter);
        self.updated_at = Utc::now();
        self
    }

    /// Set delivery options
    pub fn with_delivery_options(mut self, options: DeliveryOptions) -> Self {
        self.delivery_options = options;
        self.updated_at = Utc::now();
        self
    }

    /// Add metadata field
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self.updated_at = Utc::now();
        self
    }

    /// Check if subscription matches an event
    pub fn matches_event(&self, event: &DaemonEvent) -> bool {
        // Check subscription status
        if self.status != SubscriptionStatus::Active {
            return false;
        }

        // Check authorization
        if !self.auth_context.can_access_event(event) {
            return false;
        }

        // Apply filters - if any filter matches, the event is accepted
        if self.filters.is_empty() {
            return true; // No filters means accept all events
        }

        self.filters.iter().any(|filter| filter.matches(event))
    }

    /// Update subscription status
    pub fn update_status(&mut self, status: SubscriptionStatus) {
        self.status = status;
        self.updated_at = Utc::now();
    }
}

/// Unique subscription identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SubscriptionId(pub Uuid);

impl SubscriptionId {
    /// Generate a new subscription ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create subscription ID from string
    pub fn from_string(s: String) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(&s)?))
    }

    /// Get subscription ID as string
    pub fn as_string(&self) -> String {
        self.0.to_string()
    }
}

impl Default for SubscriptionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Subscription type determines how events are delivered
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SubscriptionType {
    /// Real-time delivery via WebSocket
    Realtime,

    /// Batched delivery at specified intervals
    Batched {
        /// Batch interval in seconds
        interval_seconds: u64,
        /// Maximum batch size
        max_batch_size: usize,
    },

    /// Persistent delivery with storage for offline plugins
    Persistent {
        /// Maximum number of events to store
        max_stored_events: usize,
        /// TTL for stored events
        ttl: Duration,
    },

    /// Conditional delivery based on event content
    Conditional {
        /// Condition expression
        condition: String,
        /// Fallback delivery method
        fallback: Box<SubscriptionType>,
    },

    /// Priority delivery for critical events
    Priority {
        /// Minimum priority for events
        min_priority: EventPriority,
        /// Delivery method for priority events
        delivery_method: Box<SubscriptionType>,
    },
}

/// Event delivery options
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeliveryOptions {
    /// Enable delivery acknowledgment
    pub ack_enabled: bool,

    /// Maximum retry attempts
    pub max_retries: u32,

    /// Retry backoff strategy
    pub retry_backoff: RetryBackoff,

    /// Enable compression for large events
    pub compression_enabled: bool,

    /// Compression threshold in bytes
    pub compression_threshold: usize,

    /// Enable encryption for sensitive events
    pub encryption_enabled: bool,

    /// Maximum event size to accept
    pub max_event_size: usize,

    /// Ordering guarantees
    pub ordering: EventOrdering,

    /// Backpressure handling
    pub backpressure_handling: BackpressureHandling,
}

impl Default for DeliveryOptions {
    fn default() -> Self {
        Self {
            ack_enabled: true,
            max_retries: 3,
            retry_backoff: RetryBackoff::Exponential { base_ms: 1000, max_ms: 30000 },
            compression_enabled: false,
            compression_threshold: 1024,
            encryption_enabled: false,
            max_event_size: 10 * 1024 * 1024, // 10MB
            ordering: EventOrdering::Fifo,
            backpressure_handling: BackpressureHandling::Buffer { max_size: 1000 },
        }
    }
}

/// Retry backoff strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RetryBackoff {
    /// Fixed delay between retries
    Fixed { delay_ms: u64 },

    /// Exponential backoff with optional jitter
    Exponential { base_ms: u64, max_ms: u64 },

    /// Linear backoff
    Linear { increment_ms: u64 },

    /// Custom backoff function
    Custom { function_name: String },
}

/// Event ordering guarantees
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventOrdering {
    /// No ordering guarantees
    None,

    /// First-in-first-out ordering
    Fifo,

    /// Priority-based ordering
    Priority,

    /// Causal ordering based on causation IDs
    Causal,
}

/// Backpressure handling strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BackpressureHandling {
    /// Buffer events up to a maximum size
    Buffer { max_size: usize },

    /// Drop oldest events when buffer is full
    DropOldest { max_size: usize },

    /// Drop newest events when buffer is full
    DropNewest,

    /// Apply backpressure to event source
    ApplyBackpressure,

    /// Custom backpressure handler
    Custom { handler_name: String },
}

/// Subscription status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SubscriptionStatus {
    /// Subscription is active and receiving events
    Active,

    /// Subscription is paused (not receiving events)
    Paused,

    /// Subscription is suspended due to errors
    Suspended {
        /// Suspension reason
        reason: String,
        /// Suspension timestamp
        suspended_at: DateTime<Utc>,
        /// Retry after timestamp
        retry_after: Option<DateTime<Utc>>,
    },

    /// Subscription is terminated
    Terminated {
        /// Termination reason
        reason: String,
        /// Termination timestamp
        terminated_at: DateTime<Utc>,
    },
}

/// Authorization context for subscriptions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthContext {
    /// User or plugin ID
    pub principal: String,

    /// Granted permissions
    pub permissions: Vec<EventPermission>,

    /// Security level
    pub security_level: SecurityLevel,

    /// Additional authorization metadata
    pub metadata: HashMap<String, String>,
}

impl AuthContext {
    /// Create new authorization context
    pub fn new(principal: String, permissions: Vec<EventPermission>) -> Self {
        Self {
            principal,
            permissions,
            security_level: SecurityLevel::Normal,
            metadata: HashMap::new(),
        }
    }

    /// Check if the context allows access to an event
    pub fn can_access_event(&self, event: &DaemonEvent) -> bool {
        self.permissions.iter().any(|perm| perm.allows_event(event))
    }

    /// Add permission
    pub fn add_permission(&mut self, permission: EventPermission) {
        if !self.permissions.contains(&permission) {
            self.permissions.push(permission);
        }
    }

    /// Remove permission
    pub fn remove_permission(&mut self, permission: &EventPermission) {
        self.permissions.retain(|p| p != permission);
    }
}

/// Event permission definition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventPermission {
    /// Permission scope
    pub scope: PermissionScope,

    /// Allowed event types (empty means all)
    pub event_types: Vec<String>,

    /// Allowed event categories (empty means all)
    pub categories: Vec<String>,

    /// Allowed sources (empty means all)
    pub sources: Vec<String>,

    /// Maximum priority level allowed
    pub max_priority: Option<EventPriority>,
}

impl EventPermission {
    /// Check if permission allows an event
    pub fn allows_event(&self, event: &DaemonEvent) -> bool {
        // Check event types
        if !self.event_types.is_empty() {
            let event_type_str = match &event.event_type {
                EventType::Filesystem(_) => "filesystem",
                EventType::Database(_) => "database",
                EventType::External(_) => "external",
                EventType::Mcp(_) => "mcp",
                EventType::Service(_) => "service",
                EventType::System(_) => "system",
                EventType::Custom(name) => name,
            };
            if !self.event_types.contains(&event_type_str.to_string()) {
                return false;
            }
        }

        // Check categories
        if !self.categories.is_empty() {
            let category_str = format!("{:?}", event.event_type.category());
            if !self.categories.contains(&category_str) {
                return false;
            }
        }

        // Check sources
        if !self.sources.is_empty() && !self.sources.contains(&event.source.id) {
            return false;
        }

        // Check priority
        if let Some(max_priority) = self.max_priority {
            if event.priority > max_priority {
                return false;
            }
        }

        true
    }
}

/// Permission scope
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionScope {
    /// Global permission (all events)
    Global,

    /// Plugin-specific events
    Plugin,

    /// Service-specific events
    Service { service_id: String },

    /// Custom scope
    Custom { scope_name: String },
}

/// Security level for subscriptions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecurityLevel {
    /// Low security (public events)
    Low,

    /// Normal security (standard events)
    Normal,

    /// High security (sensitive events)
    High,

    /// Critical security (security events)
    Critical,
}

/// Event delivery status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeliveryStatus {
    /// Event is queued for delivery
    Queued {
        queued_at: DateTime<Utc>,
        position: u64,
    },

    /// Event is being delivered
    InFlight {
        started_at: DateTime<Utc>,
        attempt: u32,
    },

    /// Event was successfully delivered
    Delivered {
        delivered_at: DateTime<Utc>,
        total_attempts: u32,
    },

    /// Event delivery failed
    Failed {
        failed_at: DateTime<Utc>,
        error: String,
        total_attempts: u32,
        retry_after: Option<DateTime<Utc>>,
    },

    /// Event was skipped due to filtering or authorization
    Skipped {
        skipped_at: DateTime<Utc>,
        reason: String,
    },
}

/// Event delivery tracking information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventDelivery {
    /// Event ID
    pub event_id: Uuid,

    /// Subscription ID
    pub subscription_id: SubscriptionId,

    /// Delivery status
    pub status: DeliveryStatus,

    /// Delivery attempts
    pub attempts: Vec<DeliveryAttempt>,

    /// Delivery metadata
    pub metadata: HashMap<String, String>,
}

/// Individual delivery attempt
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeliveryAttempt {
    /// Attempt number
    pub attempt: u32,

    /// Attempt timestamp
    pub timestamp: DateTime<Utc>,

    /// Attempt result
    pub result: DeliveryResult,

    /// Attempt duration
    pub duration_ms: u64,
}

/// Delivery attempt result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeliveryResult {
    /// Delivery succeeded
    Success,

    /// Delivery failed with error
    Failed { error: String },

    /// Delivery timed out
    Timeout,

    /// Plugin was unavailable
    Unavailable,

    /// Event was rejected by plugin
    Rejected { reason: String },
}

/// Subscription statistics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubscriptionStats {
    /// Subscription ID
    pub subscription_id: SubscriptionId,

    /// Total events received
    pub events_received: u64,

    /// Total events delivered
    pub events_delivered: u64,

    /// Total events failed
    pub events_failed: u64,

    /// Average delivery time in milliseconds
    pub avg_delivery_time_ms: f64,

    /// Current queue size
    pub queue_size: usize,

    /// Last activity timestamp
    pub last_activity: Option<DateTime<Utc>>,

    /// Performance metrics
    pub performance: PerformanceMetrics,
}

/// Performance metrics for subscriptions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerformanceMetrics {
    /// Events per second
    pub events_per_sec: f64,

    /// Latency percentiles in milliseconds
    pub latency_p50: f64,
    pub latency_p95: f64,
    pub latency_p99: f64,

    /// Error rate percentage
    pub error_rate_percent: f64,

    /// Memory usage in bytes
    pub memory_usage_bytes: u64,

    /// CPU usage percentage
    pub cpu_usage_percent: f64,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            events_per_sec: 0.0,
            latency_p50: 0.0,
            latency_p95: 0.0,
            latency_p99: 0.0,
            error_rate_percent: 0.0,
            memory_usage_bytes: 0,
            cpu_usage_percent: 0.0,
        }
    }
}

impl Default for SubscriptionStats {
    fn default() -> Self {
        Self {
            subscription_id: SubscriptionId::default(),
            events_received: 0,
            events_delivered: 0,
            events_failed: 0,
            avg_delivery_time_ms: 0.0,
            queue_size: 0,
            last_activity: None,
            performance: PerformanceMetrics::default(),
        }
    }
}