//! # Phase 2 Comprehensive Service Integration Test Suite
//!
//! This is the culmination of all our Phase 2 work - a complete validation that our
//! service ecosystem works perfectly together. This test suite validates:
//!
//! 1. **Full Service Stack Validation** - All 4 services working together
//! 2. **Event-Driven Coordination** - Services communicate through events correctly
//! 3. **Cross-Service Workflows** - End-to-end workflows across multiple services
//! 4. **Performance Under Load** - System performs well with realistic usage
//! 5. **Resource Management** - Memory and connection management works correctly
//! 6. **Error Handling & Recovery** - System handles failures gracefully
//! 7. **Configuration & Lifecycle** - Services start/stop/configure correctly
//! 8. **JSON-RPC Tool Pattern** - Simple output pattern works as intended

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, RwLock, Mutex};
use uuid::Uuid;
use chrono::Utc;
use serde_json::{json, Value};

use crucible_services::{
    script_engine::{ScriptEngineService, ScriptEngineConfig, ScriptExecutionRequest, ScriptExecutionResponse},
    inference_engine::{InferenceEngineService, InferenceEngineConfig, DefaultModels, PerformanceSettings, CacheSettings, InferenceLimits, MonitoringSettings},
    data_store::{CrucibleDataStore, DataStoreConfig, DatabaseBackend, DatabaseBackendConfig, DocumentData, DocumentId, DocumentMetadata},
    mcp_gateway::{McpGateway, McpGatewayConfig},
    events::{
        core::{DaemonEvent, EventType, EventPriority, EventPayload, EventSource},
        routing::{EventRouter, ServiceRegistration, LoadBalancingStrategy, CircuitBreakerConfig},
        mock::MockEventRouter,
        integration::{EventIntegrationManager, LifecycleEventType},
        errors::{EventError, EventResult},
    },
    service_traits::*,
    service_types::*,
    types::*,
    errors::ServiceError,
};

/// Phase 2 Test Configuration
#[derive(Debug, Clone)]
pub struct Phase2TestConfig {
    /// Enable comprehensive service stack testing
    pub enable_full_stack: bool,
    /// Enable cross-service workflow testing
    pub enable_cross_service_workflows: bool,
    /// Enable performance testing
    pub enable_performance_testing: bool,
    /// Enable error handling and recovery testing
    pub enable_error_recovery_testing: bool,
    /// Enable memory leak testing
    pub enable_memory_testing: bool,
    /// Enable configuration and lifecycle testing
    pub enable_lifecycle_testing: bool,
    /// Event timeout for operations
    pub event_timeout_ms: u64,
    /// Maximum retries for operations
    pub max_retries: u32,
    /// Number of concurrent operations for load testing
    pub concurrent_operations: usize,
    /// Duration for memory leak testing
    pub memory_test_duration_secs: u64,
}

impl Default for Phase2TestConfig {
    fn default() -> Self {
        Self {
            enable_full_stack: true,
            enable_cross_service_workflows: true,
            enable_performance_testing: true,
            enable_error_recovery_testing: true,
            enable_memory_testing: true,
            enable_lifecycle_testing: true,
            event_timeout_ms: 10000,
            max_retries: 5,
            concurrent_operations: 50,
            memory_test_duration_secs: 60,
        }
    }
}

/// Test results and metrics
#[derive(Debug, Clone)]
pub struct Phase2TestResults {
    /// Overall test success
    pub success: bool,
    /// Individual test results
    pub test_results: HashMap<String, TestResult>,
    /// Performance metrics
    pub performance_metrics: PerformanceMetrics,
    /// Error summary
    pub error_summary: ErrorSummary,
    /// Test execution time
    pub total_execution_time: Duration,
}

/// Individual test result
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Test name
    pub name: String,
    /// Success status
    pub success: bool,
    /// Execution time
    pub duration: Duration,
    /// Error message (if any)
    pub error: Option<String>,
    /// Additional details
    pub details: HashMap<String, Value>,
}

/// Performance metrics from testing
#[derive(Debug, Clone, Default)]
pub struct PerformanceMetrics {
    /// Event processing rate (events/sec)
    pub event_processing_rate: f64,
    /// Average response time (ms)
    pub average_response_time: f64,
    /// Memory usage (MB)
    pub memory_usage_mb: f64,
    /// CPU usage (%)
    pub cpu_usage_percent: f64,
    /// Throughput (operations/sec)
    pub throughput: f64,
    /// Error rate (%)
    pub error_rate: f64,
}

/// Error summary from testing
#[derive(Debug, Clone, Default)]
pub struct ErrorSummary {
    /// Total errors encountered
    pub total_errors: u64,
    /// Circuit breaker activations
    pub circuit_breaker_activations: u64,
    /// Service failures
    pub service_failures: u64,
    /// Timeout errors
    pub timeout_errors: u64,
    /// Recovery successes
    pub recovery_successes: u64,
}

/// Complete Phase 2 Service Test Suite
pub struct Phase2ServiceTestSuite {
    /// Test configuration
    config: Phase2TestConfig,
    /// Event router for service coordination
    event_router: Arc<MockEventRouter>,
    /// All services
    services: TestServices,
    /// Event collector for validation
    event_collector: Arc<Mutex<Vec<DaemonEvent>>>,
    /// Test start time
    start_time: Instant,
}

/// All services under test
pub struct TestServices {
    pub script_engine: Arc<RwLock<ScriptEngineService>>,
    pub inference_engine: Arc<RwLock<InferenceEngineService>>,
    pub data_store: Arc<RwLock<CrucibleDataStore>>,
    pub mcp_gateway: Arc<RwLock<McpGateway>>,
}

impl Phase2ServiceTestSuite {
    /// Create a new Phase 2 test suite
    pub async fn new(config: Phase2TestConfig) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();

        // Create event router with comprehensive configuration
        let event_router = Arc::new(MockEventRouter::new());
        let event_collector = Arc::new(Mutex::new(Vec::new()));

        // Configure event router for Phase 2 testing
        if config.enable_full_stack {
            // Configure circuit breaker with sensitive thresholds for testing
            let circuit_breaker_config = CircuitBreakerConfig {
                failure_threshold: 3,
                success_threshold: 2,
                timeout: Duration::from_secs(10),
                max_retries: config.max_retries,
            };
            event_router.configure_circuit_breaker(circuit_breaker_config).await?;
        }

        // Configure load balancing for performance testing
        if config.enable_performance_testing {
            event_router.set_load_balancing_strategy(LoadBalancingStrategy::RoundRobin).await?;
        }

        // Create all services with production-like configuration
        let services = Self::create_services(event_router.clone()).await?;

        Ok(Self {
            config,
            event_router,
            services,
            event_collector,
            start_time,
        })
    }

    /// Create all services with proper configuration
    async fn create_services(event_router: Arc<MockEventRouter>) -> Result<TestServices, Box<dyn std::error::Error + Send + Sync>> {

        // 1. Create ScriptEngine Service
        let script_config = ScriptEngineConfig {
            max_concurrent_scripts: 20,
            script_timeout_seconds: 60,
            cache_enabled: true,
            security_sandbox_enabled: true,
            default_permissions: vec!["read".to_string(), "execute".to_string(), "write".to_string()],
        };
        let mut script_engine = ScriptEngineService::new(script_config, event_router.clone()).await?;
        script_engine.initialize_event_integration(event_router.clone()).await?;
        let script_engine = Arc::new(RwLock::new(script_engine));

        // 2. Create InferenceEngine Service with mock configuration
        let inference_config = InferenceEngineConfig {
            text_provider: crucible_llm::TextProviderConfig::mock(),
            embedding_provider: crucible_llm::EmbeddingConfig::mock(),
            default_models: DefaultModels {
                text_model: "mock-text-model".to_string(),
                embedding_model: "mock-embedding-model".to_string(),
                chat_model: "mock-chat-model".to_string(),
            },
            performance: PerformanceSettings {
                enable_batching: true,
                batch_size: 8,
                batch_timeout_ms: 2000,
                enable_deduplication: true,
                connection_pool_size: 20,
                request_timeout_ms: 30000,
            },
            cache: CacheSettings {
                enabled: true,
                ttl_seconds: 7200,
                max_size_bytes: 1024 * 1024 * 200, // 200MB
                eviction_policy: crucible_services::inference_engine::CacheEvictionPolicy::LRU,
            },
            limits: InferenceLimits {
                max_concurrent_requests: Some(20),
                max_request_tokens: Some(8192),
                max_response_tokens: Some(4096),
                request_timeout: Some(Duration::from_secs(60)),
                max_queue_size: Some(200),
            },
            monitoring: MonitoringSettings {
                enable_metrics: true,
                metrics_interval_seconds: 30,
                enable_profiling: true,
                export_metrics: false,
            },
        };
        let mut inference_engine = InferenceEngineService::new(inference_config).await?;
        inference_engine.initialize_event_integration(event_router.clone()).await?;
        let inference_engine = Arc::new(RwLock::new(inference_engine));

        // 3. Create DataStore Service
        let data_store_config = DataStoreConfig {
            backend: DatabaseBackend::Memory,
            database_config: DatabaseBackendConfig::Memory(crucible_services::data_store::MemoryConfig {
                max_documents: Some(50000),
                persist_to_disk: Some(false),
                persistence_path: None,
            }),
            connection_pool: crucible_services::data_store::ConnectionPoolConfig {
                max_connections: 20,
                min_connections: 2,
                connection_timeout: Duration::from_secs(30),
                idle_timeout: Duration::from_secs(300),
            },
            performance: crucible_services::data_store::PerformanceConfig {
                enable_query_cache: true,
                query_cache_size: 1000,
                enable_index_optimization: true,
                bulk_insert_batch_size: 100,
            },
            events: crucible_services::data_store::EventConfig {
                publish_data_changes: true,
                publish_schema_changes: true,
                batch_events: true,
                batch_size: 50,
            },
        };
        let mut data_store = CrucibleDataStore::new(data_store_config).await?;
        data_store.initialize_event_integration(event_router.clone()).await?;
        let data_store = Arc::new(RwLock::new(data_store));

        // 4. Create McpGateway Service
        let mcp_config = McpGatewayConfig::default();
        let mut mcp_gateway = McpGateway::new(mcp_config, event_router.clone())?;
        mcp_gateway.initialize_event_integration().await?;
        let mcp_gateway = Arc::new(RwLock::new(mcp_gateway));

        Ok(TestServices {
            script_engine,
            inference_engine,
            data_store,
            mcp_gateway,
        })
    }

    /// Execute the complete Phase 2 test suite
    pub async fn execute_complete_test_suite(&mut self) -> Result<Phase2TestResults, Box<dyn std::error::Error + Send + Sync>> {
        println!("\n🚀 Starting Phase 2 Comprehensive Service Integration Tests");
        println!("================================================================");

        let suite_start = Instant::now();
        let mut test_results = HashMap::new();
        let mut performance_metrics = PerformanceMetrics::default();
        let mut error_summary = ErrorSummary::default();

        // 1. Full Service Stack Validation
        if self.config.enable_full_stack {
            println!("\n📋 1. Full Service Stack Validation");
            println!("-----------------------------------");

            let result = self.test_full_service_stack().await;
            test_results.insert("full_service_stack".to_string(), result.clone());

            if !result.success {
                error_summary.total_errors += 1;
                error_summary.service_failures += 1;
            }
        }

        // 2. Event-Driven Coordination Tests
        println!("\n🔄 2. Event-Driven Coordination Tests");
        println!("--------------------------------------");

        let event_coordination_result = self.test_event_driven_coordination().await;
        test_results.insert("event_driven_coordination".to_string(), event_coordination_result.clone());

        if !event_coordination_result.success {
            error_summary.total_errors += 1;
        }

        // 3. Cross-Service Workflow Tests
        if self.config.enable_cross_service_workflows {
            println!("\n🔗 3. Cross-Service Workflow Tests");
            println!("---------------------------------");

            let workflow_result = self.test_cross_service_workflows().await;
            test_results.insert("cross_service_workflows".to_string(), workflow_result.clone());

            if !workflow_result.success {
                error_summary.total_errors += 1;
            }
        }

        // 4. Performance and Load Testing
        if self.config.enable_performance_testing {
            println!("\n⚡ 4. Performance and Load Testing");
            println!("---------------------------------");

            let performance_result = self.test_performance_under_load().await;
            performance_metrics = performance_result.details.get("performance_metrics")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            test_results.insert("performance_under_load".to_string(), performance_result.clone());

            if !performance_result.success {
                error_summary.total_errors += 1;
            }
        }

        // 5. Error Handling and Recovery Tests
        if self.config.enable_error_recovery_testing {
            println!("\n🛡️ 5. Error Handling and Recovery Tests");
            println!("--------------------------------------");

            let error_recovery_result = self.test_error_handling_and_recovery().await;
            test_results.insert("error_handling_recovery".to_string(), error_recovery_result.clone());

            if !error_recovery_result.success {
                error_summary.total_errors += 1;
                error_summary.recovery_successes += 1; // Successful recovery counts
            }
        }

        // 6. Configuration and Lifecycle Tests
        if self.config.enable_lifecycle_testing {
            println!("\n⚙️ 6. Configuration and Lifecycle Tests");
            println!("--------------------------------------");

            let lifecycle_result = self.test_configuration_and_lifecycle().await;
            test_results.insert("configuration_lifecycle".to_string(), lifecycle_result.clone());

            if !lifecycle_result.success {
                error_summary.total_errors += 1;
            }
        }

        // 7. Memory Leak and Resource Management Tests
        if self.config.enable_memory_testing {
            println!("\n🧠 7. Memory Leak and Resource Management Tests");
            println!("---------------------------------------------");

            let memory_result = self.test_memory_leak_and_resource_management().await;
            test_results.insert("memory_leak_resource_management".to_string(), memory_result.clone());

            if !memory_result.success {
                error_summary.total_errors += 1;
            }
        }

        // 8. JSON-RPC Tool Pattern Tests
        println!("\n🔧 8. JSON-RPC Tool Pattern Tests");
        println!("--------------------------------");

        let tool_pattern_result = self.test_json_rpc_tool_pattern().await;
        test_results.insert("json_rpc_tool_pattern".to_string(), tool_pattern_result.clone());

        if !tool_pattern_result.success {
            error_summary.total_errors += 1;
        }

        let total_execution_time = suite_start.elapsed();

        // Calculate overall success
        let success_count = test_results.values().filter(|r| r.success).count();
        let total_tests = test_results.len();
        let overall_success = success_count == total_tests;

        // Print final results
        println!("\n📊 Phase 2 Test Results Summary");
        println!("===============================");
        println!("Overall Success: {}/{} tests passed", success_count, total_tests);
        println!("Total Execution Time: {:?}", total_execution_time);
        println!("Event Processing Rate: {:.2} events/sec", performance_metrics.event_processing_rate);
        println!("Average Response Time: {:.2} ms", performance_metrics.average_response_time);
        println!("Memory Usage: {:.2} MB", performance_metrics.memory_usage_mb);
        println!("Total Errors: {}", error_summary.total_errors);

        if overall_success {
            println!("\n✅ Phase 2 Service Integration Tests PASSED!");
            println!("🎉 Our service ecosystem is working perfectly together!");
        } else {
            println!("\n❌ Phase 2 Service Integration Tests FAILED!");
            println!("🔧 Some issues need to be addressed before production deployment.");
        }

        Ok(Phase2TestResults {
            success: overall_success,
            test_results,
            performance_metrics,
            error_summary,
            total_execution_time,
        })
    }

    /// Start all services
    async fn start_all_services(&self) -> Result<(), ServiceError> {
        println!("Starting all services...");

        self.services.script_engine.write().await.start().await?;
        self.services.inference_engine.write().await.start().await?;
        self.services.data_store.write().await.start().await?;
        self.services.mcp_gateway.write().await.start().await?;

        // Wait for services to be ready
        tokio::time::sleep(Duration::from_millis(500)).await;

        println!("All services started successfully");
        Ok(())
    }

    /// Stop all services
    async fn stop_all_services(&self) -> Result<(), ServiceError> {
        println!("Stopping all services...");

        self.services.script_engine.write().await.stop().await?;
        self.services.inference_engine.write().await.stop().await?;
        self.services.data_store.write().await.stop().await?;
        self.services.mcp_gateway.write().await.stop().await?;

        println!("All services stopped successfully");
        Ok(())
    }

    /// Wait for events and collect them
    async fn wait_for_events(&self, expected_count: usize, timeout_ms: u64) -> bool {
        let start = Instant::now();
        let timeout = Duration::from_millis(timeout_ms);

        while start.elapsed() < timeout {
            let events = self.event_router.get_published_events().await;
            if events.len() >= expected_count {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }

    /// Clear all collected events
    async fn clear_events(&self) {
        self.event_collector.lock().await.clear();
        self.event_router.clear_events().await;
    }
}

// -------------------------------------------------------------------------
// Test Implementation Functions
// -------------------------------------------------------------------------

impl Phase2ServiceTestSuite {
    /// Test 1: Full Service Stack Validation
    async fn test_full_service_stack(&mut self) -> TestResult {
        let start_time = Instant::now();
        let mut details = HashMap::new();

        println!("Testing full service stack startup and registration...");

        let result = match self.start_all_services().await {
            Ok(_) => {
                // Verify all services are healthy
                let mut healthy_services = 0;
                let total_services = 4;

                // Check ScriptEngine health
                if let Ok(health) = self.services.script_engine.read().await.health_check().await {
                    if health.status == ServiceStatus::Healthy {
                        healthy_services += 1;
                    }
                    details.insert("script_engine_health".to_string(), json!(health));
                }

                // Check InferenceEngine health
                if let Ok(health) = self.services.inference_engine.read().await.health_check().await {
                    if health.status == ServiceStatus::Healthy {
                        healthy_services += 1;
                    }
                    details.insert("inference_engine_health".to_string(), json!(health));
                }

                // Check DataStore health
                if let Ok(health) = self.services.data_store.read().await.health_check().await {
                    if health.status == ServiceStatus::Healthy {
                        healthy_services += 1;
                    }
                    details.insert("datastore_health".to_string(), json!(health));
                }

                // Check McpGateway health
                if let Ok(health) = self.services.mcp_gateway.read().await.health_check().await {
                    if health.status == ServiceStatus::Healthy {
                        healthy_services += 1;
                    }
                    details.insert("mcp_gateway_health".to_string(), json!(health));
                }

                details.insert("healthy_services".to_string(), json!(healthy_services));
                details.insert("total_services".to_string(), json!(total_services));

                // Wait for service registration events
                tokio::time::sleep(Duration::from_millis(200)).await;
                let events = self.event_router.get_published_events().await;
                let registration_events = events.iter()
                    .filter(|e| matches!(&e.event_type, EventType::Service(crucible_services::events::core::ServiceEventType::ServiceRegistered { .. })))
                    .count();

                details.insert("registration_events".to_string(), json!(registration_events));

                if healthy_services == total_services && registration_events >= 4 {
                    println!("✅ All {} services are healthy and registered", total_services);
                    TestResult {
                        name: "Full Service Stack Validation".to_string(),
                        success: true,
                        duration: start_time.elapsed(),
                        error: None,
                        details,
                    }
                } else {
                    let error = format!("Only {}/{} services healthy, {}/{} registration events",
                        healthy_services, total_services, registration_events, 4);
                    println!("❌ {}", error);
                    TestResult {
                        name: "Full Service Stack Validation".to_string(),
                        success: false,
                        duration: start_time.elapsed(),
                        error: Some(error),
                        details,
                    }
                }
            }
            Err(e) => {
                let error = format!("Failed to start services: {}", e);
                println!("❌ {}", error);
                TestResult {
                    name: "Full Service Stack Validation".to_string(),
                    success: false,
                    duration: start_time.elapsed(),
                    error: Some(error),
                    details,
                }
            }
        };

        // Clean up
        let _ = self.stop_all_services().await;
        result
    }

    /// Test 2: Event-Driven Coordination
    async fn test_event_driven_coordination(&mut self) -> TestResult {
        let start_time = Instant::now();
        let mut details = HashMap::new();

        println!("Testing event-driven coordination between services...");

        // Start services
        if let Err(e) = self.start_all_services().await {
            return TestResult {
                name: "Event-Driven Coordination".to_string(),
                success: false,
                duration: start_time.elapsed(),
                error: Some(format!("Failed to start services: {}", e)),
                details,
            };
        }

        // Test 1: Service discovery via events
        println!("  Testing service discovery via events...");
        tokio::time::sleep(Duration::from_millis(100)).await;
        let events = self.event_router.get_published_events().await;
        let registration_events = events.iter()
            .filter(|e| matches!(&e.event_type, EventType::Service(crucible_services::events::core::ServiceEventType::ServiceRegistered { .. })))
            .count();

        details.insert("service_registration_events".to_string(), json!(registration_events));

        // Test 2: Cross-service event routing
        println!("  Testing cross-service event routing...");
        let test_event = DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom("coordination_test".to_string()),
            priority: EventPriority::Normal,
            source: EventSource::Service("test_coordinator".to_string()),
            targets: vec!["script-engine".to_string(), "inference-engine".to_string(), "datastore".to_string()],
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({
                "test_type": "cross_service_coordination",
                "timestamp": Utc::now().to_rfc3339(),
                "correlation_id": Uuid::new_v4().to_string(),
            })),
            metadata: HashMap::new(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            causation_id: None,
            retry_count: 0,
            max_retries: 3,
        };

        let publish_result = self.event_router.publish(Box::new(test_event)).await;
        details.insert("cross_service_event_published".to_string(), json!(publish_result.is_ok()));

        // Wait for event processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        let processed_events = self.event_router.get_published_events().await;
        let response_events = processed_events.iter()
            .filter(|e| {
                matches!(&e.event_type, EventType::Custom(event_type) if event_type.contains("coordination"))
            })
            .count();

        details.insert("cross_service_response_events".to_string(), json!(response_events));

        // Test 3: Event priority handling
        println!("  Testing event priority handling...");
        let priorities = vec![EventPriority::Critical, EventPriority::High, EventPriority::Normal, EventPriority::Low];
        let mut priority_events_published = 0;

        for (i, priority) in priorities.iter().enumerate() {
            let priority_event = DaemonEvent {
                id: Uuid::new_v4(),
                event_type: EventType::Custom(format!("priority_test_{}", i)),
                priority: *priority,
                source: EventSource::Service("priority_test_client".to_string()),
                targets: vec!["script-engine".to_string()],
                created_at: Utc::now(),
                scheduled_at: None,
                payload: EventPayload::json(json!({
                    "priority": format!("{:?}", priority),
                    "index": i,
                })),
                metadata: HashMap::new(),
                correlation_id: Some(Uuid::new_v4().to_string()),
                causation_id: None,
                retry_count: 0,
                max_retries: 3,
            };

            if self.event_router.publish(Box::new(priority_event)).await.is_ok() {
                priority_events_published += 1;
            }
        }

        details.insert("priority_events_published".to_string(), json!(priority_events_published));

        // Wait for priority processing
        tokio::time::sleep(Duration::from_millis(300)).await;

        let final_events = self.event_router.get_published_events().await;
        let priority_processed = final_events.iter()
            .filter(|e| {
                matches!(&e.event_type, EventType::Custom(event_type) if event_type.starts_with("priority_test"))
            })
            .count();

        details.insert("priority_events_processed".to_string(), json!(priority_processed));

        // Determine success
        let success = registration_events >= 4 &&
                     publish_result.is_ok() &&
                     response_events > 0 &&
                     priority_events_published == 4 &&
                     priority_processed >= 3;

        if success {
            println!("✅ Event-driven coordination working correctly");
        } else {
            println!("❌ Event-driven coordination has issues");
        }

        // Clean up
        let _ = self.stop_all_services().await;

        TestResult {
            name: "Event-Driven Coordination".to_string(),
            success,
            duration: start_time.elapsed(),
            error: if success { None } else { Some("Event coordination tests failed".to_string()) },
            details,
        }
    }

    /// Test 3: Cross-Service Workflows
    async fn test_cross_service_workflows(&mut self) -> TestResult {
        let start_time = Instant::now();
        let mut details = HashMap::new();

        println!("Testing cross-service workflows...");

        // Start services
        if let Err(e) = self.start_all_services().await {
            return TestResult {
                name: "Cross-Service Workflows".to_string(),
                success: false,
                duration: start_time.elapsed(),
                error: Some(format!("Failed to start services: {}", e)),
                details,
            };
        }

        // Workflow 1: Document Processing Pipeline
        println!("  Testing document processing pipeline...");

        // 1. Create document in DataStore
        let document = DocumentData {
            id: DocumentId("workflow_test_doc".to_string()),
            content: json!({
                "title": "Workflow Test Document",
                "content": "This document triggers a complete cross-service workflow",
                "metadata": {"workflow_id": "test_workflow_123", "type": "test_document"}
            }),
            metadata: DocumentMetadata {
                document_type: Some("workflow_test".to_string()),
                tags: vec!["test".to_string(), "workflow".to_string(), "integration".to_string()],
                author: Some("test_user".to_string()),
                content_hash: None,
                size_bytes: 250,
                custom: HashMap::new(),
            },
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let create_result = self.services.data_store.read().await
            .create("workflow_test_db", document.clone()).await;

        details.insert("document_created".to_string(), json!(create_result.is_ok()));

        if let Ok(created_id) = create_result {
            // 2. Trigger script processing via event
            let script_event = DaemonEvent {
                id: Uuid::new_v4(),
                event_type: EventType::Custom("document_processing_required".to_string()),
                priority: EventPriority::Normal,
                source: EventSource::Service("datastore".to_string()),
                targets: vec!["script-engine".to_string()],
                created_at: Utc::now(),
                scheduled_at: None,
                payload: EventPayload::json(json!({
                    "document_id": created_id,
                    "processing_type": "extract_metadata",
                    "workflow": "document_processing"
                })),
                metadata: HashMap::new(),
                correlation_id: Some(Uuid::new_v4().to_string()),
                causation_id: None,
                retry_count: 0,
                max_retries: 3,
            };

            let script_result = self.event_router.publish(Box::new(script_event)).await;
            details.insert("script_event_published".to_string(), json!(script_result.is_ok()));

            // 3. Trigger inference processing via event
            let inference_event = DaemonEvent {
                id: Uuid::new_v4(),
                event_type: EventType::Custom("embedding_generation_required".to_string()),
                priority: EventPriority::Normal,
                source: EventSource::Service("script-engine".to_string()),
                targets: vec!["inference-engine".to_string()],
                created_at: Utc::now(),
                scheduled_at: None,
                payload: EventPayload::json(json!({
                    "document_id": created_id,
                    "text": "This document triggers a complete cross-service workflow",
                    "operation": "generate_embeddings"
                })),
                metadata: HashMap::new(),
                correlation_id: Some(Uuid::new_v4().to_string()),
                causation_id: None,
                retry_count: 0,
                max_retries: 3,
            };

            let inference_result = self.event_router.publish(Box::new(inference_event)).await;
            details.insert("inference_event_published".to_string(), json!(inference_result.is_ok()));

            // 4. Register with MCP gateway
            let mcp_event = DaemonEvent {
                id: Uuid::new_v4(),
                event_type: EventType::Mcp(crucible_services::events::core::McpEventType::ToolCall {
                    tool_name: "register_document".to_string(),
                    parameters: json!({
                        "document_id": created_id,
                        "access_level": "public",
                        "workflow_integration": true
                    }),
                }),
                priority: EventPriority::Normal,
                source: EventSource::Service("inference-engine".to_string()),
                targets: vec!["mcp-gateway".to_string()],
                created_at: Utc::now(),
                scheduled_at: None,
                payload: EventPayload::json(json!({
                    "action": "register_document_for_mcp",
                    "document_id": created_id,
                    "integration_type": "cross_service_workflow"
                })),
                metadata: HashMap::new(),
                correlation_id: Some(Uuid::new_v4().to_string()),
                causation_id: None,
                retry_count: 0,
                max_retries: 3,
            };

            let mcp_result = self.event_router.publish(Box::new(mcp_event)).await;
            details.insert("mcp_event_published".to_string(), json!(mcp_result.is_ok()));

            // Wait for workflow completion
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Verify workflow events
            let events = self.event_router.get_published_events().await;
            let workflow_events = events.iter()
                .filter(|e| {
                    matches!(&e.event_type, EventType::Custom(event_type)
                        if event_type.contains("processing") || event_type.contains("embedding") || event_type.contains("workflow"))
                })
                .count();

            details.insert("workflow_events".to_string(), json!(workflow_events));

            // Workflow 2: Tool Execution Chain
            println!("  Testing tool execution chain...");

            let tool_request = ScriptExecutionRequest {
                script_id: "integration_test_script".to_string(),
                script_content: r#"
# Integration test script that simulates tool usage
result = {
    "status": "success",
    "operation": "cross_service_tool_test",
    "services_used": ["datastore", "inference_engine", "script_engine"],
    "timestamp": "$(date -Iseconds)"
}
print(f"Tool execution completed: {result}")
"#.to_string(),
                language: "bash".to_string(),
                parameters: HashMap::new(),
                permissions: vec!["execute".to_string(), "read".to_string()],
                timeout_ms: Some(5000),
                request_id: Uuid::new_v4().to_string(),
            };

            let tool_result = self.services.script_engine.write().await
                .execute_script(tool_request).await;

            details.insert("tool_execution_success".to_string(), json!(tool_result.is_ok()));

            if let Ok(response) = tool_result {
                details.insert("tool_execution_result".to_string(), json!(response.result));
            }
        }

        // Determine success
        let workflow_success = create_result.is_ok() &&
            details.get("script_event_published").and_then(|v| v.as_bool()).unwrap_or(false) &&
            details.get("inference_event_published").and_then(|v| v.as_bool()).unwrap_or(false) &&
            details.get("mcp_event_published").and_then(|v| v.as_bool()).unwrap_or(false) &&
            details.get("workflow_events").and_then(|v| v.as_u64()).unwrap_or(0) > 0 &&
            details.get("tool_execution_success").and_then(|v| v.as_bool()).unwrap_or(false);

        if workflow_success {
            println!("✅ Cross-service workflows working correctly");
        } else {
            println!("❌ Cross-service workflows have issues");
        }

        // Clean up
        let _ = self.stop_all_services().await;

        TestResult {
            name: "Cross-Service Workflows".to_string(),
            success: workflow_success,
            duration: start_time.elapsed(),
            error: if workflow_success { None } else { Some("Cross-service workflow tests failed".to_string()) },
            details,
        }
    }

    /// Test 4: Performance Under Load
    async fn test_performance_under_load(&mut self) -> TestResult {
        let start_time = Instant::now();
        let mut details = HashMap::new();

        println!("Testing performance under load...");

        // Start services
        if let Err(e) = self.start_all_services().await {
            return TestResult {
                name: "Performance Under Load".to_string(),
                success: false,
                duration: start_time.elapsed(),
                error: Some(format!("Failed to start services: {}", e)),
                details,
            };
        }

        let concurrent_ops = self.config.concurrent_operations;
        let event_count = 1000;
        println!("  Executing {} events with {} concurrent operations...", event_count, concurrent_ops);

        let performance_start = Instant::now();

        // Create concurrent load test
        let mut handles = Vec::new();
        let event_router = self.event_router.clone();

        for batch in 0..(concurrent_ops / 10) {
            let router = event_router.clone();
            let handle = tokio::spawn(async move {
                let mut batch_success = 0;
                let mut batch_total = 0;

                for i in 0..(event_count / (concurrent_ops / 10)) {
                    let event = DaemonEvent {
                        id: Uuid::new_v4(),
                        event_type: EventType::Custom(format!("performance_test_{}_{}", batch, i)),
                        priority: EventPriority::Normal,
                        source: EventSource::Service("performance_test_client".to_string()),
                        targets: vec!["script-engine".to_string()],
                        created_at: Utc::now(),
                        scheduled_at: None,
                        payload: EventPayload::json(json!({
                            "batch": batch,
                            "index": i,
                            "performance": true,
                            "data": "x".repeat(100), // 100 bytes per event
                        })),
                        metadata: HashMap::new(),
                        correlation_id: Some(Uuid::new_v4().to_string()),
                        causation_id: None,
                        retry_count: 0,
                        max_retries: 2,
                    };

                    batch_total += 1;
                    if router.publish(Box::new(event)).await.is_ok() {
                        batch_success += 1;
                    }
                }

                (batch_success, batch_total)
            });
            handles.push(handle);
        }

        // Wait for all batches to complete
        let mut total_success = 0;
        let mut total_attempts = 0;

        for handle in handles {
            if let Ok((success, total)) = handle.await {
                total_success += success;
                total_attempts += total;
            }
        }

        let publishing_time = performance_start.elapsed();

        // Wait for event processing
        tokio::time::sleep(Duration::from_millis(2000)).await;

        let total_time = performance_start.elapsed();

        // Collect metrics
        let events = self.event_router.get_published_events().await;
        let processed_events = events.iter()
            .filter(|e| {
                matches!(&e.event_type, EventType::Custom(event_type) if event_type.starts_with("performance_test"))
            })
            .count();

        // Calculate performance metrics
        let publishing_rate = total_attempts as f64 / publishing_time.as_secs_f64();
        let processing_rate = processed_events as f64 / total_time.as_secs_f64();
        let average_response_time = total_time.as_millis() as f64 / processed_events.max(1) as f64;
        let success_rate = (total_success as f64 / total_attempts.max(1) as f64) * 100.0;

        // Get service metrics
        let script_metrics = self.services.script_engine.read().await.get_metrics().await.unwrap_or_default();
        let inference_metrics = self.services.inference_engine.read().await.get_metrics().await.unwrap_or_default();
        let datastore_metrics = self.services.data_store.read().await.get_metrics().await.unwrap_or_default();

        let avg_memory = (script_metrics.memory_usage + inference_metrics.memory_usage + datastore_metrics.memory_usage) as f64 / 3.0 / 1024.0 / 1024.0;

        let performance_metrics = PerformanceMetrics {
            event_processing_rate: processing_rate,
            average_response_time,
            memory_usage_mb: avg_memory,
            cpu_usage_percent: 0.0, // Would need system monitoring
            throughput: processing_rate,
            error_rate: 100.0 - success_rate,
        };

        details.insert("total_events_sent".to_string(), json!(total_attempts));
        details.insert("total_events_processed".to_string(), json!(processed_events));
        details.insert("publishing_rate".to_string(), json!(publishing_rate));
        details.insert("processing_rate".to_string(), json!(processing_rate));
        details.insert("success_rate".to_string(), json!(success_rate));
        details.insert("average_response_time".to_string(), json!(average_response_time));
        details.insert("memory_usage_mb".to_string(), json!(avg_memory));
        details.insert("performance_metrics".to_string(), serde_json::to_value(&performance_metrics).unwrap());

        // Performance criteria
        let performance_criteria_met = processing_rate > 100.0 && // > 100 events/sec
            success_rate > 95.0 && // > 95% success rate
            average_response_time < 100.0 && // < 100ms avg response
            avg_memory < 100.0; // < 100MB memory usage

        if performance_criteria_met {
            println!("✅ Performance test passed");
            println!("  Processing rate: {:.2} events/sec", processing_rate);
            println!("  Success rate: {:.1}%", success_rate);
            println!("  Average response time: {:.2} ms", average_response_time);
            println!("  Memory usage: {:.2} MB", avg_memory);
        } else {
            println!("❌ Performance test failed criteria");
        }

        // Clean up
        let _ = self.stop_all_services().await;

        TestResult {
            name: "Performance Under Load".to_string(),
            success: performance_criteria_met,
            duration: start_time.elapsed(),
            error: if performance_criteria_met { None } else { Some("Performance criteria not met".to_string()) },
            details,
        }
    }

    /// Test 5: Error Handling and Recovery
    async fn test_error_handling_and_recovery(&mut self) -> TestResult {
        let start_time = Instant::now();
        let mut details = HashMap::new();

        println!("Testing error handling and recovery...");

        // Start services
        if let Err(e) = self.start_all_services().await {
            return TestResult {
                name: "Error Handling and Recovery".to_string(),
                success: false,
                duration: start_time.elapsed(),
                error: Some(format!("Failed to start services: {}", e)),
                details,
            };
        }

        // Test 1: Circuit breaker activation
        println!("  Testing circuit breaker activation...");

        // Configure sensitive circuit breaker for testing
        let circuit_breaker_config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_millis(500),
            max_retries: 2,
        };
        let _ = self.event_router.configure_circuit_breaker(circuit_breaker_config).await;

        // Send failing events to trigger circuit breaker
        let mut failures = 0;
        for i in 0..5 {
            let failing_event = DaemonEvent {
                id: Uuid::new_v4(),
                event_type: EventType::Custom(format!("failure_test_{}", i)),
                priority: EventPriority::Normal,
                source: EventSource::Service("error_test_client".to_string()),
                targets: vec!["nonexistent_service".to_string()], // This will fail
                created_at: Utc::now(),
                scheduled_at: None,
                payload: EventPayload::json(json!({
                    "test_index": i,
                    "should_fail": true,
                })),
                metadata: HashMap::new(),
                correlation_id: Some(Uuid::new_v4().to_string()),
                causation_id: None,
                retry_count: 0,
                max_retries: 2,
            };

            if self.event_router.publish(Box::new(failing_event)).await.is_err() {
                failures += 1;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        details.insert("circuit_breaker_failures".to_string(), json!(failures));

        // Check circuit breaker state
        let circuit_state = self.event_router.get_circuit_breaker_state().await;
        details.insert("circuit_breaker_open".to_string(), json!(circuit_state.is_open));
        details.insert("circuit_breaker_half_open".to_string(), json!(circuit_state.is_half_open));

        // Test 2: Service recovery
        println!("  Testing service recovery...");

        // Wait for circuit breaker timeout
        tokio::time::sleep(Duration::from_millis(600)).await;

        // Send successful events to close circuit breaker
        let mut recovery_successes = 0;
        for i in 0..4 {
            let recovery_event = DaemonEvent {
                id: Uuid::new_v4(),
                event_type: EventType::Custom(format!("recovery_test_{}", i)),
                priority: EventPriority::Normal,
                source: EventSource::Service("recovery_test_client".to_string()),
                targets: vec!["script-engine".to_string()], // This should work
                created_at: Utc::now(),
                scheduled_at: None,
                payload: EventPayload::json(json!({
                    "test_index": i,
                    "recovery": true,
                })),
                metadata: HashMap::new(),
                correlation_id: Some(Uuid::new_v4().to_string()),
                causation_id: None,
                retry_count: 0,
                max_retries: 2,
            };

            if self.event_router.publish(Box::new(recovery_event)).await.is_ok() {
                recovery_successes += 1;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        details.insert("recovery_successes".to_string(), json!(recovery_successes));

        // Check final circuit breaker state
        tokio::time::sleep(Duration::from_millis(200)).await;
        let final_circuit_state = self.event_router.get_circuit_breaker_state().await;
        details.insert("circuit_breaker_closed_after_recovery".to_string(), json!(!final_circuit_state.is_open && !final_circuit_state.is_half_open));

        // Test 3: Service restart resilience
        println!("  Testing service restart resilience...");

        // Store initial state
        let initial_events = self.event_router.get_published_events().await.len();

        // Restart one service
        let restart_result = self.services.data_store.write().await.restart().await;
        details.insert("service_restart_success".to_string(), json!(restart_result.is_ok()));

        // Verify service is still responsive
        tokio::time::sleep(Duration::from_millis(300)).await;

        let post_restart_health = self.services.data_store.read().await.health_check().await;
        details.insert("post_restart_health".to_string(), json!(post_restart_health.is_ok()));

        // Send test events after restart
        let post_restart_success = self.event_router.publish(Box::new(DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom("post_restart_test".to_string()),
            priority: EventPriority::Normal,
            source: EventSource::Service("restart_test_client".to_string()),
            targets: vec!["datastore".to_string()],
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(json!({"restart_resilience": true})),
            metadata: HashMap::new(),
            correlation_id: Some(Uuid::new_v4().to_string()),
            causation_id: None,
            retry_count: 0,
            max_retries: 3,
        })).await.is_ok();

        details.insert("post_restart_event_success".to_string(), json!(post_restart_success));

        let final_events = self.event_router.get_published_events().await.len();
        details.insert("events_after_restart".to_string(), json!(final_events > initial_events));

        // Determine success
        let error_handling_success = failures >= 3 && // Circuit breaker should trigger
            circuit_state.is_open &&
            recovery_successes >= 3 && // Should recover successfully
            !final_circuit_state.is_open && // Circuit breaker should be closed
            restart_result.is_ok() &&
            post_restart_health.is_ok() &&
            post_restart_success;

        if error_handling_success {
            println!("✅ Error handling and recovery working correctly");
        } else {
            println!("❌ Error handling and recovery has issues");
        }

        // Clean up
        let _ = self.stop_all_services().await;

        TestResult {
            name: "Error Handling and Recovery".to_string(),
            success: error_handling_success,
            duration: start_time.elapsed(),
            error: if error_handling_success { None } else { Some("Error handling tests failed".to_string()) },
            details,
        }
    }

    /// Test 6: Configuration and Lifecycle Management
    async fn test_configuration_and_lifecycle(&mut self) -> TestResult {
        let start_time = Instant::now();
        let mut details = HashMap::new();

        println!("Testing configuration and lifecycle management...");

        // Test 1: Service startup sequence
        println!("  Testing service startup sequence...");

        let startup_start = Instant::now();
        let startup_result = self.start_all_services().await;
        let startup_time = startup_start.elapsed();

        details.insert("startup_success".to_string(), json!(startup_result.is_ok()));
        details.insert("startup_time_ms".to_string(), json!(startup_time.as_millis()));

        if startup_result.is_ok() {
            // Test 2: Configuration validation
            println!("  Testing configuration validation...");

            let script_config = self.services.script_engine.read().await.get_config().await;
            let inference_config = self.services.inference_engine.read().await.get_config().await;

            details.insert("script_config_available".to_string(), json!(script_config.is_ok()));
            details.insert("inference_config_available".to_string(), json!(inference_config.is_ok()));

            // Test 3: Runtime configuration updates
            println!("  Testing runtime configuration updates...");

            // Update script engine configuration
            let mut script_engine = self.services.script_engine.write().await;
            let original_config = script_engine.get_config().await.unwrap_or_default();
            let mut updated_config = original_config.clone();
            updated_config.max_concurrent_scripts = 30; // Change from default

            let config_update_result = script_engine.update_config(updated_config).await;
            details.insert("script_config_update_success".to_string(), json!(config_update_result.is_ok()));

            drop(script_engine); // Release write lock

            // Test 4: Health monitoring
            println!("  Testing health monitoring...");

            let script_health = self.services.script_engine.read().await.health_check().await;
            let inference_health = self.services.inference_engine.read().await.health_check().await;
            let datastore_health = self.services.data_store.read().await.health_check().await;
            let mcp_health = self.services.mcp_gateway.read().await.health_check().await;

            let healthy_services = [script_health, inference_health, datastore_health, mcp_health]
                .iter()
                .filter(|h| h.as_ref().map(|h| h.status == ServiceStatus::Healthy).unwrap_or(false))
                .count();

            details.insert("healthy_services".to_string(), json!(healthy_services));
            details.insert("total_services".to_string(), json!(4));

            // Test 5: Graceful shutdown
            println!("  Testing graceful shutdown...");

            let shutdown_start = Instant::now();
            let shutdown_result = self.stop_all_services().await;
            let shutdown_time = shutdown_start.elapsed();

            details.insert("shutdown_success".to_string(), json!(shutdown_result.is_ok()));
            details.insert("shutdown_time_ms".to_string(), json!(shutdown_time.as_millis()));

            // Test 6: Service restart
            println!("  Testing service restart...");

            let restart_start = Instant::now();
            let restart_result = self.start_all_services().await;
            let restart_time = restart_start.elapsed();

            details.insert("restart_success".to_string(), json!(restart_result.is_ok()));
            details.insert("restart_time_ms".to_string(), json!(restart_time.as_millis()));

            if restart_result.is_ok() {
                // Verify services are healthy after restart
                let post_restart_health = self.services.script_engine.read().await.health_check().await;
                details.insert("post_restart_healthy".to_string(),
                    json!(post_restart_health.is_ok() &&
                          post_restart_health.unwrap().status == ServiceStatus::Healthy));
            }

            // Test 7: Metrics collection
            println!("  Testing metrics collection...");

            let script_metrics = self.services.script_engine.read().await.get_metrics().await;
            let inference_metrics = self.services.inference_engine.read().await.get_metrics().await;

            details.insert("script_metrics_available".to_string(), json!(script_metrics.is_ok()));
            details.insert("inference_metrics_available".to_string(), json!(inference_metrics.is_ok()));

            if let (Ok(script_metrics), Ok(inference_metrics)) = (script_metrics, inference_metrics) {
                details.insert("script_total_requests".to_string(), json!(script_metrics.total_requests));
                details.insert("inference_total_requests".to_string(), json!(inference_metrics.total_requests));
            }
        }

        // Determine success
        let lifecycle_success = startup_result.is_ok() &&
            details.get("healthy_services").and_then(|v| v.as_u64()).unwrap_or(0) == 4 &&
            details.get("script_config_update_success").and_then(|v| v.as_bool()).unwrap_or(false) &&
            details.get("shutdown_success").and_then(|v| v.as_bool()).unwrap_or(false) &&
            details.get("restart_success").and_then(|v| v.as_bool()).unwrap_or(false) &&
            startup_time.as_millis() < 5000 && // Should start within 5 seconds
            shutdown_time.as_millis() < 2000; // Should shutdown within 2 seconds

        if lifecycle_success {
            println!("✅ Configuration and lifecycle management working correctly");
        } else {
            println!("❌ Configuration and lifecycle management has issues");
        }

        // Clean up if services are still running
        let _ = self.stop_all_services().await;

        TestResult {
            name: "Configuration and Lifecycle Management".to_string(),
            success: lifecycle_success,
            duration: start_time.elapsed(),
            error: if lifecycle_success { None } else { Some("Lifecycle management tests failed".to_string()) },
            details,
        }
    }

    /// Test 7: Memory Leak and Resource Management
    async fn test_memory_leak_and_resource_management(&mut self) -> TestResult {
        let start_time = Instant::now();
        let mut details = HashMap::new();

        println!("Testing memory leak and resource management...");

        // Start services
        if let Err(e) = self.start_all_services().await {
            return TestResult {
                name: "Memory Leak and Resource Management".to_string(),
                success: false,
                duration: start_time.elapsed(),
                error: Some(format!("Failed to start services: {}", e)),
                details,
            };
        }

        // Get baseline memory usage
        let baseline_script_metrics = self.services.script_engine.read().await.get_metrics().await.unwrap_or_default();
        let baseline_inference_metrics = self.services.inference_engine.read().await.get_metrics().await.unwrap_or_default();
        let baseline_datastore_metrics = self.services.data_store.read().await.get_metrics().await.unwrap_or_default();

        let baseline_memory = baseline_script_metrics.memory_usage +
                             baseline_inference_metrics.memory_usage +
                             baseline_datastore_metrics.memory_usage;

        details.insert("baseline_memory_bytes".to_string(), json!(baseline_memory));

        // Memory stress test
        println!("  Running memory stress test for {} seconds...", self.config.memory_test_duration_secs);

        let stress_start = Instant::now();
        let test_duration = Duration::from_secs(self.config.memory_test_duration_secs);
        let mut operations_completed = 0;
        let mut memory_samples = Vec::new();

        while stress_start.elapsed() < test_duration {
            // Create memory pressure with various operations

            // 1. Script execution with memory allocation
            let script_request = ScriptExecutionRequest {
                script_id: format!("memory_test_script_{}", operations_completed),
                script_content: format!(r#"
# Memory test script {}
data = []
for i in range(1000):
    data.append("x" * 100)  # Allocate memory
result = {{
    "operation": "memory_test",
    "data_size": len(data),
    "timestamp": "$(date -Iseconds)"
}}
print(f"Memory test operation {} completed")
"#, operations_completed, operations_completed),
                language: "python".to_string(),
                parameters: HashMap::new(),
                permissions: vec!["execute".to_string()],
                timeout_ms: Some(5000),
                request_id: Uuid::new_v4().to_string(),
            };

            let _ = self.services.script_engine.write().await.execute_script(script_request).await;

            // 2. DataStore operations
            let test_document = DocumentData {
                id: DocumentId(format!("memory_test_doc_{}", operations_completed)),
                content: json!({
                    "test_data": "x".repeat(1000), // 1KB of data
                    "operation_id": operations_completed,
                    "timestamp": Utc::now().to_rfc3339(),
                }),
                metadata: DocumentMetadata {
                    document_type: Some("memory_test".to_string()),
                    tags: vec!["memory".to_string(), "test".to_string()],
                    author: Some("memory_test_user".to_string()),
                    content_hash: None,
                    size_bytes: 1000,
                    custom: HashMap::new(),
                },
                version: 1,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            let _ = self.services.data_store.read().await
                .create("memory_test_db", test_document).await;

            // 3. Event publishing
            let memory_event = DaemonEvent {
                id: Uuid::new_v4(),
                event_type: EventType::Custom(format!("memory_test_event_{}", operations_completed)),
                priority: EventPriority::Normal,
                source: EventSource::Service("memory_test_client".to_string()),
                targets: vec!["script-engine".to_string(), "datastore".to_string()],
                created_at: Utc::now(),
                scheduled_at: None,
                payload: EventPayload::json(json!({
                    "operation_id": operations_completed,
                    "data": "x".repeat(500), // 500 bytes of payload data
                })),
                metadata: HashMap::new(),
                correlation_id: Some(Uuid::new_v4().to_string()),
                causation_id: None,
                retry_count: 0,
                max_retries: 2,
            };

            let _ = self.event_router.publish(Box::new(memory_event)).await;

            operations_completed += 1;

            // Sample memory usage every 10 operations
            if operations_completed % 10 == 0 {
                let current_script_metrics = self.services.script_engine.read().await.get_metrics().await.unwrap_or_default();
                let current_inference_metrics = self.services.inference_engine.read().await.get_metrics().await.unwrap_or_default();
                let current_datastore_metrics = self.services.data_store.read().await.get_metrics().await.unwrap_or_default();

                let current_memory = current_script_metrics.memory_usage +
                                   current_inference_metrics.memory_usage +
                                   current_datastore_metrics.memory_usage;

                memory_samples.push(current_memory);
            }

            // Small delay to prevent overwhelming the system
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let stress_time = stress_start.elapsed();
        details.insert("stress_test_duration_ms".to_string(), json!(stress_time.as_millis()));
        details.insert("operations_completed".to_string(), json!(operations_completed));
        details.insert("memory_samples".to_string(), json!(memory_samples.len()));

        // Get final memory usage
        let final_script_metrics = self.services.script_engine.read().await.get_metrics().await.unwrap_or_default();
        let final_inference_metrics = self.services.inference_engine.read().await.get_metrics().await.unwrap_or_default();
        let final_datastore_metrics = self.services.data_store.read().await.get_metrics().await.unwrap_or_default();

        let final_memory = final_script_metrics.memory_usage +
                           final_inference_metrics.memory_usage +
                           final_datastore_metrics.memory_usage;

        details.insert("final_memory_bytes".to_string(), json!(final_memory));

        let memory_increase = final_memory.saturating_sub(baseline_memory);
        let memory_increase_mb = memory_increase as f64 / 1024.0 / 1024.0;
        details.insert("memory_increase_bytes".to_string(), json!(memory_increase));
        details.insert("memory_increase_mb".to_string(), json!(memory_increase_mb));

        // Memory cleanup test
        println!("  Testing memory cleanup...");

        // Clear events and data
        self.clear_events().await;

        // Trigger garbage collection if possible
        let _ = self.services.script_engine.write().await.cleanup_resources().await;
        let _ = self.services.data_store.read().await.cleanup_resources().await;

        // Wait for cleanup
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Get post-cleanup memory usage
        let cleanup_script_metrics = self.services.script_engine.read().await.get_metrics().await.unwrap_or_default();
        let cleanup_inference_metrics = self.services.inference_engine.read().await.get_metrics().await.unwrap_or_default();
        let cleanup_datastore_metrics = self.services.data_store.read().await.get_metrics().await.unwrap_or_default();

        let cleanup_memory = cleanup_script_metrics.memory_usage +
                             cleanup_inference_metrics.memory_usage +
                             cleanup_datastore_metrics.memory_usage;

        details.insert("cleanup_memory_bytes".to_string(), json!(cleanup_memory));

        let memory_after_cleanup = cleanup_memory.saturating_sub(baseline_memory);
        let memory_recovered = memory_increase.saturating_sub(memory_after_cleanup);
        let recovery_rate = if memory_increase > 0 {
            (memory_recovered as f64 / memory_increase as f64) * 100.0
        } else {
            100.0
        };

        details.insert("memory_recovered_bytes".to_string(), json!(memory_recovered));
        details.insert("recovery_rate_percent".to_string(), json!(recovery_rate));

        // Determine success
        let memory_management_success = memory_increase_mb < 100.0 && // Less than 100MB increase
            recovery_rate > 50.0 && // At least 50% memory recovered
            operations_completed > 100; // Reasonable number of operations completed

        if memory_management_success {
            println!("✅ Memory leak and resource management working correctly");
            println!("  Memory increase: {:.2} MB", memory_increase_mb);
            println!("  Memory recovery rate: {:.1}%", recovery_rate);
            println!("  Operations completed: {}", operations_completed);
        } else {
            println!("❌ Memory leak and resource management has issues");
        }

        // Clean up
        let _ = self.stop_all_services().await;

        TestResult {
            name: "Memory Leak and Resource Management".to_string(),
            success: memory_management_success,
            duration: start_time.elapsed(),
            error: if memory_management_success { None } else { Some("Memory management tests failed".to_string()) },
            details,
        }
    }

    /// Test 8: JSON-RPC Tool Pattern
    async fn test_json_rpc_tool_pattern(&mut self) -> TestResult {
        let start_time = Instant::now();
        let mut details = HashMap::new();

        println!("Testing JSON-RPC tool pattern...");

        // Start services
        if let Err(e) = self.start_all_services().await {
            return TestResult {
                name: "JSON-RPC Tool Pattern".to_string(),
                success: false,
                duration: start_time.elapsed(),
                error: Some(format!("Failed to start services: {}", e)),
                details,
            };
        }

        // Test 1: Simple JSON-RPC tool execution
        println!("  Testing simple JSON-RPC tool execution...");

        let simple_script = ScriptExecutionRequest {
            script_id: "json_rpc_test_simple".to_string(),
            script_content: r#"
#!/usr/bin/env python3
import json
import sys

# Simple JSON-RPC tool pattern
result = {
    "jsonrpc": "2.0",
    "result": {
        "status": "success",
        "message": "Hello from JSON-RPC tool!",
        "data": {
            "operation": "simple_test",
            "timestamp": "$(date -Iseconds)",
            "input_parameters": {"test": true}
        }
    },
    "id": 1
}

print(json.dumps(result, indent=2))
"#.to_string(),
            language: "python".to_string(),
            parameters: HashMap::new(),
            permissions: vec!["execute".to_string()],
            timeout_ms: Some(5000),
            request_id: Uuid::new_v4().to_string(),
        };

        let simple_result = self.services.script_engine.write().await.execute_script(simple_script).await;
        details.insert("simple_json_rpc_success".to_string(), json!(simple_result.is_ok()));

        if let Ok(response) = simple_result {
            if let Some(result_json) = &response.result {
                // Verify it's valid JSON-RPC format
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(result_json);
                details.insert("simple_json_rpc_valid".to_string(), json!(parsed.is_ok()));

                if let Ok(json_value) = parsed {
                    if let Some(result) = json_value.get("result") {
                        details.insert("simple_result_content".to_string(), result.clone());
                    }
                }
            }
        }

        // Test 2: Complex JSON-RPC tool with parameters
        println!("  Testing complex JSON-RPC tool with parameters...");

        let complex_script = ScriptExecutionRequest {
            script_id: "json_rpc_test_complex".to_string(),
            script_content: r#"
#!/usr/bin/env python3
import json
import sys

# Complex JSON-RPC tool with parameter processing
input_data = {
    "operation": "data_processing",
    "parameters": {
        "input_text": "This is a test for JSON-RPC tool pattern",
        "processing_options": {
            "uppercase": True,
            "word_count": True,
            "sentiment": "neutral"
        }
    }
}

# Process the data
text = input_data["parameters"]["input_text"]
options = input_data["parameters"]["processing_options"]

processed_result = {
    "original_text": text,
    "processed_text": text.upper() if options.get("uppercase") else text,
    "word_count": len(text.split()) if options.get("word_count") else None,
    "sentiment": options.get("sentiment"),
    "processing_timestamp": "$(date -Iseconds)"
}

# JSON-RPC response format
response = {
    "jsonrpc": "2.0",
    "result": {
        "status": "success",
        "operation": "data_processing",
        "data": processed_result
    },
    "id": 2
}

print(json.dumps(response, indent=2))
"#.to_string(),
            language: "python".to_string(),
            parameters: HashMap::from([
                ("input_text".to_string(), serde_json::Value::String("Testing complex JSON-RPC tool pattern".to_string())),
                ("uppercase".to_string(), serde_json::Value::Bool(true)),
            ]),
            permissions: vec!["execute".to_string()],
            timeout_ms: Some(10000),
            request_id: Uuid::new_v4().to_string(),
        };

        let complex_result = self.services.script_engine.write().await.execute_script(complex_script).await;
        details.insert("complex_json_rpc_success".to_string(), json!(complex_result.is_ok()));

        if let Ok(response) = complex_result {
            if let Some(result_json) = &response.result {
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(result_json);
                details.insert("complex_json_rpc_valid".to_string(), json!(parsed.is_ok()));

                if let Ok(json_value) = parsed {
                    if let Some(result) = json_value.get("result") {
                        details.insert("complex_result_content".to_string(), result.clone());
                    }
                }
            }
        }

        // Test 3: JSON-RPC error handling
        println!("  Testing JSON-RPC error handling...");

        let error_script = ScriptExecutionRequest {
            script_id: "json_rpc_test_error".to_string(),
            script_content: r#"
#!/usr/bin/env python3
import json

# JSON-RPC error response
error_response = {
    "jsonrpc": "2.0",
    "error": {
        "code": -32602,
        "message": "Invalid params",
        "data": {
            "details": "Parameter validation failed",
            "expected": "string",
            "received": "number",
            "parameter": "input_text"
        }
    },
    "id": 3
}

print(json.dumps(error_response, indent=2))
"#.to_string(),
            language: "python".to_string(),
            parameters: HashMap::from([
                ("input_text".to_string(), serde_json::Value::Number(serde_json::Number::from(123))), // Invalid type
            ]),
            permissions: vec!["execute".to_string()],
            timeout_ms: Some(5000),
            request_id: Uuid::new_v4().to_string(),
        };

        let error_result = self.services.script_engine.write().await.execute_script(error_script).await;
        details.insert("error_json_rpc_success".to_string(), json!(error_result.is_ok()));

        if let Ok(response) = error_result {
            if let Some(result_json) = &response.result {
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(result_json);
                details.insert("error_json_rpc_valid".to_string(), json!(parsed.is_ok()));

                if let Ok(json_value) = parsed {
                    if let Some(error) = json_value.get("error") {
                        details.insert("error_response_content".to_string(), error.clone());
                    }
                }
            }
        }

        // Test 4: Integration with DataStore via JSON-RPC
        println!("  Testing DataStore integration via JSON-RPC...");

        // Create a document first
        let test_doc = DocumentData {
            id: DocumentId("json_rpc_integration_doc".to_string()),
            content: json!({
                "title": "JSON-RPC Integration Test",
                "content": "Testing JSON-RPC pattern with DataStore integration",
                "test_type": "integration"
            }),
            metadata: DocumentMetadata {
                document_type: Some("json_rpc_test".to_string()),
                tags: vec!["json-rpc".to_string(), "integration".to_string()],
                author: Some("integration_test_user".to_string()),
                content_hash: None,
                size_bytes: 150,
                custom: HashMap::new(),
            },
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let doc_result = self.services.data_store.read().await
            .create("json_rpc_test_db", test_doc).await;

        details.insert("datastore_document_created".to_string(), json!(doc_result.is_ok()));

        if let Ok(doc_id) = doc_result {
            let integration_script = ScriptExecutionRequest {
                script_id: "json_rpc_integration".to_string(),
                script_content: format!(r#"
#!/usr/bin/env python3
import json

# Integration script that works with DataStore
document_id = "{}"

integration_result = {{
    "operation": "datastore_integration",
    "document_id": document_id,
    "status": "processed",
    "integration_features": [
        "document_creation",
        "json_rpc_response",
        "event_coordination"
    ],
    "processing_timestamp": "$(date -Iseconds)"
}}

response = {{
    "jsonrpc": "2.0",
    "result": {{
        "status": "success",
        "operation": "integration_test",
        "data": integration_result
    }},
    "id": 4
}}

print(json.dumps(response, indent=2))
"#, doc_id),
                language: "python".to_string(),
                parameters: HashMap::new(),
                permissions: vec!["execute".to_string(), "read".to_string()],
                timeout_ms: Some(10000),
                request_id: Uuid::new_v4().to_string(),
            };

            let integration_result = self.services.script_engine.write().await.execute_script(integration_script).await;
            details.insert("integration_json_rpc_success".to_string(), json!(integration_result.is_ok()));

            if let Ok(response) = integration_result {
                if let Some(result_json) = &response.result {
                    let parsed: Result<serde_json::Value, _> = serde_json::from_str(result_json);
                    details.insert("integration_json_rpc_valid".to_string(), json!(parsed.is_ok()));
                }
            }
        }

        // Test 5: Performance with JSON-RPC pattern
        println!("  Testing JSON-RPC pattern performance...");

        let performance_start = Instant::now();
        let mut successful_json_rpc_calls = 0;
        let total_calls = 50;

        for i in 0..total_calls {
            let perf_script = ScriptExecutionRequest {
                script_id: format!("json_rpc_perf_{}", i),
                script_content: format!(r#"
#!/usr/bin/env python3
import json

performance_result = {{
    "call_id": {},
    "operation": "performance_test",
    "data": "x" * 100,  # 100 bytes of data
    "timestamp": "$(date -Iseconds)"
}}

response = {{
    "jsonrpc": "2.0",
    "result": performance_result,
    "id": {}
}}

print(json.dumps(response))
"#, i, i),
                language: "python".to_string(),
                parameters: HashMap::new(),
                permissions: vec!["execute".to_string()],
                timeout_ms: Some(1000),
                request_id: Uuid::new_v4().to_string(),
            };

            if self.services.script_engine.write().await.execute_script(perf_script).await.is_ok() {
                successful_json_rpc_calls += 1;
            }
        }

        let performance_time = performance_start.elapsed();
        let json_rpc_rate = successful_json_rpc_calls as f64 / performance_time.as_secs_f64();
        let json_rpc_avg_time = performance_time.as_millis() as f64 / successful_json_rpc_calls.max(1) as f64;

        details.insert("json_rpc_performance_calls".to_string(), json!(successful_json_rpc_calls));
        details.insert("json_rpc_performance_total".to_string(), json!(total_calls));
        details.insert("json_rpc_rate".to_string(), json!(json_rpc_rate));
        details.insert("json_rpc_avg_time_ms".to_string(), json!(json_rpc_avg_time));

        // Determine success
        let json_rpc_success =
            details.get("simple_json_rpc_success").and_then(|v| v.as_bool()).unwrap_or(false) &&
            details.get("simple_json_rpc_valid").and_then(|v| v.as_bool()).unwrap_or(false) &&
            details.get("complex_json_rpc_success").and_then(|v| v.as_bool()).unwrap_or(false) &&
            details.get("complex_json_rpc_valid").and_then(|v| v.as_bool()).unwrap_or(false) &&
            details.get("error_json_rpc_success").and_then(|v| v.as_bool()).unwrap_or(false) &&
            details.get("error_json_rpc_valid").and_then(|v| v.as_bool()).unwrap_or(false) &&
            details.get("integration_json_rpc_success").and_then(|v| v.as_bool()).unwrap_or(false) &&
            successful_json_rpc_calls >= total_calls * 90 / 100 && // At least 90% success rate
            json_rpc_rate > 10.0 && // At least 10 calls/sec
            json_rpc_avg_time < 100.0; // Average time under 100ms

        if json_rpc_success {
            println!("✅ JSON-RPC tool pattern working correctly");
            println!("  Performance: {:.2} calls/sec, {:.2} ms avg", json_rpc_rate, json_rpc_avg_time);
        } else {
            println!("❌ JSON-RPC tool pattern has issues");
        }

        // Clean up
        let _ = self.stop_all_services().await;

        TestResult {
            name: "JSON-RPC Tool Pattern".to_string(),
            success: json_rpc_success,
            duration: start_time.elapsed(),
            error: if json_rpc_success { None } else { Some("JSON-RPC pattern tests failed".to_string()) },
            details,
        }
    }
}

// -------------------------------------------------------------------------
// Test Execution Functions
// -------------------------------------------------------------------------

/// Execute the complete Phase 2 test suite
pub async fn execute_phase2_tests() -> Result<Phase2TestResults, Box<dyn std::error::Error + Send + Sync>> {
    let config = Phase2TestConfig::default();
    let mut test_suite = Phase2ServiceTestSuite::new(config).await?;
    test_suite.execute_complete_test_suite().await
}

/// Execute Phase 2 tests with custom configuration
pub async fn execute_phase2_tests_with_config(config: Phase2TestConfig) -> Result<Phase2TestResults, Box<dyn std::error::Error + Send + Sync>> {
    let mut test_suite = Phase2ServiceTestSuite::new(config).await?;
    test_suite.execute_complete_test_suite().await
}

// -------------------------------------------------------------------------
// Unit Tests
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_phase2_test_suite_creation() {
        let config = Phase2TestConfig::default();
        let result = Phase2ServiceTestSuite::new(config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_phase2_configuration() {
        let config = Phase2TestConfig {
            enable_full_stack: false,
            enable_performance_testing: false,
            concurrent_operations: 10,
            memory_test_duration_secs: 5,
            ..Default::default()
        };

        assert!(!config.enable_full_stack);
        assert!(!config.enable_performance_testing);
        assert_eq!(config.concurrent_operations, 10);
        assert_eq!(config.memory_test_duration_secs, 5);
    }

    #[tokio::test]
    async fn test_phase2_results_structure() {
        let mut test_results = HashMap::new();
        test_results.insert("test1".to_string(), TestResult {
            name: "Test 1".to_string(),
            success: true,
            duration: Duration::from_millis(100),
            error: None,
            details: HashMap::new(),
        });

        let results = Phase2TestResults {
            success: true,
            test_results,
            performance_metrics: PerformanceMetrics::default(),
            error_summary: ErrorSummary::default(),
            total_execution_time: Duration::from_secs(10),
        };

        assert!(results.success);
        assert_eq!(results.test_results.len(), 1);
        assert!(results.test_results.contains_key("test1"));
    }
}