//! Daemon performance benchmarks
//!
//! These benchmarks measure the performance of daemon services including
//! event routing, service discovery, health checks, and concurrent coordination.

use criterion::{black_box, criterion_group, BenchmarkId, Criterion, Throughput};
use std::sync::Arc;
use tokio::runtime::Runtime;
use std::time::Duration;

use crate::benchmark_utils::{
    TestDataGenerator, BenchmarkConfig, ResourceMonitor,
    ConcurrencyLevels, run_async_benchmark
};

/// Benchmark event routing throughput
fn bench_event_routing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("daemon_event_routing");

    for event_count in [100, 1000, 10000] {
        group.throughput(Throughput::Elements(event_count));

        group.bench_with_input(
            BenchmarkId::new("single_threaded_routing", event_count),
            &event_count,
            |b, &count| {
                b.to_async(&rt).iter(|| async {
                    let monitor = ResourceMonitor::new();

                    // Generate test events
                    let events = generate_test_events(count);

                    // Route events single-threaded
                    let routed = route_events_single_threaded(events).await;

                    black_box(routed);
                    black_box(monitor.elapsed());
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("concurrent_routing", event_count),
            &event_count,
            |b, &count| {
                b.to_async(&rt).iter(|| async {
                    let monitor = ResourceMonitor::new();

                    // Generate test events
                    let events = generate_test_events(count);

                    // Route events concurrently
                    let routed = route_events_concurrently(events).await;

                    black_box(routed);
                    black_box(monitor.elapsed());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark service discovery performance
fn bench_service_discovery(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("daemon_service_discovery");

    group.bench_function("service_registration", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Register a new service
            let service_id = register_service("test_service").await;

            black_box(service_id);
            black_box(monitor.elapsed());
        });
    });

    group.bench_function("service_lookup", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Look up a service
            let service = lookup_service("test_service").await;

            black_box(service);
            black_box(monitor.elapsed());
        });
    });

    for service_count in [10, 100, 1000] {
        group.throughput(Throughput::Elements(service_count));
        group.bench_with_input(
            BenchmarkId::new("batch_discovery", service_count),
            &service_count,
            |b, &count| {
                b.to_async(&rt).iter(|| async {
                    let monitor = ResourceMonitor::new();

                    // Discover all services
                    let services = discover_all_services(count).await;

                    black_box(services);
                    black_box(monitor.elapsed());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark health check overhead
fn bench_health_checks(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("daemon_health_checks");

    group.bench_function("single_health_check", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Perform health check
            let health = perform_health_check("test_service").await;

            black_box(health);
            black_box(monitor.elapsed());
        });
    });

    for service_count in [10, 100, 1000] {
        group.throughput(Throughput::Elements(service_count));
        group.bench_with_input(
            BenchmarkId::new("concurrent_health_checks", service_count),
            &service_count,
            |b, &count| {
                b.to_async(&rt).iter(|| async {
                    let monitor = ResourceMonitor::new();

                    // Perform concurrent health checks
                    let results = perform_concurrent_health_checks(count).await;

                    black_box(results);
                    black_box(monitor.elapsed());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark concurrent service coordination
fn bench_concurrent_coordination(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("daemon_concurrent_coordination");

    for concurrency in [
        ConcurrencyLevels::LOW,
        ConcurrencyLevels::MEDIUM,
        ConcurrencyLevels::HIGH,
    ] {
        group.throughput(Throughput::Elements(concurrency as u64));
        group.bench_with_input(
            BenchmarkId::new("service_coordination", concurrency),
            &concurrency,
            |b, &concurrency| {
                b.to_async(&rt).iter(|| async {
                    let monitor = ResourceMonitor::new();

                    // Coordinate concurrent service operations
                    let results = coordinate_concurrent_services(concurrency).await;

                    black_box(results);
                    black_box(monitor.elapsed());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark event subscription management
fn bench_event_subscription(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("daemon_event_subscription");

    group.bench_function("subscription_creation", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Create event subscription
            let subscription_id = create_subscription("event_type_*").await;

            black_box(subscription_id);
            black_box(monitor.elapsed());
        });
    });

    group.bench_function("subscription_matching", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Match event against subscriptions
            let matches = match_event_to_subscriptions("user_login", &["user_*", "login_*", "auth_*"]).await;

            black_box(matches);
            black_box(monitor.elapsed());
        });
    });

    for subscription_count in [10, 100, 1000] {
        group.throughput(Throughput::Elements(subscription_count));
        group.bench_with_input(
            BenchmarkId::new("batch_subscription_matching", subscription_count),
            &subscription_count,
            |b, &count| {
                b.to_async(&rt).iter(|| async {
                    let monitor = ResourceMonitor::new();

                    // Match event against many subscriptions
                    let subscriptions = generate_test_subscriptions(count);
                    let matches = match_event_to_many_subscriptions("user_login", &subscriptions).await;

                    black_box(matches);
                    black_box(monitor.elapsed());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark service lifecycle management
fn bench_service_lifecycle(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("daemon_service_lifecycle");

    group.bench_function("service_start", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Start a service
            let service_handle = start_service("test_service").await;

            black_box(service_handle);
            black_box(monitor.elapsed());
        });
    });

    group.bench_function("service_stop", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Stop a service
            let result = stop_service("test_service").await;

            black_box(result);
            black_box(monitor.elapsed());
        });
    });

    group.bench_function("service_restart", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Restart a service
            let result = restart_service("test_service").await;

            black_box(result);
            black_box(monitor.elapsed());
        });
    });

    group.finish();
}

/// Benchmark daemon memory usage and cleanup
fn bench_daemon_memory(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("daemon_memory_management");

    group.bench_function("event_buffer_memory", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Fill event buffer
            let buffer_size = fill_event_buffer(10000).await;

            black_box(buffer_size);
            black_box(monitor.memory_diff());
        });
    });

    group.bench_function("subscription_memory", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Create many subscriptions
            let subscription_count = create_many_subscriptions(1000).await;

            black_box(subscription_count);
            black_box(monitor.memory_diff());
        });
    });

    group.bench_function("garbage_collection_overhead", |b| {
        b.to_async(&rt).iter(|| async {
            let monitor = ResourceMonitor::new();

            // Perform operations that create garbage
            create_and_discard_events(1000).await;

            // Simulate garbage collection
            let gc_result = perform_garbage_collection().await;

            black_box(gc_result);
            black_box(monitor.elapsed());
        });
    });

    group.finish();
}

/// Benchmark network communication overhead
fn bench_network_communication(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("daemon_network_communication");

    group.bench_function("message_serialization", |b| {
        b.iter(|| {
            let monitor = ResourceMonitor::new();

            // Serialize event message
            let serialized = serialize_event_message();

            black_box(serialized);
            black_box(monitor.elapsed());
        });
    });

    group.bench_function("message_deserialization", |b| {
        b.iter(|| {
            let monitor = ResourceMonitor::new();

            // Deserialize event message
            let message = "[{\"type\":\"event\",\"data\":\"test\"}]";
            let deserialized = deserialize_event_message(message);

            black_box(deserialized);
            black_box(monitor.elapsed());
        });
    });

    for message_size in [1, 10, 100] { // KB
        group.throughput(Throughput::Bytes(message_size as u64 * 1024));
        group.bench_with_input(
            BenchmarkId::new("large_message_processing", message_size),
            &message_size,
            |b, &size| {
                b.to_async(&rt).iter(|| async {
                    let monitor = ResourceMonitor::new();

                    // Process large message
                    let result = process_large_message(size).await;

                    black_box(result);
                    black_box(monitor.elapsed());
                });
            },
        );
    }

    group.finish();
}

// Mock implementations for daemon benchmarking

async fn generate_test_events(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("event_{}", i)).collect()
}

async fn route_events_single_threaded(events: Vec<String>) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate single-threaded event routing
    let mut routed = 0;
    for event in events {
        tokio::time::sleep(Duration::from_micros(10)).await;
        // Route event
        black_box(event);
        routed += 1;
    }
    Ok(routed)
}

async fn route_events_concurrently(events: Vec<String>) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate concurrent event routing
    let chunk_size = (events.len() / 4).max(1);
    let mut handles = Vec::new();

    for chunk in events.chunks(chunk_size) {
        let chunk = chunk.to_vec();
        let handle = tokio::spawn(async move {
            let mut routed = 0;
            for event in chunk {
                tokio::time::sleep(Duration::from_micros(10)).await;
                black_box(event);
                routed += 1;
            }
            routed
        });
        handles.push(handle);
    }

    let mut total_routed = 0;
    for handle in handles {
        total_routed += handle.await.unwrap();
    }

    Ok(total_routed)
}

async fn register_service(service_name: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate service registration
    tokio::time::sleep(Duration::from_micros(100)).await;
    Ok(format!("service_id_{}", service_name))
}

async fn lookup_service(service_name: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate service lookup
    tokio::time::sleep(Duration::from_micros(50)).await;
    Ok(format!("service_endpoint_for_{}", service_name))
}

async fn discover_all_services(max_count: usize) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate service discovery
    tokio::time::sleep(Duration::from_micros(max_count as u64)).await;
    Ok((0..max_count).map(|i| format!("service_{}", i)).collect())
}

async fn perform_health_check(service_name: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate health check
    tokio::time::sleep(Duration::from_micros(200)).await;
    Ok(true)
}

async fn perform_concurrent_health_checks(service_count: usize) -> Result<Vec<bool>, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate concurrent health checks
    let mut handles = Vec::new();

    for i in 0..service_count {
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_micros(200)).await;
            true
        });
        handles.push(handle);
    }

    let mut results = Vec::with_capacity(service_count);
    for handle in handles {
        results.push(handle.await.unwrap());
    }

    Ok(results)
}

async fn coordinate_concurrent_services(concurrency: usize) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate service coordination
    let mut handles = Vec::new();

    for i in 0..concurrency {
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_micros((i * 10) as u64)).await;
            format!("coordination_result_{}", i)
        });
        handles.push(handle);
    }

    let mut results = Vec::with_capacity(concurrency);
    for handle in handles {
        results.push(handle.await.unwrap());
    }

    Ok(results)
}

async fn create_subscription(pattern: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate subscription creation
    tokio::time::sleep(Duration::from_micros(50)).await;
    Ok(format!("subscription_id_for_{}", pattern))
}

async fn match_event_to_subscriptions(event: &str, patterns: &[&str]) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate subscription matching
    tokio::time::sleep(Duration::from_micros(20)).await;

    let matches: Vec<String> = patterns
        .iter()
        .filter(|&&pattern| pattern.contains('*') || event.contains(pattern))
        .map(|&pattern| format!("matched_{}", pattern))
        .collect();

    Ok(matches)
}

async fn generate_test_subscriptions(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("pattern_{}*", i)).collect()
}

async fn match_event_to_many_subscriptions(event: &str, subscriptions: &[String]) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate matching against many subscriptions
    tokio::time::sleep(Duration::from_micros(subscriptions.len() as u64 / 10)).await;

    let matches = subscriptions.iter().filter(|sub| {
        sub.contains('*') || event.contains(sub)
    }).count();

    Ok(matches)
}

async fn start_service(service_name: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate service start
    tokio::time::sleep(Duration::from_micros(500)).await;
    Ok(format!("handle_for_{}", service_name))
}

async fn stop_service(service_name: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate service stop
    tokio::time::sleep(Duration::from_micros(200)).await;
    Ok(true)
}

async fn restart_service(service_name: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate service restart
    tokio::time::sleep(Duration::from_micros(700)).await;
    Ok(true)
}

async fn fill_event_buffer(count: usize) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate filling event buffer
    let events: Vec<String> = (0..count).map(|i| format!("buffered_event_{}", i)).collect();
    tokio::time::sleep(Duration::from_micros(events.len() as u64)).await;
    Ok(events.len())
}

async fn create_many_subscriptions(count: usize) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate creating many subscriptions
    let subscriptions: Vec<String> = (0..count).map(|i| format!("subscription_{}", i)).collect();
    tokio::time::sleep(Duration::from_micros(subscriptions.len() as u64 * 5)).await;
    Ok(subscriptions.len())
}

async fn create_and_discard_events(count: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Simulate creating and discarding events (creates garbage)
    for i in 0..count {
        let event = format!("temporary_event_{}", i);
        black_box(event); // Use the event briefly
        // Event goes out of scope here, creating garbage
    }
    tokio::time::sleep(Duration::from_micros(count as u64)).await;
    Ok(())
}

async fn perform_garbage_collection() -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate garbage collection
    tokio::time::sleep(Duration::from_micros(1000)).await;
    Ok(1000) // Bytes collected
}

fn serialize_event_message() -> Vec<u8> {
    // Simulate message serialization
    r#"{"type":"event","data":"test_payload","timestamp":1234567890}"#.as_bytes().to_vec()
}

fn deserialize_event_message(message: &str) -> Result<serde_json::Value, serde_json::Error> {
    // Simulate message deserialization
    serde_json::from_str(message)
}

async fn process_large_message(size_kb: usize) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    // Simulate processing large message
    let message = "x".repeat(size_kb * 1024);
    tokio::time::sleep(Duration::from_micros(message.len() as u64 / 10)).await;
    Ok(message.len())
}

pub fn daemon_benchmarks(c: &mut Criterion) {
    bench_event_routing(c);
    bench_service_discovery(c);
    bench_health_checks(c);
    bench_concurrent_coordination(c);
    bench_event_subscription(c);
    bench_service_lifecycle(c);
    bench_daemon_memory(c);
    bench_network_communication(c);
}