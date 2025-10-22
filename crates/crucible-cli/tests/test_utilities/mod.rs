//! Comprehensive test utilities for CLI testing
//!
//! This module provides common utilities, mocks, and helpers for testing
//! CLI commands with proper isolation and mocked services.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use crate::config::CliConfig;
use crate::cli::{ServiceCommands, MigrationCommands};

/// Test context for isolated CLI testing
pub struct TestContext {
    pub temp_dir: TempDir,
    pub config: CliConfig,
    pub mock_services: Arc<MockServiceRegistry>,
    pub captured_output: Arc<Mutex<Vec<String>>>,
}

impl TestContext {
    /// Create a new test context with isolated environment
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let config = Self::create_test_config(&temp_dir)?;
        let mock_services = Arc::new(MockServiceRegistry::new());
        let captured_output = Arc::new(Mutex::new(Vec::new()));

        Ok(Self {
            temp_dir,
            config,
            mock_services,
            captured_output,
        })
    }

    /// Create test configuration with isolated paths
    fn create_test_config(temp_dir: &TempDir) -> Result<CliConfig> {
        let mut config = CliConfig::default();

        // Use isolated paths
        config.vault.path = temp_dir.path().join("vault");
        config.services.script_engine.max_cache_size = 10; // Small for testing
        config.migration.max_cache_size = 5; // Small for testing
        config.migration.enabled = true; // Enable migration for testing

        // Create necessary directories
        std::fs::create_dir_all(&config.vault.path)?;
        std::fs::create_dir_all(config.vault.path.join(".crucible"))?;

        Ok(config)
    }

    /// Create a test script file
    pub fn create_test_script(&self, name: &str, content: &str) -> PathBuf {
        let script_path = self.temp_dir.path().join(format!("{}.rn", name));
        std::fs::write(&script_path, content).unwrap();
        script_path
    }

    /// Get captured output
    pub fn get_captured_output(&self) -> Vec<String> {
        self.captured_output.lock().unwrap().clone()
    }

    /// Clear captured output
    pub fn clear_captured_output(&self) {
        self.captured_output.lock().unwrap().clear();
    }
}

/// Mock service registry for testing CLI-service integration
#[derive(Debug)]
pub struct MockServiceRegistry {
    services: RwLock<HashMap<String, Arc<MockService>>>,
    health_status: RwLock<HashMap<String, ServiceHealth>>,
    metrics: RwLock<HashMap<String, ServiceMetrics>>,
}

impl MockServiceRegistry {
    pub fn new() -> Self {
        let mut services = HashMap::new();
        let mut health_status = HashMap::new();
        let mut metrics = HashMap::new();

        // Add default mock services
        let script_engine = Arc::new(MockService::new("crucible-script-engine"));
        let rune_service = Arc::new(MockService::new("crucible-rune-service"));
        let plugin_manager = Arc::new(MockService::new("crucible-plugin-manager"));

        services.insert("crucible-script-engine".to_string(), script_engine.clone());
        services.insert("crucible-rune-service".to_string(), rune_service.clone());
        services.insert("crucible-plugin-manager".to_string(), plugin_manager.clone());

        health_status.insert("crucible-script-engine".to_string(), ServiceHealth::Healthy);
        health_status.insert("crucible-rune-service".to_string(), ServiceHealth::Healthy);
        health_status.insert("crucible-plugin-manager".to_string(), ServiceHealth::Degraded);

        metrics.insert("crucible-script-engine".to_string(), ServiceMetrics::default());
        metrics.insert("crucible-rune-service".to_string(), ServiceMetrics::default());
        metrics.insert("crucible-plugin-manager".to_string(), ServiceMetrics::default());

        Self {
            services: RwLock::new(services),
            health_status: RwLock::new(health_status),
            metrics: RwLock::new(metrics),
        }
    }

    pub async fn add_service(&self, name: String, service: Arc<MockService>) {
        let mut services = self.services.write().await;
        services.insert(name.clone(), service);

        // Initialize with healthy status
        let mut health = self.health_status.write().await;
        health.insert(name, ServiceHealth::Healthy);
    }

    pub async fn get_service(&self, name: &str) -> Option<Arc<MockService>> {
        let services = self.services.read().await;
        services.get(name).cloned()
    }

    pub async fn set_health_status(&self, service: &str, status: ServiceHealth) {
        let mut health = self.health_status.write().await;
        health.insert(service.to_string(), status);
    }

    pub async fn get_health_status(&self, service: &str) -> ServiceHealth {
        let health = self.health_status.read().await;
        health.get(service).cloned().unwrap_or(ServiceHealth::Unknown)
    }

    pub async fn get_all_health_status(&self) -> HashMap<String, ServiceHealth> {
        let health = self.health_status.read().await;
        health.clone()
    }

    pub async fn update_metrics(&self, service: &str, metrics: ServiceMetrics) {
        let mut metrics_map = self.metrics.write().await;
        metrics_map.insert(service.to_string(), metrics);
    }

    pub async fn get_metrics(&self, service: &str) -> Option<ServiceMetrics> {
        let metrics = self.metrics.read().await;
        metrics.get(service).cloned()
    }

    pub async fn get_all_metrics(&self) -> HashMap<String, ServiceMetrics> {
        let metrics = self.metrics.read().await;
        metrics.clone()
    }

    pub async fn simulate_service_failure(&self, service: &str) {
        self.set_health_status(service, ServiceHealth::Unhealthy).await;

        if let Some(mock_service) = self.get_service(service).await {
            mock_service.set_failed(true).await;
        }
    }

    pub async fn simulate_service_recovery(&self, service: &str) {
        self.set_health_status(service, ServiceHealth::Healthy).await;

        if let Some(mock_service) = self.get_service(service).await {
            mock_service.set_failed(false).await;
        }
    }
}

/// Mock service for testing
#[derive(Debug)]
pub struct MockService {
    pub name: String,
    pub state: RwLock<ServiceState>,
    pub start_count: RwLock<u32>,
    pub stop_count: RwLock<u32>,
    pub execution_count: RwLock<u32>,
    pub last_execution: RwLock<Option<Instant>>,
}

#[derive(Debug, Clone)]
pub struct ServiceState {
    pub running: bool,
    pub failed: bool,
    pub start_time: Option<Instant>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServiceHealth {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub active_connections: u32,
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: f64,
    pub uptime_seconds: u64,
    pub response_time_ms: f64,
}

impl Default for ServiceMetrics {
    fn default() -> Self {
        Self {
            total_requests: 100,
            successful_requests: 95,
            failed_requests: 5,
            active_connections: 10,
            cpu_usage_percent: 25.5,
            memory_usage_mb: 128.0,
            uptime_seconds: 3600,
            response_time_ms: 150.0,
        }
    }
}

impl MockService {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            state: RwLock::new(ServiceState {
                running: false,
                failed: false,
                start_time: None,
                last_error: None,
            }),
            start_count: RwLock::new(0),
            stop_count: RwLock::new(0),
            execution_count: RwLock::new(0),
            last_execution: RwLock::new(None),
        }
    }

    pub async fn start(&self) -> Result<()> {
        let mut state = self.state.write().await;
        if state.failed {
            return Err(anyhow::anyhow!("Service {} is in failed state", self.name));
        }

        state.running = true;
        state.start_time = Some(Instant::now());
        state.last_error = None;

        *self.start_count.write().await += 1;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        let mut state = self.state.write().await;
        state.running = false;
        state.start_time = None;

        *self.stop_count.write().await += 1;
        Ok(())
    }

    pub async fn restart(&self) -> Result<()> {
        self.stop().await?;
        tokio::time::sleep(Duration::from_millis(100)).await; // Simulate restart time
        self.start().await
    }

    pub async fn execute(&self, operation: &str) -> Result<String> {
        let state = self.state.read().await;
        if !state.running {
            return Err(anyhow::anyhow!("Service {} is not running", self.name));
        }
        if state.failed {
            return Err(anyhow::anyhow!("Service {} is in failed state", self.name));
        }

        drop(state); // Release read lock

        *self.execution_count.write().await += 1;
        *self.last_execution.write().await = Some(Instant::now());

        Ok(format!("{} executed '{}' successfully", self.name, operation))
    }

    pub async fn is_running(&self) -> bool {
        let state = self.state.read().await;
        state.running
    }

    pub async fn is_failed(&self) -> bool {
        let state = self.state.read().await;
        state.failed
    }

    pub async fn set_failed(&self, failed: bool) {
        let mut state = self.state.write().await;
        state.failed = failed;
        if failed {
            state.last_error = Some("Simulated failure".to_string());
        }
    }

    pub async fn get_start_count(&self) -> u32 {
        *self.start_count.read().await
    }

    pub async fn get_stop_count(&self) -> u32 {
        *self.stop_count.read().await
    }

    pub async fn get_execution_count(&self) -> u32 {
        *self.execution_count.read().await
    }

    pub async fn get_uptime(&self) -> Option<Duration> {
        let state = self.state.read().await;
        state.start_time.map(|start| start.elapsed())
    }
}

/// Mock migration bridge for testing migration commands
#[derive(Debug)]
pub struct MockMigrationBridge {
    pub migrated_tools: RwLock<HashMap<String, MigratedTool>>,
    pub migration_stats: RwLock<MigrationStats>,
    pub failures: RwLock<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigratedTool {
    pub original_name: String,
    pub migrated_script_id: String,
    pub active: bool,
    pub migrated_at: chrono::DateTime<chrono::Utc>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub security_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStats {
    pub total_migrated: u32,
    pub active_tools: u32,
    pub inactive_tools: u32,
    pub failed_migrations: u32,
    pub migration_timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationValidation {
    pub valid: bool,
    pub total_tools: u32,
    pub valid_tools: u32,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
}

impl MockMigrationBridge {
    pub fn new() -> Self {
        let migrated_tools = RwLock::new(HashMap::new());
        let migration_stats = RwLock::new(MigrationStats {
            total_migrated: 0,
            active_tools: 0,
            inactive_tools: 0,
            failed_migrations: 0,
            migration_timestamp: chrono::Utc::now(),
        });
        let failures = RwLock::new(Vec::new());

        Self {
            migrated_tools,
            migration_stats,
            failures,
        }
    }

    pub async fn add_migrated_tool(&self, tool: MigratedTool) {
        let mut tools = self.migrated_tools.write().await;
        tools.insert(tool.original_name.clone(), tool.clone());

        let mut stats = self.migration_stats.write().await;
        stats.total_migrated += 1;
        if tool.active {
            stats.active_tools += 1;
        } else {
            stats.inactive_tools += 1;
        }
    }

    pub async fn get_migrated_tool(&self, name: &str) -> Option<MigratedTool> {
        let tools = self.migrated_tools.read().await;
        tools.get(name).cloned()
    }

    pub async fn list_migrated_tools(&self) -> Vec<MigratedTool> {
        let tools = self.migrated_tools.read().await;
        tools.values().cloned().collect()
    }

    pub async fn deactivate_tool(&self, name: &str) -> Result<()> {
        let mut tools = self.migrated_tools.write().await;
        if let Some(tool) = tools.get_mut(name) {
            tool.active = false;

            let mut stats = self.migration_stats.write().await;
            stats.active_tools = stats.active_tools.saturating_sub(1);
            stats.inactive_tools += 1;

            Ok(())
        } else {
            Err(anyhow::anyhow!("Tool {} not found", name))
        }
    }

    pub async fn remove_tool(&self, name: &str) -> Result<()> {
        let mut tools = self.migrated_tools.write().await;
        if let Some(tool) = tools.remove(name) {
            let mut stats = self.migration_stats.write().await;
            stats.total_migrated = stats.total_migrated.saturating_sub(1);
            if tool.active {
                stats.active_tools = stats.active_tools.saturating_sub(1);
            } else {
                stats.inactive_tools = stats.inactive_tools.saturating_sub(1);
            }
            Ok(())
        } else {
            Err(anyhow::anyhow!("Tool {} not found", name))
        }
    }

    pub async fn get_migration_stats(&self) -> MigrationStats {
        let stats = self.migration_stats.read().await;
        MigrationStats {
            total_migrated: stats.total_migrated,
            active_tools: stats.active_tools,
            inactive_tools: stats.inactive_tools,
            failed_migrations: stats.failed_migrations,
            migration_timestamp: stats.migration_timestamp,
        }
    }

    pub async fn validate_migration(&self) -> MigrationValidation {
        let tools = self.migrated_tools.read().await;
        let total_tools = tools.len() as u32;
        let valid_tools = tools.values().filter(|t| t.active).count() as u32;

        let mut issues = Vec::new();
        let mut warnings = Vec::new();

        // Check for issues
        for (name, tool) in tools.iter() {
            if !tool.active {
                warnings.push(format!("Tool '{}' is inactive", name));
            }
            if tool.migrated_script_id.is_empty() {
                issues.push(format!("Tool '{}' has empty script ID", name));
            }
        }

        MigrationValidation {
            valid: issues.is_empty(),
            total_tools,
            valid_tools,
            issues,
            warnings,
        }
    }

    pub async fn simulate_migration_failure(&self, tool_name: &str, error: String) {
        let mut failures = self.failures.write().await;
        failures.push(format!("{}: {}", tool_name, error));

        let mut stats = self.migration_stats.write().await;
        stats.failed_migrations += 1;
    }
}

/// Output capture utility for testing CLI output
pub struct OutputCapture;

impl OutputCapture {
    /// Capture stdout and stderr from a function
    pub fn capture<F, R>(f: F) -> (R, String, String)
    where
        F: FnOnce() -> R,
    {
        use std::io::{self, Write};
        use std::sync::mpsc;

        let (stdout_sender, stdout_receiver) = mpsc::channel();
        let (stderr_sender, stderr_receiver) = mpsc::channel();

        // Capture stdout
        let original_stdout = io::stdout();
        let stdout_capture = std::thread::spawn(move || {
            let mut captured = String::new();
            // This is a simplified capture - in real implementation you'd
            // need more sophisticated redirection
            captured
        });

        // Execute the function
        let result = f();

        // Get captured output
        let _ = stdout_capture.join();
        let stdout_output = stdout_receiver.try_recv().unwrap_or_default();
        let stderr_output = stderr_receiver.try_recv().unwrap_or_default();

        (result, stdout_output, stderr_output)
    }
}

/// Performance measurement utilities
pub struct PerformanceMeasurement;

impl PerformanceMeasurement {
    /// Measure execution time of a function
    pub async fn measure<F, R, Fut>(f: F) -> (R, Duration)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = R>,
    {
        let start = Instant::now();
        let result = f().await;
        let duration = start.elapsed();
        (result, duration)
    }

    /// Measure memory usage before and after execution
    pub fn measure_memory<F, R>(f: F) -> (R, MemoryUsage, MemoryUsage)
    where
        F: FnOnce() -> R,
    {
        let before = MemoryUsage::current();
        let result = f();
        let after = MemoryUsage::current();
        (result, before, after)
    }
}

#[derive(Debug, Clone)]
pub struct MemoryUsage {
    pub rss_bytes: u64,
    pub virtual_bytes: u64,
}

impl MemoryUsage {
    pub fn current() -> Self {
        // Simplified memory usage - in real implementation you'd
        // use system-specific APIs to get actual memory usage
        Self {
            rss_bytes: 0,
            virtual_bytes: 0,
        }
    }
}

/// Assert utilities for CLI testing
pub struct AssertUtils;

impl AssertUtils {
    /// Assert that output contains expected text
    pub fn assert_output_contains(output: &str, expected: &str) {
        assert!(
            output.contains(expected),
            "Expected output to contain '{}', but got: {}",
            expected,
            output
        );
    }

    /// Assert that output contains all expected strings
    pub fn assert_output_contains_all(output: &str, expected: &[&str]) {
        for expected_text in expected {
            Self::assert_output_contains(output, expected_text);
        }
    }

    /// Assert that output is valid JSON
    pub fn assert_valid_json(output: &str) -> serde_json::Value {
        serde_json::from_str(output)
            .unwrap_or_else(|e| panic!("Output is not valid JSON: {}\nOutput: {}", e, output))
    }

    /// Assert that output is valid TOML
    pub fn assert_valid_toml(output: &str) -> toml::Value {
        toml::from_str(output)
            .unwrap_or_else(|e| panic!("Output is not valid TOML: {}\nOutput: {}", e, output))
    }

    /// Assert that command execution time is within acceptable bounds
    pub fn assert_execution_time_within(
        duration: Duration,
        min: Duration,
        max: Duration,
        context: &str,
    ) {
        assert!(
            duration >= min && duration <= max,
            "Command '{}' took {:?}, expected between {:?} and {:?}",
            context,
            duration,
            min,
            max
        );
    }

    /// Assert that service is in expected state
    pub fn assert_service_state(
        service: &MockService,
        expected_running: bool,
        expected_failed: bool,
    ) {
        // Note: This would need to be async in real implementation
        // For now, this is a placeholder for the assertion concept
    }
}

/// Test data generators
pub struct TestDataGenerator;

impl TestDataGenerator {
    /// Generate test service metrics
    pub fn generate_metrics(total_requests: u64, success_rate: f64) -> ServiceMetrics {
        let successful_requests = (total_requests as f64 * success_rate) as u64;
        let failed_requests = total_requests - successful_requests;

        ServiceMetrics {
            total_requests,
            successful_requests,
            failed_requests,
            active_connections: 10,
            cpu_usage_percent: 25.5 + (failed_requests as f64 / total_requests as f64) * 50.0,
            memory_usage_mb: 128.0,
            uptime_seconds: 3600,
            response_time_ms: 150.0 + (failed_requests as f64 * 10.0),
        }
    }

    /// Generate test migrated tools
    pub fn generate_migrated_tools(count: u32) -> Vec<MigratedTool> {
        let mut tools = Vec::new();
        for i in 0..count {
            tools.push(MigratedTool {
                original_name: format!("test-tool-{}", i),
                migrated_script_id: format!("script-id-{}", uuid::Uuid::new_v4()),
                active: i % 2 == 0, // Half active, half inactive
                migrated_at: chrono::Utc::now(),
                metadata: {
                    let mut metadata = HashMap::new();
                    metadata.insert("version".to_string(), serde_json::Value::String("1.0.0".to_string()));
                    metadata.insert("author".to_string(), serde_json::Value::String("test".to_string()));
                    metadata
                },
                security_level: "safe".to_string(),
            });
        }
        tools
    }

    /// Generate test configuration
    pub fn generate_test_config() -> CliConfig {
        let mut config = CliConfig::default();
        config.services.script_engine.enabled = true;
        config.services.script_engine.security_level = "safe".to_string();
        config.migration.enabled = true;
        config.migration.auto_migrate = false;
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_service_lifecycle() {
        let service = MockService::new("test-service");

        // Initially not running
        assert!(!service.is_running().await);

        // Start service
        service.start().await.unwrap();
        assert!(service.is_running().await);
        assert_eq!(service.get_start_count().await, 1);

        // Execute operation
        let result = service.execute("test-operation").await.unwrap();
        assert!(result.contains("test-operation"));
        assert_eq!(service.get_execution_count().await, 1);

        // Stop service
        service.stop().await.unwrap();
        assert!(!service.is_running().await);
        assert_eq!(service.get_stop_count().await, 1);
    }

    #[tokio::test]
    async fn test_mock_service_failure() {
        let service = MockService::new("failing-service");

        service.set_failed(true).await;
        assert!(service.is_failed().await);

        // Should fail to start
        assert!(service.start().await.is_err());

        // Should fail to execute
        assert!(service.execute("test").await.is_err());
    }

    #[tokio::test]
    async fn test_mock_service_registry() {
        let registry = MockServiceRegistry::new();

        // Test getting services
        assert!(registry.get_service("crucible-script-engine").await.is_some());
        assert!(registry.get_service("non-existent").await.is_none());

        // Test health status
        assert_eq!(
            registry.get_health_status("crucible-script-engine").await,
            ServiceHealth::Healthy
        );

        // Test metrics
        let metrics = registry.get_metrics("crucible-script-engine").await;
        assert!(metrics.is_some());

        // Test health updates
        registry.set_health_status("crucible-script-engine", ServiceHealth::Degraded).await;
        assert_eq!(
            registry.get_health_status("crucible-script-engine").await,
            ServiceHealth::Degraded
        );
    }

    #[tokio::test]
    async fn test_mock_migration_bridge() {
        let bridge = MockMigrationBridge::new();

        // Test adding migrated tools
        let tool = TestDataGenerator::generate_migrated_tools(1).remove(0);
        bridge.add_migrated_tool(tool.clone()).await;

        // Test retrieving tools
        let retrieved = bridge.get_migrated_tool(&tool.original_name).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().original_name, tool.original_name);

        // Test listing tools
        let tools = bridge.list_migrated_tools().await;
        assert_eq!(tools.len(), 1);

        // Test migration stats
        let stats = bridge.get_migration_stats().await;
        assert_eq!(stats.total_migrated, 1);
        assert_eq!(stats.active_tools, if tool.active { 1 } else { 0 });

        // Test validation
        let validation = bridge.validate_migration().await;
        assert!(validation.valid);
        assert_eq!(validation.total_tools, 1);
    }

    #[test]
    fn test_test_context_creation() {
        let context = TestContext::new().unwrap();

        // Verify temp directory structure
        assert!(context.temp_dir.path().exists());
        assert!(context.config.vault.path.exists());
        assert!(context.config.vault.path.join(".crucible").exists());

        // Verify test configuration
        assert!(context.config.migration.enabled);
        assert_eq!(context.config.services.script_engine.max_cache_size, 10);
        assert_eq!(context.config.migration.max_cache_size, 5);
    }

    #[test]
    fn test_test_data_generator() {
        // Test metrics generation
        let metrics = TestDataGenerator::generate_metrics(100, 0.95);
        assert_eq!(metrics.total_requests, 100);
        assert_eq!(metrics.successful_requests, 95);
        assert_eq!(metrics.failed_requests, 5);

        // Test migrated tools generation
        let tools = TestDataGenerator::generate_migrated_tools(5);
        assert_eq!(tools.len(), 5);
        assert_eq!(tools[0].original_name, "test-tool-0");

        // Test configuration generation
        let config = TestDataGenerator::generate_test_config();
        assert!(config.services.script_engine.enabled);
        assert!(config.migration.enabled);
        assert!(!config.migration.auto_migrate);
    }
}