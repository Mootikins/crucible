//! Request routing system for Crucible
//!
//! This module provides a request routing and dispatch layer that integrates
//! with the crucible-services crate to handle service discovery, load balancing,
//! and request/response processing.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

use crate::config::ConfigManager;
use crucible_services::*;

/// Router metrics
#[derive(Debug, Clone, Default)]
pub struct RouterMetrics {
    pub requests_processed: u64,
    pub requests_failed: u64,
    pub average_request_time: Duration,
    pub active_requests: u64,
}

/// Request router wrapper around crucible-services router
#[derive(Debug)]
pub struct RequestRouter {
    /// Service router implementation
    router: Arc<dyn ServiceRouter>,
    /// Router metrics
    metrics: Arc<RwLock<RouterMetrics>>,
    /// Request history for metrics calculation
    request_history: Arc<RwLock<Vec<Duration>>>,
    /// Configuration manager
    config_manager: Arc<ConfigManager>,
    /// Event broadcaster
    event_tx: broadcast::Sender<RouterEvent>,
    /// Running state
    running: Arc<RwLock<bool>>,
}

/// Router events
#[derive(Debug, Clone)]
pub enum RouterEvent {
    RequestRouted {
        request_id: uuid::Uuid,
        service_type: String,
        service_instance: String,
    },
    RequestCompleted {
        request_id: uuid::Uuid,
        duration: Duration,
        success: bool,
    },
    ServiceRegistered {
        service_id: uuid::Uuid,
        service_type: String,
    },
    ServiceUnregistered {
        service_id: uuid::Uuid,
    },
}

impl RequestRouter {
    /// Create a new request router
    pub async fn new(config_manager: Arc<ConfigManager>) -> Result<Self> {
        let config = config_manager.get().await;
        let router = Self::create_service_router(&config)?;

        Ok(Self {
            router: Arc::new(router),
            metrics: Arc::new(RwLock::new(RouterMetrics::default())),
            request_history: Arc::new(RwLock::new(Vec::new())),
            config_manager,
            event_tx: broadcast::channel(1000).0,
            running: Arc::new(RwLock::new(false)),
        })
    }

    /// Create the underlying service router
    fn create_service_router(config: &crate::config::CrucibleConfig) -> Result<impl ServiceRouter> {
        // Use service builder to create router with appropriate configuration
        let load_balancer = LoadBalancerFactory::create(LoadBalancingStrategy::RoundRobin);
        let registry = ServiceDiscoveryFactory::create_registry();

        let router = ServiceBuilder::new()
            .with_registry(registry)
            .with_load_balancer(load_balancer)
            .with_timeout(config.network.http.request_timeout.as_millis() as u64)
            .with_logging(LoggingConfig {
                log_requests: true,
                log_responses: true,
                log_errors: true,
                log_payloads: false,
            })
            .build()?;

        Ok(router)
    }

    /// Start the router
    pub async fn start(&self) -> Result<()> {
        let mut running = self.running.write().await;
        if *running {
            warn!("Router is already running");
            return Ok(());
        }

        *running = true;
        info!("Request router started");

        // Start metrics cleanup task
        self.start_metrics_cleanup();

        Ok(())
    }

    /// Stop the router
    pub async fn stop(&self) -> Result<()> {
        let mut running = self.running.write().await;
        if !*running {
            warn!("Router is not running");
            return Ok(());
        }

        *running = false;
        info!("Request router stopped");

        Ok(())
    }

    /// Route a request to the appropriate service
    pub async fn route_request(&self, request: ServiceRequest) -> Result<ServiceResponse> {
        let running = *self.running.read().await;
        if !running {
            return Err(anyhow::anyhow!("Router is not running"));
        }

        let request_id = request.request_id;
        let service_type = request.service_type.clone();
        let start_time = Instant::now();

        // Log request start
        let event = RouterEvent::RequestRouted {
            request_id,
            service_type: format!("{:?}", service_type),
            service_instance: request.service_instance.clone().unwrap_or_default(),
        };
        let _ = self.event_tx.send(event);

        // Route the request
        let result = self.router.route_request(request).await;
        let duration = start_time.elapsed();

        // Update metrics
        self.update_metrics(duration, result.is_ok()).await;

        // Log completion
        let event = RouterEvent::RequestCompleted {
            request_id,
            duration,
            success: result.is_ok(),
        };
        let _ = self.event_tx.send(event);

        debug!("Request {} completed in {:?} (success: {})", request_id, duration, result.is_ok());

        result.map_err(|e| anyhow::anyhow!("Request routing failed: {}", e))
    }

    /// Register a service
    pub async fn register_service(
        &self,
        service_info: ServiceInfo,
        service: Arc<dyn BaseService>,
    ) -> Result<()> {
        let service_id = service_info.id;
        let service_type = format!("{:?}", service_info.service_type);

        self.router.register_service(service_info, service).await
            .map_err(|e| anyhow::anyhow!("Failed to register service: {}", e))?;

        info!("Registered service: {} ({})", service_id, service_type);

        let event = RouterEvent::ServiceRegistered { service_id, service_type };
        let _ = self.event_tx.send(event);

        Ok(())
    }

    /// Unregister a service
    pub async fn unregister_service(&self, service_id: uuid::Uuid) -> Result<bool> {
        let result = self.router.unregister_service(service_id).await
            .map_err(|e| anyhow::anyhow!("Failed to unregister service: {}", e))?;

        if result {
            info!("Unregistered service: {}", service_id);

            let event = RouterEvent::ServiceUnregistered { service_id };
            let _ = self.event_tx.send(event);
        }

        Ok(result)
    }

    /// Get available services
    pub async fn get_available_services(&self, service_type: ServiceType) -> Result<Vec<ServiceInfo>> {
        self.router.get_available_services(service_type).await
            .map_err(|e| anyhow::anyhow!("Failed to get available services: {}", e))
    }

    /// Get router metrics
    pub async fn get_metrics(&self) -> RouterMetrics {
        self.metrics.read().await.clone()
    }

    /// Perform health check
    pub async fn health_check(&self) -> Result<bool> {
        let running = *self.running.read().await;
        if !running {
            return Ok(false);
        }

        // Check router stats
        let stats = self.router.get_router_stats().await
            .map_err(|e| {
                error!("Router health check failed: {}", e);
                e
            })?;

        // Consider router healthy if it has registered services or is in startup phase
        Ok(stats.total_services > 0 || stats.total_requests == 0)
    }

    /// Subscribe to router events
    pub fn subscribe_events(&self) -> broadcast::Receiver<RouterEvent> {
        self.event_tx.subscribe()
    }

    /// Update router metrics
    async fn update_metrics(&self, duration: Duration, success: bool) {
        let mut metrics = self.metrics.write().await;
        let mut history = self.request_history.write().await;

        // Update counters
        if success {
            metrics.requests_processed += 1;
        } else {
            metrics.requests_failed += 1;
        }

        // Update request history for average calculation
        history.push(duration);

        // Keep only last 1000 requests for average calculation
        if history.len() > 1000 {
            history.remove(0);
        }

        // Update average
        if !history.is_empty() {
            let total: Duration = history.iter().sum();
            metrics.average_request_time = total / history.len() as u32;
        }
    }

    /// Start metrics cleanup task
    fn start_metrics_cleanup(&self) {
        let history = self.request_history.clone();
        let metrics = self.metrics.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60)); // Cleanup every minute

            loop {
                interval.tick().await;

                // Trim history to last 1000 entries
                let mut history = history.write().await;
                if history.len() > 1000 {
                    history.truncate(1000);
                }

                // Reset metrics if needed
                let mut metrics = metrics.write().await;
                if metrics.requests_processed > 1_000_000 {
                    // Reset counters to prevent overflow
                    metrics.requests_processed = 0;
                    metrics.requests_failed = 0;
                }
            }
        });
    }

    /// Create a simple service request
    pub fn create_request(
        service_type: ServiceType,
        method: String,
        payload: serde_json::Value,
    ) -> ServiceRequest {
        ServiceRequest {
            request_id: uuid::Uuid::new_v4(),
            service_type,
            service_instance: None,
            method,
            payload,
            metadata: RequestMetadata {
                user_id: None,
                session_id: None,
                request_source: "crucible-core".to_string(),
                correlation_id: Some(uuid::Uuid::new_v4().to_string()),
                timestamp: chrono::Utc::now(),
                tags: HashMap::new(),
            },
            timeout_ms: Some(30000),
        }
    }

    /// Create a tool execution request
    pub fn create_tool_request(
        tool_name: String,
        parameters: serde_json::Value,
    ) -> ServiceRequest {
        Self::create_request(
            ServiceType::Tool,
            "execute".to_string(),
            serde_json::json!({
                "tool": tool_name,
                "parameters": parameters
            }),
        )
    }

    /// Create a database query request
    pub fn create_database_request(
        query: String,
        parameters: Option<Vec<serde_json::Value>>,
    ) -> ServiceRequest {
        Self::create_request(
            ServiceType::Database,
            "query".to_string(),
            serde_json::json!({
                "query": query,
                "parameters": parameters
            }),
        )
    }

    /// Create an LLM request
    pub fn create_llm_request(
        prompt: String,
        model: Option<String>,
        parameters: Option<serde_json::Value>,
    ) -> ServiceRequest {
        let mut payload = serde_json::json!({
            "prompt": prompt
        });

        if let Some(model) = model {
            payload["model"] = serde_json::Value::String(model);
        }

        if let Some(params) = parameters {
            payload["parameters"] = params;
        }

        Self::create_request(
            ServiceType::LLM,
            "generate".to_string(),
            payload,
        )
    }
}

/// Service registration builder
pub struct ServiceRegistrationBuilder {
    service_info: ServiceInfo,
}

impl ServiceRegistrationBuilder {
    /// Create a new service registration builder
    pub fn new(name: String, service_type: ServiceType) -> Self {
        Self {
            service_info: ServiceInfo {
                id: uuid::Uuid::new_v4(),
                name,
                service_type,
                version: "1.0.0".to_string(),
                description: None,
                status: ServiceStatus::Starting,
                capabilities: Vec::new(),
                config_schema: None,
                metadata: HashMap::new(),
            },
        }
    }

    /// Set service version
    pub fn with_version(mut self, version: String) -> Self {
        self.service_info.version = version;
        self
    }

    /// Set service description
    pub fn with_description(mut self, description: String) -> Self {
        self.service_info.description = Some(description);
        self
    }

    /// Add service capability
    pub fn with_capability(mut self, capability: String) -> Self {
        self.service_info.capabilities.push(capability);
        self
    }

    /// Add service metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.service_info.metadata.insert(key, value);
        self
    }

    /// Set service status
    pub fn with_status(mut self, status: ServiceStatus) -> Self {
        self.service_info.status = status;
        self
    }

    /// Build service info
    pub fn build(self) -> ServiceInfo {
        self.service_info
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CrucibleConfig, ServiceConfig};
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_router_creation() {
        let config = Arc::new(ConfigManager::new().await.unwrap());
        let router = RequestRouter::new(config.clone()).await;

        assert!(router.is_ok());
    }

    #[tokio::test]
    async fn test_router_lifecycle() {
        let config = Arc::new(ConfigManager::new().await.unwrap());
        let router = RequestRouter::new(config.clone()).await.unwrap();

        // Start router
        router.start().await.unwrap();
        assert_eq!(*router.running.read().await, true);

        // Stop router
        router.stop().await.unwrap();
        assert_eq!(*router.running.read().await, false);
    }

    #[tokio::test]
    async fn test_request_creation() {
        let tool_request = RequestRouter::create_tool_request(
            "test_tool".to_string(),
            serde_json::json!({"param": "value"}),
        );

        assert_eq!(tool_request.service_type, ServiceType::Tool);
        assert_eq!(tool_request.method, "execute");

        let db_request = RequestRouter::create_database_request(
            "SELECT * FROM test".to_string(),
            None,
        );

        assert_eq!(db_request.service_type, ServiceType::Database);
        assert_eq!(db_request.method, "query");
    }

    #[tokio::test]
    async fn test_service_registration_builder() {
        let service_info = ServiceRegistrationBuilder::new(
            "test-service".to_string(),
            ServiceType::Tool,
        )
        .with_version("2.0.0".to_string())
        .with_description("Test service".to_string())
        .with_capability("test".to_string())
        .with_metadata("key".to_string(), "value".to_string())
        .with_status(ServiceStatus::Healthy)
        .build();

        assert_eq!(service_info.name, "test-service");
        assert_eq!(service_info.version, "2.0.0");
        assert_eq!(service_info.description, Some("Test service".to_string()));
        assert_eq!(service_info.capabilities, vec!["test"]);
        assert_eq!(service_info.metadata.get("key"), Some(&"value".to_string()));
        assert_eq!(service_info.status, ServiceStatus::Healthy);
    }

    #[tokio::test]
    async fn test_metrics_update() {
        let config = Arc::new(ConfigManager::new().await.unwrap());
        let router = RequestRouter::new(config).await.unwrap();

        // Update metrics with successful request
        router.update_metrics(Duration::from_millis(100), true).await;
        let metrics = router.get_metrics().await;
        assert_eq!(metrics.requests_processed, 1);
        assert_eq!(metrics.requests_failed, 0);
        assert_eq!(metrics.average_request_time, Duration::from_millis(100));

        // Update metrics with failed request
        router.update_metrics(Duration::from_millis(200), false).await;
        let metrics = router.get_metrics().await;
        assert_eq!(metrics.requests_processed, 1);
        assert_eq!(metrics.requests_failed, 1);
        assert_eq!(metrics.average_request_time, Duration::from_millis(150)); // Average of 100 and 200
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let config = Arc::new(ConfigManager::new().await.unwrap());
        let router = RequestRouter::new(config).await.unwrap();
        let mut events = router.subscribe_events();

        // Register a service
        let service_info = ServiceRegistrationBuilder::new(
            "test".to_string(),
            ServiceType::Tool,
        ).build();

        let mock_service = Arc::new(crate::router::tests::MockService);
        router.register_service(service_info.clone(), mock_service).await.unwrap();

        // Should receive service registered event
        let event = tokio::time::timeout(Duration::from_millis(100), events.recv())
            .await
            .unwrap()
            .unwrap();

        match event {
            RouterEvent::ServiceRegistered { service_id, service_type } => {
                assert_eq!(service_id, service_info.id);
                assert_eq!(service_type, "Tool");
            }
            _ => panic!("Expected service registered event"),
        }
    }
}

// Mock service for testing
#[cfg(test)]
pub mod tests {
    use super::*;
    use async_trait::async_trait;
    use crucible_services::*;

    pub struct MockService;

    #[async_trait]
    impl BaseService for MockService {
        fn service_info(&self) -> ServiceInfo {
            ServiceRegistrationBuilder::new(
                "mock".to_string(),
                ServiceType::Tool,
            ).build()
        }

        async fn start(&self) -> ServiceResult<()> {
            Ok(())
        }

        async fn stop(&self) -> ServiceResult<()> {
            Ok(())
        }

        async fn health_check(&self) -> ServiceResult<bool> {
            Ok(true)
        }

        async fn handle_request(&self, request: ServiceRequest) -> ServiceResult<ServiceResponse> {
            Ok(ServiceResponse {
                request_id: request.request_id,
                status: ResponseStatus::Success,
                payload: serde_json::json!({"result": "mock_result"}),
                metadata: ResponseMetadata {
                    execution_time_ms: 10,
                    service_version: "1.0.0".to_string(),
                    cache_hit: false,
                    warnings: Vec::new(),
                },
            })
        }

        async fn get_metrics(&self) -> ServiceResult<ServiceMetrics> {
            Ok(ServiceMetrics {
                requests_processed: 0,
                requests_failed: 0,
                average_response_time_ms: 0,
                uptime_seconds: 0,
                memory_usage_mb: 0,
                cpu_usage_percent: 0.0,
            })
        }
    }
}