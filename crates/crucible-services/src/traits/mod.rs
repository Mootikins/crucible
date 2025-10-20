use async_trait::async_trait;
use crate::errors::{ServiceError, ServiceResult};
use crate::types::*;
use std::collections::HashMap;
use uuid::Uuid;

pub mod tool;
pub mod database;
pub mod llm;
pub mod config;

// Re-export all service traits
pub use tool::ToolService;
pub use database::DatabaseService;
pub use llm::LLMService;
pub use config::ConfigService;

/// Base service trait that all services must implement
#[async_trait]
pub trait BaseService: Send + Sync {
    /// Get service information
    async fn info(&self) -> ServiceResult<ServiceInfo>;

    /// Check service health
    async fn health_check(&self) -> ServiceResult<ServiceHealth>;

    /// Start the service
    async fn start(&self) -> ServiceResult<()>;

    /// Stop the service
    async fn stop(&self) -> ServiceResult<()>;

    /// Get service dependencies
    async fn dependencies(&self) -> ServiceResult<Vec<ServiceDependency>>;

    /// Handle generic service request
    async fn handle_request(&self, request: ServiceRequest) -> ServiceResult<ServiceResponse>;

    /// Get service metrics
    async fn metrics(&self) -> ServiceResult<ServiceMetrics>;
}

/// Service registry trait for managing service instances
#[async_trait]
pub trait ServiceRegistry: Send + Sync {
    /// Register a new service
    async fn register_service(&self, service_info: ServiceInfo) -> ServiceResult<()>;

    /// Unregister a service
    async fn unregister_service(&self, service_id: Uuid) -> ServiceResult<()>;

    /// Get service information by ID
    async fn get_service(&self, service_id: Uuid) -> ServiceResult<Option<ServiceInfo>>;

    /// List all services
    async fn list_services(&self) -> ServiceResult<Vec<ServiceInfo>>;

    /// List services by type
    async fn list_services_by_type(&self, service_type: ServiceType) -> ServiceResult<Vec<ServiceInfo>>;

    /// Find services by capability
    async fn find_services_by_capability(&self, capability: &str) -> ServiceResult<Vec<ServiceInfo>>;

    /// Update service status
    async fn update_service_status(&self, service_id: Uuid, status: ServiceStatus) -> ServiceResult<()>;

    /// Get service health
    async fn get_service_health(&self, service_id: Uuid) -> ServiceResult<Option<ServiceHealth>>;

    /// List healthy services
    async fn list_healthy_services(&self) -> ServiceResult<Vec<ServiceInfo>>;

    /// List unhealthy services
    async fn list_unhealthy_services(&self) -> ServiceResult<Vec<ServiceInfo>>;
}

/// Service factory trait for creating service instances
#[async_trait]
pub trait ServiceFactory: Send + Sync {
    /// Create a new service instance
    async fn create_service(&self, service_type: ServiceType, config: ServiceConfig) -> ServiceResult<Box<dyn BaseService>>;

    /// Get supported service types
    fn supported_types(&self) -> Vec<ServiceType>;

    /// Validate service configuration
    fn validate_config(&self, service_type: ServiceType, config: &ServiceConfig) -> ServiceResult<()>;
}

/// Service discovery trait for finding and connecting to services
#[async_trait]
pub trait ServiceDiscovery: Send + Sync {
    /// Discover services of a given type
    async fn discover_services(&self, service_type: ServiceType) -> ServiceResult<Vec<ServiceInfo>>;

    /// Watch for service changes
    async fn watch_services(&self, service_type: ServiceType) -> ServiceResult<Box<dyn ServiceWatcher>>;

    /// Resolve service endpoint
    async fn resolve_endpoint(&self, service_id: Uuid) -> ServiceResult<String>;

    /// Get service load information
    async fn get_service_load(&self, service_id: Uuid) -> ServiceResult<ServiceLoad>;
}

/// Service watcher trait for monitoring service changes
#[async_trait]
pub trait ServiceWatcher: Send + Sync {
    /// Get the next service change event
    async fn next_change(&mut self) -> ServiceResult<ServiceChangeEvent>;

    /// Stop watching
    async fn stop(&self) -> ServiceResult<()>;
}

/// Service change event
#[derive(Debug, Clone)]
pub enum ServiceChangeEvent {
    /// Service added
    Added(ServiceInfo),
    /// Service removed
    Removed(Uuid),
    /// Service updated
    Updated(ServiceInfo),
    /// Service health changed
    HealthChanged { service_id: Uuid, health: ServiceHealth },
}

/// Service load information
#[derive(Debug, Clone)]
pub struct ServiceLoad {
    /// Service identifier
    pub service_id: Uuid,
    /// Current request count
    pub current_requests: u32,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// CPU usage percentage
    pub cpu_usage_percent: f64,
    /// Memory usage percentage
    pub memory_usage_percent: f64,
    /// Load score (0.0 to 1.0)
    pub load_score: f64,
}

/// Service lifecycle manager trait
#[async_trait]
pub trait ServiceLifecycleManager: Send + Sync {
    /// Start all services
    async fn start_all(&self) -> ServiceResult<()>;

    /// Stop all services
    async fn stop_all(&self) -> ServiceResult<()>;

    /// Restart a service
    async fn restart_service(&self, service_id: Uuid) -> ServiceResult<()>;

    /// Scale a service (if supported)
    async fn scale_service(&self, service_id: Uuid, replicas: u32) -> ServiceResult<()>;

    /// Get service status
    async fn get_service_status(&self, service_id: Uuid) -> ServiceResult<ServiceStatus>;

    /// Get system status
    async fn get_system_status(&self) -> ServiceResult<SystemStatus>;
}

/// System status information
#[derive(Debug, Clone)]
pub struct SystemStatus {
    /// Overall system health
    pub overall_health: ServiceStatus,
    /// Total services
    pub total_services: usize,
    /// Healthy services
    pub healthy_services: usize,
    /// Unhealthy services
    pub unhealthy_services: usize,
    /// System uptime in seconds
    pub uptime_seconds: u64,
    /// System metrics
    pub metrics: HashMap<String, f64>,
}

/// Service middleware trait for request/response processing
#[async_trait]
pub trait ServiceMiddleware: Send + Sync {
    /// Process incoming request
    async fn process_request(&self, request: ServiceRequest) -> ServiceResult<ServiceRequest>;

    /// Process outgoing response
    async fn process_response(&self, response: ServiceResponse) -> ServiceResult<ServiceResponse>;

    /// Handle errors
    async fn handle_error(&self, error: ServiceError, request: &ServiceRequest) -> ServiceResult<ServiceResponse>;
}

/// Service circuit breaker trait for fault tolerance
#[async_trait]
pub trait ServiceCircuitBreaker: Send + Sync {
    /// Execute operation with circuit breaker protection
    async fn execute<F, T>(&self, operation: F) -> ServiceResult<T>
    where
        F: std::future::Future<Output = ServiceResult<T>> + Send;

    /// Get circuit breaker state
    async fn state(&self) -> CircuitBreakerState;

    /// Reset circuit breaker
    async fn reset(&self) -> ServiceResult<()>;

    /// Get circuit breaker metrics
    async fn metrics(&self) -> CircuitBreakerMetrics;
}

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitBreakerState {
    /// Circuit is closed (normal operation)
    Closed,
    /// Circuit is open (rejecting requests)
    Open,
    /// Circuit is half-open (testing recovery)
    HalfOpen,
}

/// Circuit breaker metrics
#[derive(Debug, Clone)]
pub struct CircuitBreakerMetrics {
    /// Total requests
    pub total_requests: u64,
    /// Successful requests
    pub successful_requests: u64,
    /// Failed requests
    pub failed_requests: u64,
    /// Current failure rate
    pub failure_rate: f64,
    /// Time until next state change (milliseconds)
    pub time_to_next_state_ms: Option<u64>,
}

/// Service rate limiter trait
#[async_trait]
pub trait ServiceRateLimiter: Send + Sync {
    /// Check if request is allowed
    async fn is_allowed(&self, key: &str, limit: u32, window_seconds: u32) -> ServiceResult<bool>;

    /// Get current usage
    async fn get_usage(&self, key: &str, window_seconds: u32) -> ServiceResult<RateLimitUsage>;

    /// Reset rate limit for a key
    async fn reset(&self, key: &str) -> ServiceResult<()>;
}

/// Rate limit usage information
#[derive(Debug, Clone)]
pub struct RateLimitUsage {
    /// Current usage count
    pub current_count: u32,
    /// Maximum allowed count
    pub limit: u32,
    /// Remaining requests
    pub remaining: u32,
    /// Time until reset (seconds)
    pub reset_time_seconds: u32,
}

/// Service cache trait for caching responses
#[async_trait]
pub trait ServiceCache: Send + Sync {
    /// Get cached value
    async fn get(&self, key: &str) -> ServiceResult<Option<serde_json::Value>>;

    /// Set cached value with TTL
    async fn set(&self, key: &str, value: serde_json::Value, ttl_seconds: u32) -> ServiceResult<()>;

    /// Delete cached value
    async fn delete(&self, key: &str) -> ServiceResult<bool>;

    /// Clear all cache entries
    async fn clear(&self) -> ServiceResult<()>;

    /// Get cache statistics
    async fn stats(&self) -> ServiceResult<CacheStats>;
}

/// Mock service cache for testing
pub struct MockServiceCache;

#[async_trait]
impl ServiceCache for MockServiceCache {
    async fn get(&self, _key: &str) -> ServiceResult<Option<serde_json::Value>> {
        Ok(None)
    }

    async fn set(&self, _key: &str, _value: serde_json::Value, _ttl_seconds: u32) -> ServiceResult<()> {
        Ok(())
    }

    async fn delete(&self, _key: &str) -> ServiceResult<bool> {
        Ok(false)
    }

    async fn clear(&self) -> ServiceResult<()> {
        Ok(())
    }

    async fn stats(&self) -> ServiceResult<CacheStats> {
        Ok(CacheStats {
            total_entries: 0,
            hits: 0,
            misses: 0,
            hit_rate: 0.0,
            memory_usage_bytes: 0,
        })
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Total cache entries
    pub total_entries: u64,
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
    /// Hit rate
    pub hit_rate: f64,
    /// Memory usage in bytes
    pub memory_usage_bytes: u64,
}