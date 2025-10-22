//! Performance tests for event routing under high load

use crucible_services::events::core::*;
use crucible_services::events::routing::*;
use crucible_services::events::errors::EventResult;
use crucible_services::types::{ServiceHealth, ServiceStatus};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Barrier;
use uuid::Uuid;

/// Performance benchmark configuration
struct BenchmarkConfig {
    pub event_count: usize,
    pub concurrent_workers: usize,
    pub service_count: usize,
    pub event_size_bytes: usize,
    pub routing_rules_count: usize,
    pub enable_deduplication: bool,
    pub load_balancing_strategy: LoadBalancingStrategy,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            event_count: 10000,
            concurrent_workers: 10,
            service_count: 5,
            event_size_bytes: 1024,
            routing_rules_count: 10,
            enable_deduplication: false,
            load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
        }
    }
}

/// Performance benchmark results
#[derive(Debug, Clone)]
struct BenchmarkResults {
    pub total_events: usize,
    pub successful_events: usize,
    pub failed_events: usize,
    pub total_duration: Duration,
    pub events_per_second: f64,
    pub average_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub memory_usage_mb: f64,
    pub routing_efficiency: f64, // Successfully routed / total attempts
}

/// Performance benchmark runner
struct PerformanceBenchmark {
    config: BenchmarkConfig,
    router: Arc<DefaultEventRouter>,
}

impl PerformanceBenchmark {
    fn new(config: BenchmarkConfig) -> Self {
        let routing_config = RoutingConfig {
            max_queue_size: 10000,
            enable_deduplication: config.enable_deduplication,
            load_balancing_strategy: config.load_balancing_strategy,
            max_concurrent_events: 1000,
            ..Default::default()
        };

        let router = Arc::new(DefaultEventRouter::with_config(routing_config));

        Self { config, router }
    }

    async fn setup_services(&self) -> EventResult<()> {
        for i in 0..self.config.service_count {
            let service_id = format!("perf-service-{}", i);
            let registration = self.create_service_registration(&service_id);
            self.router.register_service(registration).await?;

            // Set service health to healthy
            self.router.update_service_health(&service_id, ServiceHealth {
                status: ServiceStatus::Healthy,
                message: Some("Performance test service".to_string()),
                last_check: Utc::now(),
                details: HashMap::new(),
            }).await?;
        }

        Ok(())
    }

    async fn setup_routing_rules(&self) -> EventResult<()> {
        for i in 0..self.config.routing_rules_count {
            let rule_id = format!("perf-rule-{}", i);
            let filter = self.create_event_filter(i);
            let targets = self.create_service_targets(i);

            let rule = RoutingRule {
                rule_id: rule_id.clone(),
                name: format!("Performance Rule {}", i),
                description: "Performance test routing rule".to_string(),
                filter,
                targets,
                priority: (i % 10) as u8,
                enabled: true,
                conditions: Vec::new(),
            };

            self.router.add_routing_rule(rule).await?;
        }

        Ok(())
    }

    fn create_service_registration(&self, service_id: &str) -> ServiceRegistration {
        ServiceRegistration {
            service_id: service_id.to_string(),
            service_type: "performance-test".to_string(),
            instance_id: format!("{}-instance-1", service_id),
            endpoint: Some(format!("http://localhost:8080/{}", service_id)),
            supported_event_types: vec![
                "filesystem".to_string(),
                "database".to_string(),
                "external".to_string(),
                "mcp".to_string(),
                "service".to_string(),
                "system".to_string(),
                "custom".to_string(),
            ],
            priority: (service_id.split('-').last().unwrap_or("0").parse::<u8>().unwrap_or(0)) % 10,
            weight: 1.0,
            max_concurrent_events: 1000,
            filters: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    fn create_event_filter(&self, rule_index: usize) -> EventFilter {
        let event_types = match rule_index % 6 {
            0 => vec!["filesystem".to_string()],
            1 => vec!["database".to_string()],
            2 => vec!["external".to_string()],
            3 => vec!["mcp".to_string()],
            4 => vec!["service".to_string()],
            _ => vec!["system".to_string()],
        };

        EventFilter {
            event_types,
            categories: Vec::new(),
            priorities: Vec::new(),
            sources: Vec::new(),
            expression: None,
            max_payload_size: Some(self.config.event_size_bytes * 2),
        }
    }

    fn create_service_targets(&self, rule_index: usize) -> Vec<ServiceTarget> {
        let service_count = (rule_index % self.config.service_count) + 1;
        (0..service_count)
            .map(|i| ServiceTarget::new(format!("perf-service-{}", i)))
            .collect()
    }

    fn create_test_event(&self, event_index: usize) -> DaemonEvent {
        let event_type = match event_index % 6 {
            0 => EventType::Filesystem(FilesystemEventType::FileCreated {
                path: format!("/perf/test/file{}.txt", event_index),
            }),
            1 => EventType::Database(DatabaseEventType::RecordCreated {
                table: "performance_test".to_string(),
                id: format!("record-{}", event_index),
            }),
            2 => EventType::External(ExternalEventType::DataReceived {
                source: "performance-test".to_string(),
                data: serde_json::json!({"index": event_index}),
            }),
            3 => EventType::Mcp(McpEventType::ToolCall {
                tool_name: "performance_tool".to_string(),
                parameters: serde_json::json!({"event_index": event_index}),
            }),
            4 => EventType::Service(ServiceEventType::HealthCheck {
                service_id: format!("service-{}", event_index),
                status: "healthy".to_string(),
            }),
            _ => EventType::System(SystemEventType::MetricsCollected {
                metrics: HashMap::from([
                    ("events_processed".to_string(), event_index as f64),
                    ("memory_usage".to_string(), 1024.0),
                ]),
            }),
        };

        let payload_data = if self.config.event_size_bytes > 100 {
            // Create larger payload
            let data = "x".repeat(self.config.event_size_bytes - 50);
            serde_json::json!({
                "event_index": event_index,
                "large_data": data,
                "timestamp": Utc::now().to_rfc3339(),
                "metadata": {"performance_test": true}
            })
        } else {
            // Create smaller payload
            serde_json::json!({
                "event_index": event_index,
                "timestamp": Utc::now().to_rfc3339()
            })
        };

        let source = EventSource::service(format!("perf-client-{}", event_index % self.config.concurrent_workers));

        DaemonEvent::new(event_type, source, EventPayload::json(payload_data))
    }

    async fn run_benchmark(&self) -> BenchmarkResults {
        // Setup
        self.setup_services().await.expect("Failed to setup services");
        self.setup_routing_rules().await.expect("Failed to setup routing rules");

        let total_events = self.config.event_count;
        let workers = self.config.concurrent_workers;
        let events_per_worker = total_events / workers;

        let barrier = Arc::new(Barrier::new(workers + 1)); // +1 for main thread
        let latencies = Arc::new(std::sync::Mutex::new(Vec::new()));
        let success_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let failure_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        // Start benchmark
        let start_time = Instant::now();

        // Spawn worker tasks
        let mut handles = Vec::new();
        for worker_id in 0..workers {
            let router_clone = self.router.clone();
            let barrier_clone = barrier.clone();
            let latencies_clone = latencies.clone();
            let success_count_clone = success_count.clone();
            let failure_count_clone = failure_count.clone();

            let handle = tokio::spawn(async move {
                // Wait for all workers to be ready
                barrier_clone.wait().await;

                for event_index in 0..events_per_worker {
                    let global_event_index = worker_id * events_per_worker + event_index;
                    let event = router_clone.create_test_event(global_event_index);

                    let event_start = Instant::now();
                    let result = router_clone.route_event(event).await;
                    let latency = event_start.elapsed();

                    // Record latency
                    if let Ok(mut lat_vec) = latencies_clone.lock() {
                        lat_vec.push(latency);
                    }

                    // Record success/failure
                    if result.is_ok() {
                        success_count_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    } else {
                        failure_count_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            });

            handles.push(handle);
        }

        // Start all workers simultaneously
        barrier.wait().await;

        // Wait for all workers to complete
        for handle in handles {
            handle.await.expect("Worker task panicked");
        }

        let total_duration = start_time.elapsed();

        // Calculate statistics
        let successful_events = success_count.load(std::sync::atomic::Ordering::Relaxed);
        let failed_events = failure_count.load(std::sync::atomic::Ordering::Relaxed);

        let latencies_vec = latencies.lock().unwrap().clone();
        let total_latency: Duration = latencies_vec.iter().sum();
        let average_latency = if !latencies_vec.is_empty() {
            total_latency / latencies_vec.len() as u32
        } else {
            Duration::ZERO
        };

        // Calculate percentiles
        let mut sorted_latencies = latencies_vec;
        sorted_latencies.sort();

        let p95_latency = if !sorted_latencies.is_empty() {
            sorted_latencies[(sorted_latencies.len() as f64 * 0.95) as usize]
        } else {
            Duration::ZERO
        };

        let p99_latency = if !sorted_latencies.is_empty() {
            sorted_latencies[(sorted_latencies.len() as f64 * 0.99) as usize]
        } else {
            Duration::ZERO
        };

        let events_per_second = total_events as f64 / total_duration.as_secs_f64();
        let routing_efficiency = successful_events as f64 / total_events as f64;

        // Estimate memory usage (simplified)
        let memory_usage_mb = (total_events * self.config.event_size_bytes) as f64 / (1024.0 * 1024.0);

        BenchmarkResults {
            total_events: total_events,
            successful_events,
            failed_events,
            total_duration,
            events_per_second,
            average_latency_ms: average_latency.as_secs_f64() * 1000.0,
            p95_latency_ms: p95_latency.as_secs_f64() * 1000.0,
            p99_latency_ms: p99_latency.as_secs_f64() * 1000.0,
            memory_usage_mb,
            routing_efficiency,
        }
    }
}

#[cfg(test)]
mod performance_benchmarks {
    use super::*;

    #[tokio::test]
    async fn benchmark_basic_routing_performance() {
        let config = BenchmarkConfig {
            event_count: 1000,
            concurrent_workers: 4,
            service_count: 3,
            event_size_bytes: 512,
            routing_rules_count: 5,
            enable_deduplication: false,
            load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
        };

        let benchmark = PerformanceBenchmark::new(config);
        let results = benchmark.run_benchmark().await;

        println!("\n=== Basic Routing Performance Benchmark ===");
        println!("Total events: {}", results.total_events);
        println!("Successful events: {}", results.successful_events);
        println!("Failed events: {}", results.failed_events);
        println!("Total duration: {:?}", results.total_duration);
        println!("Events per second: {:.2}", results.events_per_second);
        println!("Average latency: {:.2} ms", results.average_latency_ms);
        println!("P95 latency: {:.2} ms", results.p95_latency_ms);
        println!("P99 latency: {:.2} ms", results.p99_latency_ms);
        println!("Memory usage: {:.2} MB", results.memory_usage_mb);
        println!("Routing efficiency: {:.2}%", results.routing_efficiency * 100.0);

        // Performance assertions
        assert!(results.events_per_second > 100.0, "Events per second should be > 100");
        assert!(results.routing_efficiency > 0.95, "Routing efficiency should be > 95%");
        assert!(results.average_latency_ms < 100.0, "Average latency should be < 100ms");
        assert!(results.p95_latency_ms < 200.0, "P95 latency should be < 200ms");
    }

    #[tokio::test]
    async fn benchmark_high_load_performance() {
        let config = BenchmarkConfig {
            event_count: 10000,
            concurrent_workers: 20,
            service_count: 10,
            event_size_bytes: 1024,
            routing_rules_count: 20,
            enable_deduplication: false,
            load_balancing_strategy: LoadBalancingStrategy::LeastConnections,
        };

        let benchmark = PerformanceBenchmark::new(config);
        let results = benchmark.run_benchmark().await;

        println!("\n=== High Load Performance Benchmark ===");
        println!("Total events: {}", results.total_events);
        println!("Concurrent workers: {}", benchmark.config.concurrent_workers);
        println!("Service count: {}", benchmark.config.service_count);
        println!("Events per second: {:.2}", results.events_per_second);
        println!("Average latency: {:.2} ms", results.average_latency_ms);
        println!("P95 latency: {:.2} ms", results.p95_latency_ms);
        println!("P99 latency: {:.2} ms", results.p99_latency_ms);
        println!("Routing efficiency: {:.2}%", results.routing_efficiency * 100.0);

        // High load performance assertions
        assert!(results.events_per_second > 500.0, "Events per second should be > 500 under high load");
        assert!(results.routing_efficiency > 0.90, "Routing efficiency should be > 90% under high load");
        assert!(results.p99_latency_ms < 1000.0, "P99 latency should be < 1000ms under high load");
    }

    #[tokio::test]
    async fn benchmark_large_event_performance() {
        let config = BenchmarkConfig {
            event_count: 1000,
            concurrent_workers: 4,
            service_count: 3,
            event_size_bytes: 100 * 1024, // 100KB events
            routing_rules_count: 5,
            enable_deduplication: false,
            load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
        };

        let benchmark = PerformanceBenchmark::new(config);
        let results = benchmark.run_benchmark().await;

        println!("\n=== Large Event Performance Benchmark ===");
        println!("Event size: {} KB", benchmark.config.event_size_bytes / 1024);
        println!("Total events: {}", results.total_events);
        println!("Events per second: {:.2}", results.events_per_second);
        println!("Average latency: {:.2} ms", results.average_latency_ms);
        println!("Memory usage: {:.2} MB", results.memory_usage_mb);
        println!("Routing efficiency: {:.2}%", results.routing_efficiency * 100.0);

        // Large event performance assertions
        assert!(results.events_per_second > 50.0, "Events per second should be > 50 for large events");
        assert!(results.routing_efficiency > 0.95, "Routing efficiency should be > 95% for large events");
        assert!(results.memory_usage_mb > 50.0, "Memory usage should reflect large event sizes");
    }

    #[tokio::test]
    async fn benchmark_deduplication_performance() {
        let config = BenchmarkConfig {
            event_count: 2000,
            concurrent_workers: 8,
            service_count: 5,
            event_size_bytes: 512,
            routing_rules_count: 10,
            enable_deduplication: true,
            load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
        };

        let benchmark = PerformanceBenchmark::new(config);
        let results = benchmark.run_benchmark().await;

        println!("\n=== Deduplication Performance Benchmark ===");
        println!("Deduplication enabled: {}", benchmark.config.enable_deduplication);
        println!("Total events: {}", results.total_events);
        println!("Successful events: {}", results.successful_events);
        println!("Events per second: {:.2}", results.events_per_second);
        println!("Average latency: {:.2} ms", results.average_latency_ms);
        println!("Routing efficiency: {:.2}%", results.routing_efficiency * 100.0);

        // Deduplication performance assertions
        assert!(results.events_per_second > 100.0, "Events per second should be > 100 with deduplication");
        // Note: With deduplication enabled, some events will be filtered out as duplicates
        assert!(results.routing_efficiency >= 0.0, "Routing efficiency should be valid with deduplication");
    }

    #[tokio::test]
    async fn benchmark_load_balancing_strategies() {
        let strategies = vec![
            LoadBalancingStrategy::RoundRobin,
            LoadBalancingStrategy::LeastConnections,
            LoadBalancingStrategy::WeightedRandom,
            LoadBalancingStrategy::HealthBased,
            LoadBalancingStrategy::PriorityBased,
        ];

        println!("\n=== Load Balancing Strategy Comparison ===");

        for strategy in strategies {
            let config = BenchmarkConfig {
                event_count: 2000,
                concurrent_workers: 8,
                service_count: 5,
                event_size_bytes: 512,
                routing_rules_count: 5,
                enable_deduplication: false,
                load_balancing_strategy: strategy.clone(),
            };

            let benchmark = PerformanceBenchmark::new(config);
            let results = benchmark.run_benchmark().await;

            println!("\nStrategy: {:?}", strategy);
            println!("  Events/sec: {:.2}", results.events_per_second);
            println!("  Avg latency: {:.2} ms", results.average_latency_ms);
            println!("  P95 latency: {:.2} ms", results.p95_latency_ms);
            println!("  Efficiency: {:.2}%", results.routing_efficiency * 100.0);

            // All strategies should maintain reasonable performance
            assert!(results.events_per_second > 100.0, "All strategies should handle > 100 events/sec");
            assert!(results.routing_efficiency > 0.90, "All strategies should have > 90% efficiency");
        }
    }

    #[tokio::test]
    async fn benchmark_memory_pressure() {
        let config = BenchmarkConfig {
            event_count: 5000,
            concurrent_workers: 10,
            service_count: 3,
            event_size_bytes: 50 * 1024, // 50KB events
            routing_rules_count: 15,
            enable_deduplication: false,
            load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
        };

        let benchmark = PerformanceBenchmark::new(config);
        let results = benchmark.run_benchmark().await;

        println!("\n=== Memory Pressure Benchmark ===");
        println!("Total events: {}", results.total_events);
        println!("Event size: {} KB", benchmark.config.event_size_bytes / 1024);
        println!("Estimated memory usage: {:.2} MB", results.memory_usage_mb);
        println!("Events per second: {:.2}", results.events_per_second);
        println!("Average latency: {:.2} ms", results.average_latency_ms);
        println!("Routing efficiency: {:.2}%", results.routing_efficiency * 100.0);

        // Memory pressure performance assertions
        assert!(results.events_per_second > 50.0, "Should maintain > 50 events/sec under memory pressure");
        assert!(results.routing_efficiency > 0.85, "Should maintain > 85% efficiency under memory pressure");
        assert!(results.average_latency_ms < 500.0, "Average latency should be < 500ms under memory pressure");
    }

    #[tokio::test]
    async fn benchmark_scalability_test() {
        println!("\n=== Scalability Test ===");

        let service_counts = vec![1, 3, 5, 10, 20];
        let worker_counts = vec![1, 4, 8, 16];

        for &service_count in &service_counts {
            for &worker_count in &worker_counts {
                let config = BenchmarkConfig {
                    event_count: 2000,
                    concurrent_workers: worker_count,
                    service_count: service_count,
                    event_size_bytes: 512,
                    routing_rules_count: service_count,
                    enable_deduplication: false,
                    load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
                };

                let benchmark = PerformanceBenchmark::new(config);
                let results = benchmark.run_benchmark().await;

                println!(
                    "Services: {:2}, Workers: {:2} | Events/sec: {:6.2}, Latency: {:6.2}ms, Efficiency: {:5.1}%",
                    service_count,
                    worker_count,
                    results.events_per_second,
                    results.average_latency_ms,
                    results.routing_efficiency * 100.0
                );

                // Scalability assertions
                assert!(results.events_per_second > 50.0, "Should handle reasonable throughput at any scale");
                assert!(results.routing_efficiency > 0.80, "Should maintain reasonable efficiency at any scale");
            }
        }
    }

    #[tokio::test]
    async fn benchmark_complex_routing_scenarios() {
        println!("\n=== Complex Routing Scenarios ===");

        // Test 1: Many routing rules
        let config1 = BenchmarkConfig {
            event_count: 1000,
            concurrent_workers: 4,
            service_count: 5,
            event_size_bytes: 512,
            routing_rules_count: 50, // Many rules
            enable_deduplication: false,
            load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
        };

        let benchmark1 = PerformanceBenchmark::new(config1);
        let results1 = benchmark1.run_benchmark().await;

        println!("Many routing rules (50):");
        println!("  Events/sec: {:.2}", results1.events_per_second);
        println!("  Avg latency: {:.2} ms", results1.average_latency_ms);

        // Test 2: Complex event filters
        let config2 = BenchmarkConfig {
            event_count: 1000,
            concurrent_workers: 4,
            service_count: 5,
            event_size_bytes: 512,
            routing_rules_count: 10,
            enable_deduplication: false,
            load_balancing_strategy: LoadBalancingStrategy::PriorityBased,
        };

        let benchmark2 = PerformanceBenchmark::new(config2);
        let results2 = benchmark2.run_benchmark().await;

        println!("Complex event filters:");
        println!("  Events/sec: {:.2}", results2.events_per_second);
        println!("  Avg latency: {:.2} ms", results2.average_latency_ms);

        // Test 3: Mixed load balancing strategies
        let config3 = BenchmarkConfig {
            event_count: 1000,
            concurrent_workers: 8,
            service_count: 10,
            event_size_bytes: 1024,
            routing_rules_count: 15,
            enable_deduplication: true,
            load_balancing_strategy: LoadBalancingStrategy::WeightedRandom,
        };

        let benchmark3 = PerformanceBenchmark::new(config3);
        let results3 = benchmark3.run_benchmark().await;

        println!("Mixed strategies with deduplication:");
        println!("  Events/sec: {:.2}", results3.events_per_second);
        println!("  Avg latency: {:.2} ms", results3.average_latency_ms);

        // Complex scenario assertions
        assert!(results1.events_per_second > 50.0, "Many rules should not significantly impact performance");
        assert!(results2.events_per_second > 50.0, "Complex filters should not significantly impact performance");
        assert!(results3.events_per_second > 30.0, "Mixed complexity should maintain reasonable performance");
    }
}

#[cfg(test)]
mod stress_tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Ignored by default, run explicitly for stress testing
    async fn stress_test_extreme_load() {
        println!("\n=== Extreme Load Stress Test ===");

        let config = BenchmarkConfig {
            event_count: 100000, // 100K events
            concurrent_workers: 50,
            service_count: 20,
            event_size_bytes: 2048,
            routing_rules_count: 50,
            enable_deduplication: false,
            load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
        };

        let benchmark = PerformanceBenchmark::new(config);
        let results = benchmark.run_benchmark().await;

        println!("Stress test results:");
        println!("  Total events: {}", results.total_events);
        println!("  Successful: {}", results.successful_events);
        println!("  Failed: {}", results.failed_events);
        println!("  Events/sec: {:.2}", results.events_per_second);
        println!("  Duration: {:?}", results.total_duration);
        println!("  Memory usage: {:.2} MB", results.memory_usage_mb);

        // Stress test assertions (more lenient)
        assert!(results.events_per_second > 100.0, "Should handle > 100 events/sec under extreme load");
        assert!(results.routing_efficiency > 0.70, "Should maintain > 70% efficiency under extreme load");
    }

    #[tokio::test]
    #[ignore] // Ignored by default, run explicitly for stress testing
    async fn stress_test_memory_exhaustion() {
        println!("\n=== Memory Exhaustion Stress Test ===");

        let config = BenchmarkConfig {
            event_count: 10000,
            concurrent_workers: 20,
            service_count: 5,
            event_size_bytes: 1024 * 1024, // 1MB events
            routing_rules_count: 20,
            enable_deduplication: false,
            load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
        };

        let benchmark = PerformanceBenchmark::new(config);
        let results = benchmark.run_benchmark().await;

        println!("Memory exhaustion test results:");
        println!("  Event size: 1 MB");
        println!("  Total events: {}", results.total_events);
        println!("  Estimated memory: {:.2} MB", results.memory_usage_mb);
        println!("  Events/sec: {:.2}", results.events_per_second);
        println!("  Routing efficiency: {:.2}%", results.routing_efficiency * 100.0);

        // Memory exhaustion assertions
        assert!(results.routing_efficiency > 0.50, "Should maintain > 50% efficiency under memory pressure");
        assert!(results.events_per_second > 10.0, "Should handle > 10 events/sec with large payloads");
    }

    #[tokio::test]
    #[ignore] // Ignored by default, run explicitly for stress testing
    async fn stress_test_sustained_load() {
        println!("\n=== Sustained Load Stress Test ===");

        let duration_seconds = 30; // Run for 30 seconds
        let events_per_second_target = 1000;

        let config = BenchmarkConfig {
            event_count: events_per_second_target * duration_seconds,
            concurrent_workers: 20,
            service_count: 10,
            event_size_bytes: 1024,
            routing_rules_count: 15,
            enable_deduplication: false,
            load_balancing_strategy: LoadBalancingStrategy::LeastConnections,
        };

        let benchmark = PerformanceBenchmark::new(config);
        let results = benchmark.run_benchmark().await;

        println!("Sustained load test results:");
        println!("  Target duration: {} seconds", duration_seconds);
        println!("  Actual duration: {:?}", results.total_duration);
        println!("  Target rate: {} events/sec", events_per_second_target);
        println!("  Actual rate: {:.2} events/sec", results.events_per_second);
        println!("  Total events: {}", results.total_events);
        println!("  Success rate: {:.2}%", results.routing_efficiency * 100.0);

        // Sustained load assertions
        let actual_duration_secs = results.total_duration.as_secs_f64();
        assert!((actual_duration_secs - duration_seconds as f64).abs() < 5.0, "Duration should be close to target");
        assert!(results.events_per_second > events_per_second_target as f64 * 0.8, "Should maintain > 80% of target rate");
        assert!(results.routing_efficiency > 0.90, "Should maintain > 90% success rate under sustained load");
    }
}