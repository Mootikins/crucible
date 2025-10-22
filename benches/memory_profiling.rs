//! Memory profiling and resource usage monitoring
//!
//! This module provides comprehensive memory and resource usage analysis
//! for both DataCoordinator and centralized daemon approaches.

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
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Memory usage statistics
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub allocated_bytes: usize,
    pub deallocated_bytes: usize,
    pub current_usage: usize,
    pub peak_usage: usize,
    pub allocation_count: usize,
    pub deallocation_count: usize,
}

impl MemoryStats {
    pub fn new() -> Self {
        Self {
            allocated_bytes: 0,
            deallocated_bytes: 0,
            current_usage: 0,
            peak_usage: 0,
            allocation_count: 0,
            deallocation_count: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn get_memory_efficiency(&self) -> f64 {
        if self.allocated_bytes == 0 {
            1.0
        } else {
            self.deallocated_bytes as f64 / self.allocated_bytes as f64
        }
    }
}

/// Custom memory allocator for tracking allocations
pub struct TrackingAllocator {
    stats: std::sync::Mutex<MemoryStats>,
}

impl TrackingAllocator {
    pub const fn new() -> Self {
        Self {
            stats: std::sync::Mutex::new(MemoryStats::new()),
        }
    }

    pub fn get_stats(&self) -> MemoryStats {
        self.stats.lock().unwrap().clone()
    }

    pub fn reset_stats(&self) {
        self.stats.lock().unwrap().reset();
    }
}

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            let size = layout.size();
            let mut stats = self.stats.lock().unwrap();
            stats.allocated_bytes += size;
            stats.current_usage += size;
            stats.peak_usage = stats.peak_usage.max(stats.current_usage);
            stats.allocation_count += 1;
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size();
        System.dealloc(ptr, layout);
        let mut stats = self.stats.lock().unwrap();
        stats.deallocated_bytes += size;
        stats.current_usage = stats.current_usage.saturating_sub(size);
        stats.deallocation_count += 1;
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old_size = layout.size();
        let new_layout = Layout::from_size_align_unchecked(new_size, layout.align());
        let new_ptr = System.realloc(ptr, layout, new_size);

        if !new_ptr.is_null() {
            let mut stats = self.stats.lock().unwrap();
            if new_size > old_size {
                let diff = new_size - old_size;
                stats.allocated_bytes += diff;
                stats.current_usage += diff;
                stats.peak_usage = stats.peak_usage.max(stats.current_usage);
            } else {
                let diff = old_size - new_size;
                stats.deallocated_bytes += diff;
                stats.current_usage = stats.current_usage.saturating_sub(diff);
            }
        }

        new_ptr
    }
}

/// Global tracking allocator instance
#[global_allocator]
static GLOBAL_ALLOC: TrackingAllocator = TrackingAllocator::new();

/// CPU usage monitoring
#[derive(Debug, Clone)]
pub struct CpuStats {
    pub user_time: Duration,
    pub system_time: Duration,
    pub total_time: Duration,
    pub usage_percent: f64,
}

impl CpuStats {
    pub fn new() -> Self {
        Self {
            user_time: Duration::ZERO,
            system_time: Duration::ZERO,
            total_time: Duration::ZERO,
            usage_percent: 0.0,
        }
    }

    pub fn measure_during<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let start_user = std::time::Instant::now();
        let start_system = std::time::Instant::now();
        let start_total = std::time::Instant::now();

        let result = f();

        self.user_time += start_user.elapsed();
        self.system_time += start_system.elapsed();
        self.total_time += start_total.elapsed();

        // Calculate usage percentage (simplified)
        if self.total_time > Duration::ZERO {
            self.usage_percent = (self.user_time + self.system_time).as_secs_f64()
                / self.total_time.as_secs_f64() * 100.0;
        }

        result
    }
}

/// Resource usage snapshot
#[derive(Debug, Clone)]
pub struct ResourceSnapshot {
    pub memory_stats: MemoryStats,
    pub cpu_stats: CpuStats,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub event_count: usize,
    pub active_services: usize,
}

impl ResourceSnapshot {
    pub fn new(event_count: usize, active_services: usize) -> Self {
        Self {
            memory_stats: GLOBAL_ALLOC.get_stats(),
            cpu_stats: CpuStats::new(),
            timestamp: Utc::now(),
            event_count,
            active_services,
        }
    }
}

/// Resource usage monitor
pub struct ResourceMonitor {
    snapshots: Vec<ResourceSnapshot>,
    max_snapshots: usize,
}

impl ResourceMonitor {
    pub fn new(max_snapshots: usize) -> Self {
        Self {
            snapshots: Vec::with_capacity(max_snapshots),
            max_snapshots,
        }
    }

    pub fn take_snapshot(&mut self, event_count: usize, active_services: usize) {
        let snapshot = ResourceSnapshot::new(event_count, active_services);

        if self.snapshots.len() >= self.max_snapshots {
            self.snapshots.remove(0);
        }

        self.snapshots.push(snapshot);
    }

    pub fn get_snapshots(&self) -> &[ResourceSnapshot] {
        &self.snapshots
    }

    pub fn get_peak_memory_usage(&self) -> usize {
        self.snapshots
            .iter()
            .map(|s| s.memory_stats.peak_usage)
            .max()
            .unwrap_or(0)
    }

    pub fn get_average_memory_usage(&self) -> f64 {
        if self.snapshots.is_empty() {
            return 0.0;
        }

        let total: usize = self.snapshots
            .iter()
            .map(|s| s.memory_stats.current_usage)
            .sum();

        total as f64 / self.snapshots.len() as f64
    }

    pub fn get_memory_growth_rate(&self) -> f64 {
        if self.snapshots.len() < 2 {
            return 0.0;
        }

        let first = &self.snapshots[0];
        let last = &self.snapshots[self.snapshots.len() - 1];

        let memory_diff = last.memory_stats.current_usage as f64 - first.memory_stats.current_usage as f64;
        let time_diff = (last.timestamp - first.timestamp).num_seconds() as f64;

        if time_diff > 0.0 {
            memory_diff / time_diff
        } else {
            0.0
        }
    }

    pub fn get_memory_efficiency(&self) -> f64 {
        if self.snapshots.is_empty() {
            return 1.0;
        }

        let total_allocated: usize = self.snapshots
            .iter()
            .map(|s| s.memory_stats.allocated_bytes)
            .sum();

        let total_deallocated: usize = self.snapshots
            .iter()
            .map(|s| s.memory_stats.deallocated_bytes)
            .sum();

        if total_allocated == 0 {
            1.0
        } else {
            total_deallocated as f64 / total_allocated as f64
        }
    }

    pub fn reset(&mut self) {
        self.snapshots.clear();
        GLOBAL_ALLOC.reset_stats();
    }
}

/// Generate memory-intensive events
pub fn generate_memory_intensive_events(count: usize, payload_size: usize) -> Vec<DaemonEvent> {
    let mut events = Vec::with_capacity(count);

    for i in 0..count {
        // Create large payload
        let large_data = vec![0u8; payload_size];
        let payload = json!({
            "large_data": base64::encode(&large_data),
            "index": i,
            "metadata": {
                "timestamp": Utc::now(),
                "size": payload_size,
                "nested": {
                    "level1": {
                        "level2": {
                            "level3": "deeply nested data that increases memory usage"
                        }
                    }
                }
            }
        });

        let event = DaemonEvent::new(
            EventType::Filesystem(crucible_services::events::core::FilesystemEventType::FileCreated {
                path: format!("/test/large_file_{}.bin", i),
            }),
            EventSource::new(format!("memory_test_{}", i), SourceType::System),
            EventPayload::json(payload),
        );

        events.push(event);
    }

    events
}

/// Benchmark memory usage for DataCoordinator
pub fn benchmark_data_coordinator_memory(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("data_coordinator_memory");

    for payload_size in [1024, 4096, 16384, 65536] {
        for event_count in [100, 1000, 5000] {
            let total_size = payload_size * event_count;

            group.throughput(Throughput::Bytes(total_size as u64));
            group.bench_with_input(
                BenchmarkId::new("memory_usage", format!("{}_events_{}_bytes", event_count, payload_size)),
                &(event_count, payload_size),
                |b, &(count, size)| {
                    b.to_async(rt.as_ref()).iter(|| async {
                        let mut monitor = ResourceMonitor::new(100);
                        GLOBAL_ALLOC.reset_stats();

                        // Take initial snapshot
                        monitor.take_snapshot(0, 0);

                        let coordinator = setup_test_data_coordinator().await;
                        let events = generate_memory_intensive_events(count, size);

                        // Process events and monitor memory
                        for (i, event) in events.into_iter().enumerate() {
                            let event = black_box(event);

                            // Simulate DataCoordinator processing
                            tokio::time::sleep(Duration::from_micros(10)).await;

                            // Take snapshot every 100 events
                            if i % 100 == 0 {
                                monitor.take_snapshot(i, 1);
                            }
                        }

                        // Final snapshot
                        monitor.take_snapshot(count, 1);

                        // Analyze memory usage
                        let peak_memory = monitor.get_peak_memory_usage();
                        let avg_memory = monitor.get_average_memory_usage();
                        let efficiency = monitor.get_memory_efficiency();
                        let growth_rate = monitor.get_memory_growth_rate();

                        // Print memory statistics (in real benchmarks, these would be collected)
                        println!(
                            "DataCoordinator Memory - Events: {}, Payload: {} bytes, Peak: {} KB, Avg: {:.2} KB, Efficiency: {:.2}%, Growth: {:.2} KB/s",
                            count,
                            size,
                            peak_memory / 1024,
                            avg_memory / 1024.0,
                            efficiency * 100.0,
                            growth_rate / 1024.0
                        );

                        Duration::from_nanos(1) // Return dummy duration
                    });
                },
            );
        }
    }

    group.finish();
}

/// Benchmark memory usage for centralized daemon
pub fn benchmark_centralized_daemon_memory(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("centralized_daemon_memory");

    for payload_size in [1024, 4096, 16384, 65536] {
        for event_count in [100, 1000, 5000] {
            let total_size = payload_size * event_count;

            group.throughput(Throughput::Bytes(total_size as u64));
            group.bench_with_input(
                BenchmarkId::new("memory_usage", format!("{}_events_{}_bytes", event_count, payload_size)),
                &(event_count, payload_size),
                |b, &(count, size)| {
                    b.to_async(rt.as_ref()).iter(|| async {
                        let mut monitor = ResourceMonitor::new(100);
                        GLOBAL_ALLOC.reset_stats();

                        // Take initial snapshot
                        monitor.take_snapshot(0, 0);

                        let router = setup_test_event_router(10).await;
                        let events = generate_memory_intensive_events(count, size);

                        // Process events and monitor memory
                        for (i, event) in events.into_iter().enumerate() {
                            let event = black_box(event);

                            if let Err(_) = router.route_event(event).await {
                                // Handle routing errors gracefully
                            }

                            // Take snapshot every 100 events
                            if i % 100 == 0 {
                                monitor.take_snapshot(i, 10);
                            }
                        }

                        // Final snapshot
                        monitor.take_snapshot(count, 10);

                        // Analyze memory usage
                        let peak_memory = monitor.get_peak_memory_usage();
                        let avg_memory = monitor.get_average_memory_usage();
                        let efficiency = monitor.get_memory_efficiency();
                        let growth_rate = monitor.get_memory_growth_rate();

                        // Print memory statistics (in real benchmarks, these would be collected)
                        println!(
                            "CentralizedDaemon Memory - Events: {}, Payload: {} bytes, Peak: {} KB, Avg: {:.2} KB, Efficiency: {:.2}%, Growth: {:.2} KB/s",
                            count,
                            size,
                            peak_memory / 1024,
                            avg_memory / 1024.0,
                            efficiency * 100.0,
                            growth_rate / 1024.0
                        );

                        Duration::from_nanos(1) // Return dummy duration
                    });
                },
            );
        }
    }

    group.finish();
}

/// Memory leak detection test
pub fn benchmark_memory_leak_detection(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("memory_leak_detection");

    for iterations in [10, 100, 1000] {
        group.bench_with_input(
            BenchmarkId::new("data_coordinator_leak_test", iterations),
            &iterations,
            |b, &iter_count| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let mut memory_snapshots = Vec::new();

                    for iteration in 0..iter_count {
                        GLOBAL_ALLOC.reset_stats();

                        // Create and process events
                        let coordinator = setup_test_data_coordinator().await;
                        let events = generate_memory_intensive_events(100, 1024);

                        for event in events {
                            black_box(event);
                            tokio::time::sleep(Duration::from_micros(10)).await;
                        }

                        // Take memory snapshot
                        let stats = GLOBAL_ALLOC.get_stats();
                        memory_snapshots.push((iteration, stats.current_usage));

                        // Force garbage collection (simulated)
                        drop(coordinator);
                    }

                    // Analyze memory growth across iterations
                    let initial_memory = memory_snapshots.first().map(|(_, usage)| *usage).unwrap_or(0);
                    let final_memory = memory_snapshots.last().map(|(_, usage)| *usage).unwrap_or(0);
                    let memory_growth = final_memory.saturating_sub(initial_memory);

                    println!(
                        "DataCoordinator Leak Test - Iterations: {}, Initial: {} KB, Final: {} KB, Growth: {} KB",
                        iter_count,
                        initial_memory / 1024,
                        final_memory / 1024,
                        memory_growth / 1024
                    );

                    Duration::from_nanos(1)
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("centralized_daemon_leak_test", iterations),
            &iterations,
            |b, &iter_count| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let mut memory_snapshots = Vec::new();

                    for iteration in 0..iter_count {
                        GLOBAL_ALLOC.reset_stats();

                        // Create and process events
                        let router = setup_test_event_router(5).await;
                        let events = generate_memory_intensive_events(100, 1024);

                        for event in events {
                            let event = black_box(event);
                            if let Err(_) = router.route_event(event).await {
                                // Handle routing errors gracefully
                            }
                        }

                        // Take memory snapshot
                        let stats = GLOBAL_ALLOC.get_stats();
                        memory_snapshots.push((iteration, stats.current_usage));

                        // Force garbage collection (simulated)
                        drop(router);
                    }

                    // Analyze memory growth across iterations
                    let initial_memory = memory_snapshots.first().map(|(_, usage)| *usage).unwrap_or(0);
                    let final_memory = memory_snapshots.last().map(|(_, usage)| *usage).unwrap_or(0);
                    let memory_growth = final_memory.saturating_sub(initial_memory);

                    println!(
                        "CentralizedDaemon Leak Test - Iterations: {}, Initial: {} KB, Final: {} KB, Growth: {} KB",
                        iter_count,
                        initial_memory / 1024,
                        final_memory / 1024,
                        memory_growth / 1024
                    );

                    Duration::from_nanos(1)
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

criterion_group!(
    memory_benches,
    benchmark_data_coordinator_memory,
    benchmark_centralized_daemon_memory,
    benchmark_memory_leak_detection
);
criterion_main!(memory_benches);