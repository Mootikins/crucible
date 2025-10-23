//! Simplified request routing system for Crucible
//!
//! This module provides a basic request routing system that doesn't depend on
//! the external crucible-services crate, avoiding cyclic dependencies.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};

use crate::config::ConfigManager;

/// Simple service type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ServiceType {
    Tool,
    Database,
    LLM,
    Config,
    FileSystem,
    Network,
    Custom(String),
}

/// Simple service request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRequest {
    pub request_id: uuid::Uuid,
    pub service_type: ServiceType,
    pub method: String,
    pub payload: serde_json::Value,
    pub timeout_ms: Option<u64>,
}

/// Simple service response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceResponse {
    pub request_id: uuid::Uuid,
    pub success: bool,
    pub payload: serde_json::Value,
    pub error: Option<String>,
}

/// Simple service information
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub id: uuid::Uuid,
    pub name: String,
    pub service_type: ServiceType,
    pub status: ServiceStatus,
}

/// Service status
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceStatus {
    Healthy,
    Unhealthy,
    Starting,
    Stopping,
}

/// Simple service handler trait
#[async_trait::async_trait]
pub trait ServiceHandler: Send + Sync {
    fn service_info(&self) -> ServiceInfo;
    async fn handle_request(&self, request: ServiceRequest) -> Result<ServiceResponse>;
    async fn health_check(&self) -> Result<bool>;
}

/// Router metrics
#[derive(Debug, Clone, Default)]
pub struct RouterMetrics {
    pub requests_processed: u64,
    pub requests_failed: u64,
    pub average_request_time: Duration,
    pub active_requests: u64,
}

/// Simple request router
pub struct SimpleRequestRouter {
    /// Registered services
    services: Arc<RwLock<HashMap<uuid::Uuid, Arc<dyn ServiceHandler>>>>,
    /// Service registry by type
    services_by_type: Arc<RwLock<HashMap<ServiceType, Vec<uuid::Uuid>>>>,
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

impl std::fmt::Debug for SimpleRequestRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimpleRequestRouter")
            .field("services_count", &self.services.try_read().map(|s| s.len()).unwrap_or(0))
            .field("metrics", &self.metrics)
            .field("running", &self.running)
            .finish()
    }
}

impl SimpleRequestRouter {
    /// Create a new simple request router
    pub async fn new(config_manager: Arc<ConfigManager>) -> Result<Self> {
        Ok(Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            services_by_type: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(RouterMetrics::default())),
            request_history: Arc::new(RwLock::new(Vec::new())),
            config_manager,
            event_tx: broadcast::channel(1000).0,
            running: Arc::new(RwLock::new(false)),
        })
    }

    /// Start the router
    pub async fn start(&self) -> Result<()> {
        let mut running = self.running.write().await;
        if *running {
            warn!("Router is already running");
            return Ok(());
        }

        *running = true;
        info!("Simple request router started");

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
        info!("Simple request router stopped");

        Ok(())
    }

    /// Register a service
    pub async fn register_service(&self, handler: Arc<dyn ServiceHandler>) -> Result<()> {
        let service_info = handler.service_info();
        let service_id = service_info.id;
        let service_type = service_info.service_type.clone();

        // Add to services
        self.services.write().await.insert(service_id, handler.clone());

        // Add to services by type
        let mut services_by_type = self.services_by_type.write().await;
        services_by_type.entry(service_type.clone()).or_insert_with(Vec::new).push(service_id);

        info!("Registered service: {} ({})", service_id, service_info.name);

        // Send event
        let event = RouterEvent::ServiceRegistered {
            service_id,
            service_type: format!("{:?}", service_type),
        };
        let _ = self.event_tx.send(event);

        Ok(())
    }

    /// Unregister a service
    pub async fn unregister_service(&self, service_id: uuid::Uuid) -> Result<bool> {
        let service_info = {
            let services = self.services.read().await;
            services.get(&service_id).map(|handler| handler.service_info())
        };

        if let Some(service_info) = service_info {
            let service_type = service_info.service_type;

            // Remove from services
            let removed = self.services.write().await.remove(&service_id).is_some();

            // Remove from services by type
            let mut services_by_type = self.services_by_type.write().await;
            if let Some(services) = services_by_type.get_mut(&service_type) {
                services.retain(|&id| id != service_id);
                if services.is_empty() {
                    services_by_type.remove(&service_type);
                }
            }

            if removed {
                info!("Unregistered service: {}", service_id);

                // Send event
                let event = RouterEvent::ServiceUnregistered { service_id };
                let _ = self.event_tx.send(event);
            }

            Ok(removed)
        } else {
            Ok(false)
        }
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
        };
        let _ = self.event_tx.send(event);

        // Find a service to handle the request
        let handler = {
            let services_by_type = self.services_by_type.read().await;
            if let Some(service_ids) = services_by_type.get(&service_type) {
                if let Some(&service_id) = service_ids.first() {
                    let services = self.services.read().await;
                    services.get(&service_id).cloned()
                } else {
                    None
                }
            } else {
                None
            }
        };

        let result = match handler {
            Some(handler) => {
                // Apply timeout if specified
                if let Some(timeout_ms) = request.timeout_ms {
                    let timeout = Duration::from_millis(timeout_ms);
                    match tokio::time::timeout(timeout, handler.handle_request(request.clone())).await {
                        Ok(result) => result,
                        Err(_) => Ok(ServiceResponse {
                            request_id,
                            success: false,
                            payload: serde_json::Value::Null,
                            error: Some("Request timed out".to_string()),
                        }),
                    }
                } else {
                    handler.handle_request(request.clone()).await
                }
            }
            None => Ok(ServiceResponse {
                request_id,
                success: false,
                payload: serde_json::Value::Null,
                error: Some(format!("No service found for type: {:?}", service_type)),
            }),
        };

        let duration = start_time.elapsed();
        let success = result.as_ref().map_or(false, |r| r.success);

        // Update metrics
        self.update_metrics(duration, success).await;

        // Log completion
        let event = RouterEvent::RequestCompleted {
            request_id,
            duration,
            success,
        };
        let _ = self.event_tx.send(event);

        debug!("Request {} completed in {:?} (success: {})", request_id, duration, success);

        result
    }

    /// Get available services
    pub async fn get_available_services(&self, service_type: ServiceType) -> Vec<ServiceInfo> {
        let services_by_type = self.services_by_type.read().await;
        let services = self.services.read().await;

        if let Some(service_ids) = services_by_type.get(&service_type) {
            service_ids.iter()
                .filter_map(|&id| services.get(&id).map(|handler| handler.service_info()))
                .collect()
        } else {
            Vec::new()
        }
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

        // Check if we have any registered services
        let services = self.services.read().await;
        Ok(!services.is_empty())
    }

    /// Subscribe to router events
    pub fn subscribe_events(&self) -> broadcast::Receiver<RouterEvent> {
        self.event_tx.subscribe()
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
            method,
            payload,
            timeout_ms: Some(30000),
        }
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
}

/// Mock service handler for testing
#[cfg(test)]
pub struct MockServiceHandler {
    service_info: ServiceInfo,
}

#[cfg(test)]
impl MockServiceHandler {
    pub fn new(name: String, service_type: ServiceType) -> Self {
        Self {
            service_info: ServiceInfo {
                id: uuid::Uuid::new_v4(),
                name,
                service_type,
                status: ServiceStatus::Healthy,
            },
        }
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl ServiceHandler for MockServiceHandler {
    fn service_info(&self) -> ServiceInfo {
        self.service_info.clone()
    }

    async fn handle_request(&self, request: ServiceRequest) -> Result<ServiceResponse> {
        Ok(ServiceResponse {
            request_id: request.request_id,
            success: true,
            payload: serde_json::json!({"result": "mock_response"}),
            error: None,
        })
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simple_router_creation() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let router = SimpleRequestRouter::new(config_manager).await;

        assert!(router.is_ok());
    }

    #[tokio::test]
    async fn test_service_registration() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let router = SimpleRequestRouter::new(config_manager).await.unwrap();

        let mock_service = Arc::new(MockServiceHandler::new(
            "test-service".to_string(),
            ServiceType::Tool,
        ));

        router.register_service(mock_service).await.unwrap();

        let services = router.get_available_services(ServiceType::Tool).await;
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "test-service");
    }

    #[tokio::test]
    async fn test_request_routing() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let router = SimpleRequestRouter::new(config_manager).await.unwrap();
        router.start().await.unwrap();

        let mock_service = Arc::new(MockServiceHandler::new(
            "test-service".to_string(),
            ServiceType::Tool,
        ));

        router.register_service(mock_service).await.unwrap();

        let request = SimpleRequestRouter::create_request(
            ServiceType::Tool,
            "test".to_string(),
            serde_json::json!({"param": "value"}),
        );

        let response = router.route_request(request).await.unwrap();
        assert!(response.success);
    }

    #[tokio::test]
    async fn test_router_lifecycle() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let router = SimpleRequestRouter::new(config_manager).await.unwrap();

        // Start router
        router.start().await.unwrap();
        assert_eq!(*router.running.read().await, true);

        // Stop router
        router.stop().await.unwrap();
        assert_eq!(*router.running.read().await, false);
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let router = SimpleRequestRouter::new(config_manager).await.unwrap();
        let mut events = router.subscribe_events();

        let mock_service = Arc::new(MockServiceHandler::new(
            "test-service".to_string(),
            ServiceType::Tool,
        ));

        // Register service
        router.register_service(mock_service).await.unwrap();

        // Should receive service registered event
        let event = tokio::time::timeout(Duration::from_millis(100), events.recv())
            .await
            .unwrap()
            .unwrap();

        match event {
            RouterEvent::ServiceRegistered { service_id, service_type } => {
                assert_ne!(service_id, uuid::Uuid::nil());
                assert_eq!(service_type, "Tool");
            }
            _ => panic!("Expected service registered event"),
        }
    }
}