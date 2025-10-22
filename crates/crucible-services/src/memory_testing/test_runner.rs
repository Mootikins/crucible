//! Memory test runner for comprehensive service testing

use super::{
    MemoryTestFramework, MemoryTestConfig, MemoryTestResult, MemoryTestError,
    ServiceType, TestScenario,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};

/// Comprehensive memory test runner
pub struct MemoryTestRunner {
    framework: Arc<MemoryTestFramework>,
    service_managers: HashMap<ServiceType, Arc<dyn ServiceTestManager>>,
}

/// Trait for service-specific test management
#[async_trait::async_trait]
pub trait ServiceTestManager: Send + Sync {
    /// Get service type
    fn service_type(&self) -> ServiceType;

    /// Initialize service for testing
    async fn initialize(&self) -> Result<(), MemoryTestError>;

    /// Cleanup service after testing
    async fn cleanup(&self) -> Result<(), MemoryTestError>;

    /// Execute service-specific operation
    async fn execute_operation(&self, operation_type: &str, data: Option<&[u8]>) -> Result<HashMap<String, serde_json::Value>, MemoryTestError>;

    /// Get service-specific metrics
    async fn get_metrics(&self) -> Result<HashMap<String, f64>, MemoryTestError>;

    /// Apply load to service
    async fn apply_load(&self, load_level: f64) -> Result<(), MemoryTestError>;
}

/// ScriptEngine test manager
pub struct ScriptEngineTestManager {
    service: Arc<crate::script_engine::CrucibleScriptEngine>,
    initialized: Arc<RwLock<bool>>,
}

#[async_trait::async_trait]
impl ServiceTestManager for ScriptEngineTestManager {
    fn service_type(&self) -> ServiceType {
        ServiceType::ScriptEngine
    }

    async fn initialize(&self) -> Result<(), MemoryTestError> {
        let mut initialized = self.initialized.write().await;
        if *initialized {
            return Ok(());
        }

        info!("Initializing ScriptEngine for memory testing");

        // Start the service
        let mut service = Arc::as_ptr(&self.service) as *mut crate::script_engine::CrucibleScriptEngine;
        unsafe {
            let service_mut = &mut *service;
            service_mut.start().await.map_err(|e| {
                MemoryTestError::ServiceError(format!("Failed to start ScriptEngine: {}", e))
            })?;
        }

        // Pre-compile some scripts for testing
        let test_script = r#"
            pub fn main() -> String {
                "Hello, World!".to_string()
            }

            pub fn fibonacci(n: i32) -> i32 {
                if n <= 1 {
                    n
                } else {
                    fibonacci(n - 1) + fibonacci(n - 2)
                }
            }

            pub fn process_data(data: Vec<i32>) -> Vec<i32> {
                data.into_iter().map(|x| x * 2).collect()
            }
        "#;

        let compilation_context = crate::service_types::CompilationContext::default();
        let service_mut = unsafe { &mut *service };
        let _compiled = service_mut.compile_script(test_script, compilation_context).await.map_err(|e| {
            MemoryTestError::ServiceError(format!("Failed to compile test script: {}", e))
        })?;

        *initialized = true;
        info!("ScriptEngine initialized for memory testing");
        Ok(())
    }

    async fn cleanup(&self) -> Result<(), MemoryTestError> {
        info!("Cleaning up ScriptEngine after memory testing");

        let service_mut = unsafe { &mut *(Arc::as_ptr(&self.service) as *mut crate::script_engine::CrucibleScriptEngine) };
        service_mut.stop().await.map_err(|e| {
            MemoryTestError::ServiceError(format!("Failed to stop ScriptEngine: {}", e))
        })?;

        // Clear caches
        let _ = service_mut.clear_cache().await;

        let mut initialized = self.initialized.write().await;
        *initialized = false;

        info!("ScriptEngine cleanup completed");
        Ok(())
    }

    async fn execute_operation(&self, operation_type: &str, data: Option<&[u8]>) -> Result<HashMap<String, serde_json::Value>, MemoryTestError> {
        if !*self.initialized.read().await {
            return Err(MemoryTestError::ServiceError("ScriptEngine not initialized".to_string()));
        }

        let service_mut = unsafe { &mut *(Arc::as_ptr(&self.service) as *mut crate::script_engine::CrucibleScriptEngine) };

        match operation_type {
            "compile" => {
                let script_source = if let Some(data) = data {
                    String::from_utf8(data.to_vec()).unwrap_or_else(|_| "pub fn main() { 42 }".to_string())
                } else {
                    "pub fn main() { 42 }".to_string()
                };

                let context = crate::service_types::CompilationContext::default();
                let compiled = service_mut.compile_script(&script_source, context).await.map_err(|e| {
                    MemoryTestError::ServiceError(format!("Script compilation failed: {}", e))
                })?;

                Ok(HashMap::from([
                    ("script_id".to_string(), serde_json::Value::String(compiled.script_id)),
                    ("compiled_size".to_string(), serde_json::Value::Number(compiled.metadata.compiled_size.into())),
                ]))
            }
            "execute" => {
                let script_id = if let Some(data) = data {
                    String::from_utf8(data.to_vec()).unwrap_or_default()
                } else {
                    // Get first available script
                    let scripts = service_mut.list_scripts().await.map_err(|e| {
                        MemoryTestError::ServiceError(format!("Failed to list scripts: {}", e))
                    })?;
                    scripts.first().map(|s| s.script_id.clone()).unwrap_or_default()
                };

                if script_id.is_empty() {
                    return Err(MemoryTestError::ServiceError("No script available for execution".to_string()));
                }

                let execution_context = crate::service_types::ExecutionContext {
                    execution_id: uuid::Uuid::new_v4().to_string(),
                    script_id: script_id.clone(),
                    arguments: HashMap::new(),
                    environment: HashMap::new(),
                    working_directory: None,
                    security_context: crate::service_types::SecurityContext::default(),
                    timeout: Some(Duration::from_secs(5)),
                    available_tools: vec![],
                    user_context: None,
                };

                let result = service_mut.execute_script(&script_id, execution_context).await.map_err(|e| {
                    MemoryTestError::ServiceError(format!("Script execution failed: {}", e))
                })?;

                Ok(HashMap::from([
                    ("execution_id".to_string(), serde_json::Value::String(result.execution_id)),
                    ("success".to_string(), serde_json::Value::Bool(result.success)),
                    ("memory_usage".to_string(), serde_json::Value::Number(result.memory_usage.into())),
                    ("execution_time_ms".to_string(), serde_json::Value::Number(result.execution_time.as_millis().into())),
                ]))
            }
            "execute_source" => {
                let script_source = if let Some(data) = data {
                    String::from_utf8(data.to_vec()).unwrap_or_else(|_| "pub fn main() { 42 }".to_string())
                } else {
                    "pub fn main() { 42 }".to_string()
                };

                let execution_context = crate::service_types::ExecutionContext {
                    execution_id: uuid::Uuid::new_v4().to_string(),
                    script_id: format!("script_{}", uuid::Uuid::new_v4()),
                    arguments: HashMap::new(),
                    environment: HashMap::new(),
                    working_directory: None,
                    security_context: crate::service_types::SecurityContext::default(),
                    timeout: Some(Duration::from_secs(5)),
                    available_tools: vec![],
                    user_context: None,
                };

                let result = service_mut.execute_script_source(&script_source, execution_context).await.map_err(|e| {
                    MemoryTestError::ServiceError(format!("Direct script execution failed: {}", e))
                })?;

                Ok(HashMap::from([
                    ("execution_id".to_string(), serde_json::Value::String(result.execution_id)),
                    ("success".to_string(), serde_json::Value::Bool(result.success)),
                    ("memory_usage".to_string(), serde_json::Value::Number(result.memory_usage.into())),
                ]))
            }
            _ => Err(MemoryTestError::ServiceError(format!("Unknown operation type: {}", operation_type))),
        }
    }

    async fn get_metrics(&self) -> Result<HashMap<String, f64>, MemoryTestError> {
        if !*self.initialized.read().await {
            return Ok(HashMap::new());
        }

        let service_mut = unsafe { &*(Arc::as_ptr(&self.service) as *const crate::script_engine::CrucibleScriptEngine) };

        // Get service metrics
        let metrics = service_mut.get_metrics().await.map_err(|e| {
            MemoryTestError::ServiceError(format!("Failed to get ScriptEngine metrics: {}", e))
        })?;

        let stats = service_mut.get_execution_stats().await.map_err(|e| {
            MemoryTestError::ServiceError(format!("Failed to get ScriptEngine stats: {}", e))
        })?;

        let performance_metrics = service_mut.get_performance_metrics().await.map_err(|e| {
            MemoryTestError::ServiceError(format!("Failed to get ScriptEngine performance metrics: {}", e))
        })?;

        Ok(HashMap::from([
            ("total_requests".to_string(), metrics.total_requests as f64),
            ("successful_requests".to_string(), metrics.successful_requests as f64),
            ("failed_requests".to_string(), metrics.failed_requests as f64),
            ("average_response_time_ms".to_string(), metrics.average_response_time.as_millis() as f64),
            ("uptime_seconds".to_string(), metrics.uptime.as_secs() as f64),
            ("memory_usage".to_string(), metrics.memory_usage as f64),
            ("total_executions".to_string(), stats.total_executions as f64),
            ("successful_executions".to_string(), stats.successful_executions as f64),
            ("average_execution_time_ms".to_string(), stats.average_execution_time.as_millis() as f64),
            ("total_memory_used".to_string(), stats.total_memory_used as f64),
            ("active_connections".to_string(), performance_metrics.active_connections as f64),
            ("cache_size".to_string(), 1000.0), // Would get from actual cache
            ("cached_scripts".to_string(), 50.0), // Would get from actual cache
        ]))
    }

    async fn apply_load(&self, load_level: f64) -> Result<(), MemoryTestError> {
        if !*self.initialized.read().await {
            return Err(MemoryTestError::ServiceError("ScriptEngine not initialized".to_string()));
        }

        info!("Applying load {:.2} to ScriptEngine", load_level);

        let num_operations = (load_level * 10.0) as usize;
        let mut handles = Vec::new();

        for i in 0..num_operations {
            let manager = self.clone();
            let handle = tokio::spawn(async move {
                let operation_type = if i % 3 == 0 { "compile" } else { "execute" };
                let _ = manager.execute_operation(operation_type, None).await;
            });
            handles.push(handle);
        }

        // Wait for all operations to complete
        for handle in handles {
            let _ = handle.await;
        }

        Ok(())
    }
}

impl Clone for ScriptEngineTestManager {
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            initialized: self.initialized.clone(),
        }
    }
}

impl MemoryTestRunner {
    /// Create a new memory test runner
    pub async fn new(config: MemoryTestConfig) -> Result<Self, MemoryTestError> {
        let framework = Arc::new(MemoryTestFramework::new(config));

        // Initialize service managers
        let mut service_managers: HashMap<ServiceType, Arc<dyn ServiceTestManager>> = HashMap::new();

        // Create ScriptEngine manager
        let script_engine_config = crate::script_engine::ScriptEngineConfig::default();
        let script_engine = crate::script_engine::CrucibleScriptEngine::new(script_engine_config).await.map_err(|e| {
            MemoryTestError::ServiceError(format!("Failed to create ScriptEngine: {}", e))
        })?;

        let script_engine_manager = Arc::new(ScriptEngineTestManager {
            service: Arc::new(script_engine),
            initialized: Arc::new(RwLock::new(false)),
        });

        service_managers.insert(ServiceType::ScriptEngine, script_engine_manager);

        // TODO: Add other service managers (InferenceEngine, DataStore, McpGateway)

        Ok(Self {
            framework,
            service_managers,
        })
    }

    /// Run comprehensive memory tests for all services
    pub async fn run_comprehensive_tests(&self) -> Result<Vec<MemoryTestResult>, MemoryTestError> {
        info!("Starting comprehensive memory tests for all services");

        let mut all_results = Vec::new();

        // Test each service type
        for (service_type, _manager) in &self.service_managers {
            info!("Running comprehensive tests for service: {:?}", service_type);

            let service_results = self.run_service_tests(service_type).await?;
            all_results.extend(service_results);
        }

        // Generate summary report
        self.generate_summary_report(&all_results).await?;

        info!("Comprehensive memory tests completed with {} total results", all_results.len());
        Ok(all_results)
    }

    /// Run tests for a specific service
    pub async fn run_service_tests(&self, service_type: &ServiceType) -> Result<Vec<MemoryTestResult>, MemoryTestError> {
        info!("Running memory tests for service: {:?}", service_type);

        let manager = self.service_managers.get(service_type)
            .ok_or_else(|| MemoryTestError::ServiceError(format!("No manager for service: {:?}", service_type)))?;

        // Initialize service
        manager.initialize().await?;

        let mut results = Vec::new();

        // Define test scenarios
        let test_scenarios = vec![
            TestScenario::IdleBaseline,
            TestScenario::SingleOperation,
            TestScenario::HighFrequencyOperations,
            TestScenario::LargeDataProcessing,
            TestScenario::ConcurrentOperations,
            TestScenario::CleanupValidation,
        ];

        // Run each scenario
        for scenario in test_scenarios {
            info!("Running scenario {:?} for service {:?}", scenario, service_type);

            let test_data = HashMap::from([
                ("service_type".to_string(), serde_json::Value::String(format!("{:?}", service_type))),
                ("scenario".to_string(), serde_json::Value::String(format!("{:?}", scenario))),
            ]);

            let session_id = self.framework.start_test(service_type.clone(), scenario, test_data).await?;

            // Wait for test to complete
            self.wait_for_test_completion(&session_id, Duration::from_secs(3600)).await?;

            // Get results
            if let Some(result) = self.framework.get_test_results(&session_id).await? {
                results.push(result);
            } else {
                warn!("No results found for test session: {}", session_id);
            }
        }

        // Cleanup service
        manager.cleanup().await?;

        info!("Completed memory tests for service: {:?} with {} results", service_type, results.len());
        Ok(results)
    }

    /// Run a specific test scenario
    pub async fn run_test_scenario(
        &self,
        service_type: ServiceType,
        scenario: TestScenario,
        test_data: HashMap<String, serde_json::Value>,
    ) -> Result<MemoryTestResult, MemoryTestError> {
        info!("Running test scenario {:?} for service {:?}", scenario, service_type);

        let manager = self.service_managers.get(&service_type)
            .ok_or_else(|| MemoryTestError::ServiceError(format!("No manager for service: {:?}", service_type)))?;

        // Initialize service
        manager.initialize().await?;

        // Start test
        let session_id = self.framework.start_test(service_type, scenario.clone(), test_data).await?;

        // Wait for completion
        self.wait_for_test_completion(&session_id, Duration::from_secs(3600)).await?;

        // Get results
        let result = self.framework.get_test_results(&session_id).await?
            .ok_or_else(|| MemoryTestError::SessionNotFound(session_id))?;

        // Cleanup service
        manager.cleanup().await?;

        info!("Test scenario completed: {:?}", scenario);
        Ok(result)
    }

    /// Wait for test completion with timeout
    async fn wait_for_test_completion(&self, session_id: &str, timeout: Duration) -> Result<(), MemoryTestError> {
        let start_time = std::time::Instant::now();

        while start_time.elapsed() < timeout {
            let active_sessions = self.framework.get_active_sessions().await;

            if !active_sessions.contains_key(session_id) {
                return Ok(());
            }

            tokio::time::sleep(Duration::from_millis(1000)).await;
        }

        // Timeout reached, cancel the test
        warn!("Test timeout reached for session: {}, cancelling", session_id);
        self.framework.cancel_test(session_id).await?;

        Err(MemoryTestError::ServiceError(format!("Test timeout for session: {}", session_id)))
    }

    /// Generate summary report for all test results
    async fn generate_summary_report(&self, results: &[MemoryTestResult]) -> Result<(), MemoryTestError> {
        info!("Generating memory test summary report");

        let total_tests = results.len();
        let passed_tests = results.iter().filter(|r| matches!(r.status, super::TestStatus::Completed)).count();
        let failed_tests = total_tests - passed_tests;

        let mut total_violations = 0;
        let mut critical_violations = 0;
        let mut high_violations = 0;

        let mut leak_detected_count = 0;
        let mut total_leak_rate = 0.0;

        for result in results {
            total_violations += result.violations.len();

            for violation in &result.violations {
                match violation.severity {
                    super::ViolationSeverity::Critical => critical_violations += 1,
                    super::ViolationSeverity::High => high_violations += 1,
                    _ => {}
                }
            }

            if result.leak_detection.leak_detected {
                leak_detected_count += 1;
                total_leak_rate += result.leak_detection.leak_rate;
            }
        }

        // Calculate averages
        let average_leak_rate = if leak_detected_count > 0 {
            total_leak_rate / leak_detected_count as f64
        } else {
            0.0
        };

        // Print summary
        println!("\n" + "=".repeat(80).as_str());
        println!("MEMORY TEST SUMMARY REPORT");
        println!("=".repeat(80));

        println!("Test Results:");
        println!("  Total Tests:     {}", total_tests);
        println!("  Passed:          {} ({:.1}%)", passed_tests, (passed_tests as f64 / total_tests as f64) * 100.0);
        println!("  Failed:          {} ({:.1}%)", failed_tests, (failed_tests as f64 / total_tests as f64) * 100.0);

        println!("\nViolations:");
        println!("  Total Violations:    {}", total_violations);
        println!("  Critical:            {}", critical_violations);
        println!("  High:                {}", high_violations);

        println!("\nMemory Leaks:");
        println!("  Leaks Detected:      {} ({:.1}%)", leak_detected_count,
                 (leak_detected_count as f64 / total_tests as f64) * 100.0);
        if leak_detected_count > 0 {
            println!("  Average Leak Rate:   {:.2} bytes/s", average_leak_rate);
        }

        // Service-specific breakdown
        println!("\nService Breakdown:");
        for service_type in [ServiceType::ScriptEngine, ServiceType::InferenceEngine, ServiceType::DataStore, ServiceType::McpGateway] {
            let service_results: Vec<_> = results.iter()
                .filter(|r| r.service_type == service_type)
                .collect();

            if !service_results.is_empty() {
                let service_passed = service_results.iter()
                    .filter(|r| matches!(r.status, super::TestStatus::Completed))
                    .count();

                let service_violations: usize = service_results.iter()
                    .map(|r| r.violations.len())
                    .sum();

                let service_leaks = service_results.iter()
                    .filter(|r| r.leak_detection.leak_detected)
                    .count();

                println!("  {:?}: {} tests, {} passed, {} violations, {} leaks",
                         service_type, service_results.len(), service_passed, service_violations, service_leaks);
            }
        }

        // Recommendations
        if critical_violations > 0 || leak_detected_count > 0 {
            println!("\nRecommendations:");
            if critical_violations > 0 {
                println!("  - CRITICAL: Address critical memory violations immediately");
            }
            if leak_detected_count > 0 {
                println!("  - Investigate and fix memory leaks in {} services", leak_detected_count);
            }
            println!("  - Run detailed memory profiling with specialized tools");
            println!("  - Consider implementing more aggressive cleanup policies");
        } else {
            println!("\nStatus: All memory tests passed within acceptable limits");
        }

        println!("=".repeat(80) + "\n");

        Ok(())
    }

    /// Export test results to file
    pub async fn export_results(&self, results: &[MemoryTestResult], file_path: &str) -> Result<(), MemoryTestError> {
        use std::fs::File;
        use std::io::Write;

        let json = serde_json::to_string_pretty(results).map_err(|e| {
            MemoryTestError::IoError(std::io::Error::new(std::io::ErrorKind::Other, e))
        })?;

        let mut file = File::create(file_path).map_err(MemoryTestError::IoError)?;
        file.write_all(json.as_bytes()).map_err(MemoryTestError::IoError)?;

        info!("Test results exported to: {}", file_path);
        Ok(())
    }
}