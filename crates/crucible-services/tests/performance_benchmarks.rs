//! Performance Benchmarks for Service Integration
//!
//! This module provides comprehensive performance benchmarks for testing the
//! scalability and efficiency of the event-driven service architecture.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, RwLock, Mutex};
use uuid::Uuid;
use chrono::Utc;
use serde_json::json;

use super::test_utilities::{MockServiceTestSuite, TestConfigBuilder, EventFactory, TestDataFactory, PerformanceTracker};

/// Benchmark configuration
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub name: String,
    pub description: String,
    pub event_count: usize,
    pub concurrent_tasks: usize,
    pub event_types: Vec<String>,
    pub target_services: Vec<String>,
    pub duration_seconds: u64,
    pub warmup_events: usize,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            name: "default_benchmark".to_string(),
            description: "Default performance benchmark".to_string(),
            event_count: 1000,
            concurrent_tasks: 10,
            event_types: vec!["test_event".to_string()],
            target_services: vec!["script-engine".to_string()],
            duration_seconds: 60,
            warmup_events: 100,
        }
    }
}

/// Benchmark results
#[derive(Debug, Clone)]
pub struct BenchmarkResults {
    pub config: BenchmarkConfig,
    pub total_events: usize,
    pub successful_events: usize,
    pub failed_events: usize,
    pub total_duration: Duration,
    pub throughput_events_per_second: f64,
    pub average_latency: Duration,
    pub p50_latency: Duration,
    pub p95_latency: Duration,
    pub p99_latency: Duration,
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
    pub error_rate: f64,
    pub custom_metrics: HashMap<String, f64>,
}

/// Benchmark runner
pub struct BenchmarkRunner {
    test_suite: MockServiceTestSuite,
    performance_tracker: PerformanceTracker,
}

impl BenchmarkRunner {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let config = TestConfigBuilder::new()
            .with_load_balancing(true)
            .with_circuit_breaker(true)
            .build();

        let test_suite = MockServiceTestSuite::new(config).await?;
        let performance_tracker = PerformanceTracker::new();

        Ok(Self {
            test_suite,
            performance_tracker,
        })
    }

    pub async fn run_benchmark(&mut self, config: BenchmarkConfig) -> Result<BenchmarkResults, Box<dyn std::error::Error + Send + Sync>> {
        println!("Starting benchmark: {}", config.name);
        println!("Description: {}", config.description);
        println!("Event count: {}", config.event_count);
        println!("Concurrent tasks: {}", config.concurrent_tasks);

        // Warmup phase
        if config.warmup_events > 0 {
            println!("Running warmup with {} events...", config.warmup_events);
            self.run_warmup_phase(config.warmup_events).await?;
        }

        // Clear history after warmup
        self.test_suite.clear_all_history().await;
        self.performance_tracker.clear_measurements().await;

        // Main benchmark
        println!("Running main benchmark...");
        let start_time = Instant::now();

        let results = self.performance_tracker
            .measure(&format!("benchmark_{}", config.name), || async {
                self.run_main_benchmark(&config).await
            })
            .await?;

        let total_duration = start_time.elapsed();

        // Collect final metrics
        let events = self.test_suite.event_router.get_published_events().await;
        let successful_events = events.len();
        let failed_events = config.event_count.saturating_sub(successful_events);

        // Calculate latency statistics
        let latencies = self.extract_latencies(&events).await;
        let (average_latency, p50_latency, p95_latency, p99_latency) = self.calculate_latency_stats(&latencies);

        // Calculate throughput
        let throughput_events_per_second = successful_events as f64 / total_duration.as_secs_f64();

        // Get resource usage
        let (memory_usage_mb, cpu_usage_percent) = self.get_resource_usage().await;

        let benchmark_results = BenchmarkResults {
            config: config.clone(),
            total_events: config.event_count,
            successful_events,
            failed_events,
            total_duration,
            throughput_events_per_second,
            average_latency,
            p50_latency,
            p95_latency,
            p99_latency,
            memory_usage_mb,
            cpu_usage_percent,
            error_rate: failed_events as f64 / config.event_count as f64,
            custom_metrics: HashMap::new(),
        };

        // Print results
        self.print_benchmark_results(&benchmark_results);

        Ok(benchmark_results)
    }

    async fn run_warmup_phase(&self, warmup_events: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut handles = Vec::new();

        for i in 0..warmup_events {
            let event_router = self.test_suite.event_router.clone();
            let event = EventFactory::create_script_execution_event(
                &format!("warmup_script_{}", i),
                "print('warmup')",
            );

            let handle = tokio::spawn(async move {
                event_router.publish(Box::new(event)).await
            });
            handles.push(handle);

            // Small delay to prevent overwhelming
            if i % 10 == 0 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }

        // Wait for all warmup events to complete
        for handle in handles {
            handle.await??;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(500)).await;

        Ok(())
    }

    async fn run_main_benchmark(&self, config: &BenchmarkConfig) -> Result<BenchmarkResults, Box<dyn std::error::Error + Send + Sync>> {
        let events_per_task = config.event_count / config.concurrent_tasks;
        let mut handles = Vec::new();

        for task_id in 0..config.concurrent_tasks {
            let event_router = self.test_suite.event_router.clone();
            let target_services = config.target_services.clone();
            let event_types = config.event_types.clone();
            let events_for_this_task = if task_id == config.concurrent_tasks - 1 {
                config.event_count - (events_per_task * (config.concurrent_tasks - 1))
            } else {
                events_per_task
            };

            let handle = tokio::spawn(async move {
                let mut task_results = TaskResults {
                    sent_events: 0,
                    successful_events: 0,
                    failed_events: 0,
                    start_time: Instant::now(),
                };

                for i in 0..events_for_this_task {
                    let event_type = event_types.get(i % event_types.len()).unwrap();
                    let target_service = target_services.get(i % target_services.len()).unwrap();

                    let event = Self::create_benchmark_event(task_id, i, event_type, target_service);

                    match event_router.publish(Box::new(event)).await {
                        Ok(_) => {
                            task_results.successful_events += 1;
                        }
                        Err(_) => {
                            task_results.failed_events += 1;
                        }
                    }
                    task_results.sent_events += 1;

                    // Small delay to prevent overwhelming
                    if i % 50 == 0 {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                }

                task_results
            });

            handles.push(handle);
        }

        // Wait for all tasks to complete
        let mut total_sent = 0;
        let mut total_successful = 0;
        let mut total_failed = 0;

        for handle in handles {
            let task_results = handle.await?;
            total_sent += task_results.sent_events;
            total_successful += task_results.successful_events;
            total_failed += task_results.failed_events;
        }

        // Wait for event processing to complete
        tokio::time::sleep(Duration::from_secs(2)).await;

        Ok(BenchmarkResults {
            config: config.clone(),
            total_events: total_sent,
            successful_events: total_successful,
            failed_events: total_failed,
            total_duration: Duration::ZERO, // Will be set by caller
            throughput_events_per_second: 0.0, // Will be calculated by caller
            average_latency: Duration::ZERO, // Will be calculated by caller
            p50_latency: Duration::ZERO, // Will be calculated by caller
            p95_latency: Duration::ZERO, // Will be calculated by caller
            p99_latency: Duration::ZERO, // Will be calculated by caller
            memory_usage_mb: 0.0, // Will be calculated by caller
            cpu_usage_percent: 0.0, // Will be calculated by caller
            error_rate: total_failed as f64 / total_sent as f64,
            custom_metrics: HashMap::new(),
        })
    }

    fn create_benchmark_event(task_id: usize, event_index: usize, event_type: &str, target_service: &str) -> crate::events::core::DaemonEvent {
        use crate::events::core::{DaemonEvent, EventType, EventPriority, EventPayload, EventSource};

        let payload_content = match event_type {
            "script_execution" => json!({
                "script_id": format!("benchmark_script_{}_{}", task_id, event_index),
                "script_content": "print('benchmark test')",
                "language": "python"
            }),
            "document_creation" => {
                let doc = TestDataFactory::create_test_document(
                    &format!("doc_{}_{}", task_id, event_index),
                    "Benchmark Document",
                    &format!("Benchmark content for task {} event {}", task_id, event_index)
                );
                json!({
                    "database": "benchmark_db",
                    "document": doc,
                    "operation": "create"
                })
            },
            "inference_request" => json!({
                "model": "benchmark-model",
                "prompt": format!("Benchmark prompt {} {}", task_id, event_index),
                "request_type": "completion"
            }),
            "embedding_request" => json!({
                "model": "benchmark-embedding-model",
                "input": format!("Embedding text {} {}", task_id, event_index),
                "request_type": "embedding"
            }),
            _ => json!({
                "task_id": task_id,
                "event_index": event_index,
                "benchmark_data": "x".repeat(100) // 100 bytes of data
            })
        };

        DaemonEvent {
            id: Uuid::new_v4(),
            event_type: EventType::Custom(format!("benchmark_{}", event_type)),
            priority: EventPriority::Normal,
            source: EventSource::Service("benchmark_client".to_string()),
            targets: vec![target_service.to_string()],
            created_at: Utc::now(),
            scheduled_at: None,
            payload: EventPayload::json(payload_content),
            metadata: HashMap::from([
                ("benchmark".to_string(), "true".to_string()),
                ("task_id".to_string(), task_id.to_string()),
                ("event_index".to_string(), event_index.to_string()),
            ]),
            correlation_id: Some(format!("benchmark_task_{}", task_id)),
            causation_id: None,
            retry_count: 0,
            max_retries: 3,
        }
    }

    async fn extract_latencies(&self, events: &[crate::events::core::DaemonEvent]) -> Vec<Duration> {
        let mut latencies = Vec::new();

        for event in events {
            // In a real implementation, events would have processing time information
            // For this mock, we'll simulate latencies based on event complexity
            let simulated_latency = self.simulate_event_latency(event);
            latencies.push(simulated_latency);
        }

        latencies
    }

    fn simulate_event_latency(&self, event: &crate::events::core::DaemonEvent) -> Duration {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        // Base latency plus some randomness
        let base_latency_ms = 10;
        let random_variation_ms = rng.gen_range(0..50);
        let payload_size_factor = (event.payload.as_ref().map_or(0, |p| p.estimated_size()) / 1000) as u64;

        Duration::from_millis(base_latency_ms + random_variation_ms + payload_size_factor)
    }

    fn calculate_latency_stats(&self, latencies: &[Duration]) -> (Duration, Duration, Duration, Duration) {
        if latencies.is_empty() {
            return (Duration::ZERO, Duration::ZERO, Duration::ZERO, Duration::ZERO);
        }

        let mut sorted_latencies = latencies.to_vec();
        sorted_latencies.sort();

        let average = latencies.iter().sum::<Duration>() / latencies.len() as u32;
        let p50 = sorted_latencies[sorted_latencies.len() / 2];
        let p95 = sorted_latencies[(sorted_latencies.len() as f64 * 0.95) as usize];
        let p99 = sorted_latencies[(sorted_latencies.len() as f64 * 0.99) as usize];

        (average, p50, p95, p99)
    }

    async fn get_resource_usage(&self) -> (f64, f64) {
        // Mock resource usage - in a real implementation, this would use system monitoring
        let memory_usage_mb = 50.0 + (self.test_suite.event_router.get_published_events().await.len() as f64 / 1000.0);
        let cpu_usage_percent = 20.0 + (rand::random::<f64>() * 30.0);

        (memory_usage_mb, cpu_usage_percent)
    }

    fn print_benchmark_results(&self, results: &BenchmarkResults) {
        println!("\n=== BENCHMARK RESULTS ===");
        println!("Benchmark: {}", results.config.name);
        println!("Description: {}", results.config.description);
        println!("Duration: {:?}", results.total_duration);
        println!("Total Events: {}", results.total_events);
        println!("Successful Events: {}", results.successful_events);
        println!("Failed Events: {}", results.failed_events);
        println!("Error Rate: {:.2}%", results.error_rate * 100.0);
        println!("Throughput: {:.2} events/sec", results.throughput_events_per_second);
        println!("Average Latency: {:?}", results.average_latency);
        println!("P50 Latency: {:?}", results.p50_latency);
        println!("P95 Latency: {:?}", results.p95_latency);
        println!("P99 Latency: {:?}", results.p99_latency);
        println!("Memory Usage: {:.2} MB", results.memory_usage_mb);
        println!("CPU Usage: {:.2}%", results.cpu_usage_percent);
        println!("========================\n");
    }
}

#[derive(Debug)]
struct TaskResults {
    sent_events: usize,
    successful_events: usize,
    failed_events: usize,
    start_time: Instant,
}

/// Predefined benchmark configurations
pub struct BenchmarkConfigs;

impl BenchmarkConfigs {
    pub fn throughput_test() -> BenchmarkConfig {
        BenchmarkConfig {
            name: "throughput_test".to_string(),
            description: "High-throughput event processing test".to_string(),
            event_count: 10000,
            concurrent_tasks: 50,
            event_types: vec!["script_execution".to_string()],
            target_services: vec!["script-engine".to_string()],
            duration_seconds: 60,
            warmup_events: 500,
        }
    }

    pub fn latency_test() -> BenchmarkConfig {
        BenchmarkConfig {
            name: "latency_test".to_string(),
            description: "Low-latency event processing test".to_string(),
            event_count: 1000,
            concurrent_tasks: 1,
            event_types: vec!["script_execution".to_string()],
            target_services: vec!["script-engine".to_string()],
            duration_seconds: 30,
            warmup_events: 100,
        }
    }

    pub fn multi_service_test() -> BenchmarkConfig {
        BenchmarkConfig {
            name: "multi_service_test".to_string(),
            description: "Multi-service coordination test".to_string(),
            event_count: 5000,
            concurrent_tasks: 20,
            event_types: vec![
                "script_execution".to_string(),
                "document_creation".to_string(),
                "inference_request".to_string(),
                "embedding_request".to_string(),
            ],
            target_services: vec![
                "script-engine".to_string(),
                "datastore".to_string(),
                "inference-engine".to_string(),
                "mcp-gateway".to_string(),
            ],
            duration_seconds: 120,
            warmup_events: 200,
        }
    }

    pub fn load_balancing_test() -> BenchmarkConfig {
        BenchmarkConfig {
            name: "load_balancing_test".to_string(),
            description: "Load balancing performance test".to_string(),
            event_count: 8000,
            concurrent_tasks: 100,
            event_types: vec!["script_execution".to_string()],
            target_services: vec!["script-engine".to_string()],
            duration_seconds: 90,
            warmup_events: 400,
        }
    }

    pub fn circuit_breaker_test() -> BenchmarkConfig {
        BenchmarkConfig {
            name: "circuit_breaker_test".to_string(),
            description: "Circuit breaker performance under failure".to_string(),
            event_count: 3000,
            concurrent_tasks: 30,
            event_types: vec!["script_execution".to_string()],
            target_services: vec!["script-engine".to_string()],
            duration_seconds: 60,
            warmup_events: 150,
        }
    }

    pub fn stress_test() -> BenchmarkConfig {
        BenchmarkConfig {
            name: "stress_test".to_string(),
            description: "System stress test with maximum load".to_string(),
            event_count: 50000,
            concurrent_tasks: 200,
            event_types: vec![
                "script_execution".to_string(),
                "document_creation".to_string(),
                "inference_request".to_string(),
                "embedding_request".to_string(),
            ],
            target_services: vec![
                "script-engine".to_string(),
                "datastore".to_string(),
                "inference-engine".to_string(),
                "mcp-gateway".to_string(),
            ],
            duration_seconds: 300,
            warmup_events: 1000,
        }
    }
}

/// Benchmark suite runner
pub struct BenchmarkSuite {
    runner: BenchmarkRunner,
    results: Vec<BenchmarkResults>,
}

impl BenchmarkSuite {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let runner = BenchmarkRunner::new().await?;
        Ok(Self {
            runner,
            results: Vec::new(),
        })
    }

    pub async fn run_all_benchmarks(&mut self) -> Result<Vec<BenchmarkResults>, Box<dyn std::error::Error + Send + Sync>> {
        let benchmarks = vec![
            BenchmarkConfigs::throughput_test(),
            BenchmarkConfigs::latency_test(),
            BenchmarkConfigs::multi_service_test(),
            BenchmarkConfigs::load_balancing_test(),
            // Note: Skip stress test by default as it's very resource-intensive
        ];

        for config in benchmarks {
            let result = self.runner.run_benchmark(config).await?;
            self.results.push(result);
        }

        Ok(self.results.clone())
    }

    pub async fn run_specific_benchmark(&mut self, config: BenchmarkConfig) -> Result<BenchmarkResults, Box<dyn std::error::Error + Send + Sync>> {
        let result = self.runner.run_benchmark(config).await?;
        self.results.push(result.clone());
        Ok(result)
    }

    pub fn get_results(&self) -> &[BenchmarkResults] {
        &self.results
    }

    pub fn generate_report(&self) -> String {
        let mut report = String::new();
        report.push_str("# PERFORMANCE BENCHMARK REPORT\n\n");

        for result in &self.results {
            report.push_str(&format!("## {}\n", result.config.name));
            report.push_str(&format!("**Description**: {}\n\n", result.config.description));
            report.push_str("### Metrics\n\n");
            report.push_str(&format!("- **Total Events**: {}\n", result.total_events));
            report.push_str(&format!("- **Successful Events**: {}\n", result.successful_events));
            report.push_str(&format!("- **Failed Events**: {}\n", result.failed_events));
            report.push_str(&format!("- **Error Rate**: {:.2}%\n", result.error_rate * 100.0));
            report.push_str(&format!("- **Duration**: {:?}\n", result.total_duration));
            report.push_str(&format!("- **Throughput**: {:.2} events/sec\n", result.throughput_events_per_second));
            report.push_str(&format!("- **Average Latency**: {:?}\n", result.average_latency));
            report.push_str(&format!("- **P50 Latency**: {:?}\n", result.p50_latency));
            report.push_str(&format!("- **P95 Latency**: {:?}\n", result.p95_latency));
            report.push_str(&format!("- **P99 Latency**: {:?}\n", result.p99_latency));
            report.push_str(&format!("- **Memory Usage**: {:.2} MB\n", result.memory_usage_mb));
            report.push_str(&format!("- **CPU Usage**: {:.2}%\n\n", result.cpu_usage_percent));
        }

        report.push_str("## Summary\n\n");
        report.push_str("This report summarizes the performance characteristics of the event-driven service architecture under various load conditions.\n");

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_benchmark() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut runner = BenchmarkRunner::new().await?;

        let config = BenchmarkConfig {
            name: "test_benchmark".to_string(),
            description: "Test benchmark for validation".to_string(),
            event_count: 100,
            concurrent_tasks: 5,
            event_types: vec!["script_execution".to_string()],
            target_services: vec!["script-engine".to_string()],
            duration_seconds: 30,
            warmup_events: 10,
        };

        let result = runner.run_benchmark(config).await?;

        assert!(result.successful_events > 0);
        assert!(result.throughput_events_per_second > 0.0);
        assert!(result.error_rate < 1.0);

        Ok(())
    }

    #[tokio::test]
    async fn test_benchmark_suite() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut suite = BenchmarkSuite::new().await?;

        let config = BenchmarkConfig {
            name: "suite_test".to_string(),
            description: "Test benchmark suite".to_string(),
            event_count: 50,
            concurrent_tasks: 2,
            event_types: vec!["script_execution".to_string()],
            target_services: vec!["script-engine".to_string()],
            duration_seconds: 15,
            warmup_events: 5,
        };

        let result = suite.run_specific_benchmark(config).await?;
        let results = suite.get_results();

        assert!(!results.is_empty());
        assert_eq!(results[0].config.name, "suite_test");

        let report = suite.generate_report();
        assert!(report.contains("PERFORMANCE BENCHMARK REPORT"));
        assert!(report.contains("suite_test"));

        Ok(())
    }
}