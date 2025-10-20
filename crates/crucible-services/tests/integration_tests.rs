//! Integration tests for the crucible-services crate

use crucible_services::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Mock tool service implementation
struct MockToolService {
    service_info: ServiceInfo,
    execution_count: Arc<RwLock<u64>>,
}

impl MockToolService {
    fn new(name: &str) -> Self {
        let service_info = ServiceInfo {
            id: Uuid::new_v4(),
            name: name.to_string(),
            service_type: ServiceType::Tool,
            version: "1.0.0".to_string(),
            description: Some("Mock tool service".to_string()),
            status: ServiceStatus::Healthy,
            capabilities: vec!["execute".to_string()],
            config_schema: None,
            metadata: HashMap::new(),
        };

        Self {
            service_info,
            execution_count: Arc::new(RwLock::new(0)),
        }
    }
}

#[async_trait::async_trait]
impl traits::BaseService for MockToolService {
    async fn info(&self) -> ServiceResult<ServiceInfo> {
        Ok(self.service_info.clone())
    }

    async fn health_check(&self) -> ServiceResult<ServiceHealth> {
        Ok(ServiceHealth {
            service_id: self.service_info.id,
            status: ServiceStatus::Healthy,
            last_check: chrono::Utc::now(),
            metrics: HashMap::new(),
            message: Some("Service is healthy".to_string()),
            uptime_seconds: Some(3600),
        })
    }

    async fn start(&self) -> ServiceResult<()> {
        Ok(())
    }

    async fn stop(&self) -> ServiceResult<()> {
        Ok(())
    }

    async fn dependencies(&self) -> ServiceResult<Vec<ServiceDependency>> {
        Ok(vec![])
    }

    async fn handle_request(&self, request: ServiceRequest) -> ServiceResult<ServiceResponse> {
        // Increment execution count
        let mut count = self.execution_count.write().await;
        *count += 1;

        // Simulate some processing time
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        Ok(ServiceResponse {
            request_id: request.request_id,
            status: ResponseStatus::Success,
            payload: serde_json::json!({
                "result": "Mock execution completed",
                "execution_count": *count
            }),
            metadata: ResponseMetadata {
                timestamp: chrono::Utc::now(),
                duration_ms: 10,
                service_id: self.service_info.id,
                metadata: HashMap::new(),
            },
        })
    }

    async fn metrics(&self) -> ServiceResult<ServiceMetrics> {
        let count = *self.execution_count.read().await;
        Ok(ServiceMetrics {
            service_id: self.service_info.id,
            timestamp: chrono::Utc::now(),
            request_count: count,
            success_count: count,
            error_count: 0,
            avg_response_time_ms: 10.0,
            throughput_rps: count as f64 / 3600.0,
            memory_usage_bytes: None,
            cpu_usage_percent: None,
            custom_metrics: HashMap::new(),
        })
    }
}

#[tokio::test]
async fn test_service_registration_and_discovery() {
    // Create service registry
    let registry = ServiceDiscoveryFactory::create_registry();

    // Create mock service
    let tool_service = Arc::new(MockToolService::new("test-tool"));
    let service_info = tool_service.info().await.unwrap();

    // Register service
    registry.register_service(service_info.clone()).await.unwrap();

    // List services
    let services = registry.list_services().await.unwrap();
    assert_eq!(services.len(), 1);
    assert_eq!(services[0].name, "test-tool");

    // Get service
    let retrieved = registry.get_service(service_info.id).await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name, "test-tool");

    // List services by type
    let tools = registry.list_services_by_type(ServiceType::Tool).await.unwrap();
    assert_eq!(tools.len(), 1);

    // Unregister service
    registry.unregister_service(service_info.id).await.unwrap();

    // Verify service is gone
    let services = registry.list_services().await.unwrap();
    assert_eq!(services.len(), 0);
}

#[tokio::test]
async fn test_service_router_basic_routing() {
    // Create service router
    let registry = ServiceDiscoveryFactory::create_registry();
    let load_balancer = LoadBalancerFactory::create(LoadBalancingStrategy::RoundRobin);
    let router = Arc::new(DefaultServiceRouter::new(registry, load_balancer));

    // Create and register mock services
    let tool_service1 = Arc::new(MockToolService::new("tool-1"));
    let tool_service2 = Arc::new(MockToolService::new("tool-2"));

    let service_info1 = tool_service1.info().await.unwrap();
    let service_info2 = tool_service2.info().await.unwrap();

    router.register_service(service_info1.clone(), tool_service1.clone()).await.unwrap();
    router.register_service(service_info2.clone(), tool_service2.clone()).await.unwrap();

    // Create test request
    let request = ServiceRequest {
        request_id: Uuid::new_v4(),
        service_type: ServiceType::Tool,
        service_instance: None,
        method: "execute".to_string(),
        payload: serde_json::json!({"tool": "test"}),
        metadata: RequestMetadata {
            timestamp: chrono::Utc::now(),
            user_id: Some("test-user".to_string()),
            session_id: Some("test-session".to_string()),
            auth_token: Some("test-token".to_string()),
            client_id: Some("test-client".to_string()),
            priority: RequestPriority::Normal,
            retry_count: 0,
            trace_context: None,
        },
        timeout_ms: Some(5000),
    };

    // Route request
    let response = router.route_request(request).await.unwrap();
    assert_eq!(response.status, ResponseStatus::Success);

    // Check response payload
    let payload = response.payload;
    assert!(payload.get("result").is_some());
    assert!(payload.get("execution_count").is_some());

    // Get router stats
    let stats = router.get_router_stats().await.unwrap();
    assert_eq!(stats.total_requests, 1);
    assert_eq!(stats.successful_requests, 1);
    assert_eq!(stats.failed_requests, 0);
}

#[tokio::test]
async fn test_load_balancing_strategies() {
    // Create multiple mock services
    let tool_service1 = Arc::new(MockToolService::new("tool-1"));
    let tool_service2 = Arc::new(MockToolService::new("tool-2"));
    let tool_service3 = Arc::new(MockToolService::new("tool-3"));

    let service_info1 = tool_service1.info().await.unwrap();
    let service_info2 = tool_service2.info().await.unwrap();
    let service_info3 = tool_service3.info().await.unwrap();

    // Test round-robin load balancing
    let registry = ServiceDiscoveryFactory::create_registry();
    let load_balancer = LoadBalancerFactory::create(LoadBalancingStrategy::RoundRobin);
    let router = Arc::new(DefaultServiceRouter::new(registry, load_balancer));

    router.register_service(service_info1.clone(), tool_service1.clone()).await.unwrap();
    router.register_service(service_info2.clone(), tool_service2.clone()).await.unwrap();
    router.register_service(service_info3.clone(), tool_service3.clone()).await.unwrap();

    // Send multiple requests
    for i in 0..6 {
        let request = ServiceRequest {
            request_id: Uuid::new_v4(),
            service_type: ServiceType::Tool,
            service_instance: None,
            method: "execute".to_string(),
            payload: serde_json::json!({"request_id": i}),
            metadata: RequestMetadata::default(),
            timeout_ms: Some(5000),
        };

        let response = router.route_request(request).await.unwrap();
        assert_eq!(response.status, ResponseStatus::Success);
    }

    // Check that all services received requests (round-robin should distribute evenly)
    let metrics1 = tool_service1.metrics().await.unwrap();
    let metrics2 = tool_service2.metrics().await.unwrap();
    let metrics3 = tool_service3.metrics().await.unwrap();

    assert_eq!(metrics1.request_count, 2);
    assert_eq!(metrics2.request_count, 2);
    assert_eq!(metrics3.request_count, 2);

    // Check router stats
    let stats = router.get_router_stats().await.unwrap();
    assert_eq!(stats.total_requests, 6);
    assert_eq!(stats.successful_requests, 6);
}

#[tokio::test]
async fn test_service_builder_presets() {
    // Test basic preset
    let basic_router = presets::basic().unwrap();
    let services = basic_router.get_available_services(ServiceType::Tool).await.unwrap();
    assert_eq!(services.len(), 0); // No services registered yet

    // Test development preset
    let dev_router = presets::development().unwrap();
    let services = dev_router.get_available_services(ServiceType::Database).await.unwrap();
    assert_eq!(services.len(), 0);

    // Test production preset
    let auth_service = Arc::new(router::middleware::MockAuthService);
    let rate_limiter = Arc::new(router::middleware::MockRateLimiter);
    let metrics_collector = Arc::new(router::middleware::MockMetricsCollector);

    let prod_router = presets::production(auth_service, rate_limiter, metrics_collector).unwrap();
    let services = prod_router.get_available_services(ServiceType::LLM).await.unwrap();
    assert_eq!(services.len(), 0);
}

#[tokio::test]
async fn test_error_handling_and_propagation() {
    // Create a service that returns errors
    struct FailingService;

    #[async_trait::async_trait]
    impl traits::BaseService for FailingService {
        async fn info(&self) -> ServiceResult<ServiceInfo> {
            Ok(ServiceInfo {
                id: Uuid::new_v4(),
                name: "failing-service".to_string(),
                service_type: ServiceType::Tool,
                version: "1.0.0".to_string(),
                description: Some("A service that always fails".to_string()),
                status: ServiceStatus::Failed,
                capabilities: vec![],
                config_schema: None,
                metadata: HashMap::new(),
            })
        }

        async fn health_check(&self) -> ServiceResult<ServiceHealth> {
            Err(ServiceError::internal_error("Service is unhealthy"))
        }

        async fn start(&self) -> ServiceResult<()> {
            Err(ServiceError::internal_error("Cannot start service"))
        }

        async fn stop(&self) -> ServiceResult<()> {
            Ok(())
        }

        async fn dependencies(&self) -> ServiceResult<Vec<ServiceDependency>> {
            Ok(vec![])
        }

        async fn handle_request(&self, _request: ServiceRequest) -> ServiceResult<ServiceResponse> {
            Err(ServiceError::tool_error("Simulated tool failure"))
        }

        async fn metrics(&self) -> ServiceResult<ServiceMetrics> {
            Err(ServiceError::internal_error("Cannot get metrics"))
        }
    }

    // Create router and register failing service
    let registry = ServiceDiscoveryFactory::create_registry();
    let load_balancer = LoadBalancerFactory::create(LoadBalancingStrategy::RoundRobin);
    let router = Arc::new(DefaultServiceRouter::new(registry, load_balancer));

    let failing_service = Arc::new(FailingService);
    let service_info = failing_service.info().await.unwrap();

    router.register_service(service_info, failing_service).await.unwrap();

    // Send request to failing service
    let request = ServiceRequest {
        request_id: Uuid::new_v4(),
        service_type: ServiceType::Tool,
        service_instance: None,
        method: "execute".to_string(),
        payload: serde_json::json!({}),
        metadata: RequestMetadata::default(),
        timeout_ms: Some(5000),
    };

    let result = router.route_request(request).await;
    assert!(result.is_err());

    // Check router stats
    let stats = router.get_router_stats().await.unwrap();
    assert_eq!(stats.total_requests, 1);
    assert_eq!(stats.successful_requests, 0);
    assert_eq!(stats.failed_requests, 1);
    assert_eq!(stats.other_errors, 1);
}

#[tokio::test]
async fn test_timeout_handling() {
    // Create a slow service
    struct SlowService {
        service_info: ServiceInfo,
    }

    impl SlowService {
        fn new() -> Self {
            Self {
                service_info: ServiceInfo {
                    id: Uuid::new_v4(),
                    name: "slow-service".to_string(),
                    service_type: ServiceType::Tool,
                    version: "1.0.0".to_string(),
                    description: Some("A service that responds slowly".to_string()),
                    status: ServiceStatus::Healthy,
                    capabilities: vec!["execute".to_string()],
                    config_schema: None,
                    metadata: HashMap::new(),
                },
            }
        }
    }

    #[async_trait::async_trait]
    impl traits::BaseService for SlowService {
        async fn info(&self) -> ServiceResult<ServiceInfo> {
            Ok(self.service_info.clone())
        }

        async fn health_check(&self) -> ServiceResult<ServiceHealth> {
            Ok(ServiceHealth {
                service_id: self.service_info.id,
                status: ServiceStatus::Healthy,
                last_check: chrono::Utc::now(),
                metrics: HashMap::new(),
                message: Some("Service is healthy but slow".to_string()),
                uptime_seconds: Some(3600),
            })
        }

        async fn start(&self) -> ServiceResult<()> {
            Ok(())
        }

        async fn stop(&self) -> ServiceResult<()> {
            Ok(())
        }

        async fn dependencies(&self) -> ServiceResult<Vec<ServiceDependency>> {
            Ok(vec![])
        }

        async fn handle_request(&self, request: ServiceRequest) -> ServiceResult<ServiceResponse> {
            // Simulate slow processing (longer than timeout)
            tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;

            Ok(ServiceResponse {
                request_id: request.request_id,
                status: ResponseStatus::Success,
                payload: serde_json::json!({"result": "Slow execution completed"}),
                metadata: ResponseMetadata {
                    timestamp: chrono::Utc::now(),
                    duration_ms: 2000,
                    service_id: self.service_info.id,
                    metadata: HashMap::new(),
                },
            })
        }

        async fn metrics(&self) -> ServiceResult<ServiceMetrics> {
            Ok(ServiceMetrics {
                service_id: self.service_info.id,
                timestamp: chrono::Utc::now(),
                request_count: 0,
                success_count: 0,
                error_count: 0,
                avg_response_time_ms: 2000.0,
                throughput_rps: 0.0,
                memory_usage_bytes: None,
                cpu_usage_percent: None,
                custom_metrics: HashMap::new(),
            })
        }
    }

    // Create router with short timeout
    let registry = ServiceDiscoveryFactory::create_registry();
    let load_balancer = LoadBalancerFactory::create(LoadBalancingStrategy::RoundRobin);
    let router = Arc::new(
        DefaultServiceRouter::new(registry, load_balancer)
            .with_default_timeout(100) // 100ms timeout
    );

    let slow_service = Arc::new(SlowService::new());
    let service_info = slow_service.info().await.unwrap();

    router.register_service(service_info, slow_service).await.unwrap();

    // Send request with short timeout
    let request = ServiceRequest {
        request_id: Uuid::new_v4(),
        service_type: ServiceType::Tool,
        service_instance: None,
        method: "execute".to_string(),
        payload: serde_json::json!({}),
        metadata: RequestMetadata::default(),
        timeout_ms: Some(50), // Even shorter timeout
    };

    let result = router.route_request(request).await;
    assert!(result.is_err());

    // Should be a timeout error
    match result {
        Err(ServiceError::Timeout { timeout_ms }) => {
            assert_eq!(timeout_ms, 50);
        }
        _ => panic!("Expected timeout error"),
    }

    // Check router stats
    let stats = router.get_router_stats().await.unwrap();
    assert_eq!(stats.total_requests, 1);
    assert_eq!(stats.failed_requests, 1);
    assert_eq!(stats.timeout_errors, 1);
}

#[tokio::test]
async fn test_service_discovery_caching() {
    let registry = ServiceDiscoveryFactory::create_registry();
    let discovery = ServiceDiscoveryFactory::create_with_cache_ttl(registry.clone(), 1); // 1 second TTL

    // Initially no services
    let services = discovery.discover_services(ServiceType::Tool).await.unwrap();
    assert!(services.is_empty());

    // Register a service
    let tool_service = Arc::new(MockToolService::new("cached-tool"));
    let service_info = tool_service.info().await.unwrap();

    registry.register_service(service_info).await.unwrap();

    // Discovery should still return empty (cache)
    let services = discovery.discover_services(ServiceType::Tool).await.unwrap();
    assert!(services.is_empty());

    // Wait for cache to expire
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Now discovery should return the service
    let services = discovery.discover_services(ServiceType::Tool).await.unwrap();
    assert_eq!(services.len(), 1);
    assert_eq!(services[0].name, "cached-tool");
}

#[tokio::test]
async fn test_circuit_breaker_functionality() {
    use crate::router::circuit_breaker::*;

    let circuit_breaker = DefaultServiceCircuitBreaker::with_config(CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 2,
        timeout: std::time::Duration::from_millis(100),
        min_requests: 3,
        error_rate_threshold: 0.5,
    });

    // Successful operation should work
    let result = circuit_breaker
        .execute(async { Ok::<_, ServiceError>("success") })
        .await;
    assert!(result.is_ok());
    assert_eq!(circuit_breaker.state().await, CircuitBreakerState::Closed);

    // Fail enough times to open circuit
    for _ in 0..3 {
        let result = circuit_breaker
            .execute(async {
                Err::<(), ServiceError>(ServiceError::internal_error("test error"))
            })
            .await;
        assert!(result.is_err());
    }

    // Circuit should now be open
    assert_eq!(circuit_breaker.state().await, CircuitBreakerState::Open);

    // Next request should fail immediately
    let result = circuit_breaker
        .execute(async { Ok::<_, ServiceError>("success") })
        .await;
    assert!(result.is_err());

    // Wait for timeout
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    // Next request should work and transition to half-open
    let result = circuit_breaker
        .execute(async { Ok::<_, ServiceError>("success") })
        .await;
    assert!(result.is_ok());
    assert_eq!(circuit_breaker.state().await, CircuitBreakerState::HalfOpen);

    // Another success should close the circuit
    let result = circuit_breaker
        .execute(async { Ok::<_, ServiceError>("success") })
        .await;
    assert!(result.is_ok());
    assert_eq!(circuit_breaker.state().await, CircuitBreakerState::Closed);

    // Check metrics
    let metrics = circuit_breaker.metrics().await;
    assert_eq!(metrics.total_requests, 6);
    assert_eq!(metrics.successful_requests, 3);
    assert_eq!(metrics.failed_requests, 3);
}