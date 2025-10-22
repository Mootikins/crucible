//! Integration Test Runner
//!
//! This module provides a comprehensive test runner for executing all integration tests
//! with different configurations and scenarios.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, RwLock, Mutex};
use serde_json::json;

use super::{
    service_integration_tests::*,
    mock_services::*,
    test_utilities::*,
    event_validation::*,
    performance_benchmarks::*,
};

/// Test runner configuration
#[derive(Debug, Clone)]
pub struct TestRunnerConfig {
    pub run_unit_tests: bool,
    pub run_integration_tests: bool,
    pub run_performance_tests: bool,
    pub run_stress_tests: bool,
    pub parallel_execution: bool,
    pub verbose_output: bool,
    pub save_results: bool,
    pub test_timeout_seconds: u64,
}

impl Default for TestRunnerConfig {
    fn default() -> Self {
        Self {
            run_unit_tests: true,
            run_integration_tests: true,
            pub run_performance_tests: false, // Disabled by default as they take time
            pub run_stress_tests: false,     // Disabled by default as they're resource-intensive
            parallel_execution: true,
            verbose_output: false,
            save_results: true,
            test_timeout_seconds: 300, // 5 minutes per test
        }
    }
}

/// Test execution result
#[derive(Debug, Clone)]
pub struct TestResult {
    pub test_name: String,
    pub test_type: TestType,
    pub success: bool,
    pub duration: Duration,
    pub error_message: Option<String>,
    pub metrics: TestMetrics,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TestType {
    Unit,
    Integration,
    Performance,
    Stress,
}

#[derive(Debug, Clone, Default)]
pub struct TestMetrics {
    pub events_processed: usize,
    pub assertions_passed: usize,
    pub assertions_failed: usize,
    pub custom_metrics: HashMap<String, f64>,
}

/// Test suite results
#[derive(Debug, Clone)]
pub struct TestSuiteResults {
    pub config: TestRunnerConfig,
    pub results: Vec<TestResult>,
    pub total_duration: Duration,
    pub success_rate: f64,
    pub summary: TestSummary,
}

#[derive(Debug, Clone)]
pub struct TestSummary {
    pub total_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub skipped_tests: usize,
    pub unit_test_results: Vec<TestResult>,
    pub integration_test_results: Vec<TestResult>,
    pub performance_test_results: Vec<TestResult>,
    pub stress_test_results: Vec<TestResult>,
}

/// Comprehensive integration test runner
pub struct IntegrationTestRunner {
    config: TestRunnerConfig,
    results: Arc<Mutex<Vec<TestResult>>>,
}

impl IntegrationTestRunner {
    pub fn new(config: TestRunnerConfig) -> Self {
        Self {
            config,
            results: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn run_all_tests(&mut self) -> Result<TestSuiteResults, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = std::time::Instant::now();
        println!("🚀 Starting Integration Test Suite");
        println!("Configuration: {:?}", self.config);
        println!();

        let mut test_tasks = Vec::new();

        // Unit tests
        if self.config.run_unit_tests {
            test_tasks.extend(self.create_unit_test_tasks());
        }

        // Integration tests
        if self.config.run_integration_tests {
            test_tasks.extend(self.create_integration_test_tasks());
        }

        // Performance tests
        if self.config.run_performance_tests {
            test_tasks.extend(self.create_performance_test_tasks());
        }

        // Stress tests
        if self.config.run_stress_tests {
            test_tasks.extend(self.create_stress_test_tasks());
        }

        // Execute tests
        if self.config.parallel_execution {
            self.run_tests_parallel(test_tasks).await?;
        } else {
            self.run_tests_sequential(test_tasks).await?;
        }

        let total_duration = start_time.elapsed();
        let results = self.results.lock().await.clone();

        // Generate summary
        let summary = self.generate_summary(&results);

        let test_suite_results = TestSuiteResults {
            config: self.config.clone(),
            results: results.clone(),
            total_duration,
            success_rate: summary.passed_tests as f64 / summary.total_tests as f64,
            summary,
        };

        // Print final results
        self.print_final_results(&test_suite_results);

        // Save results if requested
        if self.config.save_results {
            self.save_results(&test_suite_results).await?;
        }

        Ok(test_suite_results)
    }

    fn create_unit_test_tasks(&self) -> Vec<TestTask> {
        vec![
            TestTask {
                name: "test_mock_event_router_basic".to_string(),
                test_type: TestType::Unit,
                test_function: Box::new(|_| async {
                    let router = MockEventRouter::new();
                    let event = DaemonEvent {
                        id: uuid::Uuid::new_v4(),
                        event_type: EventType::Custom("test".to_string()),
                        priority: EventPriority::Normal,
                        source: EventSource::Service("test".to_string()),
                        targets: vec!["test".to_string()],
                        created_at: chrono::Utc::now(),
                        scheduled_at: None,
                        payload: EventPayload::json(json!({"test": true})),
                        metadata: HashMap::new(),
                        correlation_id: None,
                        causation_id: None,
                        retry_count: 0,
                        max_retries: 3,
                    };

                    router.publish(Box::new(event)).await?;
                    let events = router.get_published_events().await;
                    assert!(!events.is_empty(), "Event should be published");

                    Ok(TestMetrics {
                        events_processed: 1,
                        assertions_passed: 1,
                        assertions_failed: 0,
                        custom_metrics: HashMap::new(),
                    })
                }),
            },
            TestTask {
                name: "test_mock_services_basic".to_string(),
                test_type: TestType::Unit,
                test_function: Box::new(|_| async {
                    let script_engine = MockScriptEngine::new();
                    let request = TestDataFactory::create_test_script_request("test", "print('test')");
                    let response = script_engine.execute_script(request).await?;
                    assert!(response.success, "Script execution should succeed");

                    Ok(TestMetrics {
                        events_processed: 0,
                        assertions_passed: 1,
                        assertions_failed: 0,
                        custom_metrics: HashMap::new(),
                    })
                }),
            },
            TestTask {
                name: "test_event_factory".to_string(),
                test_type: TestType::Unit,
                test_function: Box::new(|_| async {
                    let event = EventFactory::create_script_execution_event("test", "print('test')");
                    assert_eq!(event.targets, vec!["script-engine"]);
                    assert!(matches!(event.event_type, EventType::Custom(_)));

                    Ok(TestMetrics {
                        events_processed: 1,
                        assertions_passed: 2,
                        assertions_failed: 0,
                        custom_metrics: HashMap::new(),
                    })
                }),
            },
        ]
    }

    fn create_integration_test_tasks(&self) -> Vec<TestTask> {
        vec![
            TestTask {
                name: "test_service_lifecycle_events".to_string(),
                test_type: TestType::Integration,
                test_function: Box::new(|_| async {
                    let suite = ServiceTestSuite::new(TestConfig::default()).await?;
                    suite.start_all_services().await?;
                    tokio::time::sleep(Duration::from_millis(100)).await;

                    let events = suite.event_router.get_published_events().await;
                    let start_events: Vec<_> = events.iter()
                        .filter(|e| matches!(&e.event_type, EventType::Service(crucible_services::events::core::ServiceEventType::ServiceStart)))
                        .collect();

                    assert!(start_events.len() >= 4, "Expected at least 4 service start events");

                    suite.stop_all_services().await?;
                    Ok(TestMetrics {
                        events_processed: events.len(),
                        assertions_passed: 1,
                        assertions_failed: 0,
                        custom_metrics: HashMap::from([("services_started".to_string(), start_events.len() as f64)]),
                    })
                }),
            },
            TestTask {
                name: "test_cross_service_communication".to_string(),
                test_type: TestType::Integration,
                test_function: Box::new(|_| async {
                    let suite = ServiceTestSuite::new(TestConfig::default()).await?;
                    suite.start_all_services().await?;

                    let event = EventFactory::create_cross_service_event(
                        "cross_service_test",
                        vec!["script-engine".to_string(), "datastore".to_string()]
                    );

                    suite.event_router.publish(Box::new(event)).await?;
                    tokio::time::sleep(Duration::from_millis(100)).await;

                    let events = suite.event_router.get_published_events().await;
                    let response_events: Vec<_> = events.iter()
                        .filter(|e| matches!(&e.event_type, EventType::Custom(event_type) if event_type.contains("response")))
                        .collect();

                    assert!(!response_events.is_empty(), "Expected response events from target services");

                    Ok(TestMetrics {
                        events_processed: events.len(),
                        assertions_passed: 1,
                        assertions_failed: 0,
                        custom_metrics: HashMap::from([("response_events".to_string(), response_events.len() as f64)]),
                    })
                }),
            },
            TestTask {
                name: "test_event_priority_handling".to_string(),
                test_type: TestType::Integration,
                test_function: Box::new(|_| async {
                    let suite = ServiceTestSuite::new(TestConfig::default()).await?;
                    suite.start_all_services().await?;

                    let priorities = vec![
                        EventPriority::Low,
                        EventPriority::Normal,
                        EventPriority::High,
                        EventPriority::Critical,
                    ];

                    for (i, priority) in priorities.into_iter().enumerate() {
                        let event = DaemonEvent {
                            id: uuid::Uuid::new_v4(),
                            event_type: EventType::Custom(format!("priority_test_{}", i)),
                            priority,
                            source: EventSource::Service("test_coordinator".to_string()),
                            targets: vec!["script-engine".to_string()],
                            created_at: chrono::Utc::now(),
                            scheduled_at: None,
                            payload: EventPayload::json(json!({"priority": format!("{:?}", priority)})),
                            metadata: HashMap::new(),
                            correlation_id: Some(uuid::Uuid::new_v4().to_string()),
                            causation_id: None,
                            retry_count: 0,
                            max_retries: 3,
                        };

                        suite.event_router.publish(Box::new(event)).await?;
                    }

                    tokio::time::sleep(Duration::from_millis(200)).await;

                    let events = suite.event_router.get_published_events().await;
                    let priority_events: Vec<_> = events.iter()
                        .filter(|e| matches!(&e.event_type, EventType::Custom(event_type) if event_type.starts_with("priority_test")))
                        .collect();

                    assert_eq!(priority_events.len(), 4, "All priority test events should be processed");

                    Ok(TestMetrics {
                        events_processed: events.len(),
                        assertions_passed: 1,
                        assertions_failed: 0,
                        custom_metrics: HashMap::from([("priority_events".to_string(), priority_events.len() as f64)]),
                    })
                }),
            },
        ]
    }

    fn create_performance_test_tasks(&self) -> Vec<TestTask> {
        vec![
            TestTask {
                name: "test_event_throughput".to_string(),
                test_type: TestType::Performance,
                test_function: Box::new(|_| async {
                    let suite = ServiceTestSuite::new(TestConfig::default()).await?;
                    suite.start_all_services().await?;

                    let event_count = 1000;
                    let start_time = std::time::Instant::now();

                    let mut handles = Vec::new();
                    for batch in 0..10 {
                        let event_router = suite.event_router.clone();
                        let handle = tokio::spawn(async move {
                            for i in 0..100 {
                                let event = DaemonEvent {
                                    id: uuid::Uuid::new_v4(),
                                    event_type: EventType::Custom(format!("perf_test_{}_{}", batch, i)),
                                    priority: EventPriority::Normal,
                                    source: EventSource::Service("perf_test".to_string()),
                                    targets: vec!["script-engine".to_string()],
                                    created_at: chrono::Utc::now(),
                                    scheduled_at: None,
                                    payload: EventPayload::json(json!({"batch": batch, "index": i})),
                                    metadata: HashMap::new(),
                                    correlation_id: Some(format!("batch_{}", batch)),
                                    causation_id: None,
                                    retry_count: 0,
                                    max_retries: 1,
                                };

                                let _ = event_router.publish(Box::new(event)).await;
                            }
                        });
                        handles.push(handle);
                    }

                    for handle in handles {
                        handle.await?;
                    }

                    let publish_time = start_time.elapsed();
                    tokio::time::sleep(Duration::from_millis(1000)).await;

                    let total_time = start_time.elapsed();
                    let events = suite.event_router.get_published_events().await;
                    let processed_events = events.iter()
                        .filter(|e| matches!(&e.event_type, EventType::Custom(event_type) if event_type.starts_with("perf_test")))
                        .count();

                    let throughput = processed_events as f64 / total_time.as_secs_f64();

                    assert!(throughput > 100.0, "Throughput should be at least 100 events/sec");

                    Ok(TestMetrics {
                        events_processed: processed_events,
                        assertions_passed: 1,
                        assertions_failed: 0,
                        custom_metrics: HashMap::from([
                            ("throughput_events_per_sec".to_string(), throughput),
                            ("publish_time_ms".to_string(), publish_time.as_millis() as f64),
                            ("total_time_ms".to_string(), total_time.as_millis() as f64),
                        ]),
                    })
                }),
            },
        ]
    }

    fn create_stress_test_tasks(&self) -> Vec<TestTask> {
        vec![
            TestTask {
                name: "test_high_concurrency_stress".to_string(),
                test_type: TestType::Stress,
                test_function: Box::new(|_| async {
                    let suite = ServiceTestSuite::new(TestConfig::default()).await?;
                    suite.start_all_services().await?;

                    let concurrent_tasks = 100;
                    let events_per_task = 50;
                    let start_time = std::time::Instant::now();

                    let mut handles = Vec::new();
                    for task_id in 0..concurrent_tasks {
                        let event_router = suite.event_router.clone();
                        let handle = tokio::spawn(async move {
                            for i in 0..events_per_task {
                                let event = DaemonEvent {
                                    id: uuid::Uuid::new_v4(),
                                    event_type: EventType::Custom(format!("stress_test_{}_{}", task_id, i)),
                                    priority: EventPriority::Normal,
                                    source: EventSource::Service("stress_test".to_string()),
                                    targets: vec!["script-engine".to_string()],
                                    created_at: chrono::Utc::now(),
                                    scheduled_at: None,
                                    payload: EventPayload::json(json!({"task_id": task_id, "event_index": i})),
                                    metadata: HashMap::new(),
                                    correlation_id: Some(format!("stress_task_{}", task_id)),
                                    causation_id: None,
                                    retry_count: 0,
                                    max_retries: 2,
                                };

                                let _ = event_router.publish(Box::new(event)).await;
                            }
                        });
                        handles.push(handle);
                    }

                    for handle in handles {
                        handle.await?;
                    }

                    let total_time = start_time.elapsed();
                    tokio::time::sleep(Duration::from_millis(2000)).await;

                    let events = suite.event_router.get_published_events().await;
                    let stress_events = events.iter()
                        .filter(|e| matches!(&e.event_type, EventType::Custom(event_type) if event_type.starts_with("stress_test")))
                        .count();

                    let expected_events = concurrent_tasks * events_per_task;
                    let success_rate = stress_events as f64 / expected_events as f64;

                    assert!(success_rate > 0.8, "Success rate should be at least 80% under stress");

                    Ok(TestMetrics {
                        events_processed: stress_events,
                        assertions_passed: 1,
                        assertions_failed: 0,
                        custom_metrics: HashMap::from([
                            ("concurrent_tasks".to_string(), concurrent_tasks as f64),
                            ("events_per_task".to_string(), events_per_task as f64),
                            ("success_rate".to_string(), success_rate),
                            ("total_time_ms".to_string(), total_time.as_millis() as f64),
                        ]),
                    })
                }),
            },
        ]
    }

    async fn run_tests_parallel(&self, tasks: Vec<TestTask>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut handles = Vec::new();

        for task in tasks {
            let results = self.results.clone();
            let verbose = self.config.verbose_output;
            let timeout = Duration::from_secs(self.config.test_timeout_seconds);

            let handle = tokio::spawn(async move {
                let start_time = std::time::Instant::now();

                if verbose {
                    println!("⚡ Starting test: {} ({:?})", task.name, task.test_type);
                }

                let result = tokio::time::timeout(timeout, (task.test_function)(&task)).await;

                let duration = start_time.elapsed();
                let test_result = match result {
                    Ok(Ok(metrics)) => TestResult {
                        test_name: task.name.clone(),
                        test_type: task.test_type.clone(),
                        success: true,
                        duration,
                        error_message: None,
                        metrics,
                    },
                    Ok(Err(e)) => TestResult {
                        test_name: task.name.clone(),
                        test_type: task.test_type.clone(),
                        success: false,
                        duration,
                        error_message: Some(format!("Test failed: {}", e)),
                        metrics: TestMetrics::default(),
                    },
                    Err(_) => TestResult {
                        test_name: task.name.clone(),
                        test_type: task.test_type.clone(),
                        success: false,
                        duration,
                        error_message: Some("Test timed out".to_string()),
                        metrics: TestMetrics::default(),
                    },
                };

                if verbose {
                    if test_result.success {
                        println!("✅ {} passed in {:?}", task.name, duration);
                    } else {
                        println!("❌ {} failed in {:?}", task.name, duration);
                        if let Some(error) = &test_result.error_message {
                            println!("   Error: {}", error);
                        }
                    }
                }

                results.lock().await.push(test_result);
            });

            handles.push(handle);
        }

        // Wait for all tests to complete
        for handle in handles {
            handle.await?;
        }

        Ok(())
    }

    async fn run_tests_sequential(&self, tasks: Vec<TestTask>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for task in tasks {
            let start_time = std::time::Instant::now();

            println!("🔄 Running test: {} ({:?})", task.name, task.test_type);

            let timeout = Duration::from_secs(self.config.test_timeout_seconds);
            let result = tokio::time::timeout(timeout, (task.test_function)(&task)).await;

            let duration = start_time.elapsed();
            let test_result = match result {
                Ok(Ok(metrics)) => {
                    println!("✅ {} passed in {:?}", task.name, duration);
                    TestResult {
                        test_name: task.name.clone(),
                        test_type: task.test_type.clone(),
                        success: true,
                        duration,
                        error_message: None,
                        metrics,
                    }
                },
                Ok(Err(e)) => {
                    println!("❌ {} failed in {:?}", task.name, duration);
                    println!("   Error: {}", e);
                    TestResult {
                        test_name: task.name.clone(),
                        test_type: task.test_type.clone(),
                        success: false,
                        duration,
                        error_message: Some(format!("Test failed: {}", e)),
                        metrics: TestMetrics::default(),
                    }
                },
                Err(_) => {
                    println!("⏰ {} timed out after {:?}", task.name, duration);
                    TestResult {
                        test_name: task.name.clone(),
                        test_type: task.test_type.clone(),
                        success: false,
                        duration,
                        error_message: Some("Test timed out".to_string()),
                        metrics: TestMetrics::default(),
                    }
                },
            };

            self.results.lock().await.push(test_result);
        }

        Ok(())
    }

    fn generate_summary(&self, results: &[TestResult]) -> TestSummary {
        let mut unit_test_results = Vec::new();
        let mut integration_test_results = Vec::new();
        let mut performance_test_results = Vec::new();
        let mut stress_test_results = Vec::new();

        for result in results {
            match result.test_type {
                TestType::Unit => unit_test_results.push(result.clone()),
                TestType::Integration => integration_test_results.push(result.clone()),
                TestType::Performance => performance_test_results.push(result.clone()),
                TestType::Stress => stress_test_results.push(result.clone()),
            }
        }

        let total_tests = results.len();
        let passed_tests = results.iter().filter(|r| r.success).count();
        let failed_tests = results.iter().filter(|r| !r.success).count();
        let skipped_tests = 0; // No skipping mechanism in this implementation

        TestSummary {
            total_tests,
            passed_tests,
            failed_tests,
            skipped_tests,
            unit_test_results,
            integration_test_results,
            performance_test_results,
            stress_test_results,
        }
    }

    fn print_final_results(&self, results: &TestSuiteResults) {
        println!("\n" + "=".repeat(80).as_str());
        println!("🏁 INTEGRATION TEST SUITE COMPLETED");
        println!("=".repeat(80));
        println!("⏱️  Total Duration: {:?}", results.total_duration);
        println!("📊 Success Rate: {:.1}%", results.success_rate * 100.0);
        println!();

        let summary = &results.summary;
        println!("📈 Test Summary:");
        println!("   Total Tests: {}", summary.total_tests);
        println!("   ✅ Passed: {}", summary.passed_tests);
        println!("   ❌ Failed: {}", summary.failed_tests);
        println!("   ⏭️  Skipped: {}", summary.skipped_tests);
        println!();

        println!("📋 Results by Type:");
        println!("   Unit Tests: {} passed / {} total",
                summary.unit_test_results.iter().filter(|r| r.success).count(),
                summary.unit_test_results.len());
        println!("   Integration Tests: {} passed / {} total",
                summary.integration_test_results.iter().filter(|r| r.success).count(),
                summary.integration_test_results.len());
        println!("   Performance Tests: {} passed / {} total",
                summary.performance_test_results.iter().filter(|r| r.success).count(),
                summary.performance_test_results.len());
        println!("   Stress Tests: {} passed / {} total",
                summary.stress_test_results.iter().filter(|r| r.success).count(),
                summary.stress_test_results.len());
        println!();

        // Show failed tests if any
        let failed_tests: Vec<_> = results.results.iter().filter(|r| !r.success).collect();
        if !failed_tests.is_empty() {
            println!("❌ Failed Tests:");
            for test in failed_tests {
                println!("   - {} ({:?})", test.test_name, test.test_type);
                if let Some(error) = &test.error_message {
                    println!("     Error: {}", error);
                }
            }
            println!();
        }

        if results.success_rate >= 0.9 {
            println!("🎉 Excellent! Test suite passed with high success rate!");
        } else if results.success_rate >= 0.7 {
            println!("⚠️  Warning: Test suite passed but some tests failed");
        } else {
            println!("🚨 Critical: Many tests failed - please review the results");
        }

        println!("=".repeat(80));
    }

    async fn save_results(&self, results: &TestSuiteResults) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("test_results_{}.json", timestamp);

        let report = json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "config": results.config,
            "summary": {
                "total_tests": results.summary.total_tests,
                "passed_tests": results.summary.passed_tests,
                "failed_tests": results.summary.failed_tests,
                "skipped_tests": results.summary.skipped_tests,
                "success_rate": results.success_rate,
                "total_duration_ms": results.total_duration.as_millis(),
            },
            "results": results.results.iter().map(|r| {
                json!({
                    "test_name": r.test_name,
                    "test_type": format!("{:?}", r.test_type),
                    "success": r.success,
                    "duration_ms": r.duration.as_millis(),
                    "error_message": r.error_message,
                    "metrics": {
                        "events_processed": r.metrics.events_processed,
                        "assertions_passed": r.metrics.assertions_passed,
                        "assertions_failed": r.metrics.assertions_failed,
                        "custom_metrics": r.metrics.custom_metrics,
                    }
                })
            }).collect::<Vec<_>>()
        });

        tokio::fs::write(&filename, serde_json::to_string_pretty(&report)?).await?;
        println!("💾 Test results saved to: {}", filename);

        Ok(())
    }
}

/// Test task definition
struct TestTask {
    name: String,
    test_type: TestType,
    test_function: Box<dyn Fn(&TestTask) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<TestMetrics, Box<dyn std::error::Error + Send + Sync>>> + Send>> + Send + Sync>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_runner_basic() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let config = TestRunnerConfig {
            run_unit_tests: true,
            run_integration_tests: false,
            run_performance_tests: false,
            run_stress_tests: false,
            parallel_execution: false,
            verbose_output: true,
            save_results: false,
            test_timeout_seconds: 30,
        };

        let mut runner = IntegrationTestRunner::new(config);
        let results = runner.run_all_tests().await?;

        assert!(results.summary.total_tests > 0);
        assert!(results.success_rate > 0.0);

        Ok(())
    }
}