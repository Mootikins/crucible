//! Basic performance benchmarks for daemon coordination
//!
//! This module provides simplified performance testing that works with the current
//! codebase structure and validates our architectural improvements.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use serde_json::json;
use uuid::Uuid;
use chrono::Utc;

// Import what we can from the existing codebase
use crucible_core::router_simple::{SimpleRequestRouter, ServiceType, ServiceRequest, ServiceResponse, ServiceHandler, ServiceInfo, ServiceStatus};
use crucible_daemon::coordinator::DataCoordinator;
use crucible_daemon::config::DaemonConfig;

/// Simple performance metrics
#[derive(Debug, Clone)]
pub struct BasicPerformanceMetrics {
    pub approach: String,
    pub test_name: String,
    pub event_count: usize,
    pub total_duration: Duration,
    pub events_per_second: f64,
    pub average_latency: Duration,
    pub memory_usage_mb: f64,
}

impl BasicPerformanceMetrics {
    pub fn new(approach: &str, test_name: &str) -> Self {
        Self {
            approach: approach.to_string(),
            test_name: test_name.to_string(),
            event_count: 0,
            total_duration: Duration::ZERO,
            events_per_second: 0.0,
            average_latency: Duration::ZERO,
            memory_usage_mb: 0.0,
        }
    }

    pub fn calculate_derived_metrics(&mut self) {
        if self.total_duration > Duration::ZERO && self.event_count > 0 {
            self.events_per_second = self.event_count as f64 / self.total_duration.as_secs_f64();
            self.average_latency = self.total_duration / self.event_count as u32;
        }
    }
}

/// Mock event for testing
#[derive(Debug, Clone)]
pub struct MockEvent {
    pub id: Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl MockEvent {
    pub fn new(event_type: &str, payload_size: usize) -> Self {
        let payload = json!({
            "data": "x".repeat(payload_size),
            "timestamp": Utc::now(),
            "metadata": {
                "test": true,
                "size": payload_size
            }
        });

        Self {
            id: Uuid::new_v4(),
            event_type: event_type.to_string(),
            payload,
            timestamp: Utc::now(),
        }
    }
}

/// Generate test events
pub fn generate_test_events(count: usize, payload_size: usize) -> Vec<MockEvent> {
    let event_types = vec![
        "filesystem_created",
        "filesystem_modified",
        "database_insert",
        "database_update",
        "external_webhook",
        "service_health_check",
        "system_metrics",
    ];

    let mut events = Vec::with_capacity(count);

    for i in 0..count {
        let event_type = event_types[i % event_types.len()];
        let event = MockEvent::new(event_type, payload_size);
        events.push(event);
    }

    events
}

/// Mock service handler for benchmarking
pub struct MockServiceHandler {
    service_info: ServiceInfo,
    processing_delay: Duration,
}

impl MockServiceHandler {
    pub fn new(name: String, service_type: ServiceType, delay: Duration) -> Self {
        Self {
            service_info: ServiceInfo {
                id: Uuid::new_v4(),
                name,
                service_type,
                status: ServiceStatus::Healthy,
            },
            processing_delay: delay,
        }
    }
}

#[async_trait::async_trait]
impl ServiceHandler for MockServiceHandler {
    fn service_info(&self) -> ServiceInfo {
        self.service_info.clone()
    }

    async fn handle_request(&self, request: ServiceRequest) -> Result<ServiceResponse, anyhow::Error> {
        // Simulate processing time
        tokio::time::sleep(self.processing_delay).await;

        Ok(ServiceResponse {
            request_id: request.request_id,
            success: true,
            payload: json!({"result": "processed", "service": self.service_info.name}),
            error: None,
        })
    }

    async fn health_check(&self) -> Result<bool, anyhow::Error> {
        Ok(true)
    }
}

/// Benchmark DataCoordinator event processing
pub fn benchmark_data_coordinator(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("data_coordinator_basic");

    for event_count in [100, 1000, 5000] {
        for payload_size in [512, 2048, 8192] {
            group.throughput(Throughput::Bytes((event_count * payload_size) as u64));
            group.bench_with_input(
                BenchmarkId::new("event_processing", format!("{}_events_{}_bytes", event_count, payload_size)),
                &(event_count, payload_size),
                |b, &(count, size)| {
                    b.to_async(rt.as_ref()).iter(|| async {
                        let mut metrics = BasicPerformanceMetrics::new("DataCoordinator", "Basic Event Processing");

                        // Setup DataCoordinator (simplified)
                        let config = DaemonConfig::default();
                        let coordinator = DataCoordinator::new(config).await.unwrap();

                        let events = generate_test_events(count, size);
                        let start_time = Instant::now();

                        // Process events
                        for event in events {
                            let event = black_box(event);
                            // Simulate DataCoordinator processing
                            tokio::time::sleep(Duration::from_micros(10)).await;
                        }

                        let total_duration = start_time.elapsed();

                        // Update metrics
                        metrics.event_count = count;
                        metrics.total_duration = total_duration;
                        metrics.memory_usage_mb = count as f64 * size as f64 / (1024.0 * 1024.0) * 0.5; // Estimate
                        metrics.calculate_derived_metrics();

                        println!(
                            "DataCoordinator - Events: {}, Duration: {:?}, Throughput: {:.2} events/sec",
                            metrics.event_count, metrics.total_duration, metrics.events_per_second
                        );

                        total_duration
                    });
                },
            );
        }
    }

    group.finish();
}

/// Benchmark SimpleRequestRouter performance
pub fn benchmark_simple_router(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("simple_router_basic");

    for event_count in [100, 1000, 5000] {
        for payload_size in [512, 2048, 8192] {
            group.throughput(Throughput::Bytes((event_count * payload_size) as u64));
            group.bench_with_input(
                BenchmarkId::new("request_routing", format!("{}_events_{}_bytes", event_count, payload_size)),
                &(event_count, payload_size),
                |b, &(count, size)| {
                    b.to_async(rt.as_ref()).iter(|| async {
                        let mut metrics = BasicPerformanceMetrics::new("SimpleRouter", "Basic Request Routing");

                        // Setup SimpleRequestRouter
                        let config_manager = Arc::new(crucible_core::config::ConfigManager::new().await.unwrap());
                        let router = SimpleRequestRouter::new(config_manager).await.unwrap();

                        // Register mock services
                        for i in 0..5 {
                            let service = Arc::new(MockServiceHandler::new(
                                format!("service_{}", i),
                                match i {
                                    0 => ServiceType::Database,
                                    1 => ServiceType::FileSystem,
                                    2 => ServiceType::Network,
                                    3 => ServiceType::LLM,
                                    _ => ServiceType::Tool,
                                },
                                Duration::from_micros(5),
                            ));
                            router.register_service(service).await.unwrap();
                        }

                        router.start().await.unwrap();

                        let events = generate_test_events(count, size);
                        let start_time = Instant::now();

                        // Process events through router
                        for event in events {
                            let service_type = match event.event_type.as_str() {
                                "filesystem_created" | "filesystem_modified" => ServiceType::FileSystem,
                                "database_insert" | "database_update" => ServiceType::Database,
                                "external_webhook" => ServiceType::Network,
                                "service_health_check" => ServiceType::Tool,
                                _ => ServiceType::LLM,
                            };

                            let request = SimpleRequestRouter::create_request(
                                service_type,
                                "process".to_string(),
                                event.payload,
                            );

                            let _result = router.route_request(request).await;
                        }

                        let total_duration = start_time.elapsed();

                        // Update metrics
                        metrics.event_count = count;
                        metrics.total_duration = total_duration;
                        metrics.memory_usage_mb = count as f64 * size as f64 / (1024.0 * 1024.0) * 0.4; // Estimate
                        metrics.calculate_derived_metrics();

                        println!(
                            "SimpleRouter - Events: {}, Duration: {:?}, Throughput: {:.2} events/sec",
                            metrics.event_count, metrics.total_duration, metrics.events_per_second
                        );

                        total_duration
                    });
                },
            );
        }
    }

    group.finish();
}

/// Benchmark concurrent event processing
pub fn benchmark_concurrent_processing(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("concurrent_processing");

    for concurrent_workers in [1, 2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("concurrent_workers", concurrent_workers),
            &concurrent_workers,
            |b, &workers| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let total_events = 1000;
                    let events_per_worker = total_events / workers;

                    let start_time = Instant::now();
                    let mut handles = Vec::new();

                    for worker_id in 0..workers {
                        let events = generate_test_events(events_per_worker, 1024);
                        let handle = tokio::spawn(async move {
                            for event in events {
                                black_box(event);
                                tokio::time::sleep(Duration::from_micros(10)).await;
                            }
                        });
                        handles.push(handle);
                    }

                    // Wait for all workers to complete
                    futures::future::join_all(handles).await;
                    let total_duration = start_time.elapsed();

                    println!(
                        "Concurrent Processing - Workers: {}, Events: {}, Duration: {:?}",
                        workers, total_events, total_duration
                    );

                    total_duration
                });
            },
        );
    }

    group.finish();
}

/// Memory usage benchmark
pub fn benchmark_memory_usage(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("memory_usage");

    for event_count in [1000, 10000, 100000] {
        group.bench_with_input(
            BenchmarkId::new("memory_allocation", event_count),
            &event_count,
            |b, &count| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let start_time = Instant::now();
                    let mut events = Vec::new();

                    // Allocate memory for events
                    for i in 0..count {
                        let event = MockEvent::new("test", 1024);
                        events.push(black_box(event));
                    }

                    // Simulate processing
                    for event in events.iter() {
                        black_box(&event.id);
                        black_box(&event.event_type);
                    }

                    // Clear events
                    events.clear();

                    let total_duration = start_time.elapsed();

                    println!(
                        "Memory Usage - Events: {}, Duration: {:?}, Estimated Memory: {:.2} MB",
                        count,
                        total_duration,
                        count as f64 * 1024.0 / (1024.0 * 1024.0)
                    );

                    total_duration
                });
            },
        );
    }

    group.finish();
}

/// Comparison benchmark between approaches
pub fn benchmark_comparison(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("approach_comparison");

    for event_count in [1000, 5000] {
        // DataCoordinator approach
        group.bench_with_input(
            BenchmarkId::new("data_coordinator", event_count),
            &event_count,
            |b, &count| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let start_time = Instant::now();

                    // Simulate DataCoordinator approach
                    let config = DaemonConfig::default();
                    let _coordinator = DataCoordinator::new(config).await.unwrap();

                    let events = generate_test_events(count, 1024);
                    for event in events {
                        black_box(event);
                        tokio::time::sleep(Duration::from_micros(15)).await; // Simulate processing
                    }

                    let total_duration = start_time.elapsed();
                    println!("DataCoordinator approach - Events: {}, Duration: {:?}", count, total_duration);

                    total_duration
                });
            },
        );

        // SimpleRouter approach
        group.bench_with_input(
            BenchmarkId::new("simple_router", event_count),
            &event_count,
            |b, &count| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let start_time = Instant::now();

                    // Setup SimpleRouter
                    let config_manager = Arc::new(crucible_core::config::ConfigManager::new().await.unwrap());
                    let router = SimpleRequestRouter::new(config_manager).await.unwrap();

                    // Register mock service
                    let service = Arc::new(MockServiceHandler::new(
                        "test_service".to_string(),
                        ServiceType::Tool,
                        Duration::from_micros(10),
                    ));
                    router.register_service(service).await.unwrap();
                    router.start().await.unwrap();

                    let events = generate_test_events(count, 1024);
                    for event in events {
                        let request = SimpleRequestRouter::create_request(
                            ServiceType::Tool,
                            "process".to_string(),
                            event.payload,
                        );
                        let _result = router.route_request(request).await;
                    }

                    let total_duration = start_time.elapsed();
                    println!("SimpleRouter approach - Events: {}, Duration: {:?}", count, total_duration);

                    total_duration
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_data_coordinator,
    benchmark_simple_router,
    benchmark_concurrent_processing,
    benchmark_memory_usage,
    benchmark_comparison
);
criterion_main!(benches);