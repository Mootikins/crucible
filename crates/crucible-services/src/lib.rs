//! # Crucible Services
//!
//! This crate provides a comprehensive service abstraction layer for the Crucible knowledge management system.
//! It enables clean separation of concerns and provides a unified interface for different types of services.
//!
//! ## Features
//!
//! - **Service Traits**: Abstract interfaces for Tool, Database, LLM, and Configuration services
//! - **Request Routing**: Intelligent service discovery and load balancing
//! - **Circuit Breaker**: Fault tolerance with automatic failover
//! - **Middleware**: Request/response processing pipeline
//! - **Service Discovery**: Dynamic service registration and discovery
//! - **Error Handling**: Comprehensive error types and context
//! - **Metrics**: Built-in performance monitoring and statistics
//!
//! ## Architecture
//!
//! The service layer is organized around several key components:
//!
//! - **Service Traits**: Define interfaces for different service types
//! - **Service Router**: Handles request routing and load balancing
//! - **Service Registry**: Manages service registration and discovery
//! - **Middleware Pipeline**: Processes requests and responses
//! - **Circuit Breakers**: Provides fault tolerance
//!
//! ## Quick Start
//!
//! ```rust
//! use crucible_services::*;
//! use std::sync::Arc;
//!
//! // Create a service registry
//! let registry = ServiceDiscoveryFactory::create_registry();
//!
//! // Create a load balancer
//! let load_balancer = LoadBalancerFactory::create(LoadBalancingStrategy::RoundRobin);
//!
//! // Create a service router
//! let mut router = DefaultServiceRouter::new(registry, load_balancer);
//!
//! // Add middleware
//! router.add_middleware(Arc::new(LoggingMiddleware::new(LoggingConfig::default())));
//! router.add_middleware(Arc::new(AuthenticationMiddleware::new(Arc::new(MockAuthService))));
//!
//! // Register services
//! // ... (service implementation details)
//!
//! // Route requests
//! let request = ServiceRequest {
//!     request_id: uuid::Uuid::new_v4(),
//!     service_type: ServiceType::Tool,
//!     service_instance: None,
//!     method: "execute".to_string(),
//!     payload: serde_json::json!({"tool": "test"}),
//!     metadata: RequestMetadata::default(),
//!     timeout_ms: Some(5000),
//! };
//!
//! let response = router.route_request(request).await?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use async_trait::async_trait;

// Export core modules
pub mod errors;
pub mod traits;
pub mod types;
pub mod router;

// Re-export main components for easier access
pub use errors::*;
pub use traits::*;
pub use types::*;
pub use router::*;

// Re-export factory functions
pub use router::load_balancer::LoadBalancerFactory;
pub use router::discovery::ServiceDiscoveryFactory;
pub use router::circuit_breaker::CircuitBreakerFactory;
pub use router::middleware::*;

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default configurations
pub mod defaults {
    use super::*;

    /// Default request timeout in milliseconds
    pub const DEFAULT_TIMEOUT_MS: u64 = 30000;

    /// Default cache TTL in seconds
    pub const DEFAULT_CACHE_TTL_SECONDS: u64 = 300;

    /// Default health check interval in seconds
    pub const DEFAULT_HEALTH_CHECK_INTERVAL_SECONDS: u64 = 60;

    /// Default circuit breaker failure threshold
    pub const DEFAULT_CIRCUIT_BREAKER_FAILURE_THRESHOLD: u32 = 5;

    /// Default circuit breaker timeout in seconds
    pub const DEFAULT_CIRCUIT_BREAKER_TIMEOUT_SECONDS: u64 = 60;

    /// Default rate limit requests per minute
    pub const DEFAULT_RATE_LIMIT_RPM: u32 = 100;
}

/// Service builder for convenient service setup
pub struct ServiceBuilder {
    registry: Option<Arc<dyn ServiceRegistry>>,
    load_balancer: Option<Arc<dyn LoadBalancer>>,
    timeout_ms: u64,
    middlewares: Vec<Arc<dyn ServiceMiddleware>>,
}

impl ServiceBuilder {
    /// Create a new service builder
    pub fn new() -> Self {
        Self {
            registry: None,
            load_balancer: None,
            timeout_ms: defaults::DEFAULT_TIMEOUT_MS,
            middlewares: Vec::new(),
        }
    }

    /// Set the service registry
    pub fn with_registry(mut self, registry: Arc<dyn ServiceRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Set the load balancer
    pub fn with_load_balancer(mut self, load_balancer: Arc<dyn LoadBalancer>) -> Self {
        self.load_balancer = Some(load_balancer);
        self
    }

    /// Set the request timeout
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Add middleware
    pub fn with_middleware(mut self, middleware: Arc<dyn ServiceMiddleware>) -> Self {
        self.middlewares.push(middleware);
        self
    }

    /// Add authentication middleware
    pub fn with_auth(self, auth_service: Arc<dyn AuthService>) -> Self {
        self.with_middleware(Arc::new(AuthenticationMiddleware::new(auth_service)))
    }

    /// Add logging middleware
    pub fn with_logging(self, config: LoggingConfig) -> Self {
        self.with_middleware(Arc::new(LoggingMiddleware::new(config)))
    }

    /// Add rate limiting middleware
    pub fn with_rate_limiting(self, rate_limiter: Arc<dyn ServiceRateLimiter>) -> Self {
        self.with_middleware(Arc::new(RateLimitMiddleware::new(rate_limiter)))
    }

    /// Add metrics middleware
    pub fn with_metrics(self, metrics_collector: Arc<dyn MetricsCollector>) -> Self {
        self.with_middleware(Arc::new(MetricsMiddleware::new(metrics_collector)))
    }

    /// Build the service router
    pub fn build(self) -> Result<Arc<dyn ServiceRouter>, crate::errors::ServiceError> {
        let registry = self.registry
            .ok_or_else(|| crate::errors::ServiceError::configuration_error("Service registry is required"))?;

        let load_balancer = self.load_balancer
            .ok_or_else(|| crate::errors::ServiceError::configuration_error("Load balancer is required"))?;

        let mut router = DefaultServiceRouter::new(registry, load_balancer)
            .with_default_timeout(self.timeout_ms);

        // Add all middleware
        for middleware in self.middlewares {
            router.add_middleware(middleware);
        }

        Ok(Arc::new(router))
    }
}

impl Default for ServiceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience functions for common setups
pub mod presets {
    use super::*;

    /// Create a basic service router with logging
    pub fn basic() -> Result<Arc<dyn ServiceRouter>, ServiceError> {
        ServiceBuilder::new()
            .with_registry(ServiceDiscoveryFactory::create_registry())
            .with_load_balancer(LoadBalancerFactory::create(LoadBalancingStrategy::RoundRobin))
            .with_logging(LoggingConfig::default())
            .build()
    }

    /// Create a production-ready service router with all middleware
    pub fn production(
        auth_service: Arc<dyn AuthService>,
        rate_limiter: Arc<dyn ServiceRateLimiter>,
        metrics_collector: Arc<dyn MetricsCollector>,
    ) -> Result<Arc<dyn ServiceRouter>, ServiceError> {
        ServiceBuilder::new()
            .with_registry(ServiceDiscoveryFactory::create_registry())
            .with_load_balancer(LoadBalancerFactory::create(LoadBalancingStrategy::WeightedRoundRobin))
            .with_auth(auth_service)
            .with_logging(LoggingConfig::default())
            .with_rate_limiting(rate_limiter)
            .with_metrics(metrics_collector)
            .with_timeout(30000)
            .build()
    }

    /// Create a development service router with minimal middleware
    pub fn development() -> Result<Arc<dyn ServiceRouter>, ServiceError> {
        ServiceBuilder::new()
            .with_registry(ServiceDiscoveryFactory::create_registry())
            .with_load_balancer(LoadBalancerFactory::create(LoadBalancingStrategy::Random))
            .with_logging(LoggingConfig {
                log_requests: true,
                log_responses: true,
                log_errors: true,
                log_payloads: true, // More verbose in development
            })
            .with_timeout(60000) // Longer timeout for development
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_service_builder() {
        let registry = ServiceDiscoveryFactory::create_registry();
        let load_balancer = LoadBalancerFactory::create(LoadBalancingStrategy::RoundRobin);

        let builder = ServiceBuilder::new()
            .with_registry(registry.clone())
            .with_load_balancer(load_balancer.clone())
            .with_timeout(5000);

        assert!(builder.build().is_ok());
    }

    #[test]
    fn test_presets() {
        // Test basic preset
        assert!(presets::basic().is_ok());

        // Test development preset
        assert!(presets::development().is_ok());

        // Test production preset (requires mocked dependencies)
        let auth_service = Arc::new(MockAuthService);
        let rate_limiter = Arc::new(crate::router::middleware::MockRateLimiter);
        let metrics_collector = Arc::new(crate::router::middleware::MockMetricsCollector);

        assert!(presets::production(auth_service, rate_limiter, metrics_collector).is_ok());
    }
}

/// Mock rate limiter for testing
pub struct MockRateLimiter;

#[async_trait]
impl ServiceRateLimiter for MockRateLimiter {
    async fn is_allowed(&self, _key: &str, _limit: u32, _window_seconds: u32) -> ServiceResult<bool> {
        Ok(true)
    }

    async fn get_usage(&self, _key: &str, _window_seconds: u32) -> ServiceResult<RateLimitUsage> {
        Ok(RateLimitUsage {
            current_count: 0,
            limit: 100,
            remaining: 100,
            reset_time_seconds: 60,
        })
    }

    async fn reset(&self, _key: &str) -> ServiceResult<()> {
        Ok(())
    }
}

/// Mock metrics collector for testing
pub struct MockMetricsCollector;

#[async_trait]
impl MetricsCollector for MockMetricsCollector {
    async fn record_request_duration(&self, _service_id: &str, _duration_ms: u64) {
        // Mock implementation
    }

    async fn record_error(&self, _service_type: &str, _error: &str) {
        // Mock implementation
    }

    async fn get_service_metrics(&self, _service_id: &str) -> Option<router::middleware::ServicePerformanceMetrics> {
        None
    }
}

/// Mock implementations for testing
#[cfg(test)]
pub mod mocks {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    // Mock implementations are included in their respective modules
    // This namespace can be used to organize additional test utilities
}

#[cfg(all(test, feature = "integration_tests"))]
pub mod integration_tests {
    //! Integration tests for the service layer
    //!
    //! These tests require external dependencies and can be run with:
    //! ```bash
    //! cargo test --features integration_tests
    //! ```

    use super::*;

    #[tokio::test]
    async fn test_end_to_end_service_routing() {
        // This would test the complete service routing flow
        // with actual service implementations
    }

    #[tokio::test]
    async fn test_service_discovery_integration() {
        // Test service discovery with real backends
    }

    #[tokio::test]
    async fn test_load_balancing_strategies() {
        // Compare different load balancing strategies
    }
}