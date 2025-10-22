//! # Memory Testing Framework
//!
//! Comprehensive memory testing and profiling utilities for Crucible services.
//! This module provides tools to test memory usage, detect leaks, and validate
//! memory efficiency across all service implementations.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Mutex};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

/// Memory profiling and testing framework
pub struct MemoryTestFramework {
    /// Test configuration
    config: MemoryTestConfig,
    /// Active test sessions
    active_sessions: Arc<RwLock<HashMap<String, MemoryTestSession>>>,
    /// Historical test results
    test_history: Arc<RwLock<Vec<MemoryTestResult>>>,
    /// Memory profiler instance
    profiler: Arc<RwLock<MemoryProfiler>>,
}

/// Configuration for memory testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTestConfig {
    /// Test duration for different scenarios
    pub test_durations: TestDurations,
    /// Memory thresholds and limits
    pub thresholds: MemoryThresholds,
    /// Load testing parameters
    pub load_testing: LoadTestingConfig,
    /// Leak detection settings
    pub leak_detection: LeakDetectionConfig,
    /// Reporting configuration
    pub reporting: ReportingConfig,
}

/// Test durations for different scenarios
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestDurations {
    /// Short tests (basic operations)
    pub short_test_seconds: u64,
    /// Medium tests (load testing)
    pub medium_test_seconds: u64,
    /// Long tests (leak detection)
    pub long_test_seconds: u64,
    /// Interval between measurements
    pub measurement_interval_ms: u64,
}

/// Memory thresholds for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryThresholds {
    /// Maximum baseline memory (bytes)
    pub max_baseline_memory_bytes: u64,
    /// Maximum memory growth rate (bytes/second)
    pub max_memory_growth_rate: u64,
    /// Maximum memory per operation (bytes)
    pub max_memory_per_operation: u64,
    /// Memory leak threshold (bytes)
    pub leak_threshold_bytes: u64,
    /// Cleanup timeout (seconds)
    pub cleanup_timeout_seconds: u64,
}

/// Load testing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadTestingConfig {
    /// Concurrent operations for load testing
    pub concurrent_operations: u32,
    /// Operations per second for stress testing
    pub operations_per_second: u32,
    /// Large data size for memory testing (bytes)
    pub large_data_size_bytes: u64,
    /// Maximum payload size for testing
    pub max_payload_size_bytes: u64,
}

/// Leak detection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeakDetectionConfig {
    /// Enable leak detection
    pub enabled: bool,
    /// Sampling interval for leak detection (ms)
    pub sampling_interval_ms: u64,
    /// Minimum samples for leak detection
    pub min_samples: u32,
    /// Statistical significance threshold
    pub significance_threshold: f64,
    /// Memory growth pattern analysis
    pub enable_pattern_analysis: bool,
}

/// Reporting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportingConfig {
    /// Enable detailed logging
    pub enable_detailed_logging: bool,
    /// Export results to file
    pub export_to_file: bool,
    /// Generate charts and graphs
    pub generate_charts: bool,
    /// Alert on threshold violations
    pub alert_on_violations: bool,
}

/// Active memory test session
#[derive(Debug)]
pub struct MemoryTestSession {
    /// Session ID
    session_id: String,
    /// Service being tested
    service_type: ServiceType,
    /// Test scenario
    scenario: TestScenario,
    /// Start time
    start_time: Instant,
    /// Memory measurements
    measurements: Arc<Mutex<Vec<MemoryMeasurement>>>,
    /// Current test status
    status: TestStatus,
    /// Test-specific data
    test_data: HashMap<String, serde_json::Value>,
}

/// Memory measurement snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMeasurement {
    /// Timestamp
    timestamp: chrono::DateTime<chrono::Utc>,
    /// Total memory usage (bytes)
    total_memory_bytes: u64,
    /// Heap memory usage (bytes)
    heap_memory_bytes: u64,
    /// Stack memory usage (bytes)
    stack_memory_bytes: u64,
    /// Cache memory usage (bytes)
    cache_memory_bytes: u64,
    /// Connection memory usage (bytes)
    connection_memory_bytes: u64,
    /// Arc/Mutex reference count
    arc_ref_count: u32,
    /// Active task count
    active_tasks: u32,
    /// Custom metrics
    custom_metrics: HashMap<String, f64>,
}

/// Types of services to test
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServiceType {
    ScriptEngine,
    InferenceEngine,
    DataStore,
    McpGateway,
}

/// Test scenarios for memory validation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TestScenario {
    /// Idle baseline measurement
    IdleBaseline,
    /// Single operation memory impact
    SingleOperation,
    /// High frequency operations
    HighFrequencyOperations,
    /// Large data processing
    LargeDataProcessing,
    /// Concurrent operations
    ConcurrentOperations,
    /// Long-running stability test
    LongRunningStability,
    /// Resource exhaustion behavior
    ResourceExhaustion,
    /// Cleanup validation
    CleanupValidation,
}

/// Test execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TestStatus {
    Initializing,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

/// Complete test results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTestResult {
    /// Test session ID
    session_id: String,
    /// Service type
    service_type: ServiceType,
    /// Test scenario
    scenario: TestScenario,
    /// Start and end times
    start_time: chrono::DateTime<chrono::Utc>,
    end_time: chrono::DateTime<chrono::Utc>,
    /// Test duration
    duration: Duration,
    /// Test status
    status: TestStatus,
    /// Memory statistics
    memory_stats: MemoryStatistics,
    /// Leak detection results
    leak_detection: LeakDetectionResult,
    /// Performance metrics
    performance_metrics: PerformanceMetrics,
    /// Threshold violations
    violations: Vec<ThresholdViolation>,
    /// Recommendations
    recommendations: Vec<String>,
}

/// Memory statistics for test results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStatistics {
    /// Baseline memory usage (bytes)
    baseline_memory_bytes: u64,
    /// Peak memory usage (bytes)
    peak_memory_bytes: u64,
    /// Average memory usage (bytes)
    average_memory_bytes: u64,
    /// Memory growth rate (bytes/second)
    memory_growth_rate: f64,
    /// Memory volatility (standard deviation)
    memory_volatility: f64,
    /// Memory cleanup efficiency (0-1)
    cleanup_efficiency: f64,
    /// Memory per operation (bytes)
    memory_per_operation: f64,
}

/// Leak detection results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeakDetectionResult {
    /// Leak detected
    leak_detected: bool,
    /// Leak rate (bytes/second)
    leak_rate: f64,
    /// Confidence level (0-1)
    confidence: f64,
    /// Leak pattern analysis
    pattern_analysis: Option<LeakPatternAnalysis>,
    /// Suspected leak sources
    suspected_sources: Vec<String>,
}

/// Leak pattern analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeakPatternAnalysis {
    /// Pattern type
    pattern_type: LeakPatternType,
    /// Growth characteristics
    growth_characteristics: GrowthCharacteristics,
    /// Correlation with operations
    operation_correlation: f64,
    /// Time-based patterns
    time_patterns: Vec<TimePattern>,
}

/// Types of memory leak patterns
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LeakPatternType {
    Linear,
    Exponential,
    Stepped,
    Sporadic,
    Cyclic,
}

/// Growth characteristics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrowthCharacteristics {
    /// Growth rate (bytes/second)
    rate: f64,
    /// Acceleration (bytes/second²)
    acceleration: f64,
    /// Pattern consistency (0-1)
    consistency: f64,
}

/// Time-based patterns in memory usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimePattern {
    /// Pattern period (seconds)
    period: Duration,
    /// Pattern amplitude (bytes)
    amplitude: u64,
    /// Pattern phase
    phase: f64,
}

/// Performance metrics during test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Operations per second
    operations_per_second: f64,
    /// Average response time (milliseconds)
    average_response_time_ms: f64,
    /// P95 response time (milliseconds)
    p95_response_time_ms: f64,
    /// Error rate (0-1)
    error_rate: f64,
    /// Throughput (bytes/second)
    throughput: f64,
    /// Resource utilization
    resource_utilization: ResourceUtilization,
}

/// Resource utilization metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUtilization {
    /// CPU utilization (0-1)
    cpu_utilization: f64,
    /// Memory utilization (0-1)
    memory_utilization: f64,
    /// Connection utilization (0-1)
    connection_utilization: f64,
    /// Cache hit rate (0-1)
    cache_hit_rate: f64,
}

/// Threshold violation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdViolation {
    /// Violation type
    violation_type: ViolationType,
    /// Threshold value
    threshold: f64,
    /// Actual value
    actual: f64,
    /// Severity level
    severity: ViolationSeverity,
    /// Description
    description: String,
}

/// Types of threshold violations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ViolationType {
    MemoryBaseline,
    MemoryGrowthRate,
    MemoryPerOperation,
    MemoryLeak,
    CleanupTimeout,
    ResourceExhaustion,
}

/// Severity levels for violations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ViolationSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Memory profiler for tracking memory usage
pub struct MemoryProfiler {
    /// Profile configuration
    config: ProfilerConfig,
    /// Active measurements
    measurements: Vec<MemoryMeasurement>,
    /// Profiling state
    state: ProfilerState,
}

/// Memory profiler configuration
#[derive(Debug, Clone)]
pub struct ProfilerConfig {
    /// Sampling interval
    sampling_interval: Duration,
    /// Enable detailed profiling
    enable_detailed: bool,
    /// Track Arc/Mutex references
    track_references: bool,
    /// Custom metrics to track
    custom_metrics: Vec<String>,
}

/// Profiler state
#[derive(Debug, Clone)]
pub struct ProfilerState {
    /// Is profiling active
    active: bool,
    /// Start time
    start_time: Option<Instant>,
    /// Last measurement time
    last_measurement: Option<Instant>,
}

impl Default for MemoryTestConfig {
    fn default() -> Self {
        Self {
            test_durations: TestDurations {
                short_test_seconds: 300,      // 5 minutes
                medium_test_seconds: 3600,    // 1 hour
                long_test_seconds: 28800,     // 8 hours
                measurement_interval_ms: 1000, // 1 second
            },
            thresholds: MemoryThresholds {
                max_baseline_memory_bytes: 100 * 1024 * 1024, // 100MB
                max_memory_growth_rate: 1024 * 1024,         // 1MB/s
                max_memory_per_operation: 10 * 1024 * 1024,  // 10MB
                leak_threshold_bytes: 5 * 1024 * 1024,       // 5MB
                cleanup_timeout_seconds: 60,                  // 1 minute
            },
            load_testing: LoadTestingConfig {
                concurrent_operations: 100,
                operations_per_second: 1000,
                large_data_size_bytes: 100 * 1024 * 1024, // 100MB
                max_payload_size_bytes: 10 * 1024 * 1024, // 10MB
            },
            leak_detection: LeakDetectionConfig {
                enabled: true,
                sampling_interval_ms: 500,
                min_samples: 10,
                significance_threshold: 0.95,
                enable_pattern_analysis: true,
            },
            reporting: ReportingConfig {
                enable_detailed_logging: true,
                export_to_file: true,
                generate_charts: false,
                alert_on_violations: true,
            },
        }
    }
}

impl Default for ProfilerConfig {
    fn default() -> Self {
        Self {
            sampling_interval: Duration::from_millis(1000),
            enable_detailed: true,
            track_references: true,
            custom_metrics: vec![
                "cache_size".to_string(),
                "active_connections".to_string(),
                "queue_size".to_string(),
            ],
        }
    }
}

impl MemoryTestFramework {
    /// Create a new memory testing framework
    pub fn new(config: MemoryTestConfig) -> Self {
        Self {
            config,
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
            test_history: Arc::new(RwLock::new(Vec::new())),
            profiler: Arc::new(RwLock::new(MemoryProfiler::new(ProfilerConfig::default()))),
        }
    }

    /// Start a new memory test session
    pub async fn start_test(
        &self,
        service_type: ServiceType,
        scenario: TestScenario,
        test_data: HashMap<String, serde_json::Value>,
    ) -> Result<String, MemoryTestError> {
        let session_id = Uuid::new_v4().to_string();

        info!("Starting memory test: {} for service: {:?} scenario: {:?}",
              session_id, service_type, scenario);

        let session = MemoryTestSession {
            session_id: session_id.clone(),
            service_type: service_type.clone(),
            scenario: scenario.clone(),
            start_time: Instant::now(),
            measurements: Arc::new(Mutex::new(Vec::new())),
            status: TestStatus::Initializing,
            test_data,
        };

        // Store the session
        {
            let mut sessions = self.active_sessions.write().await;
            sessions.insert(session_id.clone(), session);
        }

        // Initialize the profiler
        {
            let mut profiler = self.profiler.write().await;
            profiler.start_profiling().await?;
        }

        // Start the test execution
        let framework = self.clone();
        tokio::spawn(async move {
            if let Err(e) = framework.execute_test(session_id.clone()).await {
                error!("Test execution failed: {:?}", e);
                framework.update_session_status(&session_id, TestStatus::Failed(e.to_string())).await;
            }
        });

        Ok(session_id)
    }

    /// Execute a memory test
    async fn execute_test(&self, session_id: String) -> Result<(), MemoryTestError> {
        // Update session status to running
        self.update_session_status(&session_id, TestStatus::Running).await;

        // Get session details
        let (service_type, scenario, duration) = {
            let sessions = self.active_sessions.read().await;
            let session = sessions.get(&session_id)
                .ok_or(MemoryTestError::SessionNotFound(session_id.clone()))?;

            let duration = match scenario {
                TestScenario::IdleBaseline => Duration::from_secs(self.config.test_durations.short_test_seconds),
                TestScenario::SingleOperation => Duration::from_secs(60),
                TestScenario::HighFrequencyOperations => Duration::from_secs(self.config.test_durations.medium_test_seconds),
                TestScenario::LargeDataProcessing => Duration::from_secs(self.config.test_durations.medium_test_seconds),
                TestScenario::ConcurrentOperations => Duration::from_secs(self.config.test_durations.medium_test_seconds),
                TestScenario::LongRunningStability => Duration::from_secs(self.config.test_durations.long_test_seconds),
                TestScenario::ResourceExhaustion => Duration::from_secs(self.config.test_durations.short_test_seconds),
                TestScenario::CleanupValidation => Duration::from_secs(self.config.test_durations.short_test_seconds),
            };

            (session.service_type.clone(), scenario.clone(), duration)
        };

        info!("Executing test: {} for service: {:?} scenario: {:?} duration: {:?}",
              session_id, service_type, scenario, duration);

        // Start memory monitoring
        let monitoring_handle = self.start_memory_monitoring(session_id.clone()).await?;

        // Execute the specific test scenario
        match scenario {
            TestScenario::IdleBaseline => self.execute_idle_baseline_test(&session_id).await?,
            TestScenario::SingleOperation => self.execute_single_operation_test(&session_id, &service_type).await?,
            TestScenario::HighFrequencyOperations => self.execute_high_frequency_test(&session_id, &service_type).await?,
            TestScenario::LargeDataProcessing => self.execute_large_data_test(&session_id, &service_type).await?,
            TestScenario::ConcurrentOperations => self.execute_concurrent_test(&session_id, &service_type).await?,
            TestScenario::LongRunningStability => self.execute_stability_test(&session_id, &service_type, duration).await?,
            TestScenario::ResourceExhaustion => self.execute_exhaustion_test(&session_id, &service_type).await?,
            TestScenario::CleanupValidation => self.execute_cleanup_test(&session_id, &service_type).await?,
        }

        // Stop memory monitoring
        monitoring_handle.abort();

        // Analyze results and generate report
        let test_result = self.analyze_test_results(&session_id).await?;

        // Store results
        {
            let mut history = self.test_history.write().await;
            history.push(test_result.clone());
        }

        // Clean up session
        {
            let mut sessions = self.active_sessions.write().await;
            sessions.remove(&session_id);
        }

        info!("Test completed: {} - Status: {:?}", session_id, test_result.status);

        Ok(())
    }

    /// Start memory monitoring for a test session
    async fn start_memory_monitoring(&self, session_id: String) -> Result<tokio::task::JoinHandle<()>, MemoryTestError> {
        let measurement_interval = Duration::from_millis(self.config.test_durations.measurement_interval_ms);
        let sessions = self.active_sessions.clone();
        let profiler = self.profiler.clone();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(measurement_interval);

            loop {
                interval.tick().await;

                // Check if session is still active
                let session_active = {
                    let sessions = sessions.read().await;
                    sessions.contains_key(&session_id)
                };

                if !session_active {
                    break;
                }

                // Take memory measurement
                if let Ok(measurement) = profiler.read().await.take_measurement().await {
                    // Store measurement
                    {
                        let sessions = sessions.read().await;
                        if let Some(session) = sessions.get(&session_id) {
                            let mut measurements = session.measurements.lock().await;
                            measurements.push(measurement);
                        }
                    }
                }
            }
        });

        Ok(handle)
    }

    /// Get test results for a session
    pub async fn get_test_results(&self, session_id: &str) -> Result<Option<MemoryTestResult>, MemoryTestError> {
        let history = self.test_history.read().await;
        Ok(history.iter().find(|r| r.session_id == session_id).cloned())
    }

    /// Get all test results
    pub async fn get_all_test_results(&self) -> Vec<MemoryTestResult> {
        let history = self.test_history.read().await;
        history.clone()
    }

    /// Get active test sessions
    pub async fn get_active_sessions(&self) -> HashMap<String, (ServiceType, TestScenario, TestStatus)> {
        let sessions = self.active_sessions.read().await;
        sessions.iter().map(|(id, session)| {
            (id.clone(), (session.service_type.clone(), session.scenario.clone(), session.status.clone()))
        }).collect()
    }

    /// Cancel a running test
    pub async fn cancel_test(&self, session_id: &str) -> Result<(), MemoryTestError> {
        self.update_session_status(session_id, TestStatus::Cancelled).await;
        info!("Test cancelled: {}", session_id);
        Ok(())
    }

    /// Update session status
    async fn update_session_status(&self, session_id: &str, status: TestStatus) {
        let mut sessions = self.active_sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.status = status;
        }
    }

    // Test scenario implementations will be added in the next part...

    /// Analyze test results and generate comprehensive report
    async fn analyze_test_results(&self, session_id: &str) -> Result<MemoryTestResult, MemoryTestError> {
        let (session, measurements) = {
            let sessions = self.active_sessions.read().await;
            let session = sessions.get(session_id)
                .ok_or(MemoryTestError::SessionNotFound(session_id.to_string()))?;
            let measurements = session.measurements.lock().await.clone();
            (session.clone(), measurements)
        };

        if measurements.is_empty() {
            return Err(MemoryTestError::InsufficientData("No measurements collected".to_string()));
        }

        // Calculate memory statistics
        let memory_stats = self.calculate_memory_statistics(&measurements).await?;

        // Detect memory leaks
        let leak_detection = self.detect_memory_leaks(&measurements).await?;

        // Calculate performance metrics
        let performance_metrics = self.calculate_performance_metrics(&session, &measurements).await?;

        // Check for threshold violations
        let violations = self.check_threshold_violations(&memory_stats, &leak_detection).await?;

        // Generate recommendations
        let recommendations = self.generate_recommendations(&memory_stats, &leak_detection, &violations).await?;

        Ok(MemoryTestResult {
            session_id: session_id.to_string(),
            service_type: session.service_type,
            scenario: session.scenario,
            start_time: chrono::Utc::now() - session.start_time.elapsed(),
            end_time: chrono::Utc::now(),
            duration: session.start_time.elapsed(),
            status: session.status,
            memory_stats,
            leak_detection,
            performance_metrics,
            violations,
            recommendations,
        })
    }

    // Helper methods for analysis will be added in the next part...
}

// Clone implementation for MemoryTestFramework
impl Clone for MemoryTestFramework {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            active_sessions: self.active_sessions.clone(),
            test_history: self.test_history.clone(),
            profiler: self.profiler.clone(),
        }
    }
}

/// Memory test error types
#[derive(Debug, thiserror::Error)]
pub enum MemoryTestError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    #[error("Insufficient data: {0}")]
    InsufficientData(String),
    #[error("Profiling error: {0}")]
    ProfilingError(String),
    #[error("Service error: {0}")]
    ServiceError(String),
    #[error("Analysis error: {0}")]
    AnalysisError(String),
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

// Export submodules
pub mod profiler;
pub mod test_scenarios;
pub mod analysis;
pub mod test_runner;

// Re-export main types for convenience
pub use profiler::MemoryProfiler;
pub use test_runner::{MemoryTestRunner, ServiceTestManager, ScriptEngineTestManager};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_framework_creation() {
        let config = MemoryTestConfig::default();
        let framework = MemoryTestFramework::new(config);

        // Should start with no active sessions
        let active = framework.get_active_sessions().await;
        assert!(active.is_empty());

        // Should start with empty history
        let history = framework.get_all_test_results().await;
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn test_session_creation() {
        let framework = MemoryTestFramework::new(MemoryTestConfig::default());

        let session_id = framework.start_test(
            ServiceType::ScriptEngine,
            TestScenario::IdleBaseline,
            HashMap::new(),
        ).await.unwrap();

        // Should have one active session
        let active = framework.get_active_sessions().await;
        assert_eq!(active.len(), 1);
        assert!(active.contains_key(&session_id));

        // Clean up
        framework.cancel_test(&session_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_memory_profiler() {
        let profiler = MemoryProfiler::new(ProfilerConfig::default());

        profiler.start_profiling().await.unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        let measurement = profiler.take_measurement().await.unwrap();
        assert!(measurement.total_memory_bytes > 0);

        profiler.stop_profiling().await.unwrap();
    }
}