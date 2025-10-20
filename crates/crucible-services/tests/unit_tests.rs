//! Unit tests for individual modules in crucible-services

use crucible_services::*;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

#[test]
fn test_error_types_and_conversions() {
    // Test error creation
    let tool_error = ServiceError::tool_error("Tool failed");
    assert!(matches!(tool_error, ServiceError::ToolError { .. }));
    assert_eq!(tool_error.to_string(), "Tool service error: Tool failed");

    let db_error = ServiceError::database_error("Connection failed");
    assert!(db_error.is_retryable());
    assert!(db_error.is_server_error());

    let access_error = ServiceError::access_denied("No permission");
    assert!(access_error.is_client_error());
    assert!(!access_error.is_retryable());

    let timeout_error = ServiceError::timeout(5000);
    assert!(timeout_error.is_retryable());
    assert!(!timeout_error.is_client_error());

    // Test contextual errors
    let context = ErrorContext::new("test_operation", "test_service")
        .with_context("key1", "value1")
        .with_request_id("req-123");

    let contextual_error = ContextualError::new(
        ServiceError::internal_error("Something went wrong"),
        context,
    );

    assert_eq!(
        contextual_error.service_error().to_string(),
        "Internal service error: Something went wrong"
    );
}

#[test]
fn test_service_type_conversions() {
    // Test built-in types
    assert_eq!(ServiceType::Tool.as_str(), "tool");
    assert_eq!(ServiceType::Database.as_str(), "database");
    assert_eq!(ServiceType::LLM.as_str(), "llm");
    assert_eq!(ServiceType::Config.as_str(), "config");

    // Test string conversion
    assert_eq!(ServiceType::from_str("tool"), ServiceType::Tool);
    assert_eq!(ServiceType::from_str("database"), ServiceType::Database);
    assert_eq!(ServiceType::from_str("llm"), ServiceType::LLM);
    assert_eq!(ServiceType::from_str("config"), ServiceType::Config);

    // Test custom types
    assert_eq!(
        ServiceType::from_str("custom_service"),
        ServiceType::Custom("custom_service".to_string())
    );
    assert_eq!(
        ServiceType::Custom("my_service".to_string()).as_str(),
        "my_service"
    );
}

#[test]
fn test_service_status() {
    assert!(ServiceStatus::Healthy.is_available());
    assert!(ServiceStatus::Degraded.is_available());
    assert!(!ServiceStatus::Starting.is_available());
    assert!(!ServiceStatus::Stopping.is_available());
    assert!(!ServiceStatus::Failed.is_available());
    assert!(!ServiceStatus::Maintenance.is_available());

    assert!(!ServiceStatus::Healthy.is_terminal());
    assert!(!ServiceStatus::Degraded.is_terminal());
    assert!(!ServiceStatus::Starting.is_terminal());
    assert!(!ServiceStatus::Stopping.is_terminal());
    assert!(ServiceStatus::Failed.is_terminal());
    assert!(!ServiceStatus::Maintenance.is_terminal());
}

#[test]
fn test_request_priority() {
    assert!(RequestPriority::Critical > RequestPriority::High);
    assert!(RequestPriority::High > RequestPriority::Normal);
    assert!(RequestPriority::Normal > RequestPriority::Low);

    assert_eq!(RequestPriority::default(), RequestPriority::Normal);

    let priorities = vec![
        RequestPriority::Low,
        RequestPriority::Normal,
        RequestPriority::High,
        RequestPriority::Critical,
    ];
    let mut sorted = priorities.clone();
    sorted.sort();
    assert_eq!(sorted, priorities);
}

#[test]
fn test_service_metrics_calculations() {
    let mut metrics = ServiceMetrics {
        service_id: Uuid::new_v4(),
        timestamp: chrono::Utc::now(),
        request_count: 100,
        success_count: 95,
        error_count: 5,
        avg_response_time_ms: 150.5,
        throughput_rps: 10.0,
        memory_usage_bytes: Some(1024 * 1024 * 512), // 512MB
        cpu_usage_percent: Some(75.5),
        custom_metrics: HashMap::new(),
    };

    assert_eq!(metrics.success_rate(), 0.95);
    assert_eq!(metrics.error_rate(), 0.05);

    // Test with zero requests
    metrics.request_count = 0;
    assert_eq!(metrics.success_rate(), 0.0);
    assert_eq!(metrics.error_rate(), 0.0);
}

#[test]
fn test_load_balancer_strategy_creation() {
    // Test factory methods
    let rr_lb = LoadBalancerFactory::create(LoadBalancingStrategy::RoundRobin);
    assert_eq!(rr_lb.name(), "RoundRobin");

    let weighted_lb = LoadBalancerFactory::create(LoadBalancingStrategy::WeightedRoundRobin);
    assert_eq!(weighted_lb.name(), "WeightedRoundRobin");

    let lc_lb = LoadBalancerFactory::create(LoadBalancingStrategy::LeastConnections);
    assert_eq!(lc_lb.name(), "LeastConnections");

    let random_lb = LoadBalancerFactory::create(LoadBalancingStrategy::Random);
    assert_eq!(random_lb.name(), "Random");
}

#[test]
fn test_load_balancing_strategy_names() {
    assert_eq!(LoadBalancingStrategy::RoundRobin.name(), "round_robin");
    assert_eq!(LoadBalancingStrategy::WeightedRoundRobin.name(), "weighted_round_robin");
    assert_eq!(LoadBalancingStrategy::LeastConnections.name(), "least_connections");
    assert_eq!(LoadBalancingStrategy::Random.name(), "random");

    // Test string conversion
    assert_eq!(
        LoadBalancingStrategy::from_name("round_robin"),
        Some(LoadBalancingStrategy::RoundRobin)
    );
    assert_eq!(
        LoadBalancingStrategy::from_name("least_connections"),
        Some(LoadBalancingStrategy::LeastConnections)
    );
    assert_eq!(LoadBalancingStrategy::from_name("unknown"), None);
}

#[test]
fn test_circuit_breaker_presets() {
    let lenient_cb = CircuitBreakerFactory::create_preset(CircuitBreakerPreset::Lenient);
    let balanced_cb = CircuitBreakerFactory::create_preset(CircuitBreakerPreset::Balanced);
    let strict_cb = CircuitBreakerFactory::create_preset(CircuitBreakerPreset::Strict);

    // All should be created successfully
    assert!(lenient_cb.state().await.is_closed());
    assert!(balanced_cb.state().await.is_closed());
    assert!(strict_cb.state().await.is_closed());
}

#[test]
fn test_service_builder_configuration() {
    // Test basic builder
    let registry = ServiceDiscoveryFactory::create_registry();
    let load_balancer = LoadBalancerFactory::create(LoadBalancingStrategy::RoundRobin);

    let result = ServiceBuilder::new()
        .with_registry(registry.clone())
        .with_load_balancer(load_balancer.clone())
        .with_timeout(10000)
        .build();

    assert!(result.is_ok());

    // Test missing required components
    let result = ServiceBuilder::new()
        .with_registry(registry)
        // Missing load balancer
        .build();

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ServiceError::ConfigurationError { .. }
    ));

    let result = ServiceBuilder::new()
        // Missing registry
        .with_load_balancer(load_balancer)
        .build();

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ServiceError::ConfigurationError { .. }
    ));
}

#[test]
fn test_cache_statistics() {
    let stats = CacheStats {
        total_entries: 100,
        hits: 75,
        misses: 25,
        hit_rate: 0.75,
        memory_usage_bytes: 1024 * 1024, // 1MB
    };

    assert_eq!(stats.total_entries, 100);
    assert_eq!(stats.hit_rate, 0.75);
    assert_eq!(stats.memory_usage_bytes, 1048576);
}

#[test]
fn test_rate_limit_usage() {
    let usage = RateLimitUsage {
        current_count: 75,
        limit: 100,
        remaining: 25,
        reset_time_seconds: 45,
    };

    assert_eq!(usage.current_count, 75);
    assert_eq!(usage.remaining, 25);
    assert_eq!(usage.reset_time_seconds, 45);
}

#[test]
fn test_validation_result() {
    let valid_result = ValidationResult {
        valid: true,
        errors: vec![],
        warnings: vec![],
        normalized_value: Some(serde_json::json!({"key": "value"})),
    };

    assert!(valid_result.valid);
    assert!(valid_result.errors.is_empty());
    assert!(valid_result.normalized_value.is_some());

    let invalid_result = ValidationResult {
        valid: false,
        errors: vec![ValidationError {
            message: "Invalid value".to_string(),
            path: "/field".to_string(),
            rule: "required".to_string(),
            severity: ValidationSeverity::Error,
            value: serde_json::Value::Null,
        }],
        warnings: vec![],
        normalized_value: None,
    };

    assert!(!invalid_result.valid);
    assert_eq!(invalid_result.errors.len(), 1);
    assert!(invalid_result.normalized_value.is_none());
}

#[test]
fn test_service_load_calculations() {
    let load = ServiceLoad {
        service_id: Uuid::new_v4(),
        current_requests: 10,
        avg_response_time_ms: 150.0,
        cpu_usage_percent: 75.5,
        memory_usage_percent: 60.0,
        load_score: 0.68,
    };

    assert_eq!(load.current_requests, 10);
    assert_eq!(load.avg_response_time_ms, 150.0);
    assert_eq!(load.cpu_usage_percent, 75.5);
    assert_eq!(load.load_score, 0.68);
}

#[test]
fn test_request_metadata() {
    let metadata = RequestMetadata {
        timestamp: chrono::Utc::now(),
        user_id: Some("user123".to_string()),
        session_id: Some("session456".to_string()),
        auth_token: Some("token789".to_string()),
        client_id: Some("client000".to_string()),
        priority: RequestPriority::High,
        retry_count: 2,
        trace_context: None,
    };

    assert_eq!(metadata.user_id, Some("user123".to_string()));
    assert_eq!(metadata.priority, RequestPriority::High);
    assert_eq!(metadata.retry_count, 2);
}

#[test]
fn test_response_metadata() {
    let metadata = ResponseMetadata {
        timestamp: chrono::Utc::now(),
        duration_ms: 250,
        service_id: Uuid::new_v4(),
        metadata: {
            let mut map = HashMap::new();
            map.insert("key1".to_string(), "value1".to_string());
            map.insert("key2".to_string(), "value2".to_string());
            map
        },
    };

    assert_eq!(metadata.duration_ms, 250);
    assert_eq!(metadata.metadata.len(), 2);
    assert_eq!(metadata.metadata.get("key1"), Some(&"value1".to_string()));
}

#[test]
fn test_service_health() {
    let health = ServiceHealth {
        service_id: Uuid::new_v4(),
        status: ServiceStatus::Healthy,
        last_check: chrono::Utc::now(),
        metrics: {
            let mut map = HashMap::new();
            map.insert("cpu_usage".to_string(), 45.5);
            map.insert("memory_usage".to_string(), 60.0);
            map
        },
        message: Some("Service is running normally".to_string()),
        uptime_seconds: Some(86400), // 1 day
    };

    assert_eq!(health.status, ServiceStatus::Healthy);
    assert_eq!(health.metrics.len(), 2);
    assert_eq!(health.uptime_seconds, Some(86400));
    assert_eq!(
        health.metrics.get("cpu_usage"),
        Some(&45.5)
    );
}

#[test]
fn test_service_configuration() {
    let config = ServiceConfig {
        service_id: Uuid::new_v4(),
        config: {
            let mut map = HashMap::new();
            map.insert("timeout".to_string(), serde_json::json!(5000));
            map.insert("retries".to_string(), serde_json::json!(3));
            map
        },
        environment_overrides: {
            let mut env_map = HashMap::new();
            let mut prod_config = HashMap::new();
            prod_config.insert("timeout".to_string(), serde_json::json!(10000));
            env_map.insert("production".to_string(), prod_config);
            env_map
        },
        version: 1,
        last_modified: chrono::Utc::now(),
    };

    assert_eq!(config.version, 1);
    assert_eq!(config.config.len(), 2);
    assert_eq!(config.environment_overrides.len(), 1);

    let prod_overrides = config.environment_overrides.get("production").unwrap();
    assert_eq!(prod_overrides.len(), 1);
    assert_eq!(
        prod_overrides.get("timeout"),
        Some(&serde_json::json!(10000))
    );
}

#[test]
fn test_version_constant() {
    assert!(!crucible_services::VERSION.is_empty());
    assert!(crucible_services::VERSION.contains('.'));
}

#[test]
fn test_default_constants() {
    assert_eq!(crucible_services::defaults::DEFAULT_TIMEOUT_MS, 30000);
    assert_eq!(crucible_services::defaults::DEFAULT_CACHE_TTL_SECONDS, 300);
    assert_eq!(crucible_services::defaults::DEFAULT_HEALTH_CHECK_INTERVAL_SECONDS, 60);
    assert_eq!(crucible_services::defaults::DEFAULT_CIRCUIT_BREAKER_FAILURE_THRESHOLD, 5);
    assert_eq!(crucible_services::defaults::DEFAULT_CIRCUIT_BREAKER_TIMEOUT_SECONDS, 60);
    assert_eq!(crucible_services::defaults::DEFAULT_RATE_LIMIT_RPM, 100);
}

#[test]
fn test_json_serialization() {
    // Test that key types can be serialized/deserialized
    let service_type = ServiceType::Tool;
    let serialized = serde_json::to_string(&service_type).unwrap();
    let deserialized: ServiceType = serde_json::from_str(&serialized).unwrap();
    assert_eq!(service_type, deserialized);

    let service_status = ServiceStatus::Healthy;
    let serialized = serde_json::to_string(&service_status).unwrap();
    let deserialized: ServiceStatus = serde_json::from_str(&serialized).unwrap();
    assert_eq!(service_status, deserialized);

    let priority = RequestPriority::High;
    let serialized = serde_json::to_string(&priority).unwrap();
    let deserialized: RequestPriority = serde_json::from_str(&serialized).unwrap();
    assert_eq!(priority, deserialized);
}