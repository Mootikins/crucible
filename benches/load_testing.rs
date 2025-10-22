//! Load testing scenarios for daemon coordination performance
//!
//! This module provides comprehensive load testing with realistic event patterns
//! to stress test both the current DataCoordinator and centralized daemon approaches.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::Barrier;
use futures::future::join_all;
use crucible_daemon::coordinator::DataCoordinator;
use crucible_daemon::config::DaemonConfig;
use crucible_services::events::core::{DaemonEvent, EventType, EventPayload, EventSource, EventPriority, SourceType};
use crucible_services::events::routing::{DefaultEventRouter, RoutingConfig, EventRouter};
use crucible_services::types::{ServiceHealth, ServiceStatus};
use uuid::Uuid;
use chrono::Utc;
use serde_json::json;
use rand::{Rng, SeedableRng};
use rand::seq::SliceRandom;

/// Realistic event patterns for load testing
#[derive(Debug, Clone)]
pub enum EventPattern {
    /// Steady stream of events (constant rate)
    Steady { events_per_second: u32, duration_secs: u32 },
    /// Burst pattern (short high-intensity bursts)
    Burst { burst_size: u32, burst_interval_ms: u32, num_bursts: u32 },
    /// Gradual ramp-up (increasing load over time)
    RampUp { initial_events_per_sec: u32, max_events_per_sec: u32, ramp_time_secs: u32 },
    /// Spike pattern (sudden spikes in load)
    Spike { baseline_events_per_sec: u32, spike_events_per_sec: u32, spike_duration_secs: u32, total_duration_secs: u32 },
    /// Mixed workload (combination of different event types)
    Mixed { total_events: usize, distribution: EventTypeDistribution },
}

/// Distribution of event types in mixed workloads
#[derive(Debug, Clone)]
pub struct EventTypeDistribution {
    pub filesystem: f64,
    pub database: f64,
    pub external: f64,
    pub mcp: f64,
    pub service: f64,
    pub system: f64,
}

impl Default for EventTypeDistribution {
    fn default() -> Self {
        Self {
            filesystem: 0.30, // 30%
            database: 0.25,   // 25%
            external: 0.20,   // 20%
            mcp: 0.10,        // 10%
            service: 0.10,    // 10%
            system: 0.05,     // 5%
        }
    }
}

/// Load testing configuration
#[derive(Debug, Clone)]
pub struct LoadTestConfig {
    pub pattern: EventPattern,
    pub payload_size_range: (usize, usize), // min, max in bytes
    pub concurrent_services: usize,
    pub enable_failures: bool,
    pub failure_rate: f64, // 0.0 to 1.0
}

impl Default for LoadTestConfig {
    fn default() -> Self {
        Self {
            pattern: EventPattern::Steady {
                events_per_second: 100,
                duration_secs: 10,
            },
            payload_size_range: (512, 4096),
            concurrent_services: 10,
            enable_failures: false,
            failure_rate: 0.01,
        }
    }
}

/// Generate realistic events based on distribution
pub fn generate_realistic_events(
    count: usize,
    distribution: &EventTypeDistribution,
    payload_size_range: (usize, usize),
) -> Vec<DaemonEvent> {
    let mut events = Vec::with_capacity(count);
    let mut rng = rand::thread_rng();

    for i in 0..count {
        let event_type = determine_event_type(&mut rng, distribution);
        let payload_size = rng.gen_range(payload_size_range.0..=payload_size_range.1);
        let payload = generate_realistic_payload(payload_size, &mut rng);

        let source = determine_event_source(&mut rng, i);
        let priority = determine_event_priority(&mut rng, &event_type);

        let event = DaemonEvent::new(event_type, source, payload)
            .with_priority(priority);

        events.push(event);
    }

    events
}

/// Determine event type based on distribution
fn determine_event_type(rng: &mut impl Rng, distribution: &EventTypeDistribution) -> EventType {
    let roll = rng.gen::<f64>();
    let mut cumulative = 0.0;

    cumulative += distribution.filesystem;
    if roll < cumulative {
        return generate_filesystem_event(rng);
    }

    cumulative += distribution.database;
    if roll < cumulative {
        return generate_database_event(rng);
    }

    cumulative += distribution.external;
    if roll < cumulative {
        return generate_external_event(rng);
    }

    cumulative += distribution.mcp;
    if roll < cumulative {
        return generate_mcp_event(rng);
    }

    cumulative += distribution.service;
    if roll < cumulative {
        return generate_service_event(rng);
    }

    generate_system_event(rng)
}

/// Generate filesystem events
fn generate_filesystem_event(rng: &mut impl Rng) -> EventType {
    let file_paths = vec![
        "/home/user/documents/report.pdf",
        "/home/user/code/src/main.rs",
        "/home/user/data/analysis.csv",
        "/tmp/scratch_file.tmp",
        "/var/log/application.log",
        "/home/user/images/photo.jpg",
        "/home/user/videos/presentation.mp4",
    ];

    let path = file_paths.choose(rng).unwrap();

    match rng.gen_range(0..4) {
        0 => EventType::Filesystem(crucible_services::events::core::FilesystemEventType::FileCreated {
            path: path.to_string(),
        }),
        1 => EventType::Filesystem(crucible_services::events::core::FilesystemEventType::FileModified {
            path: path.to_string(),
        }),
        2 => EventType::Filesystem(crucible_services::events::core::FilesystemEventType::FileDeleted {
            path: path.to_string(),
        }),
        _ => EventType::Filesystem(crucible_services::events::core::FilesystemEventType::FileMoved {
            from: path.to_string(),
            to: format!("{}.moved", path),
        }),
    }
}

/// Generate database events
fn generate_database_event(rng: &mut impl Rng) -> EventType {
    let tables = vec!["users", "documents", "sessions", "audit_log", "metrics"];
    let table = tables.choose(rng).unwrap();
    let id = format!("{}_{}", table, rng.gen_range(1..10000));

    match rng.gen_range(0..4) {
        0 => EventType::Database(crucible_services::events::core::DatabaseEventType::RecordCreated {
            table: table.to_string(),
            id,
        }),
        1 => EventType::Database(crucible_services::events::core::DatabaseEventType::RecordUpdated {
            table: table.to_string(),
            id,
            changes: serde_json::Map::new(),
        }),
        2 => EventType::Database(crucible_services::events::core::DatabaseEventType::RecordDeleted {
            table: table.to_string(),
            id,
        }),
        _ => EventType::Database(crucible_services::events::core::DatabaseEventType::TransactionStarted {
            id: Uuid::new_v4().to_string(),
        }),
    }
}

/// Generate external events
fn generate_external_event(rng: &mut impl Rng) -> EventType {
    let sources = vec!["webhook", "api_client", "external_service", "notification_system"];
    let source = sources.choose(rng).unwrap();

    match rng.gen_range(0..3) {
        0 => EventType::External(crucible_services::events::core::ExternalEventType::DataReceived {
            source: source.to_string(),
            data: json!({"timestamp": Utc::now(), "data": "sample"}),
        }),
        1 => EventType::External(crucible_services::events::core::ExternalEventType::WebhookTriggered {
            url: format!("https://api.example.com/webhooks/{}", rng.gen_range(1..100)),
            payload: json!({"event": "triggered"}),
        }),
        _ => EventType::External(crucible_services::events::core::ExternalEventType::ApiCallCompleted {
            endpoint: format!("https://api.example.com/{}", source),
            status: 200,
            response: json!({"success": true}),
        }),
    }
}

/// Generate MCP events
fn generate_mcp_event(rng: &mut impl Rng) -> EventType {
    let tools = vec!["read_file", "write_file", "search", "execute_command", "query_database"];
    let tool = tools.choose(rng).unwrap();

    match rng.gen_range(0..3) {
        0 => EventType::Mcp(crucible_services::events::core::McpEventType::ToolCall {
            tool_name: tool.to_string(),
            parameters: json!({"input": "test"}),
        }),
        1 => EventType::Mcp(crucible_services::events::core::McpEventType::ToolResponse {
            tool_name: tool.to_string(),
            result: json!({"output": "success"}),
        }),
        _ => EventType::Mcp(crucible_services::events::core::McpEventType::ResourceRequested {
            resource_type: "file".to_string(),
            parameters: json!({"path": "/test/file.txt"}),
        }),
    }
}

/// Generate service events
fn generate_service_event(rng: &mut impl Rng) -> EventType {
    let services = vec!["database_service", "file_service", "event_service", "sync_service"];
    let service = services.choose(rng).unwrap();

    match rng.gen_range(0..3) {
        0 => EventType::Service(crucible_services::events::core::ServiceEventType::HealthCheck {
            service_id: service.to_string(),
            status: "healthy".to_string(),
        }),
        1 => EventType::Service(crucible_services::events::core::ServiceEventType::ServiceStatusChanged {
            service_id: service.to_string(),
            old_status: "starting".to_string(),
            new_status: "healthy".to_string(),
        }),
        _ => EventType::Service(crucible_services::events::core::ServiceEventType::ConfigurationChanged {
            service_id: service.to_string(),
            changes: serde_json::Map::new(),
        }),
    }
}

/// Generate system events
fn generate_system_event(rng: &mut impl Rng) -> EventType {
    match rng.gen_range(0..3) {
        0 => EventType::System(crucible_services::events::core::SystemEventType::MetricsCollected {
            metrics: std::collections::HashMap::new(),
        }),
        1 => EventType::System(crucible_services::events::core::SystemEventType::LogRotated {
            log_file: "/var/log/crucible.log".to_string(),
        }),
        _ => EventType::System(crucible_services::events::core::SystemEventType::ConfigurationReloaded {
            config_hash: format!("hash_{}", rng.gen_range(1..1000)),
        }),
    }
}

/// Determine event source
fn determine_event_source(rng: &mut impl Rng, event_index: usize) -> EventSource {
    let sources = vec![
        ("filesystem_watcher".to_string(), SourceType::Filesystem),
        ("database_trigger".to_string(), SourceType::Database),
        ("api_gateway".to_string(), SourceType::External),
        ("mcp_server".to_string(), SourceType::Mcp),
        ("service_manager".to_string(), SourceType::Service),
        ("system_monitor".to_string(), SourceType::System),
    ];

    let (id, source_type) = sources.choose(rng).unwrap();
    EventSource::new(
        format!("{}_{}", id, event_index % 10),
        source_type.clone(),
    )
}

/// Determine event priority based on event type
fn determine_event_priority(rng: &mut impl Rng, event_type: &EventType) -> EventPriority {
    match event_type {
        EventType::System(_) => {
            // System events are often critical
            if rng.gen_bool(0.3) {
                EventPriority::Critical
            } else {
                EventPriority::High
            }
        }
        EventType::Service(service_event) => {
            // Service health events are high priority
            match service_event {
                crucible_services::events::core::ServiceEventType::HealthCheck { .. } => {
                    if rng.gen_bool(0.5) {
                        EventPriority::High
                    } else {
                        EventPriority::Normal
                    }
                }
                _ => EventPriority::Normal,
            }
        }
        _ => {
            // Most events are normal or low priority
            if rng.gen_bool(0.1) {
                EventPriority::High
            } else if rng.gen_bool(0.7) {
                EventPriority::Normal
            } else {
                EventPriority::Low
            }
        }
    }
}

/// Generate realistic payload
fn generate_realistic_payload(size: usize, rng: &mut impl Rng) -> EventPayload {
    let mut data = serde_json::Map::new();

    // Add common fields
    data.insert("timestamp".to_string(), json!(Utc::now()));
    data.insert("id".to_string(), json!(Uuid::new_v4()));
    data.insert("source".to_string(), json!("load_test"));

    // Add size-appropriate content
    let content_size = size.saturating_sub(200); // Account for overhead
    if content_size > 0 {
        let content = "x".repeat(content_size);
        data.insert("content".to_string(), json!(content));
    }

    // Add metadata
    let mut metadata = serde_json::Map::new();
    metadata.insert("size".to_string(), json!(size));
    metadata.insert("test".to_string(), json!(true));

    data.insert("metadata".to_string(), json!(metadata));

    EventPayload::json(json!(data))
}

/// Load testing for steady event streams
pub fn benchmark_steady_load(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("steady_load");

    for events_per_second in [50, 100, 500, 1000] {
        group.bench_with_input(
            BenchmarkId::new("data_coordinator_steady", events_per_second),
            &events_per_second,
            |b, &eps| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let config = LoadTestConfig {
                        pattern: EventPattern::Steady {
                            events_per_second: eps,
                            duration_secs: 5,
                        },
                        ..Default::default()
                    };

                    run_steady_load_test(config, "data_coordinator").await
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("centralized_daemon_steady", events_per_second),
            &events_per_second,
            |b, &eps| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let config = LoadTestConfig {
                        pattern: EventPattern::Steady {
                            events_per_second: eps,
                            duration_secs: 5,
                        },
                        ..Default::default()
                    };

                    run_steady_load_test(config, "centralized_daemon").await
                });
            },
        );
    }

    group.finish();
}

/// Load testing for burst patterns
pub fn benchmark_burst_load(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("burst_load");

    for burst_size in [100, 500, 1000] {
        group.bench_with_input(
            BenchmarkId::new("data_coordinator_burst", burst_size),
            &burst_size,
            |b, &size| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let config = LoadTestConfig {
                        pattern: EventPattern::Burst {
                            burst_size: size,
                            burst_interval_ms: 100,
                            num_bursts: 5,
                        },
                        ..Default::default()
                    };

                    run_burst_load_test(config, "data_coordinator").await
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("centralized_daemon_burst", burst_size),
            &burst_size,
            |b, &size| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let config = LoadTestConfig {
                        pattern: EventPattern::Burst {
                            burst_size: size,
                            burst_interval_ms: 100,
                            num_bursts: 5,
                        },
                        ..Default::default()
                    };

                    run_burst_load_test(config, "centralized_daemon").await
                });
            },
        );
    }

    group.finish();
}

/// Load testing for mixed workloads
pub fn benchmark_mixed_workload(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("mixed_workload");

    for total_events in [1000, 5000, 10000] {
        group.throughput(Throughput::Elements(total_events as u64));

        group.bench_with_input(
            BenchmarkId::new("data_coordinator_mixed", total_events),
            &total_events,
            |b, &count| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let config = LoadTestConfig {
                        pattern: EventPattern::Mixed {
                            total_events: count,
                            distribution: EventTypeDistribution::default(),
                        },
                        ..Default::default()
                    };

                    run_mixed_workload_test(config, "data_coordinator").await
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("centralized_daemon_mixed", total_events),
            &total_events,
            |b, &count| {
                b.to_async(rt.as_ref()).iter(|| async {
                    let config = LoadTestConfig {
                        pattern: EventPattern::Mixed {
                            total_events: count,
                            distribution: EventTypeDistribution::default(),
                        },
                        ..Default::default()
                    };

                    run_mixed_workload_test(config, "centralized_daemon").await
                });
            },
        );
    }

    group.finish();
}

/// Run steady load test
async fn run_steady_load_test(config: LoadTestConfig, _approach: &str) -> Duration {
    let start = std::time::Instant::now();

    if let EventPattern::Steady { events_per_second, duration_secs } = config.pattern {
        let total_events = (events_per_second * duration_secs) as usize;
        let events = generate_realistic_events(
            total_events,
            &EventTypeDistribution::default(),
            config.payload_size_range,
        );

        let interval = Duration::from_millis(1000 / events_per_second as u64);

        for event in events {
            let event = black_box(event);
            tokio::time::sleep(interval).await;
        }
    }

    start.elapsed()
}

/// Run burst load test
async fn run_burst_load_test(config: LoadTestConfig, _approach: &str) -> Duration {
    let start = std::time::Instant::now();

    if let EventPattern::Burst { burst_size, burst_interval_ms, num_bursts } = config.pattern {
        for burst in 0..num_bursts {
            let events = generate_realistic_events(
                burst_size as usize,
                &EventTypeDistribution::default(),
                config.payload_size_range,
            );

            // Process burst rapidly
            for event in events {
                black_box(event);
            }

            // Wait between bursts
            if burst < num_bursts - 1 {
                tokio::time::sleep(Duration::from_millis(burst_interval_ms)).await;
            }
        }
    }

    start.elapsed()
}

/// Run mixed workload test
async fn run_mixed_workload_test(config: LoadTestConfig, _approach: &str) -> Duration {
    let start = std::time::Instant::now();

    if let EventPattern::Mixed { total_events, distribution } = config.pattern {
        let events = generate_realistic_events(
            total_events,
            &distribution,
            config.payload_size_range,
        );

        // Process events with realistic timing
        for event in events {
            let event = black_box(event);
            // Simulate variable processing time
            let processing_time = rand::thread_rng().gen_range(1..10);
            tokio::time::sleep(Duration::from_millis(processing_time)).await;
        }
    }

    start.elapsed()
}

/// Stress test with extreme load
pub fn benchmark_stress_test(c: &mut Criterion) {
    let runtime = Runtime::new().unwrap();
    let rt = Arc::new(runtime);

    let mut group = c.benchmark_group("stress_test");

    for concurrent_tasks in [10, 50, 100] {
        group.bench_with_input(
            BenchmarkId::new("data_coordinator_stress", concurrent_tasks),
            &concurrent_tasks,
            |b, &tasks| {
                b.to_async(rt.as_ref()).iter(|| async {
                    run_stress_test(tasks, "data_coordinator").await
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("centralized_daemon_stress", concurrent_tasks),
            &concurrent_tasks,
            |b, &tasks| {
                b.to_async(rt.as_ref()).iter(|| async {
                    run_stress_test(tasks, "centralized_daemon").await
                });
            },
        );
    }

    group.finish();
}

/// Run stress test
async fn run_stress_test(concurrent_tasks: usize, _approach: &str) -> Duration {
    let start = std::time::Instant::now();
    let barrier = Arc::new(Barrier::new(concurrent_tasks));

    let mut tasks = Vec::new();

    for task_id in 0..concurrent_tasks {
        let barrier = barrier.clone();
        let task = tokio::spawn(async move {
            // Wait for all tasks to be ready
            barrier.wait().await;

            let events_per_task = 1000;
            let events = generate_realistic_events(
                events_per_task,
                &EventTypeDistribution::default(),
                (512, 2048),
            );

            let task_start = std::time::Instant::now();

            for event in events {
                black_box(event);
                // Minimal processing time for stress test
                tokio::time::sleep(Duration::from_micros(1)).await;
            }

            task_start.elapsed()
        });
        tasks.push(task);
    }

    // Wait for all tasks to complete
    let _ = join_all(tasks).await;
    start.elapsed()
}

criterion_group!(
    load_benches,
    benchmark_steady_load,
    benchmark_burst_load,
    benchmark_mixed_workload,
    benchmark_stress_test
);
criterion_main!(load_benches);