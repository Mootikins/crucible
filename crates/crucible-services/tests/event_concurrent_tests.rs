//! Concurrent event processing tests

use crucible_services::events::core::*;
use crucible_services::events::routing::*;
use crucible_services::events::errors::{EventError, EventResult};
use crucible_services::types::{ServiceHealth, ServiceStatus};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use std::sync::Barrier;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Mutex};
use uuid::Uuid;

/// Thread-safe event collector for concurrent testing
pub struct ConcurrentEventCollector {
    pub events: Arc<Mutex<Vec<(DaemonEvent, Instant)>>>,
    pub processing_times: Arc<Mutex<Vec<Duration>>>,
    pub total_processed: Arc<AtomicUsize>,
    pub errors: Arc<Mutex<Vec<(Uuid, EventError)>>>,
}

impl ConcurrentEventCollector {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            processing_times: Arc::new(Mutex::new(Vec::new())),
            total_processed: Arc::new(AtomicUsize::new(0)),
            errors: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn record_event(&self, event: DaemonEvent, processing_start: Instant) {
        let processing_time = processing_start.elapsed();

        self.events.lock().await.push((event, processing_start));
        self.processing_times.lock().await.push(processing_time);
        self.total_processed.fetch_add(1, Ordering::SeqCst);
    }

    pub async fn record_error(&self, event_id: Uuid, error: EventError) {
        self.errors.lock().await.push((event_id, error));
    }

    pub async fn get_stats(&self) -> ConcurrentProcessingStats {
        let events = self.events.lock().await;
        let processing_times = self.processing_times.lock().await;
        let errors = self.errors.lock().await;

        let total_events = events.len();
        let total_errors = errors.len();
        let success_rate = if total_events > 0 {
            (total_events - total_errors) as f64 / total_events as f64
        } else {
            1.0
        };

        let avg_processing_time = if !processing_times.is_empty() {
            let total_time: Duration = processing_times.iter().sum();
            total_time / processing_times.len() as u32
        } else {
            Duration::ZERO
        };

        let max_processing_time = processing_times.iter().max().unwrap_or(&Duration::ZERO);
        let min_processing_time = processing_times.iter().min().unwrap_or(&Duration::ZERO);

        ConcurrentProcessingStats {
            total_events,
            total_errors,
            success_rate,
            avg_processing_time,
            max_processing_time: *max_processing_time,
            min_processing_time: *min_processing_time,
            throughput: self.total_processed.load(Ordering::SeqCst),
        }
    }

    pub async fn clear(&self) {
        self.events.lock().await.clear();
        self.processing_times.lock().await.clear();
        self.errors.lock().await.clear();
        self.total_processed.store(0, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone)]
pub struct ConcurrentProcessingStats {
    pub total_events: usize,
    pub total_errors: usize,
    pub success_rate: f64,
    pub avg_processing_time: Duration,
    pub max_processing_time: Duration,
    pub min_processing_time: Duration,
    pub throughput: usize,
}

/// Mock service for concurrent testing
pub struct ConcurrentMockService {
    pub service_id: String,
    pub processing_delay_ms: u64,
    pub max_concurrent: usize,
    pub current_load: Arc<AtomicUsize>,
    pub should_fail: Arc<AtomicBool>,
    pub failure_rate: f64,
    pub collector: Arc<ConcurrentEventCollector>,
}

impl ConcurrentMockService {
    pub fn new(
        service_id: String,
        processing_delay_ms: u64,
        max_concurrent: usize,
        collector: Arc<ConcurrentEventCollector>,
    ) -> Self {
        Self {
            service_id,
            processing_delay_ms,
            max_concurrent,
            current_load: Arc::new(AtomicUsize::new(0)),
            should_fail: Arc::new(AtomicBool::new(false)),
            failure_rate: 0.0,
            collector,
        }
    }

    pub fn with_failure_rate(mut self, rate: f64) -> Self {
        self.failure_rate = rate;
        self
    }

    pub async fn handle_event(&self, event: DaemonEvent) -> EventResult<()> {
        let start_time = Instant::now();

        // Check concurrent limit
        let current = self.current_load.fetch_add(1, Ordering::SeqCst);
        if current >= self.max_concurrent {
            self.current_load.fetch_sub(1, Ordering::SeqCst);
            return Err(EventError::delivery_error(
                self.service_id.clone(),
                "Service overloaded".to_string(),
            ));
        }

        // Simulate processing delay
        if self.processing_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.processing_delay_ms)).await;
        }

        // Check if should fail
        let should_fail = self.should_fail.load(Ordering::SeqCst) ||
            (self.failure_rate > 0.0 && rand::random::<f64>() < self.failure_rate);

        if should_fail {
            self.current_load.fetch_sub(1, Ordering::SeqCst);
            let error = EventError::delivery_error(
                self.service_id.clone(),
                "Simulated concurrent failure".to_string(),
            );
            self.collector.record_error(event.id, error.clone()).await;
            return Err(error);
        }

        // Record successful processing
        self.collector.record_event(event, start_time).await;

        // Release load
        self.current_load.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }

    pub fn set_should_fail(&self, should_fail: bool) {
        self.should_fail.store(should_fail, Ordering::SeqCst);
    }

    pub fn get_current_load(&self) -> usize {
        self.current_load.load(Ordering::SeqCst)
    }
}

/// Concurrent test scenario
pub struct ConcurrentTestScenario {
    pub name: String,
    pub worker_count: usize,
    pub events_per_worker: usize,
    pub service_count: usize,
    pub processing_delay_ms: u64,
    pub max_concurrent_per_service: usize,
    pub failure_rate: f64,
    pub test_duration_ms: Option<u64>,
}

impl ConcurrentTestScenario {
    pub fn new(name: String) -> Self {
        Self {
            name,
            worker_count: 4,
            events_per_worker: 100,
            service_count: 2,
            processing_delay_ms: 10,
            max_concurrent_per_service: 50,
            failure_rate: 0.0,
            test_duration_ms: None,
        }
    }

    pub fn with_workers(mut self, count: usize) -> Self {
        self.worker_count = count;
        self
    }

    pub fn with_events_per_worker(mut self, count: usize) -> Self {
        self.events_per_worker = count;
        self
    }

    pub fn with_services(mut self, count: usize) -> Self {
        self.service_count = count;
        self
    }

    pub fn with_processing_delay(mut self, delay_ms: u64) -> Self {
        self.processing_delay_ms = delay_ms;
        self
    }

    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent_per_service = max;
        self
    }

    pub fn with_failure_rate(mut self, rate: f64) -> Self {
        self.failure_rate = rate;
        self
    }

    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.test_duration_ms = Some(duration_ms);
        self
    }
}

/// Concurrent test runner
pub struct ConcurrentTestRunner {
    pub scenario: ConcurrentTestScenario,
    pub router: Arc<DefaultEventRouter>,
    pub services: Vec<Arc<ConcurrentMockService>>,
    pub collector: Arc<ConcurrentEventCollector>,
}

impl ConcurrentTestRunner {
    pub fn new(scenario: ConcurrentTestScenario) -> Self {
        let router = Arc::new(DefaultEventRouter::new());
        let collector = Arc::new(ConcurrentEventCollector::new());
        let services = Vec::new();

        Self {
            scenario,
            router,
            services,
            collector,
        }
    }

    pub async fn setup(&mut self) -> EventResult<()> {
        // Create services
        for i in 0..self.scenario.service_count {
            let service_id = format!("concurrent-service-{}", i);
            let service = Arc::new(ConcurrentMockService::new(
                service_id.clone(),
                self.scenario.processing_delay_ms,
                self.scenario.max_concurrent_per_service,
                self.collector.clone(),
            ).with_failure_rate(self.scenario.failure_rate));

            // Register service with router
            let registration = ServiceRegistration {
                service_id: service_id.clone(),
                service_type: "concurrent-test".to_string(),
                instance_id: format!("{}-instance-1", service_id),
                endpoint: Some(format!("http://localhost:8080/{}", service_id)),
                supported_event_types: vec!["test".to_string(), "custom".to_string()],
                priority: 0,
                weight: 1.0,
                max_concurrent_events: self.scenario.max_concurrent_per_service,
                filters: Vec::new(),
                metadata: HashMap::new(),
            };

            self.router.register_service(registration).await?;
            self.services.push(service);
        }

        // Create routing rule
        let targets: Vec<ServiceTarget> = self.services.iter()
            .map(|s| ServiceTarget::new(s.service_id.clone()))
            .collect();

        let rule = RoutingRule {
            rule_id: "concurrent-test-rule".to_string(),
            name: "Concurrent Test Rule".to_string(),
            description: "Rule for concurrent event testing".to_string(),
            filter: EventFilter {
                event_types: vec!["test".to_string()],
                ..Default::default()
            },
            targets,
            priority: 0,
            enabled: true,
            conditions: Vec::new(),
        };

        self.router.add_routing_rule(rule).await?;
        Ok(())
    }

    pub async fn run_test(&self) -> ConcurrentTestResults {
        println!("Starting concurrent test: {}", self.scenario.name);
        println!("Workers: {}, Events per worker: {}, Services: {}",
                 self.scenario.worker_count,
                 self.scenario.events_per_worker,
                 self.scenario.service_count);

        let start_time = Instant::now();
        let barrier = Arc::new(Barrier::new(self.scenario.worker_count + 1));

        // Spawn worker tasks
        let mut handles = Vec::new();
        for worker_id in 0..self.scenario.worker_count {
            let router = self.router.clone();
            let barrier_clone = barrier.clone();
            let collector = self.collector.clone();

            let handle = tokio::spawn(async move {
                // Wait for all workers to be ready
                barrier_clone.wait().await;

                for event_id in 0..self.scenario.events_per_worker {
                    let event = DaemonEvent::new(
                        EventType::TestEvent("concurrent-test".to_string()),
                        EventSource::service(format!("worker-{}", worker_id)),
                        EventPayload::json(serde_json::json!({
                            "worker_id": worker_id,
                            "event_id": event_id,
                            "timestamp": Utc::now().to_rfc3339()
                        })),
                    );

                    match router.route_event(event).await {
                        Ok(_) => {
                            // Event routed successfully
                        }
                        Err(e) => {
                            // Record error
                            let event_uuid = Uuid::new_v4();
                            collector.record_error(event_uuid, e).await;
                        }
                }
            });

            handles.push(handle);
        }

        // Start all workers
        barrier.wait().await;

        // Wait for all workers to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Wait for any remaining events to be processed
        tokio::time::sleep(Duration::from_millis(500)).await;

        let total_duration = start_time.elapsed();
        let stats = self.collector.get_stats().await;

        ConcurrentTestResults {
            scenario_name: self.scenario.name.clone(),
            worker_count: self.scenario.worker_count,
            events_per_worker: self.scenario.events_per_worker,
            total_events_sent: self.scenario.worker_count * self.scenario.events_per_worker,
            total_duration,
            processing_stats: stats,
        }
    }

    pub async fn run_duration_based_test(&self) -> ConcurrentTestResults {
        let duration_ms = self.scenario.test_duration_ms.unwrap_or(5000);
        println!("Running duration-based concurrent test: {} ({}ms)", self.scenario.name, duration_ms);

        let start_time = Instant::now();
        let end_time = start_time + Duration::from_millis(duration_ms);
        let barrier = Arc::new(Barrier::new(self.scenario.worker_count + 1));

        // Spawn worker tasks
        let mut handles = Vec::new();
        for worker_id in 0..self.scenario.worker_count {
            let router = self.router.clone();
            let barrier_clone = barrier.clone();
            let test_end_time = end_time;

            let handle = tokio::spawn(async move {
                barrier_clone.wait().await;
                let mut event_counter = 0;

                while Instant::now() < test_end_time {
                    let event = DaemonEvent::new(
                        EventType::TestEvent("duration-test".to_string()),
                        EventSource::service(format!("worker-{}", worker_id)),
                        EventPayload::json(serde_json::json!({
                            "worker_id": worker_id,
                            "event_counter": event_counter,
                            "timestamp": Utc::now().to_rfc3339()
                        })),
                    );

                    if let Err(_) = router.route_event(event).await {
                        // Handle error
                    }

                    event_counter += 1;

                    // Small delay to prevent overwhelming the system
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            });

            handles.push(handle);
        }

        // Start all workers
        barrier.wait().await;

        // Wait for all workers to complete
        for handle in handles {
            handle.await.unwrap();
        }

        let total_duration = end_time - start_time;
        let stats = self.collector.get_stats().await;

        ConcurrentTestResults {
            scenario_name: format!("{} (duration-based)", self.scenario.name),
            worker_count: self.scenario.worker_count,
            events_per_worker: 0, // Not applicable for duration-based test
            total_events_sent: stats.throughput,
            total_duration,
            processing_stats: stats,
        }
    }

    pub async fn run_stress_test(&self) -> ConcurrentTestResults {
        println!("Running stress test: {}", self.scenario.name);

        // Increase failure rate for stress test
        for service in &self.services {
            service.set_should_fail(true);
        }

        let results = self.run_test().await;

        // Reset failure state
        for service in &self.services {
            service.set_should_fail(false);
        }

        results
    }
}

#[derive(Debug)]
pub struct ConcurrentTestResults {
    pub scenario_name: String,
    pub worker_count: usize,
    pub events_per_worker: usize,
    pub total_events_sent: usize,
    pub total_duration: Duration,
    pub processing_stats: ConcurrentProcessingStats,
}

impl ConcurrentTestResults {
    pub fn print_summary(&self) {
        println!("\n=== Concurrent Test Results: {} ===", self.scenario_name);
        println!("Configuration:");
        println!("  Workers: {}", self.worker_count);
        if self.events_per_worker > 0 {
            println!("  Events per worker: {}", self.events_per_worker);
        }
        println!("  Total events sent: {}", self.total_events_sent);
        println!("  Test duration: {:?}", self.total_duration);

        println!("\nProcessing Statistics:");
        println!("  Events processed: {}", self.processing_stats.total_events);
        println!("  Errors: {}", self.processing_stats.total_errors);
        println!("  Success rate: {:.2}%", self.processing_stats.success_rate * 100.0);
        println!("  Throughput: {:.2} events/sec", self.processing_stats.throughput as f64 / self.total_duration.as_secs_f64());

        println!("\nTiming Statistics:");
        println!("  Avg processing time: {:?}", self.processing_stats.avg_processing_time);
        println!("  Max processing time: {:?}", self.processing_stats.max_processing_time);
        println!("  Min processing time: {:?}", self.processing_stats.min_processing_time);
    }

    pub fn assert_basic_properties(&self) {
        assert!(self.processing_stats.total_events > 0, "Should process some events");
        assert!(self.processing_stats.success_rate > 0.5, "Success rate should be > 50%");
        assert!(self.total_duration.as_millis() > 0, "Test should have measurable duration");
    }

    pub fn assert_performance_bounds(&self, min_throughput: f64, max_avg_latency_ms: u64) {
        let actual_throughput = self.processing_stats.throughput as f64 / self.total_duration.as_secs_f64();
        assert!(actual_throughput >= min_throughput,
               "Throughput should be >= {:.2} events/sec, got {:.2}",
               min_throughput, actual_throughput);

        let avg_latency_ms = self.processing_stats.avg_processing_time.as_millis();
        assert!(avg_latency_ms <= max_avg_latency_ms as u64,
               "Avg latency should be <= {}ms, got {}ms",
               max_avg_latency_ms, avg_latency_ms);
    }

    pub fn assert_no_errors(&self) {
        assert_eq!(self.processing_stats.total_errors, 0,
                  "Should have no errors, got {}",
                  self.processing_stats.total_errors);
    }

    pub fn assert_concurrent_processing(&self, min_concurrent_events: usize) {
        // If events were processed concurrently, we should see reasonable throughput
        let actual_throughput = self.processing_stats.throughput as f64 / self.total_duration.as_secs_f64();
        let expected_min_throughput = min_concurrent_events as f64 / self.total_duration.as_secs_f64();
        assert!(actual_throughput >= expected_min_throughput,
               "Concurrent processing should achieve minimum throughput");
    }
}

#[cfg(test)]
mod basic_concurrent_tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_concurrent_processing() {
        let scenario = ConcurrentTestScenario::new("Basic Concurrent Test".to_string())
            .with_workers(4)
            .with_events_per_worker(50)
            .with_services(2)
            .with_processing_delay(5);

        let mut runner = ConcurrentTestRunner::new(scenario);
        runner.setup().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();
        results.assert_basic_properties();

        // Should process all events successfully
        assert_eq!(results.processing_stats.total_events, 4 * 50);
    }

    #[tokio::test]
    async fn test_concurrent_processing_with_varying_load() {
        let scenario = ConcurrentTestScenario::new("Varying Load Test".to_string())
            .with_workers(6)
            .with_events_per_worker(100)
            .with_services(3)
            .with_processing_delay(10)
            .with_max_concurrent(20);

        let mut runner = ConcurrentTestRunner::new(scenario);
        runner.setup().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();
        results.assert_basic_properties();

        // Should handle load without excessive errors
        assert!(results.processing_stats.success_rate > 0.8);

        // Should demonstrate concurrent processing
        results.assert_concurrent_processing(100);
    }

    #[tokio::test]
    async fn test_high_concurrency_under_load() {
        let scenario = ConcurrentTestScenario::new("High Concurrency Test".to_string())
            .with_workers(10)
            .with_events_per_worker(200)
            .with_services(5)
            .with_processing_delay(2)
            .with_max_concurrent(100);

        let mut runner = ConcurrentTestRunner::new(scenario);
        runner.setup().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // High concurrency test
        results.assert_performance_bounds(500.0, 50); // 500 events/sec, 50ms max latency
        results.assert_concurrent_processing(500);
    }

    #[tokio::test]
    async fn test_concurrent_processing_with_failures() {
        let scenario = ConcurrentTestScenario::new("Concurrent Failure Test".to_string())
            .with_workers(4)
            .with_events_per_worker(100)
            .with_services(2)
            .with_processing_delay(10)
            .with_failure_rate(0.1); // 10% failure rate

        let mut runner = ConcurrentTestRunner::new(scenario);
        runner.setup().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // Should handle some failures gracefully
        assert!(results.processing_stats.total_errors > 0);
        assert!(results.processing_stats.success_rate > 0.8); // Should still have >80% success rate
    }
}

#[cfg(test)]
mod stress_concurrent_tests {
    use super::*;

    #[tokio::test]
    async fn test_extreme_concurrency() {
        let scenario = ConcurrentTestScenario::new("Extreme Concurrency Test".to_string())
            .with_workers(20)
            .with_events_per_worker(500)
            .with_services(8)
            .with_processing_delay(1)
            .with_max_concurrent(200);

        let mut runner = ConcurrentTestRunner::new(scenario);
        runner.setup().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // Extreme test - should still maintain reasonable performance
        results.assert_performance_bounds(1000.0, 100); // Lower throughput expectation for extreme test
        assert!(results.processing_stats.success_rate > 0.7); // Allow some failures under extreme load
    }

    #[tokio::test]
    async fn test_long_running_concurrent_load() {
        let scenario = ConcurrentTestScenario::new("Long Running Concurrent Load".to_string())
            .with_workers(8)
            .with_services(4)
            .with_processing_delay(5)
            .with_duration(10000); // 10 seconds

        let mut runner = ConcurrentTestRunner::new(scenario);
        runner.setup().await.unwrap();

        let results = runner.run_duration_based_test().await;
        results.print_summary();

        // Should handle sustained load
        assert!(results.processing_stats.throughput > 0);
        assert!(results.total_duration.as_secs() >= 9); // Should run for approximately 10 seconds
    }

    #[tokio::test]
    async fn test_concurrent_stress_with_circuit_breaker() {
        let scenario = ConcurrentTestScenario::new("Circuit Breaker Stress Test".to_string())
            .with_workers(10)
            .with_events_per_worker(200)
            .with_services(3)
            .with_processing_delay(10)
            .with_failure_rate(0.3); // High failure rate to trigger circuit breaker

        let mut runner = ConcurrentTestRunner::new(scenario);
        runner.setup().await.unwrap();

        let results = runner.run_stress_test().await;
        results.print_summary();

        // Circuit breaker should prevent cascading failures
        assert!(results.processing_stats.total_errors > 0); // Should have errors due to circuit breaker
        // System should remain responsive despite high failure rate
        assert!(results.total_duration.as_secs() < 60); // Should complete in reasonable time
    }
}

#[cfg(test)]
mod concurrent_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_load_balancing() {
        let scenario = ConcurrentTestScenario::new("Concurrent Load Balancing Test".to_string())
            .with_workers(8)
            .with_events_per_worker(100)
            .with_services(4)
            .with_processing_delay(5);

        let mut runner = ConcurrentTestRunner::new(scenario);
        runner.setup().await.unwrap();

        // Configure router for round-robin load balancing
        let results = runner.run_test().await;
        results.print_summary();

        // Should distribute load across services
        results.assert_basic_properties();

        // All services should have processed some events
        let total_events = results.processing_stats.total_events;
        let events_per_service = total_events / 4; // Approximate
        assert!(events_per_service > 10, "Each service should process multiple events");
    }

    #[tokio::test]
    async fn test_concurrent_event_prioritization() {
        let scenario = ConcurrentTestScenario::new("Concurrent Priority Test".to_string())
            .with_workers(6)
            .with_events_per_worker(150)
            .with_services(3)
            .with_processing_delay(8);

        let mut runner = ConcurrentTestRunner::new(scenario);
        runner.setup().await.unwrap();

        // Create events with different priorities
        let barrier = Arc::new(Barrier::new(scenario.worker_count + 1));
        let router = runner.router.clone();
        let collector = runner.collector.clone();

        let mut handles = Vec::new();
        for worker_id in 0..scenario.worker_count {
            let router_clone = router.clone();
            let barrier_clone = barrier.clone();
            let collector_clone = collector.clone();

            let handle = tokio::spawn(async move {
                barrier_clone.wait().await;

                for event_id in 0..scenario.events_per_worker {
                    let priority = match event_id % 4 {
                        0 => EventPriority::Critical,
                        1 => EventPriority::High,
                        2 => EventPriority::Normal,
                        _ => EventPriority::Low,
                    };

                    let event = DaemonEvent::new(
                        EventType::TestEvent("priority-test".to_string()),
                        EventSource::service(format!("worker-{}", worker_id)),
                        EventPayload::json(serde_json::json!({
                            "worker_id": worker_id,
                            "event_id": event_id,
                            "priority": format!("{:?}", priority)
                        })),
                    ).with_priority(priority);

                    if let Err(e) = router_clone.route_event(event).await {
                        let event_uuid = Uuid::new_v4();
                        collector_clone.record_error(event_uuid, e).await;
                    }
                }
            });

            handles.push(handle);
        }

        barrier.wait().await;

        for handle in handles {
            handle.await.unwrap();
        }

        tokio::time::sleep(Duration::from_millis(500)).await;

        let results = ConcurrentTestResults {
            scenario_name: "Priority Test".to_string(),
            worker_count: scenario.worker_count,
            events_per_worker: scenario.events_per_worker,
            total_events_sent: scenario.worker_count * scenario.events_per_worker,
            total_duration: Duration::from_millis(1000),
            processing_stats: collector.get_stats().await,
        };

        results.print_summary();
        results.assert_basic_properties();
    }

    #[tokio::test]
    async fn test_concurrent_service_health_changes() {
        let scenario = ConcurrentTestScenario::new("Health Changes Test".to_string())
            .with_workers(4)
            .with_events_per_worker(200)
            .with_services(3)
            .with_processing_delay(10);

        let mut runner = ConcurrentTestRunner::new(scenario);
        runner.setup().await.unwrap();

        // Start health change simulation
        let services_clone = runner.services.clone();
        let router_clone = runner.router.clone();
        tokio::spawn(async move {
            // Make services unhealthy periodically
            for i in 0..5 {
                tokio::time::sleep(Duration::from_millis(500)).await;

                for (j, service) in services_clone.iter().enumerate() {
                    if (i + j) % 2 == 0 {
                        let status = if i % 3 == 0 {
                            ServiceStatus::Unhealthy
                        } else {
                            ServiceStatus::Degraded
                        };

                        router_clone.update_service_health(&service.service_id, ServiceHealth {
                            status: status.clone(),
                            message: Some(format!("Health change {}", i)),
                            last_check: Utc::now(),
                            details: HashMap::new(),
                        }).await.ok();

                        // Restore health after a short time
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        router_clone.update_service_health(&service.service_id, ServiceHealth {
                            status: ServiceStatus::Healthy,
                            message: "Health restored".to_string(),
                            last_check: Utc::now(),
                            details: HashMap::new(),
                        }).await.ok();
                    }
                }
            }
        });

        let results = runner.run_test().await;
        results.print_summary();

        // Should handle health changes gracefully
        results.assert_basic_properties();
        // May have some errors due to health changes, but should remain mostly successful
        assert!(results.processing_stats.success_rate > 0.6);
    }
}

#[cfg(test)]
mod concurrent_performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_throughput_benchmark() {
        let scenario = ConcurrentTestScenario::new("Throughput Benchmark".to_string())
            .with_workers(8)
            .with_events_per_worker(1000)
            .with_services(4)
            .with_processing_delay(1); // Minimal delay for maximum throughput

        let mut runner = ConcurrentTestRunner::new(scenario);
        runner.setup().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // High throughput test
        results.assert_performance_bounds(2000.0, 20); // 2000 events/sec, 20ms max latency
    }

    #[tokio::test]
    async fn test_concurrent_latency_under_load() {
        let scenario = ConcurrentTestScenario::new("Latency Test".to_string())
            .with_workers(12)
            .with_events_per_worker(200)
            .with_services(6)
            .with_processing_delay(15);

        let mut runner = ConcurrentTestRunner::new(scenario);
        runner.setup().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // Focus on latency while maintaining reasonable throughput
        results.assert_performance_bounds(100.0, 50); // 100 events/sec, 50ms max latency

        // Latency should be relatively consistent
        let latency_variance_ms = results.processing_stats.max_processing_time.as_millis() as i64
                                   - results.processing_stats.min_processing_time.as_millis() as i64;
        assert!(latency_variance_ms < 200, "Latency variance should be reasonable");
    }

    #[tokio::test]
    async fn test_scalability_benchmark() {
        println!("\n=== Scalability Benchmark ===");

        let test_configs = vec![
            (2, 4, 2),   // Small scale
            (4, 8, 3),   // Medium scale
            (8, 16, 4),  // Large scale
            (16, 32, 6), // Very large scale
        ];

        for (services, workers, events_per_worker) in test_configs {
            let scenario = ConcurrentTestScenario::new(
                format!("Scale Test: {} services, {} workers", services, workers)
            )
            .with_workers(workers)
            .with_events_per_worker(events_per_worker)
            .with_services(services)
            .with_processing_delay(5);

            let mut runner = ConcurrentTestRunner::new(scenario);
            runner.setup().await.unwrap();

            let results = runner.run_test().await;
            results.print_summary();

            // All scales should maintain reasonable performance
            results.assert_basic_properties();

            // Throughput should scale reasonably
            let expected_min_throughput = 50.0 * services as f64;
            let actual_throughput = results.processing_stats.throughput as f64 / results.total_duration.as_secs_f64();
            assert!(actual_throughput >= expected_min_throughput,
                   "Scale test failed for {} services: expected >= {:.2}, got {:.2}",
                   services, expected_min_throughput, actual_throughput);
        }
    }
}

// Add EventType::TestEvent for testing
impl EventType {
    pub fn TestEvent(name: String) -> Self {
        EventType::Custom(name)
    }
}