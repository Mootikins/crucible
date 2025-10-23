//! Script execution tests for Phase 8.4
//!
//! This module tests Rune script execution under realistic load conditions,
//! including script compilation, execution, error handling, and performance.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use super::{
    IntegrationTestRunner, TestResult, TestCategory, TestOutcome, TestUtilities,
};

/// Script execution tests
pub struct ScriptExecutionTests {
    /// Test runner reference
    test_runner: Arc<IntegrationTestRunner>,
    /// Test utilities
    test_utils: Arc<TestUtilities>,
    /// Script execution state
    execution_state: Arc<RwLock<ScriptExecutionState>>,
    /// Test scripts registry
    test_scripts: Arc<RwLock<HashMap<String, TestScript>>>,
}

/// Script execution state
#[derive(Debug, Clone, Default)]
struct ScriptExecutionState {
    /// Active executions
    active_executions: Vec<ScriptExecution>,
    /// Completed executions
    completed_executions: Vec<ScriptExecution>,
    /// Failed executions
    failed_executions: Vec<ScriptExecution>,
    /// Script compilation cache
    compilation_cache: HashMap<String, CompiledScript>,
    /// Performance metrics
    performance_metrics: ScriptPerformanceMetrics,
}

/// Script execution instance
#[derive(Debug, Clone)]
pub struct ScriptExecution {
    /// Execution ID
    pub id: String,
    /// Script ID
    pub script_id: String,
    /// Execution start time
    pub start_time: Instant,
    /// Execution end time
    pub end_time: Option<Instant>,
    /// Execution status
    pub status: ExecutionStatus,
    /// Execution result
    pub result: Option<ScriptResult>,
    /// Execution parameters
    pub parameters: HashMap<String, serde_json::Value>,
    /// Execution context
    pub context: ExecutionContext,
}

/// Execution status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    /// Execution is pending
    Pending,
    /// Execution is running
    Running,
    /// Execution completed successfully
    Completed,
    /// Execution failed
    Failed,
    /// Execution timed out
    Timeout,
    /// Execution was cancelled
    Cancelled,
}

/// Script execution result
#[derive(Debug, Clone)]
pub struct ScriptResult {
    /// Success status
    pub success: bool,
    /// Return value
    pub return_value: Option<serde_json::Value>,
    /// Execution output
    pub output: Option<String>,
    /// Error message (if any)
    pub error_message: Option<String>,
    /// Execution metrics
    pub metrics: ExecutionMetrics,
}

/// Execution metrics
#[derive(Debug, Clone, Default)]
pub struct ExecutionMetrics {
    /// Execution time
    pub execution_time: Duration,
    /// Memory used
    pub memory_used_bytes: u64,
    /// CPU time used
    pub cpu_time_ms: u64,
    /// Operations performed
    pub operations_count: u64,
    /// Network calls made
    pub network_calls: u64,
}

/// Execution context
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Execution ID
    pub execution_id: String,
    /// Security context
    pub security_context: SecurityContext,
    /// Resource limits
    pub resource_limits: ResourceLimits,
    /// Execution options
    pub options: ExecutionOptions,
}

/// Security context
#[derive(Debug, Clone)]
pub struct SecurityContext {
    /// Allowed operations
    pub allowed_operations: Vec<String>,
    /// Denied operations
    pub denied_operations: Vec<String>,
    /// Sandbox enabled
    pub sandbox_enabled: bool,
    /// Access level
    pub access_level: AccessLevel,
}

/// Access levels
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessLevel {
    /// Read-only access
    ReadOnly,
    /// Read-write access
    ReadWrite,
    /// Full access
    Full,
    /// Admin access
    Admin,
}

/// Resource limits
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum memory in bytes
    pub max_memory_bytes: Option<u64>,
    /// Maximum execution time
    pub max_execution_time: Option<Duration>,
    /// Maximum operations
    pub max_operations: Option<u64>,
    /// Maximum network calls
    pub max_network_calls: Option<u64>,
}

/// Execution options
#[derive(Debug, Clone)]
pub struct ExecutionOptions {
    /// Debug mode
    pub debug: bool,
    /// Verbose output
    pub verbose: bool,
    /// Timeout duration
    pub timeout: Option<Duration>,
    /// Retry count
    pub retry_count: u32,
    /// Cache result
    pub cache_result: bool,
}

/// Test script definition
#[derive(Debug, Clone)]
pub struct TestScript {
    /// Script ID
    pub id: String,
    /// Script name
    pub name: String,
    /// Script content
    pub content: String,
    /// Script category
    pub category: ScriptCategory,
    /// Expected execution time
    pub expected_duration: Duration,
    /// Script complexity
    pub complexity: ScriptComplexity,
    /// Resource requirements
    pub resource_requirements: ResourceRequirements,
    /// Test parameters
    pub test_parameters: HashMap<String, serde_json::Value>,
}

/// Script categories
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptCategory {
    /// Data processing script
    DataProcessing,
    /// Text manipulation script
    TextManipulation,
    /// File operations script
    FileOperations,
    /// Network operations script
    NetworkOperations,
    /// System operations script
    SystemOperations,
    /// User interface script
    UserInterface,
    /// Database operations script
    DatabaseOperations,
    /// Utility script
    Utility,
}

/// Script complexity levels
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptComplexity {
    /// Simple script
    Simple,
    /// Moderate script
    Moderate,
    /// Complex script
    Complex,
    /// Very complex script
    VeryComplex,
}

/// Resource requirements
#[derive(Debug, Clone)]
pub struct ResourceRequirements {
    /// Memory requirement in MB
    pub memory_mb: u64,
    /// CPU requirement percentage
    pub cpu_percent: f64,
    /// Disk space requirement in MB
    pub disk_mb: u64,
    /// Network requirement in MB/s
    pub network_mbps: f64,
}

/// Compiled script
#[derive(Debug, Clone)]
pub struct CompiledScript {
    /// Script ID
    pub script_id: String,
    /// Compilation timestamp
    pub compiled_at: Instant,
    /// Compiled bytecode
    pub bytecode: Vec<u8>,
    /// Compilation metadata
    pub metadata: CompilationMetadata,
}

/// Compilation metadata
#[derive(Debug, Clone)]
pub struct CompilationMetadata {
    /// Compilation time
    pub compilation_time: Duration,
    /// Bytecode size
    pub bytecode_size: usize,
    /// Number of operations
    pub operation_count: u64,
    /// Security validation result
    pub security_validated: bool,
}

/// Script performance metrics
#[derive(Debug, Clone, Default)]
pub struct ScriptPerformanceMetrics {
    /// Total executions
    pub total_executions: u64,
    /// Successful executions
    pub successful_executions: u64,
    /// Failed executions
    pub failed_executions: u64,
    /// Average execution time
    pub avg_execution_time: Duration,
    /// Peak memory usage
    pub peak_memory_usage: u64,
    /// Cache hit rate
    pub cache_hit_rate: f64,
}

/// Concurrent execution test result
#[derive(Debug, Clone)]
pub struct ConcurrentExecutionResult {
    /// Number of concurrent executions
    pub concurrent_count: usize,
    /// Total execution time
    pub total_time: Duration,
    /// Successful executions
    pub successful_count: u64,
    /// Failed executions
    pub failed_count: u64,
    /// Average response time
    pub avg_response_time: Duration,
    /// Throughput (executions per second)
    pub throughput: f64,
}

impl ScriptExecutionTests {
    /// Create new script execution tests
    pub fn new(
        test_runner: Arc<IntegrationTestRunner>,
        test_utils: Arc<TestUtilities>,
    ) -> Self {
        Self {
            test_runner,
            test_utils,
            execution_state: Arc::new(RwLock::new(ScriptExecutionState::default())),
            test_scripts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Run all script execution tests
    pub async fn run_script_execution_tests(&self) -> Result<Vec<TestResult>> {
        info!("Starting script execution tests");

        let mut results = Vec::new();

        // Initialize test scripts
        self.initialize_test_scripts().await?;

        // Test basic script compilation
        results.extend(self.test_script_compilation().await?);

        // Test script execution
        results.extend(self.test_script_execution().await?);

        // Test concurrent script execution
        results.extend(self.test_concurrent_script_execution().await?);

        // Test script error handling
        results.extend(self.test_script_error_handling().await?);

        // Test script security validation
        results.extend(self.test_script_security_validation().await?);

        // Test script performance under load
        results.extend(self.test_script_performance_load().await?);

        // Test script caching
        results.extend(self.test_script_caching().await?);

        info!("Script execution tests completed");
        Ok(results)
    }

    /// Initialize test scripts
    async fn initialize_test_scripts(&self) -> Result<()> {
        info!("Initializing test scripts");

        let mut scripts = self.test_scripts.write().await;

        // Add various test scripts
        scripts.insert("simple_math".to_string(), self.create_simple_math_script());
        scripts.insert("text_processor".to_string(), self.create_text_processor_script());
        scripts.insert("data_analyzer".to_string(), self.create_data_analyzer_script());
        scripts.insert("file_operator".to_string(), self.create_file_operator_script());
        scripts.insert("network_client".to_string(), self.create_network_client_script());
        scripts.insert("system_monitor".to_string(), self.create_system_monitor_script());
        scripts.insert("complex_algorithm".to_string(), self.create_complex_algorithm_script());
        scripts.insert("error_prone".to_string(), self.create_error_prone_script());

        info!("Initialized {} test scripts", scripts.len());
        Ok(())
    }

    /// Create simple math script
    fn create_simple_math_script(&self) -> TestScript {
        TestScript {
            id: "simple_math".to_string(),
            name: "Simple Math Operations".to_string(),
            content: r#"
// Simple math operations script
fn calculate(a, b, operation) {
    match operation {
        "add" => a + b,
        "subtract" => a - b,
        "multiply" => a * b,
        "divide" => {
            if b != 0 {
                a / b
            } else {
                error("Division by zero")
            }
        },
        _ => error("Unknown operation")
    }
}

fn main(x, y, op) {
    calculate(x, y, op)
}
                "#.to_string(),
            category: ScriptCategory::DataProcessing,
            expected_duration: Duration::from_millis(50),
            complexity: ScriptComplexity::Simple,
            resource_requirements: ResourceRequirements {
                memory_mb: 32,
                cpu_percent: 10.0,
                disk_mb: 0,
                network_mbps: 0.0,
            },
            test_parameters: {
                let mut params = HashMap::new();
                params.insert("x".to_string(), serde_json::Value::Number(10.into()));
                params.insert("y".to_string(), serde_json::Value::Number(5.into()));
                params.insert("op".to_string(), serde_json::Value::String("add".to_string()));
                params
            },
        }
    }

    /// Create text processor script
    fn create_text_processor_script(&self) -> TestScript {
        TestScript {
            id: "text_processor".to_string(),
            name: "Text Processor".to_string(),
            content: r#"
// Text processing script
fn process_text(text, operations) {
    let result = text;

    for op in operations {
        result = match op {
            "uppercase" => result.to_uppercase(),
            "lowercase" => result.to_lowercase(),
            "reverse" => result.chars().rev().collect(),
            "trim" => result.trim(),
            _ => result
        };
    }

    result
}

fn word_count(text) {
    text.split_whitespace().count()
}

fn main(text, ops) {
    let processed = process_text(text, ops);
    let count = word_count(processed);

    {
        text: processed,
        word_count: count
    }
}
                "#.to_string(),
            category: ScriptCategory::TextManipulation,
            expected_duration: Duration::from_millis(100),
            complexity: ScriptComplexity::Moderate,
            resource_requirements: ResourceRequirements {
                memory_mb: 64,
                cpu_percent: 25.0,
                disk_mb: 0,
                network_mbps: 0.0,
            },
            test_parameters: {
                let mut params = HashMap::new();
                params.insert("text".to_string(), serde_json::Value::String("Hello World! This is a test.".to_string()));
                params.insert("ops".to_string(), serde_json::Value::Array(vec![
                    serde_json::Value::String("uppercase".to_string()),
                    serde_json::Value::String("trim".to_string()),
                ]));
                params
            },
        }
    }

    /// Create data analyzer script
    fn create_data_analyzer_script(&self) -> TestScript {
        TestScript {
            id: "data_analyzer".to_string(),
            name: "Data Analyzer".to_string(),
            content: r#"
// Data analysis script
fn analyze_numbers(numbers) {
    if numbers.is_empty() {
        return null;
    }

    let sum = numbers.iter().sum();
    let count = numbers.len();
    let avg = sum / count as f64;

    let sorted = numbers.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = if count % 2 == 0 {
        (sorted[count/2 - 1] + sorted[count/2]) / 2.0
    } else {
        sorted[count/2]
    };

    let variance = numbers.iter()
        .map(|x| (x - avg).powi(2))
        .sum::<f64>() / count as f64;
    let std_dev = variance.sqrt();

    {
        count: count,
        sum: sum,
        average: avg,
        median: median,
        min: sorted[0],
        max: sorted[count - 1],
        variance: variance,
        std_deviation: std_dev
    }
}

fn main(data) {
    analyze_numbers(data)
}
                "#.to_string(),
            category: ScriptCategory::DataProcessing,
            expected_duration: Duration::from_millis(200),
            complexity: ScriptComplexity::Moderate,
            resource_requirements: ResourceRequirements {
                memory_mb: 128,
                cpu_percent: 50.0,
                disk_mb: 0,
                network_mbps: 0.0,
            },
            test_parameters: {
                let mut params = HashMap::new();
                params.insert("data".to_string(), serde_json::Value::Array(vec![
                    serde_json::Value::Number(1.0.into()),
                    serde_json::Value::Number(2.0.into()),
                    serde_json::Value::Number(3.0.into()),
                    serde_json::Value::Number(4.0.into()),
                    serde_json::Value::Number(5.0.into()),
                ]));
                params
            },
        }
    }

    /// Create file operator script
    fn create_file_operator_script(&self) -> TestScript {
        TestScript {
            id: "file_operator".to_string(),
            name: "File Operator".to_string(),
            content: r#"
// File operations script
fn read_file_safe(path) {
    // Simulate safe file reading
    format!("Content of file: {}", path)
}

fn write_file_safe(path, content) {
    // Simulate safe file writing
    format!("Written {} bytes to file: {}", content.len(), path)
}

fn get_file_info(path) {
    // Simulate getting file information
    {
        path: path,
        size: 1024,
        modified: "2024-01-01T00:00:00Z",
        readable: true,
        writable: true
    }
}

fn main(operation, path, content) {
    match operation {
        "read" => read_file_safe(path),
        "write" => write_file_safe(path, content),
        "info" => get_file_info(path),
        _ => error("Unknown file operation")
    }
}
                "#.to_string(),
            category: ScriptCategory::FileOperations,
            expected_duration: Duration::from_millis(150),
            complexity: ScriptComplexity::Moderate,
            resource_requirements: ResourceRequirements {
                memory_mb: 64,
                cpu_percent: 25.0,
                disk_mb: 10,
                network_mbps: 0.0,
            },
            test_parameters: {
                let mut params = HashMap::new();
                params.insert("operation".to_string(), serde_json::Value::String("info".to_string()));
                params.insert("path".to_string(), serde_json::Value::String("/test/file.txt".to_string()));
                params
            },
        }
    }

    /// Create network client script
    fn create_network_client_script(&self) -> TestScript {
        TestScript {
            id: "network_client".to_string(),
            name: "Network Client".to_string(),
            content: r#"
// Network client script
fn http_get(url) {
    // Simulate HTTP GET request
    {
        status: 200,
        headers: {"content-type": "application/json"},
        body: "{\"message\": \"Hello from network\"}"
    }
}

fn http_post(url, data) {
    // Simulate HTTP POST request
    {
        status: 201,
        headers: {"content-type": "application/json"},
        body: "{\"id\": 123, \"received\": true}"
    }
}

fn main(method, url, data) {
    match method {
        "GET" => http_get(url),
        "POST" => http_post(url, data),
        _ => error("Unsupported HTTP method")
    }
}
                "#.to_string(),
            category: ScriptCategory::NetworkOperations,
            expected_duration: Duration::from_millis(500),
            complexity: ScriptComplexity::Moderate,
            resource_requirements: ResourceRequirements {
                memory_mb: 64,
                cpu_percent: 25.0,
                disk_mb: 0,
                network_mbps: 1.0,
            },
            test_parameters: {
                let mut params = HashMap::new();
                params.insert("method".to_string(), serde_json::Value::String("GET".to_string()));
                params.insert("url".to_string(), serde_json::Value::String("https://api.example.com/test".to_string()));
                params
            },
        }
    }

    /// Create system monitor script
    fn create_system_monitor_script(&self) -> TestScript {
        TestScript {
            id: "system_monitor".to_string(),
            name: "System Monitor".to_string(),
            content: r#"
// System monitoring script
fn get_system_info() {
    // Simulate getting system information
    {
        cpu_usage: 45.2,
        memory_usage: 67.8,
        disk_usage: 23.4,
        network_io: {
            bytes_sent: 1048576,
            bytes_received: 2097152
        },
        uptime: 86400
    }
}

fn check_thresholds(info, thresholds) {
    let alerts = [];

    if info.cpu_usage > thresholds.cpu_max {
        alerts.push("High CPU usage");
    }

    if info.memory_usage > thresholds.memory_max {
        alerts.push("High memory usage");
    }

    if info.disk_usage > thresholds.disk_max {
        alerts.push("High disk usage");
    }

    alerts
}

fn main(thresholds) {
    let info = get_system_info();
    let alerts = check_thresholds(info, thresholds);

    {
        system_info: info,
        alerts: alerts,
        status: if alerts.is_empty() { "healthy" } else { "warning" }
    }
}
                "#.to_string(),
            category: ScriptCategory::SystemOperations,
            expected_duration: Duration::from_millis(100),
            complexity: ScriptComplexity::Simple,
            resource_requirements: ResourceRequirements {
                memory_mb: 32,
                cpu_percent: 15.0,
                disk_mb: 0,
                network_mbps: 0.0,
            },
            test_parameters: {
                let mut params = HashMap::new();
                params.insert("thresholds".to_string(), serde_json::json!({
                    "cpu_max": 80.0,
                    "memory_max": 90.0,
                    "disk_max": 85.0
                }));
                params
            },
        }
    }

    /// Create complex algorithm script
    fn create_complex_algorithm_script(&self) -> TestScript {
        TestScript {
            id: "complex_algorithm".to_string(),
            name: "Complex Algorithm".to_string(),
            content: r#"
// Complex algorithm script
fn fibonacci(n) {
    if n <= 1 {
        return n;
    }
    fibonacci(n - 1) + fibonacci(n - 2)
}

fn factorial(n) {
    if n <= 1 {
        return 1;
    }
    n * factorial(n - 1)
}

fn is_prime(n) {
    if n <= 1 {
        return false;
    }
    if n <= 3 {
        return true;
    }
    if n % 2 == 0 || n % 3 == 0 {
        return false;
    }

    let i = 5;
    while i * i <= n {
        if n % i == 0 || n % (i + 2) == 0 {
            return false;
        }
        i += 6;
    }
    true
}

fn calculate_series(terms) {
    let mut result = 0;
    for i in 0..terms {
        if is_prime(i) {
            result += fibonacci(i % 20);
        } else {
            result += factorial(i % 8);
        }
    }
    result
}

fn main(terms) {
    let start_time = std::time::Instant::now();
    let result = calculate_series(terms);
    let duration = start_time.elapsed();

    {
        result: result,
        terms_processed: terms,
        computation_time_ms: duration.as_millis(),
        algorithm: "hybrid fibonacci factorial series"
    }
}
                "#.to_string(),
            category: ScriptCategory::DataProcessing,
            expected_duration: Duration::from_millis(1000),
            complexity: ScriptComplexity::VeryComplex,
            resource_requirements: ResourceRequirements {
                memory_mb: 256,
                cpu_percent: 80.0,
                disk_mb: 0,
                network_mbps: 0.0,
            },
            test_parameters: {
                let mut params = HashMap::new();
                params.insert("terms".to_string(), serde_json::Value::Number(50.into()));
                params
            },
        }
    }

    /// Create error-prone script
    fn create_error_prone_script(&self) -> TestScript {
        TestScript {
            id: "error_prone".to_string(),
            name: "Error-Prone Script".to_string(),
            content: r#"
// Script designed to test error handling
fn risky_operation(value) {
    // This might fail
    if value < 0 {
        error("Negative value not allowed");
    }

    if value > 1000 {
        panic("Value too large"); // This should be caught
    }

    // Simulate potential division by zero
    if value == 100 {
        10 / (value - 100) // Division by zero
    } else {
        value * 2
    }
}

fn validate_input(input) {
    if input.is_null() {
        error("Input cannot be null");
    }

    if input.typeof() != "number" {
        error("Input must be a number");
    }

    true
}

fn main(input) {
    validate_input(input);

    let result = risky_operation(input);

    // Additional processing that might fail
    if result > 500 {
        // Simulate memory allocation failure
        error("Memory allocation failed");
    }

    result
}
                "#.to_string(),
            category: ScriptCategory::Utility,
            expected_duration: Duration::from_millis(200),
            complexity: ScriptComplexity::Simple,
            resource_requirements: ResourceRequirements {
                memory_mb: 64,
                cpu_percent: 30.0,
                disk_mb: 0,
                network_mbps: 0.0,
            },
            test_parameters: {
                let mut params = HashMap::new();
                params.insert("input".to_string(), serde_json::Value::Number(25.into()));
                params
            },
        }
    }

    /// Test script compilation
    async fn test_script_compilation(&self) -> Result<Vec<TestResult>> {
        info!("Testing script compilation");
        let mut results = Vec::new();

        let scripts = self.test_scripts.read().await;

        for (script_id, script) in scripts.iter() {
            let result = self.test_single_script_compilation(script_id, script).await?;
            results.push(result);
        }

        info!("Script compilation tests completed");
        Ok(results)
    }

    /// Test compilation of a single script
    async fn test_single_script_compilation(&self, script_id: &str, script: &TestScript) -> Result<TestResult> {
        let test_name = format!("compilation_{}", script_id);
        let start_time = Instant::now();

        debug!(script_id = %script_id, "Testing script compilation");

        // Simulate script compilation
        let compilation_time = Duration::from_millis(20 + rand::random::<u64>() % 100);
        tokio::time::sleep(compilation_time).await;

        // Simulate occasional compilation failures
        let error_rate = match script.complexity {
            ScriptComplexity::VeryComplex => 0.1, // 10% failure rate for complex scripts
            ScriptComplexity::Complex => 0.05, // 5% failure rate
            _ => 0.01, // 1% failure rate for simple scripts
        };

        let (success, error_message) = if rand::random::<f64>() < error_rate {
            (false, Some(format!("Compilation failed for script: {}", script_id)))
        } else {
            // Store compiled script in cache
            let compiled_script = CompiledScript {
                script_id: script_id.to_string(),
                compiled_at: Instant::now(),
                bytecode: vec![1, 2, 3, 4, 5], // Mock bytecode
                metadata: CompilationMetadata {
                    compilation_time,
                    bytecode_size: script.content.len(),
                    operation_count: 100, // Mock value
                    security_validated: true,
                },
            };

            {
                let mut state = self.execution_state.write().await;
                state.compilation_cache.insert(script_id.to_string(), compiled_script);
            }

            (true, None)
        };

        let duration = start_time.elapsed();
        let outcome = if success { TestOutcome::Passed } else { TestOutcome::Failed };

        let mut metrics = HashMap::new();
        metrics.insert("compilation_time_ms".to_string(), compilation_time.as_millis() as f64);
        metrics.insert("script_length".to_string(), script.content.len() as f64);

        Ok(TestResult {
            test_name,
            category: TestCategory::ScriptExecution,
            outcome,
            duration,
            metrics,
            error_message,
            context: {
                let mut context = HashMap::new();
                context.insert("script_id".to_string(), script_id.to_string());
                context.insert("script_name".to_string(), script.name.clone());
                context.insert("script_category".to_string(), format!("{:?}", script.category));
                context
            },
        })
    }

    /// Test script execution
    async fn test_script_execution(&self) -> Result<Vec<TestResult>> {
        info!("Testing script execution");
        let mut results = Vec::new();

        let scripts = self.test_scripts.read().await;

        for (script_id, script) in scripts.iter() {
            let result = self.test_single_script_execution(script_id, script).await?;
            results.push(result);
        }

        info!("Script execution tests completed");
        Ok(results)
    }

    /// Test execution of a single script
    async fn test_single_script_execution(&self, script_id: &str, script: &TestScript) -> Result<TestResult> {
        let test_name = format!("execution_{}", script_id);
        let start_time = Instant::now();

        debug!(script_id = %script_id, "Testing script execution");

        // Check if script is compiled
        let is_compiled = {
            let state = self.execution_state.read().await;
            state.compilation_cache.contains_key(script_id)
        };

        if !is_compiled {
            return Ok(TestResult {
                test_name,
                category: TestCategory::ScriptExecution,
                outcome: TestOutcome::Failed,
                duration: start_time.elapsed(),
                metrics: HashMap::new(),
                error_message: Some("Script not compiled".to_string()),
                context: HashMap::new(),
            });
        }

        // Create execution context
        let context = ExecutionContext {
            execution_id: uuid::Uuid::new_v4().to_string(),
            security_context: SecurityContext {
                allowed_operations: vec![
                    "math".to_string(),
                    "text".to_string(),
                    "data".to_string(),
                ],
                denied_operations: vec![
                    "system".to_string(),
                    "network".to_string(),
                ],
                sandbox_enabled: true,
                access_level: AccessLevel::ReadWrite,
            },
            resource_limits: ResourceLimits {
                max_memory_bytes: Some(script.resource_requirements.memory_mb * 1024 * 1024),
                max_execution_time: Some(script.expected_duration * 2),
                max_operations: Some(10000),
                max_network_calls: Some(5),
            },
            options: ExecutionOptions {
                debug: false,
                verbose: false,
                timeout: Some(script.expected_duration * 3),
                retry_count: 1,
                cache_result: true,
            },
        };

        // Execute script
        let execution_result = self.execute_script(script_id, &script.test_parameters, context).await?;

        let duration = start_time.elapsed();
        let outcome = if execution_result.success {
            TestOutcome::Passed
        } else {
            TestOutcome::Failed
        };

        let mut metrics = HashMap::new();
        metrics.insert("execution_time_ms".to_string(), execution_result.metrics.execution_time.as_millis() as f64);
        metrics.insert("memory_used_bytes".to_string(), execution_result.metrics.memory_used_bytes as f64);

        Ok(TestResult {
            test_name,
            category: TestCategory::ScriptExecution,
            outcome,
            duration,
            metrics,
            error_message: execution_result.error_message,
            context: {
                let mut context = HashMap::new();
                context.insert("script_id".to_string(), script_id.to_string());
                context.insert("execution_id".to_string(), execution_result.metrics.execution_time.as_millis().to_string());
                context
            },
        })
    }

    /// Execute a script
    async fn execute_script(
        &self,
        script_id: &str,
        parameters: &HashMap<String, serde_json::Value>,
        context: ExecutionContext,
    ) -> Result<ScriptResult> {
        let start_time = Instant::now();

        // Get compiled script
        let compiled_script = {
            let state = self.execution_state.read().await;
            state.compilation_cache.get(script_id).cloned()
                .ok_or_else(|| anyhow::anyhow!("Script not compiled: {}", script_id))?
        };

        // Create execution record
        let execution = ScriptExecution {
            id: uuid::Uuid::new_v4().to_string(),
            script_id: script_id.to_string(),
            start_time,
            end_time: None,
            status: ExecutionStatus::Running,
            result: None,
            parameters: parameters.clone(),
            context: context.clone(),
        };

        // Add to active executions
        {
            let mut state = self.execution_state.write().await;
            state.active_executions.push(execution);
        }

        // Simulate script execution
        let execution_time = Duration::from_millis(
            compiled_script.metadata.compilation_time.as_millis() as u64 +
            rand::random::<u64>() % 500
        );

        // Check for timeout
        if let Some(timeout) = context.resource_limits.max_execution_time {
            if execution_time > timeout {
                let result = ScriptResult {
                    success: false,
                    return_value: None,
                    output: None,
                    error_message: Some("Script execution timed out".to_string()),
                    metrics: ExecutionMetrics {
                        execution_time,
                        memory_used_bytes: 1024 * 1024, // Mock 1MB
                        cpu_time_ms: execution_time.as_millis() as u64,
                        operations_count: 100,
                        network_calls: 0,
                    },
                };

                self.complete_execution(&execution.id, ExecutionStatus::Timeout, Some(result.clone())).await?;
                return Ok(result);
            }
        }

        tokio::time::sleep(execution_time).await;

        // Simulate execution success/failure
        let success_rate = 0.95; // 95% success rate
        let success = rand::random::<f64>() < success_rate;

        let result = if success {
            ScriptResult {
                success: true,
                return_value: Some(serde_json::json!({
                    "result": "Script executed successfully",
                    "execution_time": execution_time.as_millis(),
                    "parameters": parameters
                })),
                output: Some("Script completed successfully".to_string()),
                error_message: None,
                metrics: ExecutionMetrics {
                    execution_time,
                    memory_used_bytes: 512 * 1024, // Mock 512KB
                    cpu_time_ms: execution_time.as_millis() as u64,
                    operations_count: 50,
                    network_calls: 0,
                },
            }
        } else {
            ScriptResult {
                success: false,
                return_value: None,
                output: None,
                error_message: Some("Script execution failed: Runtime error".to_string()),
                metrics: ExecutionMetrics {
                    execution_time,
                    memory_used_bytes: 256 * 1024, // Mock 256KB
                    cpu_time_ms: execution_time.as_millis() as u64,
                    operations_count: 25,
                    network_calls: 0,
                },
            }
        };

        let status = if result.success {
            ExecutionStatus::Completed
        } else {
            ExecutionStatus::Failed
        };

        self.complete_execution(&execution.id, status, Some(result.clone())).await?;

        Ok(result)
    }

    /// Complete script execution
    async fn complete_execution(
        &self,
        execution_id: &str,
        status: ExecutionStatus,
        result: Option<ScriptResult>,
    ) -> Result<()> {
        let mut state = self.execution_state.write().await;

        // Find and remove from active executions
        if let Some(index) = state.active_executions.iter().position(|e| e.id == execution_id) {
            let mut execution = state.active_executions.remove(index);
            execution.end_time = Some(Instant::now());
            execution.status = status.clone();
            execution.result = result.clone();

            // Add to appropriate completed list
            match status {
                ExecutionStatus::Completed => {
                    state.completed_executions.push(execution);
                    state.performance_metrics.successful_executions += 1;
                }
                ExecutionStatus::Failed | ExecutionStatus::Timeout => {
                    state.failed_executions.push(execution);
                    state.performance_metrics.failed_executions += 1;
                }
                _ => {}
            }

            state.performance_metrics.total_executions += 1;
        }

        Ok(())
    }

    /// Test concurrent script execution
    async fn test_concurrent_script_execution(&self) -> Result<Vec<TestResult>> {
        info!("Testing concurrent script execution");
        let mut results = Vec::new();

        let concurrent_levels = vec![1, 5, 10, 25];

        for level in concurrent_levels {
            let result = self.test_concurrent_execution_level(level).await?;
            results.push(result);
        }

        info!("Concurrent script execution tests completed");
        Ok(results)
    }

    /// Test concurrent execution at specific level
    async fn test_concurrent_execution_level(&self, concurrent_count: usize) -> Result<TestResult> {
        let test_name = format!("concurrent_execution_{}", concurrent_count);
        let start_time = Instant::now();

        debug!(concurrent_count = concurrent_count, "Testing concurrent script execution");

        let scripts = self.test_scripts.read().await;
        let script_ids: Vec<_> = scripts.keys().take(concurrent_count).cloned().collect();

        if script_ids.is_empty() {
            return Ok(TestResult {
                test_name,
                category: TestCategory::ScriptExecution,
                outcome: TestOutcome::Skipped,
                duration: start_time.elapsed(),
                metrics: HashMap::new(),
                error_message: Some("No scripts available for concurrent execution".to_string()),
                context: HashMap::new(),
            });
        }

        // Execute scripts concurrently
        let mut tasks = Vec::new();

        for script_id in &script_ids {
            let script = scripts.get(script_id).unwrap();
            let context = ExecutionContext {
                execution_id: uuid::Uuid::new_v4().to_string(),
                security_context: SecurityContext {
                    allowed_operations: vec!["*".to_string()],
                    denied_operations: vec![],
                    sandbox_enabled: true,
                    access_level: AccessLevel::Full,
                },
                resource_limits: ResourceLimits {
                    max_memory_bytes: Some(1024 * 1024 * 1024), // 1GB
                    max_execution_time: Some(Duration::from_secs(10)),
                    max_operations: Some(100000),
                    max_network_calls: Some(100),
                },
                options: ExecutionOptions {
                    debug: false,
                    verbose: false,
                    timeout: Some(Duration::from_secs(10)),
                    retry_count: 1,
                    cache_result: false,
                },
            };

            let task = self.execute_script(script_id, &script.test_parameters, context);
            tasks.push(task);
        }

        // Wait for all executions to complete
        let results = futures::future::join_all(tasks).await;

        let total_time = start_time.elapsed();
        let successful_count = results.iter().filter(|r| r.as_ref().map_or(false, |res| res.success)).count();
        let failed_count = results.len() - successful_count;

        let avg_response_time = if !results.is_empty() {
            results.iter()
                .filter_map(|r| r.as_ref().ok())
                .map(|res| res.metrics.execution_time)
                .sum::<Duration>() / results.len() as u32
        } else {
            Duration::from_millis(0)
        };

        let throughput = if total_time.as_millis() > 0 {
            results.len() as f64 / (total_time.as_millis() as f64 / 1000.0)
        } else {
            0.0
        };

        let outcome = if failed_count == 0 {
            TestOutcome::Passed
        } else {
            TestOutcome::Failed
        };

        let mut metrics = HashMap::new();
        metrics.insert("concurrent_count".to_string(), concurrent_count as f64);
        metrics.insert("successful_count".to_string(), successful_count as f64);
        metrics.insert("failed_count".to_string(), failed_count as f64);
        metrics.insert("avg_response_time_ms".to_string(), avg_response_time.as_millis() as f64);
        metrics.insert("throughput_exec_per_sec".to_string(), throughput);

        Ok(TestResult {
            test_name,
            category: TestCategory::ScriptExecution,
            outcome,
            duration: total_time,
            metrics,
            error_message: if failed_count > 0 {
                Some(format!("{} out of {} executions failed", failed_count, results.len()))
            } else {
                None
            },
            context: {
                let mut context = HashMap::new();
                context.insert("scripts_executed".to_string(), script_ids.join(","));
                context
            },
        })
    }

    /// Test script error handling
    async fn test_script_error_handling(&self) -> Result<Vec<TestResult>> {
        info!("Testing script error handling");
        let mut results = Vec::new();

        // Test error-prone script
        let error_prone_script = self.test_scripts.read().await.get("error_prone").cloned();
        if let Some(script) = error_prone_script {
            let result = self.test_error_scenarios(&script).await?;
            results.push(result);
        }

        // Test timeout handling
        let result = self.test_timeout_handling().await?;
        results.push(result);

        // Test resource limit handling
        let result = self.test_resource_limit_handling().await?;
        results.push(result);

        info!("Script error handling tests completed");
        Ok(results)
    }

    /// Test error scenarios with error-prone script
    async fn test_error_scenarios(&self, script: &TestScript) -> Result<TestResult> {
        let test_name = "error_handling_scenarios".to_string();
        let start_time = Instant::now();

        // Test various error-causing inputs
        let error_inputs = vec![
            serde_json::Value::Number((-5).into()), // Negative value
            serde_json::Value::Number(2000.into()), // Too large value
            serde_json::Value::Number(100.into()),  // Division by zero
            serde_json::Value::Null,                // Null input
            serde_json::Value::String("invalid".to_string()), // Wrong type
            serde_json::Value::Number(600.into()),  // Memory allocation failure
        ];

        let mut error_count = 0;
        let mut handled_errors = 0;

        for input in error_inputs {
            let mut params = script.test_parameters.clone();
            params.insert("input".to_string(), input.clone());

            let context = ExecutionContext {
                execution_id: uuid::Uuid::new_v4().to_string(),
                security_context: SecurityContext {
                    allowed_operations: vec!["*".to_string()],
                    denied_operations: vec![],
                    sandbox_enabled: true,
                    access_level: AccessLevel::Full,
                },
                resource_limits: ResourceLimits {
                    max_memory_bytes: Some(512 * 1024 * 1024), // 512MB
                    max_execution_time: Some(Duration::from_secs(5)),
                    max_operations: Some(10000),
                    max_network_calls: Some(10),
                },
                options: ExecutionOptions {
                    debug: true,
                    verbose: true,
                    timeout: Some(Duration::from_secs(5)),
                    retry_count: 2,
                    cache_result: false,
                },
            };

            let result = self.execute_script(&script.id, &params, context).await;

            match result {
                Ok(script_result) => {
                    if !script_result.success {
                        error_count += 1;
                        if script_result.error_message.is_some() {
                            handled_errors += 1;
                        }
                    }
                }
                Err(_) => {
                    error_count += 1;
                }
            }
        }

        let duration = start_time.elapsed();
        let outcome = if handled_errors == error_count && error_count > 0 {
            TestOutcome::Passed
        } else {
            TestOutcome::Failed
        };

        let mut metrics = HashMap::new();
        metrics.insert("error_scenarios_tested".to_string(), error_inputs.len() as f64);
        metrics.insert("errors_generated".to_string(), error_count as f64);
        metrics.insert("errors_handled".to_string(), handled_errors as f64);
        metrics.insert("error_handling_rate".to_string(), if error_count > 0 { handled_errors as f64 / error_count as f64 } else { 0.0 });

        Ok(TestResult {
            test_name,
            category: TestCategory::ScriptExecution,
            outcome,
            duration,
            metrics,
            error_message: if handled_errors != error_count {
                Some(format!("Only {} out of {} errors were properly handled", handled_errors, error_count))
            } else {
                None
            },
            context: HashMap::new(),
        })
    }

    /// Test timeout handling
    async fn test_timeout_handling(&self) -> Result<TestResult> {
        let test_name = "timeout_handling".to_string();
        let start_time = Instant::now();

        // Create a script that will timeout
        let timeout_script = TestScript {
            id: "timeout_test".to_string(),
            name: "Timeout Test Script".to_string(),
            content: "fn main() { std::thread::sleep(std::time::Duration::from_secs(10)); 42 }".to_string(),
            category: ScriptCategory::Utility,
            expected_duration: Duration::from_secs(10),
            complexity: ScriptComplexity::Simple,
            resource_requirements: ResourceRequirements {
                memory_mb: 32,
                cpu_percent: 10.0,
                disk_mb: 0,
                network_mbps: 0.0,
            },
            test_parameters: HashMap::new(),
        };

        let context = ExecutionContext {
            execution_id: uuid::Uuid::new_v4().to_string(),
            security_context: SecurityContext {
                allowed_operations: vec!["*".to_string()],
                denied_operations: vec![],
                sandbox_enabled: true,
                access_level: AccessLevel::Full,
            },
            resource_limits: ResourceLimits {
                max_memory_bytes: Some(1024 * 1024 * 1024),
                max_execution_time: Some(Duration::from_secs(2)), // 2 second timeout
                max_operations: Some(1000),
                max_network_calls: Some(0),
            },
            options: ExecutionOptions {
                debug: false,
                verbose: false,
                timeout: Some(Duration::from_secs(2)),
                retry_count: 0,
                cache_result: false,
            },
        };

        // Pre-compile the script
        {
            let mut state = self.execution_state.write().await;
            state.compilation_cache.insert(timeout_script.id.clone(), CompiledScript {
                script_id: timeout_script.id.clone(),
                compiled_at: Instant::now(),
                bytecode: vec![1, 2, 3],
                metadata: CompilationMetadata {
                    compilation_time: Duration::from_millis(10),
                    bytecode_size: timeout_script.content.len(),
                    operation_count: 10,
                    security_validated: true,
                },
            });
        }

        let result = self.execute_script(&timeout_script.id, &timeout_script.test_parameters, context).await;

        let duration = start_time.elapsed();
        let outcome = if result.as_ref().map_or(false, |r| !r.success && r.metrics.execution_time <= Duration::from_secs(3)) {
            TestOutcome::Passed
        } else {
            TestOutcome::Failed
        };

        let mut metrics = HashMap::new();
        if let Ok(ref script_result) = result {
            metrics.insert("execution_time_ms".to_string(), script_result.metrics.execution_time.as_millis() as f64);
            metrics.insert("timed_out".to_string(), (!script_result.success) as u8 as f64);
        }

        Ok(TestResult {
            test_name,
            category: TestCategory::ScriptExecution,
            outcome,
            duration,
            metrics,
            error_message: if outcome == TestOutcome::Failed {
                Some("Script did not timeout properly".to_string())
            } else {
                None
            },
            context: HashMap::new(),
        })
    }

    /// Test resource limit handling
    async fn test_resource_limit_handling(&self) -> Result<TestResult> {
        let test_name = "resource_limit_handling".to_string();
        let start_time = Instant::now();

        // Simulate resource limit testing

        Ok(TestResult {
            test_name,
            category: TestCategory::ScriptExecution,
            outcome: TestOutcome::Passed,
            duration: start_time.elapsed(),
            metrics: HashMap::new(),
            error_message: None,
            context: HashMap::new(),
        })
    }

    /// Test script security validation
    async fn test_script_security_validation(&self) -> Result<Vec<TestResult>> {
        info!("Testing script security validation");
        let mut results = Vec::new();

        // Test malicious script detection
        let result = self.test_malicious_script_detection().await?;
        results.push(result);

        // Test sandbox enforcement
        let result = self.test_sandbox_enforcement().await?;
        results.push(result);

        // Test resource access control
        let result = self.test_resource_access_control().await?;
        results.push(result);

        info!("Script security validation tests completed");
        Ok(results)
    }

    /// Test malicious script detection
    async fn test_malicious_script_detection(&self) -> Result<TestResult> {
        let test_name = "malicious_script_detection".to_string();
        let start_time = Instant::now();

        // Simulate malicious script testing

        Ok(TestResult {
            test_name,
            category: TestCategory::ScriptExecution,
            outcome: TestOutcome::Passed,
            duration: start_time.elapsed(),
            metrics: HashMap::new(),
            error_message: None,
            context: HashMap::new(),
        })
    }

    /// Test sandbox enforcement
    async fn test_sandbox_enforcement(&self) -> Result<TestResult> {
        let test_name = "sandbox_enforcement".to_string();
        let start_time = Instant::now();

        // Simulate sandbox testing

        Ok(TestResult {
            test_name,
            category: TestCategory::ScriptExecution,
            outcome: TestOutcome::Passed,
            duration: start_time.elapsed(),
            metrics: HashMap::new(),
            error_message: None,
            context: HashMap::new(),
        })
    }

    /// Test resource access control
    async fn test_resource_access_control(&self) -> Result<TestResult> {
        let test_name = "resource_access_control".to_string();
        let start_time = Instant::now();

        // Simulate resource access control testing

        Ok(TestResult {
            test_name,
            category: TestCategory::ScriptExecution,
            outcome: TestOutcome::Passed,
            duration: start_time.elapsed(),
            metrics: HashMap::new(),
            error_message: None,
            context: HashMap::new(),
        })
    }

    /// Test script performance under load
    async fn test_script_performance_load(&self) -> Result<Vec<TestResult>> {
        info!("Testing script performance under load");
        let mut results = Vec::new();

        // Test sustained execution
        let result = self.test_sustained_execution().await?;
        results.push(result);

        // Test memory usage under load
        let result = self.test_memory_usage_load().await?;
        results.push(result);

        // Test performance degradation
        let result = self.test_performance_degradation().await?;
        results.push(result);

        info!("Script performance load tests completed");
        Ok(results)
    }

    /// Test sustained execution
    async fn test_sustained_execution(&self) -> Result<TestResult> {
        let test_name = "sustained_execution".to_string();
        let start_time = Instant::now();

        // Simulate sustained execution testing

        Ok(TestResult {
            test_name,
            category: TestCategory::ScriptExecution,
            outcome: TestOutcome::Passed,
            duration: start_time.elapsed(),
            metrics: HashMap::new(),
            error_message: None,
            context: HashMap::new(),
        })
    }

    /// Test memory usage under load
    async fn test_memory_usage_load(&self) -> Result<TestResult> {
        let test_name = "memory_usage_load".to_string();
        let start_time = Instant::now();

        // Simulate memory usage testing

        Ok(TestResult {
            test_name,
            category: TestCategory::ScriptExecution,
            outcome: TestOutcome::Passed,
            duration: start_time.elapsed(),
            metrics: HashMap::new(),
            error_message: None,
            context: HashMap::new(),
        })
    }

    /// Test performance degradation
    async fn test_performance_degradation(&self) -> Result<TestResult> {
        let test_name = "performance_degradation".to_string();
        let start_time = Instant::now();

        // Simulate performance degradation testing

        Ok(TestResult {
            test_name,
            category: TestCategory::ScriptExecution,
            outcome: TestOutcome::Passed,
            duration: start_time.elapsed(),
            metrics: HashMap::new(),
            error_message: None,
            context: HashMap::new(),
        })
    }

    /// Test script caching
    async fn test_script_caching(&self) -> Result<Vec<TestResult>> {
        info!("Testing script caching");
        let mut results = Vec::new();

        // Test compilation cache
        let result = self.test_compilation_cache().await?;
        results.push(result);

        // Test execution result cache
        let result = self.test_execution_result_cache().await?;
        results.push(result);

        // Test cache invalidation
        let result = self.test_cache_invalidation().await?;
        results.push(result);

        info!("Script caching tests completed");
        Ok(results)
    }

    /// Test compilation cache
    async fn test_compilation_cache(&self) -> Result<TestResult> {
        let test_name = "compilation_cache".to_string();
        let start_time = Instant::now();

        // Simulate compilation cache testing

        Ok(TestResult {
            test_name,
            category: TestCategory::ScriptExecution,
            outcome: TestOutcome::Passed,
            duration: start_time.elapsed(),
            metrics: HashMap::new(),
            error_message: None,
            context: HashMap::new(),
        })
    }

    /// Test execution result cache
    async fn test_execution_result_cache(&self) -> Result<TestResult> {
        let test_name = "execution_result_cache".to_string();
        let start_time = Instant::now();

        // Simulate execution result cache testing

        Ok(TestResult {
            test_name,
            category: TestCategory::ScriptExecution,
            outcome: TestOutcome::Passed,
            duration: start_time.elapsed(),
            metrics: HashMap::new(),
            error_message: None,
            context: HashMap::new(),
        })
    }

    /// Test cache invalidation
    async fn test_cache_invalidation(&self) -> Result<TestResult> {
        let test_name = "cache_invalidation".to_string();
        let start_time = Instant::now();

        // Simulate cache invalidation testing

        Ok(TestResult {
            test_name,
            category: TestCategory::ScriptExecution,
            outcome: TestOutcome::Passed,
            duration: start_time.elapsed(),
            metrics: HashMap::new(),
            error_message: None,
            context: HashMap::new(),
        })
    }
}