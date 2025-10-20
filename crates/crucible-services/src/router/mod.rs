use async_trait::async_trait;
use crate::errors::{ServiceError, ServiceResult};
use crate::types::*;
use crate::traits::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub mod load_balancer;
pub mod middleware;
pub mod circuit_breaker;
pub mod discovery;

pub use load_balancer::*;
pub use middleware::*;
pub use circuit_breaker::*;
pub use discovery::*;

/// Request router for service discovery and dispatch
#[async_trait]
pub trait ServiceRouter: Send + Sync {
    /// Route a service request to appropriate service
    async fn route_request(&self, request: ServiceRequest) -> ServiceResult<ServiceResponse>;

    /// Register a service instance
    async fn register_service(&self, service_info: ServiceInfo, service: Arc<dyn BaseService>) -> ServiceResult<()>;

    /// Unregister a service instance
    async fn unregister_service(&self, service_id: Uuid) -> ServiceResult<bool>;

    /// Get available services
    async fn get_available_services(&self, service_type: ServiceType) -> ServiceResult<Vec<ServiceInfo>>;

    /// Get service load information
    async fn get_service_load(&self, service_id: Uuid) -> ServiceResult<ServiceLoad>;

    /// Enable/disable service
    async fn set_service_enabled(&self, service_id: Uuid, enabled: bool) -> ServiceResult<bool>;

    /// Add middleware to the router
    fn add_middleware(&mut self, middleware: Arc<dyn ServiceMiddleware>);

    /// Set load balancing strategy
    fn set_load_balancer(&mut self, load_balancer: Arc<dyn LoadBalancer>);

    /// Get router statistics
    async fn get_router_stats(&self) -> ServiceResult<RouterStats>;
}

/// Default implementation of service router
pub struct DefaultServiceRouter {
    /// Registered services by type
    services: Arc<RwLock<HashMap<ServiceType, Vec<ServiceInstance>>>>,
    /// Service registry
    registry: Arc<dyn ServiceRegistry>,
    /// Load balancer
    load_balancer: Arc<dyn LoadBalancer>,
    /// Middleware chain
    middleware: Arc<RwLock<Vec<Arc<dyn ServiceMiddleware>>>>,
    /// Circuit breakers by service
    circuit_breakers: Arc<RwLock<HashMap<Uuid, Arc<dyn ServiceCircuitBreaker>>>>,
    /// Request timeout in milliseconds
    default_timeout_ms: u64,
    /// Router statistics
    stats: Arc<RwLock<RouterStats>>,
}

impl DefaultServiceRouter {
    /// Create a new service router
    pub fn new(
        registry: Arc<dyn ServiceRegistry>,
        load_balancer: Arc<dyn LoadBalancer>,
    ) -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            registry,
            load_balancer,
            middleware: Arc::new(RwLock::new(Vec::new())),
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            default_timeout_ms: 30000, // 30 seconds default
            stats: Arc::new(RwLock::new(RouterStats::default())),
        }
    }

    /// Set default timeout
    pub fn with_default_timeout(mut self, timeout_ms: u64) -> Self {
        self.default_timeout_ms = timeout_ms;
        self
    }

    /// Get service instances for a service type
    async fn get_service_instances(&self, service_type: &ServiceType) -> ServiceResult<Vec<ServiceInstance>> {
        let services = self.services.read().await;
        Ok(services
            .get(service_type)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|instance| instance.enabled && instance.is_healthy())
            .collect())
    }

    /// Select service instance using load balancer
    async fn select_service_instance(
        &self,
        service_type: &ServiceType,
        request: &ServiceRequest,
    ) -> ServiceResult<Option<ServiceInstance>> {
        let instances = self.get_service_instances(service_type).await?;
        if instances.is_empty() {
            return Ok(None);
        }

        let loads: HashMap<Uuid, ServiceLoad> = futures::future::join_all(
            instances.iter().map(|instance| async move {
                let load = instance.get_load().await.unwrap_or_else(|_| ServiceLoad {
                    service_id: instance.service_id,
                    current_requests: 0,
                    avg_response_time_ms: 0.0,
                    cpu_usage_percent: 0.0,
                    memory_usage_percent: 0.0,
                    load_score: 0.0,
                });
                (instance.service_id, load)
            })
        )
        .await
        .into_iter()
        .collect();

        Ok(self
            .load_balancer
            .select_instance(&instances, &loads, request)
            .await)
    }

    /// Process request through middleware chain
    async fn process_request_middleware(
        &self,
        mut request: ServiceRequest,
    ) -> ServiceResult<ServiceRequest> {
        let middleware = self.middleware.read().await;
        for mw in middleware.iter() {
            request = mw.process_request(request).await?;
        }
        Ok(request)
    }

    /// Process response through middleware chain (in reverse order)
    async fn process_response_middleware(
        &self,
        mut response: ServiceResponse,
    ) -> ServiceResult<ServiceResponse> {
        let middleware = self.middleware.read().await;
        for mw in middleware.iter().rev() {
            response = mw.process_response(response).await?;
        }
        Ok(response)
    }

    /// Handle service errors with middleware
    async fn handle_error_with_middleware(
        &self,
        error: ServiceError,
        request: &ServiceRequest,
    ) -> ServiceResult<ServiceResponse> {
        let middleware = self.middleware.read().await;
        for mw in middleware.iter() {
            if let Ok(response) = mw.handle_error(error.clone(), request).await {
                return Ok(response);
            }
        }
        Err(error)
    }

    /// Update router statistics
    async fn update_stats(&self, request: &ServiceRequest, response: &ServiceResult<ServiceResponse>) {
        let mut stats = self.stats.write().await;
        stats.total_requests += 1;

        match response {
            Ok(_) => {
                stats.successful_requests += 1;
            }
            Err(error) => {
                stats.failed_requests += 1;
                match error {
                    ServiceError::ServiceUnavailable { .. } => stats.service_unavailable_errors += 1,
                    ServiceError::Timeout { .. } => stats.timeout_errors += 1,
                    ServiceError::RateLimitExceeded { .. } => stats.rate_limit_errors += 1,
                    _ => stats.other_errors += 1,
                }
            }
        }

        // Update per-service-type stats
        stats.service_type_stats
            .entry(request.service_type.clone())
            .or_insert_with(ServiceTypeStats::default)
            .total_requests += 1;
    }

    /// Get or create circuit breaker for a service
    async fn get_circuit_breaker(&self, service_id: Uuid) -> Arc<dyn ServiceCircuitBreaker> {
        let mut circuit_breakers = self.circuit_breakers.write().await;
        circuit_breakers
            .entry(service_id)
            .or_insert_with(|| Arc::new(DefaultServiceCircuitBreaker::new()))
            .clone()
    }
}

#[async_trait]
impl ServiceRouter for DefaultServiceRouter {
    async fn route_request(&self, mut request: ServiceRequest) -> ServiceResult<ServiceResponse> {
        // Process request through middleware
        request = self.process_request_middleware(request).await?;

        // Set default timeout if not specified
        let timeout_ms = request.timeout_ms.unwrap_or(self.default_timeout_ms);
        let timeout = tokio::time::Duration::from_millis(timeout_ms);

        // Find and select appropriate service instance
        let service_instance = match self.select_service_instance(&request.service_type, &request).await {
            Ok(Some(instance)) => instance,
            Ok(None) => {
                return Err(ServiceError::service_unavailable(
                    format!("{:?}", request.service_type),
                ))
            }
            Err(e) => return Err(e),
        };

        // Get circuit breaker for the service
        let circuit_breaker = self.get_circuit_breaker(service_instance.service_id).await;

        // Execute request with circuit breaker protection
        let result = tokio::time::timeout(
            timeout,
            circuit_breaker.execute(async move {
                service_instance.service.handle_request(request).await
            })
        )
        .await;

        let response = match result {
            Ok(Ok(response)) => {
                // Process response through middleware
                self.process_response_middleware(response).await
            }
            Ok(Err(error)) => {
                // Handle error with middleware
                self.handle_error_with_middleware(error, &request).await
            }
            Err(_) => {
                // Timeout occurred
                let error = ServiceError::timeout(timeout_ms);
                self.handle_error_with_middleware(error, &request).await
            }
        };

        // Update statistics
        self.update_stats(&request, &response).await;

        response
    }

    async fn register_service(&self, service_info: ServiceInfo, service: Arc<dyn BaseService>) -> ServiceResult<()> {
        let service_id = service_info.id;
        let service_instance = ServiceInstance {
            service_id,
            service_info,
            service,
            enabled: true,
            registered_at: chrono::Utc::now(),
            last_health_check: chrono::Utc::now(),
        };

        let mut services = self.services.write().await;
        services
            .entry(service_instance.service_info.service_type.clone())
            .or_default()
            .push(service_instance);

        // Also register with the service registry
        self.registry.register_service(service_instance.service_info).await?;

        Ok(())
    }

    async fn unregister_service(&self, service_id: Uuid) -> ServiceResult<bool> {
        let mut services = self.services.write().await;
        let mut found = false;

        for (_, instances) in services.iter_mut() {
            if let Some(pos) = instances.iter().position(|instance| instance.service_id == service_id) {
                instances.remove(pos);
                found = true;
                break;
            }
        }

        if found {
            // Also unregister from the service registry
            self.registry.unregister_service(service_id).await?;
        }

        Ok(found)
    }

    async fn get_available_services(&self, service_type: ServiceType) -> ServiceResult<Vec<ServiceInfo>> {
        let instances = self.get_service_instances(&service_type).await?;
        Ok(instances.into_iter().map(|instance| instance.service_info).collect())
    }

    async fn get_service_load(&self, service_id: Uuid) -> ServiceResult<ServiceLoad> {
        let services = self.services.read().await;
        for instances in services.values() {
            if let Some(instance) = instances.iter().find(|i| i.service_id == service_id) {
                return instance.get_load().await;
            }
        }
        Err(ServiceError::service_unavailable(service_id.to_string()))
    }

    async fn set_service_enabled(&self, service_id: Uuid, enabled: bool) -> ServiceResult<bool> {
        let mut services = self.services.write().await;
        for instances in services.values_mut() {
            if let Some(instance) = instances.iter_mut().find(|i| i.service_id == service_id) {
                instance.enabled = enabled;
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn add_middleware(&mut self, middleware: Arc<dyn ServiceMiddleware>) {
        // Note: This would need to be made async-safe in a real implementation
        // For now, we'll use a blocking write
        let middleware_arc = self.middleware.clone();
        tokio::spawn(async move {
            let mut mw = middleware_arc.write().await;
            mw.push(middleware);
        });
    }

    fn set_load_balancer(&mut self, load_balancer: Arc<dyn LoadBalancer>) {
        self.load_balancer = load_balancer;
    }

    async fn get_router_stats(&self) -> ServiceResult<RouterStats> {
        let stats = self.stats.read().await;
        Ok(stats.clone())
    }
}

/// Service instance wrapper
#[derive(Debug, Clone)]
pub struct ServiceInstance {
    /// Service ID
    pub service_id: Uuid,
    /// Service information
    pub service_info: ServiceInfo,
    /// Service implementation
    pub service: Arc<dyn BaseService>,
    /// Whether the service is enabled
    pub enabled: bool,
    /// Registration timestamp
    pub registered_at: chrono::DateTime<chrono::Utc>,
    /// Last health check timestamp
    pub last_health_check: chrono::DateTime<chrono::Utc>,
}

impl ServiceInstance {
    /// Check if service is healthy
    pub async fn is_healthy(&self) -> bool {
        if let Ok(health) = self.service.health_check().await {
            health.status.is_available()
        } else {
            false
        }
    }

    /// Get service load information
    pub async fn get_load(&self) -> ServiceResult<ServiceLoad> {
        // In a real implementation, this would collect actual metrics
        Ok(ServiceLoad {
            service_id: self.service_id,
            current_requests: 0,
            avg_response_time_ms: 0.0,
            cpu_usage_percent: 0.0,
            memory_usage_percent: 0.0,
            load_score: 0.0,
        })
    }
}

/// Router statistics
#[derive(Debug, Clone, Default)]
pub struct RouterStats {
    /// Total requests processed
    pub total_requests: u64,
    /// Successful requests
    pub successful_requests: u64,
    /// Failed requests
    pub failed_requests: u64,
    /// Service unavailable errors
    pub service_unavailable_errors: u64,
    /// Timeout errors
    pub timeout_errors: u64,
    /// Rate limit errors
    pub rate_limit_errors: u64,
    /// Other errors
    pub other_errors: u64,
    /// Statistics by service type
    pub service_type_stats: HashMap<ServiceType, ServiceTypeStats>,
}

/// Service type statistics
#[derive(Debug, Clone, Default)]
pub struct ServiceTypeStats {
    /// Total requests for this service type
    pub total_requests: u64,
    /// Successful requests
    pub successful_requests: u64,
    /// Failed requests
    pub failed_requests: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
}

impl ServiceTypeStats {
    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.successful_requests as f64 / self.total_requests as f64
        }
    }

    /// Get error rate
    pub fn error_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.failed_requests as f64 / self.total_requests as f64
        }
    }
}