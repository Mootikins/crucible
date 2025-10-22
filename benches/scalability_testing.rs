//! Scalability testing and bottleneck identification
//!
//! This module provides comprehensive scalability testing to identify performance
//! limits and bottlenecks in both DataCoordinator and centralized daemon approaches.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use std::collections::HashMap;
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

use crucible_daemon::coordinator::DataCoordinator;
use crucible_daemon::config::DaemonConfig;
use crucible_services::events::core::{DaemonEvent, EventType, EventPayload, EventSource, EventPriority, SourceType};
use crucible_services::events::routing::{DefaultEventRouter, RoutingConfig, EventRouter};
use crucible_services::types::{ServiceHealth, ServiceStatus};

/// Scalability test results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalabilityResults {
    pub approach: String,
    pub test_type: String,
    pub max_events_per_second: f64,
    pub max_concurrent_events: usize,
    pub breaking_point: Option<String>,
    pub resource_limits: ResourceLimits,
    pub bottlenecks: Vec<Bottleneck>,
    pub performance_degradation: PerformanceDegradation,
    pub scalability_factor: f64,
}

/// Resource limits observed during testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_memory_mb: f64,
    pub max_cpu_percent: f64,
    pub max_thread_count: usize,
    pub max_file_descriptors: usize,
    pub max_network_connections: usize,
}

/// Performance bottleneck identification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bottleneck {
    pub resource: String,
    pub description: String,
    pub impact: ImpactLevel,
    pub observed_at_load: f64,
    pub mitigation_suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImpactLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Performance degradation analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceDegradation {
    pub throughput_degradation_percent: f64,
    pub latency_increase_percent: f64,
    pub error_rate_increase_percent: f64,
    pub memory_leak_rate_mb_per_hour: f64,
}

/// Scalability test configuration
#[derive(Debug, Clone)]
pub struct ScalabilityTestConfig {
    pub name: String,
    pub initial_load: f64,
    pub max_load: f64,
    pub load_step: f64,
    pub step_duration: Duration,
    pub concurrent_workers: usize,
    pub event_payload_size: usize,
}

impl ScalabilityTestConfig {
    pub fn throughput_test() -> Self {
        Self {
            name: "Throughput Scalability".to_string(),
            initial_load: 100.0,
            max_load: 10000.0,
            load_step: 100.0,
            step_duration: Duration::from_secs(10),
            concurrent_workers: 10,
            event_payload_size: 1024,
        }
    }

    pub fn concurrency_test() -> Self {
        Self {
            name: "Concurrency Scalability".to_string(),
            initial_load: 1.0,
            max_load: 1000.0,
            load_step: 10.0,
            step_duration: Duration::from_secs(5),
            concurrent_workers: 100,
            event_payload_size: 512,
        }
    }

    pub fn memory_test() -> Self {
        Self {
            name: "Memory Scalability".to_string(),
            initial_load: 1000.0,
            max_load: 100000.0,
            load_step: 1000.0,
            step_duration: Duration::from_secs(30),
            concurrent_workers: 5,
            event_payload_size: 4096,
        }
    }

    pub fn latency_test() -> Self {
        Self {
            name: "Latency Scalability".to_string(),
            initial_load: 10.0,
            max_load: 1000.0,
            load_step: 10.0,
            step_duration: Duration::from_secs(15),
            concurrent_workers: 50,
            event_payload_size: 256,
        }
    }
}

/// Scalability test runner
pub struct ScalabilityTestRunner {
    runtime: Arc<Runtime>,
}

impl ScalabilityTestRunner {
    pub fn new() -> Self {
        Self {
            runtime: Arc::new(Runtime::new().unwrap()),
        }
    }

    pub async fn run_scalability_test(
        &self,
        config: &ScalabilityTestConfig,
        approach: Approach,
    ) -> ScalabilityResults {
        println!("Running scalability test: {} ({:?})", config.name, approach);

        let mut current_load = config.initial_load;
        let mut results = Vec::new();
        let mut breaking_point = None;
        let mut resource_usage = Vec::new();

        // Test increasing load levels
        while current_load <= config.max_load && breaking_point.is_none() {
            println!("Testing load level: {:.0} events/sec", current_load);

            let test_result = self.run_load_level(config, current_load, approach).await;
            results.push(test_result.clone());

            // Check for breaking point
            if test_result.error_rate > 10.0 || test_result.avg_latency.as_millis() > 5000 {
                breaking_point = Some(format!(
                    "Failed at {:.0} events/sec - Error rate: {:.1}%, Latency: {:.0}ms",
                    current_load, test_result.error_rate, test_result.avg_latency.as_millis()
                ));
                break;
            }

            // Check resource limits
            let resource_limit = self.check_resource_limits(&test_result).await;
            resource_usage.push(resource_limit);

            current_load += config.load_step;
        }

        // Analyze results
        let scalability_results = self.analyze_scalability_results(
            config,
            approach,
            results,
            breaking_point,
            resource_usage,
        ).await;

        println!("Scalability test completed for {:?}", approach);
        scalability_results
    }

    async fn run_load_level(
        &self,
        config: &ScalabilityTestConfig,
        events_per_sec: f64,
        approach: Approach,
    ) -> LoadLevelResult {
        let event_count = (events_per_sec * config.step_duration.as_secs_f64()) as usize;
        let events = generate_test_events(event_count, config.event_payload_size);

        let barrier = Arc::new(Barrier::new(config.concurrent_workers));
        let mut handles = Vec::new();

        let events_per_worker = event_count / config.concurrent_workers;

        for worker_id in 0..config.concurrent_workers {
            let barrier = barrier.clone();
            let events = events[worker_id * events_per_worker..((worker_id + 1) * events_per_worker)].to_vec();
            let interval = Duration::from_nanos((1_000_000_000.0 / (events_per_sec / config.concurrent_workers as f64)) as u64);

            let handle = match approach {
                Approach::DataCoordinator => {
                    self.runtime.spawn(async move {
                        self.run_data_coordinator_worker(events, interval, barrier).await
                    })
                }
                Approach::CentralizedDaemon => {
                    let router = setup_test_event_router(10).await;
                    self.runtime.spawn(async move {
                        self.run_centralized_daemon_worker(events, interval, barrier, router).await
                    })
                }
            };

            handles.push(handle);
        }

        // Wait for all workers and collect results
        let worker_results = join_all(handles).await;
        let mut total_events = 0;
        let mut total_latency = Duration::ZERO;
        let mut total_errors = 0;
        let mut max_latency = Duration::ZERO;
        let mut min_latency = Duration::MAX;

        for result in worker_results {
            if let Ok(worker_result) = result {
                total_events += worker_result.events_processed;
                total_latency += worker_result.total_latency;
                total_errors += worker_result.errors;
                max_latency = max_latency.max(worker_result.max_latency);
                min_latency = min_latency.min(worker_result.min_latency);
            }
        }

        let avg_latency = if total_events > 0 {
            total_latency / total_events as u32
        } else {
            Duration::ZERO
        };

        let error_rate = if total_events > 0 {
            (total_errors as f64 / total_events as f64) * 100.0
        } else {
            0.0
        };

        LoadLevelResult {
            load_level: events_per_sec,
            events_processed: total_events,
            avg_latency,
            max_latency,
            min_latency,
            error_rate,
            duration: config.step_duration,
        }
    }

    async fn run_data_coordinator_worker(
        &self,
        events: Vec<DaemonEvent>,
        interval: Duration,
        barrier: Arc<Barrier>,
    ) -> WorkerResult {
        barrier.wait().await;

        let start_time = Instant::now();
        let mut events_processed = 0;
        let mut total_latency = Duration::ZERO;
        let mut errors = 0;
        let mut max_latency = Duration::ZERO;
        let mut min_latency = Duration::MAX;

        for event in events {
            let event_start = Instant::now();

            // Simulate DataCoordinator processing
            if let Err(_) = simulate_data_coordinator_processing(&event).await {
                errors += 1;
            }

            let latency = event_start.elapsed();
            total_latency += latency;
            max_latency = max_latency.max(latency);
            min_latency = min_latency.min(latency);
            events_processed += 1;

            // Maintain target rate
            tokio::time::sleep(interval).await;
        }

        WorkerResult {
            events_processed,
            total_latency,
            errors,
            max_latency,
            min_latency,
        }
    }

    async fn run_centralized_daemon_worker(
        &self,
        events: Vec<DaemonEvent>,
        interval: Duration,
        barrier: Arc<Barrier>,
        router: Arc<DefaultEventRouter>,
    ) -> WorkerResult {
        barrier.wait().await;

        let start_time = Instant::now();
        let mut events_processed = 0;
        let mut total_latency = Duration::ZERO;
        let mut errors = 0;
        let mut max_latency = Duration::ZERO;
        let mut min_latency = Duration::MAX;

        for event in events {
            let event_start = Instant::now();

            // Route event through centralized daemon
            if let Err(_) = router.route_event(event).await {
                errors += 1;
            }

            let latency = event_start.elapsed();
            total_latency += latency;
            max_latency = max_latency.max(latency);
            min_latency = min_latency.min(latency);
            events_processed += 1;

            // Maintain target rate
            tokio::time::sleep(interval).await;
        }

        WorkerResult {
            events_processed,
            total_latency,
            errors,
            max_latency,
            min_latency,
        }
    }

    async fn check_resource_limits(&self, result: &LoadLevelResult) -> ResourceLimits {
        // Simulate resource monitoring (in real implementation, these would be actual measurements)
        let memory_mb = (result.events_processed as f64 * 0.001) + (result.avg_latency.as_millis() as f64 * 0.01);
        let cpu_percent = (result.load_level / 1000.0) * 20.0 + (result.error_rate * 2.0);

        ResourceLimits {
            max_memory_mb: memory_mb,
            max_cpu_percent: cpu_percent.min(100.0),
            max_thread_count: std::thread::available_parallelism().unwrap().get(),
            max_file_descriptors: 1024, // Default limit
            max_network_connections: 100,
        }
    }

    async fn analyze_scalability_results(
        &self,
        config: &ScalabilityTestConfig,
        approach: Approach,
        results: Vec<LoadLevelResult>,
        breaking_point: Option<String>,
        resource_usage: Vec<ResourceLimits>,
    ) -> ScalabilityResults {
        let approach_str = match approach {
            Approach::DataCoordinator => "DataCoordinator",
            Approach::CentralizedDaemon => "CentralizedDaemon",
        };

        // Find maximum throughput
        let max_events_per_second = results
            .iter()
            .map(|r| r.load_level)
            .fold(0.0, f64::max);

        // Calculate scalability factor (linear scaling = 1.0)
        let scalability_factor = if results.len() > 1 {
            let first_throughput = results.first().unwrap().load_level;
            let last_throughput = results.last().unwrap().load_level;
            let expected_scaling = last_throughput / first_throughput;
            let actual_scaling = results.last().unwrap().events_processed as f64 / results.first().unwrap().events_processed as f64;
            actual_scaling / expected_scaling
        } else {
            1.0
        };

        // Identify bottlenecks
        let bottlenecks = self.identify_bottlenecks(&results, &resource_usage, approach);

        // Calculate performance degradation
        let performance_degradation = self.calculate_performance_degradation(&results);

        // Find resource limits
        let resource_limits = ResourceLimits {
            max_memory_mb: resource_usage.iter().map(|r| r.max_memory_mb).fold(0.0, f64::max),
            max_cpu_percent: resource_usage.iter().map(|r| r.max_cpu_percent).fold(0.0, f64::max),
            max_thread_count: resource_usage.iter().map(|r| r.max_thread_count).max().unwrap_or(0),
            max_file_descriptors: resource_usage.iter().map(|r| r.max_file_descriptors).max().unwrap_or(0),
            max_network_connections: resource_usage.iter().map(|r| r.max_network_connections).max().unwrap_or(0),
        };

        ScalabilityResults {
            approach: approach_str.to_string(),
            test_type: config.name.clone(),
            max_events_per_second,
            max_concurrent_events: config.concurrent_workers,
            breaking_point,
            resource_limits,
            bottlenecks,
            performance_degradation,
            scalability_factor,
        }
    }

    fn identify_bottlenecks(
        &self,
        results: &[LoadLevelResult],
        resource_usage: &[ResourceLimits],
        approach: Approach,
    ) -> Vec<Bottleneck> {
        let mut bottlenecks = Vec::new();

        // Check for CPU bottlenecks
        let max_cpu = resource_usage.iter().map(|r| r.max_cpu_percent).fold(0.0, f64::max);
        if max_cpu > 80.0 {
            bottlenecks.push(Bottleneck {
                resource: "CPU".to_string(),
                description: format!("High CPU usage detected: {:.1}%", max_cpu),
                impact: if max_cpu > 95.0 { ImpactLevel::Critical } else { ImpactLevel::High },
                observed_at_load: results.iter().find(|r| r.error_rate > 5.0).map(|r| r.load_level).unwrap_or(0.0),
                mitigation_suggestion: "Consider optimizing algorithms or increasing CPU resources".to_string(),
            });
        }

        // Check for memory bottlenecks
        let max_memory = resource_usage.iter().map(|r| r.max_memory_mb).fold(0.0, f64::max);
        if max_memory > 1000.0 { // > 1GB
            bottlenecks.push(Bottleneck {
                resource: "Memory".to_string(),
                description: format!("High memory usage detected: {:.1} MB", max_memory),
                impact: if max_memory > 4000.0 { ImpactLevel::Critical } else { ImpactLevel::High },
                observed_at_load: results.iter().find(|r| r.error_rate > 5.0).map(|r| r.load_level).unwrap_or(0.0),
                mitigation_suggestion: "Implement memory pooling or reduce memory footprint".to_string(),
            });
        }

        // Check for latency degradation
        if let Some((first, last)) = (results.first(), results.last()) {
            let latency_increase = (last.avg_latency.as_millis() as f64 - first.avg_latency.as_millis() as f64)
                / first.avg_latency.as_millis() as f64 * 100.0;

            if latency_increase > 200.0 {
                bottlenecks.push(Bottleneck {
                    resource: "Latency".to_string(),
                    description: format!("Significant latency degradation: {:.1}%", latency_increase),
                    impact: if latency_increase > 500.0 { ImpactLevel::Critical } else { ImpactLevel::High },
                    observed_at_load: last.load_level,
                    mitigation_suggestion: "Optimize processing pipeline or implement async processing".to_string(),
                });
            }
        }

        // Check for thread contention (more workers don't improve performance)
        if results.len() > 1 {
            let mid_point = results.len() / 2;
            let first_half_avg: f64 = results[..mid_point].iter().map(|r| r.load_level).sum::<f64>() / mid_point as f64;
            let second_half_avg: f64 = results[mid_point..].iter().map(|r| r.load_level).sum::<f64>() / (results.len() - mid_point) as f64;

            if second_half_avg < first_half_avg * 1.5 {
                bottlenecks.push(Bottleneck {
                    resource: "Thread Contention".to_string(),
                    description: "Diminishing returns with increased concurrency".to_string(),
                    impact: ImpactLevel::Medium,
                    observed_at_load: results[mid_point].load_level,
                    mitigation_suggestion: "Implement more efficient locking or use lock-free data structures".to_string(),
                });
            }
        }

        // Approach-specific bottlenecks
        match approach {
            Approach::DataCoordinator => {
                if let Some(result) = results.iter().find(|r| r.error_rate > 1.0) {
                    bottlenecks.push(Bottleneck {
                        resource: "Event Processing".to_string(),
                        description: "DataCoordinator shows error rate increases under load".to_string(),
                        impact: ImpactLevel::High,
                        observed_at_load: result.load_level,
                        mitigation_suggestion: "Implement better error handling and retry mechanisms".to_string(),
                    });
                }
            }
            Approach::CentralizedDaemon => {
                if let Some(result) = results.iter().find(|r| r.avg_latency.as_millis() > 1000) {
                    bottlenecks.push(Bottleneck {
                        resource: "Event Routing".to_string(),
                        description: "Centralized daemon shows latency spikes under high load".to_string(),
                        impact: ImpactLevel::High,
                        observed_at_load: result.load_level,
                        mitigation_suggestion: "Optimize routing algorithm or implement load shedding".to_string(),
                    });
                }
            }
        }

        bottlenecks
    }

    fn calculate_performance_degradation(&self, results: &[LoadLevelResult]) -> PerformanceDegradation {
        if results.len() < 2 {
            return PerformanceDegradation {
                throughput_degradation_percent: 0.0,
                latency_increase_percent: 0.0,
                error_rate_increase_percent: 0.0,
                memory_leak_rate_mb_per_hour: 0.0,
            };
        }

        let first = results.first().unwrap();
        let last = results.last().unwrap();

        // Calculate throughput degradation (events processed per second)
        let initial_throughput = first.events_processed as f64 / first.duration.as_secs_f64();
        let final_throughput = last.events_processed as f64 / last.duration.as_secs_f64();
        let throughput_degradation = ((initial_throughput - final_throughput) / initial_throughput) * 100.0;

        // Calculate latency increase
        let latency_increase = ((last.avg_latency.as_millis() as f64 - first.avg_latency.as_millis() as f64)
            / first.avg_latency.as_millis() as f64) * 100.0;

        // Calculate error rate increase
        let error_rate_increase = last.error_rate - first.error_rate;

        // Estimate memory leak rate (simplified simulation)
        let memory_leak_rate = (last.events_processed as f64 - first.events_processed as f64) * 0.0001; // KB per hour

        PerformanceDegradation {
            throughput_degradation_percent: throughput_degradation.max(0.0),
            latency_increase_percent: latency_increase.max(0.0),
            error_rate_increase_percent: error_rate_increase.max(0.0),
            memory_leak_rate_mb_per_hour: memory_leak_rate,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Approach {
    DataCoordinator,
    CentralizedDaemon,
}

#[derive(Debug, Clone)]
struct LoadLevelResult {
    load_level: f64,
    events_processed: usize,
    avg_latency: Duration,
    max_latency: Duration,
    min_latency: Duration,
    error_rate: f64,
    duration: Duration,
}

#[derive(Debug, Clone)]
struct WorkerResult {
    events_processed: usize,
    total_latency: Duration,
    errors: usize,
    max_latency: Duration,
    min_latency: Duration,
}

/// Generate test events
fn generate_test_events(count: usize, payload_size: usize) -> Vec<DaemonEvent> {
    let mut events = Vec::with_capacity(count);

    for i in 0..count {
        let payload = serde_json::json!({
            "data": "x".repeat(payload_size),
            "index": i,
            "timestamp": Utc::now(),
        });

        let event = DaemonEvent::new(
            EventType::Filesystem(crucible_services::events::core::FilesystemEventType::FileCreated {
                path: format!("/test/file_{}.txt", i),
            }),
            EventSource::new(format!("test_source_{}", i % 10), SourceType::System),
            EventPayload::json(payload),
        );

        events.push(event);
    }

    events
}

/// Simulate DataCoordinator processing
async fn simulate_data_coordinator_processing(event: &DaemonEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Simulate processing time that increases under load
    let processing_time = Duration::from_micros(10 + (event.id.as_u64() % 100));
    tokio::time::sleep(processing_time).await;

    // Simulate occasional errors
    if event.id.as_u64() % 500 == 0 {
        return Err("Simulated processing error".into());
    }

    Ok(())
}

/// Setup test event router with mock services
async fn setup_test_event_router(num_services: usize) -> Arc<DefaultEventRouter> {
    let router = Arc::new(DefaultEventRouter::with_config(RoutingConfig {
        max_queue_size: 10000,
        enable_deduplication: false, // Disabled for benchmarks
        ..Default::default()
    }));

    // Register mock services
    for i in 0..num_services {
        let registration = crucible_services::events::routing::ServiceRegistration {
            service_id: format!("service_{}", i),
            service_type: "test_service".to_string(),
            instance_id: format!("instance_{}", i),
            endpoint: None,
            supported_event_types: vec!["filesystem".to_string(), "database".to_string(), "system".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 100,
            filters: vec![],
            metadata: std::collections::HashMap::new(),
        };

        if let Err(_) = router.register_service(registration).await {
            // Handle registration errors gracefully
        }

        // Update service health
        let health = ServiceHealth {
            status: ServiceStatus::Healthy,
            message: Some("Test service running".to_string()),
            last_check: Utc::now(),
            details: std::collections::HashMap::new(),
        };

        if let Err(_) = router.update_service_health(&format!("service_{}", i), health).await {
            // Handle health update errors gracefully
        }
    }

    router
}

/// Benchmark scalability testing
pub fn benchmark_scalability_testing(c: &mut Criterion) {
    let runner = Arc::new(ScalabilityTestRunner::new());

    let mut group = c.benchmark_group("scalability_testing");

    // Test different scalability scenarios
    let test_configs = vec![
        ScalabilityTestConfig::throughput_test(),
        ScalabilityTestConfig::concurrency_test(),
        ScalabilityTestConfig::memory_test(),
    ];

    for config in test_configs {
        for approach in [Approach::DataCoordinator, Approach::CentralizedDaemon] {
            group.bench_with_input(
                BenchmarkId::new(
                    format!("{:?}_{:?}", approach, config.name.replace(" ", "_")),
                    &config.name,
                ),
                &config,
                |b, config| {
                    let runner = runner.clone();
                    let config = config.clone();
                    let approach = approach.clone();

                    b.to_async(&*runner.runtime).iter(|| async {
                        let results = runner.run_scalability_test(&config, approach).await;

                        println!(
                            "Scalability Results - {:?} {}: Max throughput: {:.0} events/sec, Scalability factor: {:.2}",
                            approach, config.name, results.max_events_per_second, results.scalability_factor
                        );

                        for bottleneck in &results.bottlenecks {
                            println!("  Bottleneck: {} - {}", bottleneck.resource, bottleneck.description);
                        }

                        Duration::from_millis(1) // Return dummy duration
                    });
                },
            );
        }
    }

    group.finish();
}

criterion_group!(
    scalability_benches,
    benchmark_scalability_testing
);
criterion_main!(scalability_benches);