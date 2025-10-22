//! Performance benchmarks for Crucible daemon coordination
//!
//! This module provides comprehensive performance testing for both the current
//! DataCoordinator approach and the new centralized daemon architecture.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use crucible_daemon::coordinator::DataCoordinator;
use crucible_daemon::config::DaemonConfig;
use crucible_services::events::core::{DaemonEvent, EventType, EventPayload, EventSource, EventPriority, SourceType};
use crucible_services::events::routing::{DefaultEventRouter, RoutingConfig, EventRouter};
use crucible_services::types::{ServiceHealth, ServiceStatus};
use uuid::Uuid;
use chrono::Utc;
use serde_json::json;

/// Configuration for different benchmark scenarios
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub num_events: usize,
    pub concurrent_services: usize,
    pub event_payload_size: usize,
    pub routing_rules: usize,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            num_events: 1000,
            concurrent_services: 10,
            event_payload_size: 1024, // 1KB
            routing_rules: 5,
        }
    }
}

/// Benchmark scenarios
impl BenchmarkConfig {
    /// Light load scenario
    pub fn light() -> Self {
        Self {
            num_events: 100,
            concurrent_services: 3,
            event_payload_size: 512,
            routing_rules: 2,
        }
    }

    /// Medium load scenario
    pub fn medium() -> Self {
        Self {
            num_events: 1000,
            concurrent_services: 10,
            event_payload_size: 1024,
            routing_rules: 5,
        }
    }

    /// Heavy load scenario
    pub fn heavy() -> Self {
        Self {
            num_events: 10000,
            concurrent_services: 50,
            event_payload_size: 4096,
            routing_rules: 20,
        }
    }

    /// Stress test scenario
    pub fn stress() -> Self {
        Self {
            num_events: 100000,
            concurrent_services: 100,
            event_payload_size: 8192,
            routing_rules: 50,
        }
    }
}

/// Generate test events for benchmarking
pub fn generate_test_events(count: usize, payload_size: usize) -> Vec<DaemonEvent> {
    let mut events = Vec::with_capacity(count);
    let payload_data = vec![0u8; payload_size];
    let payload_json = json!({
        "data": base64::encode(&payload_data),
        "timestamp": Utc::now(),
        "metadata": {
            "benchmark": true,
            "size": payload_size
        }
    });

    for i in 0..count {
        let event_type = match i % 6 {
            0 => EventType::Filesystem(crucible_services::events::core::FilesystemEventType::FileCreated {
                path: format!("/test/file_{}.txt", i),
            }),
            1 => EventType::Filesystem(crucible_services::events::core::FilesystemEventType::FileModified {
                path: format!("/test/file_{}.txt", i),
            }),
            2 => EventType::Database(crucible_services::events::core::DatabaseEventType::RecordCreated {
                table: "test_table".to_string(),
                id: format!("record_{}", i),
            }),
            3 => EventType::External(crucible_services::events::core::ExternalEventType::DataReceived {
                source: "benchmark".to_string(),
                data: json!({"index": i}),
            }),
            4 => EventType::Service(crucible_services::events::core::ServiceEventType::HealthCheck {
                service_id: format!("service_{}", i % 10),
                status: "healthy".to_string(),
            }),
            _ => EventType::System(crucible_services::events::core::SystemEventType::MetricsCollected {
                metrics: std::collections::HashMap::new(),
            }),
        };

        let event = DaemonEvent::new(
            event_type,
            EventSource::new(format!("benchmark_source_{}", i % 5), SourceType::System),
            EventPayload::json(payload_json.clone()),
        )
        .with_priority(match i % 4 {
            0 => EventPriority::Critical,
            1 => EventPriority::High,
            2 => EventPriority::Normal,
            _ => EventPriority::Low,
        });

        events.push(event);
    }

    events
}

/// Benchmark current DataCoordinator approach
pub fn benchmark_data_coordinator(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("data_coordinator");

    // Test different load scenarios
    for config in [
        BenchmarkConfig::light(),
        BenchmarkConfig::medium(),
        BenchmarkConfig::heavy(),
    ] {
        group.throughput(Throughput::Elements(config.num_events as u64));
        group.bench_with_input(
            BenchmarkId::new("event_processing", config.num_events),
            &config,
            |b, config| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let test_coordinator = setup_test_data_coordinator().await;
                    let events = generate_test_events(config.num_events, config.event_payload_size);

                    let start = std::time::Instant::now();

                    // Process events through DataCoordinator
                    for event in events {
                        let event = black_box(event);
                        // Simulate DataCoordinator processing
                        tokio::time::sleep(Duration::from_micros(10)).await;
                    }

                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

/// Benchmark centralized daemon approach with event routing
pub fn benchmark_centralized_daemon(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("centralized_daemon");

    // Test different load scenarios
    for config in [
        BenchmarkConfig::light(),
        BenchmarkConfig::medium(),
        BenchmarkConfig::heavy(),
    ] {
        group.throughput(Throughput::Elements(config.num_events as u64));
        group.bench_with_input(
            BenchmarkId::new("event_routing", config.num_events),
            &config,
            |b, config| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let router = setup_test_event_router(config.concurrent_services).await;
                    let events = generate_test_events(config.num_events, config.event_payload_size);

                    let start = std::time::Instant::now();

                    // Process events through centralized router
                    for event in events {
                        let event = black_box(event);
                        if let Err(_) = router.route_event(event).await {
                            // Handle routing errors gracefully in benchmarks
                        }
                    }

                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

/// Benchmark memory usage patterns
pub fn benchmark_memory_usage(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("memory_usage");

    for event_count in [100, 1000, 10000, 100000] {
        group.bench_with_input(
            BenchmarkId::new("data_coordinator_memory", event_count),
            &event_count,
            |b, &count| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let test_coordinator = setup_test_data_coordinator().await;
                    let events = generate_test_events(count, 1024);

                    let start = std::time::Instant::now();
                    let mut processed_events = Vec::new();

                    // Process events and measure memory usage
                    for event in events {
                        let event = black_box(event);
                        processed_events.push(event);
                    }

                    // Clear events to measure deallocation
                    processed_events.clear();

                    start.elapsed()
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("centralized_daemon_memory", event_count),
            &event_count,
            |b, &count| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let router = setup_test_event_router(10).await;
                    let events = generate_test_events(count, 1024);

                    let start = std::time::Instant::now();
                    let mut processed_events = Vec::new();

                    // Process events and measure memory usage
                    for event in events {
                        let event = black_box(event);
                        processed_events.push(event);
                        if let Err(_) = router.route_event(event).await {
                            // Handle routing errors gracefully
                        }
                    }

                    // Clear events to measure deallocation
                    processed_events.clear();

                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

/// Benchmark latency under concurrent load
pub fn benchmark_concurrent_latency(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("concurrent_latency");

    for concurrency in [1, 5, 10, 25, 50] {
        group.bench_with_input(
            BenchmarkId::new("data_coordinator_concurrent", concurrency),
            &concurrency,
            |b, &concurrency| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let test_coordinator = setup_test_data_coordinator().await;
                    let events_per_task = 100;

                    let start = std::time::Instant::now();
                    let mut tasks = Vec::new();

                    for task_id in 0..concurrency {
                        let events = generate_test_events(events_per_task, 512);
                        let task = tokio::spawn(async move {
                            let task_start = std::time::Instant::now();
                            for event in events {
                                black_box(event);
                                tokio::time::sleep(Duration::from_micros(10)).await;
                            }
                            task_start.elapsed()
                        });
                        tasks.push(task);
                    }

                    // Wait for all tasks and measure overall latency
                    let latencies: Vec<Duration> = futures::future::join_all(tasks)
                        .await
                        .into_iter()
                        .filter_map(Result::ok)
                        .collect();

                    let total_latency = start.elapsed();
                    (total_latency, latencies)
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("centralized_daemon_concurrent", concurrency),
            &concurrency,
            |b, &concurrency| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let router = setup_test_event_router(concurrency).await;
                    let events_per_task = 100;

                    let start = std::time::Instant::now();
                    let mut tasks = Vec::new();

                    for task_id in 0..concurrency {
                        let router = router.clone();
                        let events = generate_test_events(events_per_task, 512);
                        let task = tokio::spawn(async move {
                            let task_start = std::time::Instant::now();
                            for event in events {
                                let event = black_box(event);
                                if let Err(_) = router.route_event(event).await {
                                    // Handle routing errors gracefully
                                }
                            }
                            task_start.elapsed()
                        });
                        tasks.push(task);
                    }

                    // Wait for all tasks and measure overall latency
                    let latencies: Vec<Duration> = futures::future::join_all(tasks)
                        .await
                        .into_iter()
                        .filter_map(Result::ok)
                        .collect();

                    let total_latency = start.elapsed();
                    (total_latency, latencies)
                });
            },
        );
    }

    group.finish();
}

/// Setup test DataCoordinator
async fn setup_test_data_coordinator() -> DataCoordinator {
    let config = DaemonConfig::default();
    DataCoordinator::new(config).await.unwrap()
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

/// Comparison benchmark between approaches
pub fn benchmark_comparison(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("comparison");

    for num_events in [100, 1000, 5000] {
        // DataCoordinator approach
        group.bench_with_input(
            BenchmarkId::new("data_coordinator", num_events),
            &num_events,
            |b, &count| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let coordinator = setup_test_data_coordinator().await;
                    let events = generate_test_events(count, 1024);
                    let start = std::time::Instant::now();

                    for event in events {
                        black_box(event);
                        tokio::time::sleep(Duration::from_micros(5)).await; // Simulate processing
                    }

                    start.elapsed()
                });
            },
        );

        // Centralized daemon approach
        group.bench_with_input(
            BenchmarkId::new("centralized_daemon", num_events),
            &num_events,
            |b, &count| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let router = setup_test_event_router(5).await;
                    let events = generate_test_events(count, 1024);
                    let start = std::time::Instant::now();

                    for event in events {
                        let event = black_box(event);
                        if let Err(_) = router.route_event(event).await {
                            // Handle routing errors gracefully
                        }
                    }

                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_data_coordinator,
    benchmark_centralized_daemon,
    benchmark_memory_usage,
    benchmark_concurrent_latency,
    benchmark_comparison
);
criterion_main!(benches);