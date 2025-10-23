//! Test utilities for Phase 8.4 integration testing
//!
//! This module provides common utilities, helpers, and fixtures
//! for comprehensive integration testing across the Crucible system.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::{IntegrationTestConfig, TestResult, TestCategory, TestOutcome};

/// Common test utilities and helpers
pub struct TestUtils {
    /// Test configuration
    config: IntegrationTestConfig,
    /// Temporary test directory
    test_dir: Arc<TempDir>,
    /// Test data cache
    test_data: Arc<RwLock<HashMap<String, serde_json::Value>>>,
    /// Mock services registry
    mock_services: Arc<RwLock<HashMap<String, MockService>>>,
}

/// Mock service for testing
#[derive(Debug, Clone)]
pub struct MockService {
    /// Service name
    pub name: String,
    /// Service status
    pub status: MockServiceStatus,
    /// Service response time
    pub response_time: Duration,
    /// Service error rate (0.0 to 1.0)
    pub error_rate: f64,
    /// Service metrics
    pub metrics: HashMap<String, f64>,
}

/// Mock service status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockServiceStatus {
    /// Service is healthy and responding
    Healthy,
    /// Service is degraded but responding
    Degraded,
    /// Service is unhealthy and not responding
    Unhealthy,
    /// Service is under maintenance
    Maintenance,
}

/// Test document for knowledge management scenarios
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestDocument {
    /// Document ID
    pub id: String,
    /// Document title
    pub title: String,
    /// Document content
    pub content: String,
    /// Document tags
    pub tags: Vec<String>,
    /// Document creation date
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Document modification date
    pub modified_at: chrono::DateTime<chrono::Utc>,
    /// Document metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Test user simulation
#[derive(Debug, Clone)]
pub struct TestUser {
    /// User ID
    pub id: String,
    /// User name
    pub name: String,
    /// User session ID
    pub session_id: String,
    /// User behavior pattern
    pub behavior_pattern: UserBehaviorPattern,
    /// User activity timeline
    pub activity_timeline: Vec<UserActivity>,
}

/// User behavior pattern for simulation
#[derive(Debug, Clone)]
pub enum UserBehaviorPattern {
    /// Light user - occasional access
    Light,
    /// Regular user - daily usage
    Regular,
    /// Power user - heavy usage
    Power,
    /// Developer user - API heavy usage
    Developer,
    /// Stressed user - rapid actions
    Stressed,
}

/// User activity event
#[derive(Debug, Clone)]
pub struct UserActivity {
    /// Activity timestamp
    pub timestamp: Instant,
    /// Activity type
    pub activity_type: UserActivityType,
    /// Activity duration
    pub duration: Duration,
    /// Activity outcome
    pub outcome: ActivityOutcome,
}

/// User activity types
#[derive(Debug, Clone)]
pub enum UserActivityType {
    /// Search for documents
    Search { query: String },
    /// Create a new document
    CreateDocument { title: String },
    /// Edit an existing document
    EditDocument { document_id: String },
    /// Run a script
    RunScript { script_name: String },
    /// Access system configuration
    AccessConfiguration,
    /// View system status
    ViewStatus,
}

/// Activity outcome
#[derive(Debug, Clone)]
pub enum ActivityOutcome {
    /// Activity completed successfully
    Success,
    /// Activity failed with error
    Failed { error: String },
    /// Activity timed out
    Timeout,
    /// Activity was cancelled
    Cancelled,
}

/// Performance measurement utility
pub struct PerformanceMeasurer {
    /// Measurement start time
    start_time: Instant,
    /// Checkpoints
    checkpoints: Vec<(String, Instant)>,
    /// Memory usage tracking
    memory_samples: Vec<(Instant, u64)>,
}

/// Database test utilities
pub struct DatabaseTestUtils {
    /// Database connection pool
    connection_pool: Arc<RwLock<Vec<String>>>, // Mock connection pool
    /// Test data seeds
    test_seeds: Arc<RwLock<HashMap<String, Vec<TestDocument>>>>,
}

/// System resource monitor
pub struct ResourceMonitor {
    /// Monitor start time
    start_time: Instant,
    /// CPU usage samples
    cpu_samples: Vec<(Instant, f64)>,
    /// Memory usage samples
    memory_samples: Vec<(Instant, u64)>,
    /// Disk usage samples
    disk_samples: Vec<(Instant, u64)>,
}

impl TestUtils {
    /// Create new test utilities instance
    pub fn new(config: IntegrationTestConfig, test_dir: Arc<TempDir>) -> Self {
        Self {
            config,
            test_dir,
            test_data: Arc::new(RwLock::new(HashMap::new())),
            mock_services: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get test directory path
    pub fn test_dir_path(&self) -> &Path {
        self.test_dir.path()
    }

    /// Generate a unique test ID
    pub fn generate_test_id(&self) -> String {
        format!("test_{}", Uuid::new_v4().to_string().replace("-", "_"))
    }

    /// Create a test vault directory
    pub async fn create_test_vault(&self, name: &str) -> Result<PathBuf> {
        let vault_path = self.test_dir.path().join(name);
        fs::create_dir_all(&vault_path).await
            .context("Failed to create test vault directory")?;

        // Create standard vault structure
        fs::create_dir_all(vault_path.join("notes")).await?;
        fs::create_dir_all(vault_path.join("attachments")).await?;
        fs::create_dir_all(vault_path.join("templates")).await?;
        fs::create_dir_all(vault_path.join("scripts")).await?;

        info!(vault_path = ?vault_path, "Created test vault directory");
        Ok(vault_path)
    }

    /// Generate test documents for realistic workloads
    pub async fn generate_test_documents(&self, count: usize) -> Result<Vec<TestDocument>> {
        let mut documents = Vec::with_capacity(count);

        for i in 0..count {
            let doc = TestDocument {
                id: Uuid::new_v4().to_string(),
                title: format!("Test Document {}", i + 1),
                content: self.generate_document_content(i).await?,
                tags: self.generate_document_tags(i),
                created_at: chrono::Utc::now() - chrono::Duration::minutes((i * 10) as i64),
                modified_at: chrono::Utc::now() - chrono::Duration::minutes((i * 5) as i64),
                metadata: self.generate_document_metadata(i).await?,
            };
            documents.push(doc);
        }

        info!(document_count = documents.len(), "Generated test documents");
        Ok(documents)
    }

    /// Generate realistic document content
    async fn generate_document_content(&self, index: usize) -> Result<String> {
        let content_templates = vec![
            "# Project Planning\n\nThis document outlines the planning phase for project {}.\n\n## Objectives\n- Define project scope\n- Establish timeline\n- Allocate resources\n\n## Timeline\nThe project will run for approximately 12 weeks with the following milestones:\n\n1. **Phase 1**: Requirements gathering (Weeks 1-2)\n2. **Phase 2**: Design and architecture (Weeks 3-4)\n3. **Phase 3**: Implementation (Weeks 5-8)\n4. **Phase 4**: Testing and deployment (Weeks 9-12)\n\n## Success Criteria\n- All requirements met\n- On-time delivery\n- Within budget constraints\n- Quality standards achieved",

            "# Meeting Notes - {}\n\n**Date**: {}\n**Attendees**: Team leads, project stakeholders\n\n## Agenda\n1. Project status update\n2. Roadblock discussion\n3. Next steps\n\n## Key Decisions\n- Approve proposed changes\n- Allocate additional resources\n- Extend timeline by 2 weeks\n\n## Action Items\n- [ ] Update project documentation\n- [ ] Schedule follow-up meeting\n- [ ] Review resource allocation\n\n## Next Meeting\nNext meeting scheduled for next week at the same time.",

            "# Technical Documentation: Component {}\n\n## Overview\nThis component provides core functionality for the system architecture.\n\n## Implementation Details\n\n### API Methods\n- `initialize()`: Initialize component with configuration\n- `process()`: Process incoming requests\n- `shutdown()`: Graceful shutdown procedure\n\n### Configuration\n```json\n{{\n  \"max_connections\": 100,\n  \"timeout_ms\": 5000,\n  \"retry_attempts\": 3,\n  \"log_level\": \"info\"\n}}\n```\n\n### Dependencies\n- Database connection pool\n- Event bus system\n- Authentication service\n\n## Performance Considerations\n- Connection pooling reduces overhead\n- Caching improves response times\n- Async processing handles concurrent requests",

            "# Research Notes: Topic {}\n\n## Background\nResearch into modern software architecture patterns and best practices.\n\n## Key Findings\n\n### Microservices Architecture\n- Improved scalability\n- Independent deployment\n- Technology diversity\n- Resilience through isolation\n\n### Event-Driven Design\n- Loose coupling\n- Asynchronous processing\n- Better fault tolerance\n- Real-time updates\n\n### Performance Optimization\n- Database indexing strategies\n- Caching layers\n- Load balancing\n- Monitoring and alerting\n\n## Recommendations\n1. Adopt microservices for complex systems\n2. Implement event-driven communication\n3. Invest in comprehensive monitoring\n4. Plan for scalability from the start\n\n## References\n- Domain-Driven Design\n- The Twelve-Factor App\n- Building Microservices",
        ];

        let template_index = index % content_templates.len();
        let content = content_templates[template_index]
            .replace("{}", &(index + 1).to_string())
            .replace("{}", &chrono::Utc::now().format("%Y-%m-%d").to_string());

        Ok(content)
    }

    /// Generate document tags
    fn generate_document_tags(&self, index: usize) -> Vec<String> {
        let tag_sets = vec![
            vec!["project", "planning", "management"],
            vec!["meeting", "notes", "team"],
            vec!["technical", "documentation", "api"],
            vec!["research", "architecture", "patterns"],
            vec!["development", "implementation", "code"],
            vec!["testing", "quality", "assurance"],
            vec!["deployment", "operations", "devops"],
            vec!["security", "compliance", "audit"],
        ];

        let tag_set_index = index % tag_sets.len();
        tag_sets[tag_set_index].iter().map(|&tag| tag.to_string()).collect()
    }

    /// Generate document metadata
    async fn generate_document_metadata(&self, index: usize) -> Result<HashMap<String, serde_json::Value>> {
        let mut metadata = HashMap::new();

        metadata.insert("author".to_string(), serde_json::Value::String(format!("Test Author {}", (index % 5) + 1)));
        metadata.insert("priority".to_string(), serde_json::Value::String(match index % 3 {
            0 => "high",
            1 => "medium",
            _ => "low",
        }.to_string()));
        metadata.insert("word_count".to_string(), serde_json::Value::Number(serde_json::Number::from(150 + (index * 10))));
        metadata.insert("reviewed".to_string(), serde_json::Value::Bool(index % 2 == 0));
        metadata.insert("version".to_string(), serde_json::Value::String(format!("1.{}", (index % 10) + 1)));

        Ok(metadata)
    }

    /// Register a mock service for testing
    pub async fn register_mock_service(&self, service: MockService) {
        let mut services = self.mock_services.write().await;
        services.insert(service.name.clone(), service);
        debug!(service_name = %service.name, "Registered mock service");
    }

    /// Get a mock service by name
    pub async fn get_mock_service(&self, name: &str) -> Option<MockService> {
        let services = self.mock_services.read().await;
        services.get(name).cloned()
    }

    /// Simulate service call with realistic delays and error rates
    pub async fn simulate_service_call(&self, service_name: &str) -> Result<()> {
        if let Some(service) = self.get_mock_service(service_name).await {
            // Simulate response time
            tokio::time::sleep(service.response_time).await;

            // Simulate errors based on error rate
            if service.error_rate > 0.0 {
                let random_value: f64 = rand::random();
                if random_value < service.error_rate {
                    return Err(anyhow::anyhow!("Simulated service error from {}", service_name));
                }
            }

            debug!(service_name = %service_name, "Service call completed");
        } else {
            warn!(service_name = %service_name, "Mock service not found");
        }

        Ok(())
    }

    /// Run CLI command and capture output
    pub async fn run_cli_command(&self, args: &[&str]) -> Result<(String, String, i32)> {
        let mut cmd = Command::new("cargo");
        cmd.args(&["run", "--bin", "crucible"]).args(args);

        // Set current directory to test directory
        cmd.current_dir(self.test_dir.path());

        // Set environment variables
        cmd.env("RUST_LOG", "debug");
        cmd.env("CRUCIBLE_TEST_MODE", "1");

        debug!(args = ?args, "Running CLI command");

        let output = tokio::task::spawn_blocking(move || cmd.output()).await
            .context("Failed to spawn CLI command")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        debug!(
            exit_code = exit_code,
            stdout_len = stdout.len(),
            stderr_len = stderr.len(),
            "CLI command completed"
        );

        Ok((stdout, stderr, exit_code))
    }

    /// Wait for service to be ready
    pub async fn wait_for_service(&self, service_name: &str, timeout: Duration) -> Result<()> {
        let start_time = Instant::now();

        while start_time.elapsed() < timeout {
            if let Some(service) = self.get_mock_service(service_name).await {
                if matches!(service.status, MockServiceStatus::Healthy) {
                    debug!(service_name = %service_name, "Service is ready");
                    return Ok(());
                }
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Err(anyhow::anyhow!("Service {} not ready within timeout", service_name))
    }

    /// Create test users for concurrent simulation
    pub async fn create_test_users(&self, count: usize, patterns: &[UserBehaviorPattern]) -> Result<Vec<TestUser>> {
        let mut users = Vec::with_capacity(count);

        for i in 0..count {
            let pattern = patterns[i % patterns.len()].clone();
            let user = TestUser {
                id: Uuid::new_v4().to_string(),
                name: format!("Test User {}", i + 1),
                session_id: Uuid::new_v4().to_string(),
                behavior_pattern: pattern,
                activity_timeline: Vec::new(),
            };
            users.push(user);
        }

        info!(user_count = users.len(), "Created test users");
        Ok(users)
    }

    /// Store test data for later retrieval
    pub async fn store_test_data(&self, key: &str, data: serde_json::Value) {
        let mut test_data = self.test_data.write().await;
        test_data.insert(key.to_string(), data);
    }

    /// Retrieve stored test data
    pub async fn get_test_data(&self, key: &str) -> Option<serde_json::Value> {
        let test_data = self.test_data.read().await;
        test_data.get(key).cloned()
    }

    /// Clean up test resources
    pub async fn cleanup(&self) -> Result<()> {
        info!("Cleaning up test resources");

        // Clear test data
        {
            let mut test_data = self.test_data.write().await;
            test_data.clear();
        }

        // Clear mock services
        {
            let mut services = self.mock_services.write().await;
            services.clear();
        }

        info!("Test resources cleaned up");
        Ok(())
    }
}

impl PerformanceMeasurer {
    /// Create new performance measurer
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            checkpoints: Vec::new(),
            memory_samples: Vec::new(),
        }
    }

    /// Add a checkpoint with name
    pub fn checkpoint(&mut self, name: &str) {
        self.checkpoints.push((name.to_string(), Instant::now()));
    }

    /// Sample memory usage
    pub fn sample_memory(&mut self) {
        // In a real implementation, this would read actual memory usage
        let memory_mb = 128; // Mock value
        self.memory_samples.push((Instant::now(), memory_mb));
    }

    /// Get total elapsed time
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Get duration between checkpoints
    pub fn checkpoint_duration(&self, from: &str, to: &str) -> Option<Duration> {
        let from_time = self.checkpoints.iter().find(|(name, _)| name == from)?.1;
        let to_time = self.checkpoints.iter().find(|(name, _)| name == to)?.1;
        Some(to_time.duration_since(from_time))
    }

    /// Get peak memory usage
    pub fn peak_memory(&self) -> Option<u64> {
        self.memory_samples.iter().map(|(_, memory)| *memory).max()
    }

    /// Generate performance report
    pub fn generate_report(&self) -> HashMap<String, f64> {
        let mut report = HashMap::new();

        report.insert("total_time_ms".to_string(), self.elapsed().as_millis() as f64);

        if let Some(peak_memory) = self.peak_memory() {
            report.insert("peak_memory_mb".to_string(), peak_memory as f64);
        }

        // Add checkpoint durations
        for (name, _) in &self.checkpoints {
            if let Some(duration) = self.checkpoint_duration("start", name) {
                report.insert(format!("checkpoint_{}_ms", name), duration.as_millis() as f64);
            }
        }

        report
    }
}

impl ResourceMonitor {
    /// Create new resource monitor
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            cpu_samples: Vec::new(),
            memory_samples: Vec::new(),
            disk_samples: Vec::new(),
        }
    }

    /// Sample current system resources
    pub fn sample(&mut self) {
        let now = Instant::now();

        // In a real implementation, this would read actual system metrics
        let cpu_percent = 25.0 + (rand::random::<f64>() * 50.0); // Mock CPU usage 25-75%
        let memory_mb = 100 + (rand::random::<u64>() % 200); // Mock memory 100-300MB
        let disk_mb = 50 + (rand::random::<u64>() % 100); // Mock disk 50-150MB

        self.cpu_samples.push((now, cpu_percent));
        self.memory_samples.push((now, memory_mb));
        self.disk_samples.push((now, disk_mb));
    }

    /// Start monitoring in background
    pub async fn start_monitoring(&mut self, interval: Duration) -> tokio::task::JoinHandle<()> {
        let mut samples_taken = 0u64;
        let max_samples = 1000; // Limit samples to prevent memory issues

        tokio::spawn(async move {
            let mut local_cpu_samples = Vec::new();
            let mut local_memory_samples = Vec::new();
            let mut local_disk_samples = Vec::new();

            while samples_taken < max_samples {
                tokio::time::sleep(interval).await;

                let now = Instant::now();
                let cpu_percent = 25.0 + (rand::random::<f64>() * 50.0);
                let memory_mb = 100 + (rand::random::<u64>() % 200);
                let disk_mb = 50 + (rand::random::<u64>() % 100);

                local_cpu_samples.push((now, cpu_percent));
                local_memory_samples.push((now, memory_mb));
                local_disk_samples.push((now, disk_mb));

                samples_taken += 1;
            }

            debug!(samples_taken = samples_taken, "Resource monitoring completed");
        })
    }

    /// Get resource usage statistics
    pub fn get_statistics(&self) -> HashMap<String, f64> {
        let mut stats = HashMap::new();

        if !self.cpu_samples.is_empty() {
            let cpu_values: Vec<f64> = self.cpu_samples.iter().map(|(_, cpu)| *cpu).collect();
            stats.insert("avg_cpu_percent".to_string(), cpu_values.iter().sum::<f64>() / cpu_values.len() as f64);
            stats.insert("max_cpu_percent".to_string(), *cpu_values.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap());
        }

        if !self.memory_samples.is_empty() {
            let memory_values: Vec<u64> = self.memory_samples.iter().map(|(_, mem)| *mem).collect();
            stats.insert("avg_memory_mb".to_string(), memory_values.iter().sum::<u64>() as f64 / memory_values.len() as f64);
            stats.insert("max_memory_mb".to_string(), *memory_values.iter().max().unwrap() as f64);
        }

        if !self.disk_samples.is_empty() {
            let disk_values: Vec<u64> = self.disk_samples.iter().map(|(_, disk)| *disk).collect();
            stats.insert("avg_disk_mb".to_string(), disk_values.iter().sum::<u64>() as f64 / disk_values.len() as f64);
            stats.insert("max_disk_mb".to_string(), *disk_values.iter().max().unwrap() as f64);
        }

        stats.insert("monitoring_duration_seconds".to_string(), self.start_time.elapsed().as_secs() as f64);

        stats
    }
}

/// Default mock services for testing
pub fn create_default_mock_services() -> HashMap<String, MockService> {
    let mut services = HashMap::new();

    // ScriptEngine service
    services.insert("script_engine".to_string(), MockService {
        name: "script_engine".to_string(),
        status: MockServiceStatus::Healthy,
        response_time: Duration::from_millis(50),
        error_rate: 0.02, // 2% error rate
        metrics: HashMap::new(),
    });

    // Database service
    services.insert("database".to_string(), MockService {
        name: "database".to_string(),
        status: MockServiceStatus::Healthy,
        response_time: Duration::from_millis(25),
        error_rate: 0.01, // 1% error rate
        metrics: HashMap::new(),
    });

    // Event routing service
    services.insert("event_routing".to_string(), MockService {
        name: "event_routing".to_string(),
        status: MockServiceStatus::Healthy,
        response_time: Duration::from_millis(10),
        error_rate: 0.005, // 0.5% error rate
        metrics: HashMap::new(),
    });

    // Configuration service
    services.insert("configuration".to_string(), MockService {
        name: "configuration".to_string(),
        status: MockServiceStatus::Healthy,
        response_time: Duration::from_millis(15),
        error_rate: 0.0, // No errors for configuration
        metrics: HashMap::new(),
    });

    services
}

/// Helper function to create test result
pub fn create_test_result(
    test_name: String,
    category: TestCategory,
    outcome: TestOutcome,
    duration: Duration,
    metrics: HashMap<String, f64>,
    error_message: Option<String>,
) -> TestResult {
    TestResult {
        test_name,
        category,
        outcome,
        duration,
        metrics,
        error_message,
        context: HashMap::new(),
    }
}

/// Helper function to run test with error handling
pub async fn run_test_with_error_handling<F, Fut>(
    test_name: &str,
    category: TestCategory,
    test_fn: F,
) -> TestResult
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let start_time = Instant::now();
    let result = test_fn().await;
    let duration = start_time.elapsed();

    match result {
        Ok(_) => {
            info!(test_name = test_name, duration_ms = duration.as_millis(), "Test passed");
            create_test_result(
                test_name.to_string(),
                category,
                TestOutcome::Passed,
                duration,
                HashMap::new(),
                None,
            )
        }
        Err(e) => {
            error!(test_name = test_name, error = %e, duration_ms = duration.as_millis(), "Test failed");
            create_test_result(
                test_name.to_string(),
                category,
                TestOutcome::Failed,
                duration,
                HashMap::new(),
                Some(e.to_string()),
            )
        }
    }
}