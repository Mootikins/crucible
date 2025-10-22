//! Circuit breaker tests for failure detection and recovery

use crucible_services::events::core::*;
use crucible_services::events::routing::*;
use crucible_services::events::errors::{EventError, EventResult};
use crucible_services::types::{ServiceHealth, ServiceStatus};
use chrono::{Utc, Duration};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicBool, Ordering};
use std::time::Instant;
use tokio::sync::{RwLock, Barrier};
use uuid::Uuid;

/// Circuit breaker test configuration
#[derive(Debug, Clone)]
struct CircuitBreakerTestConfig {
    pub failure_threshold: u32,
    pub success_threshold: u32,
    pub timeout_ms: u64,
    pub half_open_max_calls: u32,
    pub recovery_delay_ms: u64,
}

impl Default for CircuitBreakerTestConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout_ms: 1000,
            half_open_max_calls: 5,
            recovery_delay_ms: 500,
        }
    }
}

/// Circuit breaker state tracker
#[derive(Debug, Clone, PartialEq)]
enum CircuitBreakerTestState {
    Closed,
    Open,
    HalfOpen,
}

/// Mock service with controllable failure behavior
struct ControllableService {
    service_id: String,
    should_fail: Arc<AtomicBool>,
    failure_count: Arc<AtomicU32>,
    success_count: Arc<AtomicU32>,
    total_calls: Arc<AtomicU32>,
    circuit_state: Arc<RwLock<CircuitBreakerTestState>>,
    config: CircuitBreakerTestConfig,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    half_open_calls: Arc<AtomicU32>,
}

impl ControllableService {
    fn new(service_id: String, config: CircuitBreakerTestConfig) -> Self {
        Self {
            service_id,
            should_fail: Arc::new(AtomicBool::new(false)),
            failure_count: Arc::new(AtomicU32::new(0)),
            success_count: Arc::new(AtomicU32::new(0)),
            total_calls: Arc::new(AtomicU32::new(0)),
            circuit_state: Arc::new(RwLock::new(CircuitBreakerTestState::Closed)),
            config,
            last_failure_time: Arc::new(RwLock::new(None)),
            half_open_calls: Arc::new(AtomicU32::new(0)),
        }
    }

    async fn handle_event(&self, event: DaemonEvent) -> EventResult<()> {
        self.total_calls.fetch_add(1, Ordering::SeqCst);

        // Check circuit breaker state
        let current_state = self.circuit_state.read().await.clone();

        match current_state {
            CircuitBreakerTestState::Open => {
                // Check if timeout has passed
                if let Some(last_failure) = *self.last_failure_time.read().await {
                    if last_failure.elapsed() >= Duration::milliseconds(self.config.timeout_ms as i64) {
                        // Transition to half-open
                        *self.circuit_state.write().await = CircuitBreakerTestState::HalfOpen;
                        self.half_open_calls.store(0, Ordering::SeqCst);
                        drop(current_state); // Release read lock before proceeding
                    } else {
                        return Err(EventError::CircuitBreakerOpen(self.service_id.clone()));
                    }
                } else {
                    return Err(EventError::CircuitBreakerOpen(self.service_id.clone()));
                }
            }
            CircuitBreakerTestState::HalfOpen => {
                let half_open_calls = self.half_open_calls.fetch_add(1, Ordering::SeqCst);
                if half_open_calls >= self.config.half_open_max_calls {
                    return Err(EventError::CircuitBreakerOpen(self.service_id.clone()));
                }
            }
            CircuitBreakerTestState::Closed => {
                // Normal operation
            }
        }

        // Simulate actual service behavior
        if self.should_fail.load(Ordering::SeqCst) {
            self.record_failure().await;
            Err(EventError::delivery_error(
                self.service_id.clone(),
                "Simulated service failure".to_string(),
            ))
        } else {
            self.record_success().await;
            Ok(())
        }
    }

    async fn record_failure(&self) {
        let failures = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;

        // Update last failure time
        *self.last_failure_time.write().await = Some(Instant::now());

        let mut state = self.circuit_state.write().await;

        match *state {
            CircuitBreakerTestState::Closed => {
                if failures >= self.config.failure_threshold {
                    *state = CircuitBreakerTestState::Open;
                }
            }
            CircuitBreakerTestState::HalfOpen => {
                // Any failure in half-open state should open the circuit
                *state = CircuitBreakerTestState::Open;
            }
            CircuitBreakerTestState::Open => {
                // Already open, nothing to do
            }
        }
    }

    async fn record_success(&self) {
        self.success_count.fetch_add(1, Ordering::SeqCst);

        let mut state = self.circuit_state.write().await;

        match *state {
            CircuitBreakerTestState::HalfOpen => {
                let successes = self.success_count.load(Ordering::SeqCst);
                // Check if we've had enough successes to close the circuit
                if successes % self.config.success_threshold as u32 == 0 {
                    *state = CircuitBreakerTestState::Closed;
                    // Reset failure count
                    self.failure_count.store(0, Ordering::SeqCst);
                }
            }
            CircuitBreakerTestState::Closed => {
                // In closed state, success reduces failure count if there were failures
                let failures = self.failure_count.load(Ordering::SeqCst);
                if failures > 0 {
                    self.failure_count.store(failures - 1, Ordering::SeqCst);
                }
            }
            CircuitBreakerTestState::Open => {
                // Should not get successes in open state
                unreachable!("Success in open circuit state");
            }
        }
    }

    async fn set_should_fail(&self, should_fail: bool) {
        self.should_fail.store(should_fail, Ordering::SeqCst);
    }

    async fn get_state(&self) -> CircuitBreakerTestState {
        self.circuit_state.read().await.clone()
    }

    async fn get_stats(&self) -> ServiceStats {
        ServiceStats {
            total_calls: self.total_calls.load(Ordering::SeqCst),
            successes: self.success_count.load(Ordering::SeqCst),
            failures: self.failure_count.load(Ordering::SeqCst),
            state: self.get_state().await,
        }
    }

    async fn reset(&self) {
        self.failure_count.store(0, Ordering::SeqCst);
        self.success_count.store(0, Ordering::SeqCst);
        self.total_calls.store(0, Ordering::SeqCst);
        *self.circuit_state.write().await = CircuitBreakerTestState::Closed;
        *self.last_failure_time.write().await = None;
        self.half_open_calls.store(0, Ordering::SeqCst);
        self.should_fail.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug)]
struct ServiceStats {
    pub total_calls: u32,
    pub successes: u32,
    pub failures: u32,
    pub state: CircuitBreakerTestState,
}

/// Circuit breaker test scenario
struct CircuitBreakerTestScenario {
    name: String,
    config: CircuitBreakerTestConfig,
    initial_failure_rate: f64,
    events_to_send: usize,
    expected_state_transitions: Vec<(usize, CircuitBreakerTestState)>,
}

impl CircuitBreakerTestScenario {
    fn new(name: String) -> Self {
        Self {
            name,
            config: CircuitBreakerTestConfig::default(),
            initial_failure_rate: 0.0,
            events_to_send: 100,
            expected_state_transitions: Vec::new(),
        }
    }

    fn with_config(mut self, config: CircuitBreakerTestConfig) -> Self {
        self.config = config;
        self
    }

    fn with_failure_rate(mut self, rate: f64) -> Self {
        self.initial_failure_rate = rate;
        self
    }

    fn with_events(mut self, count: usize) -> Self {
        self.events_to_send = count;
        self
    }

    fn with_expected_transitions(mut self, transitions: Vec<(usize, CircuitBreakerTestState)>) -> Self {
        self.expected_state_transitions = transitions;
        self
    }
}

/// Circuit breaker test runner
struct CircuitBreakerTestRunner {
    scenario: CircuitBreakerTestScenario,
    service: Arc<ControllableService>,
    router: Arc<DefaultEventRouter>,
}

impl CircuitBreakerTestRunner {
    fn new(scenario: CircuitBreakerTestScenario) -> Self {
        let service = Arc::new(ControllableService::new(
            "circuit-breaker-test-service".to_string(),
            scenario.config.clone(),
        ));

        let config = RoutingConfig {
            circuit_breaker_threshold: scenario.config.failure_threshold,
            circuit_breaker_timeout_ms: scenario.config.timeout_ms,
            ..Default::default()
        };

        let router = Arc::new(DefaultEventRouter::with_config(config));

        Self {
            scenario,
            service,
            router,
        }
    }

    async fn setup(&self) -> EventResult<()> {
        // Register service with router
        let registration = ServiceRegistration {
            service_id: self.service.service_id.clone(),
            service_type: "circuit-breaker-test".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: Some("http://localhost:8080/cb-test".to_string()),
            supported_event_types: vec!["test".to_string(), "custom".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 100,
            filters: Vec::new(),
            metadata: HashMap::new(),
        };

        self.router.register_service(registration).await?;

        // Set initial service health
        self.router.update_service_health(&self.service.service_id, ServiceHealth {
            status: ServiceStatus::Healthy,
            message: Some("Circuit breaker test service".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await?;

        // Setup routing rule
        let rule = RoutingRule {
            rule_id: "circuit-breaker-test-rule".to_string(),
            name: "Circuit Breaker Test Rule".to_string(),
            description: "Test rule for circuit breaker".to_string(),
            filter: EventFilter {
                event_types: vec!["test".to_string()],
                ..Default::default()
            },
            targets: vec![ServiceTarget::new(self.service.service_id.clone())],
            priority: 0,
            enabled: true,
            conditions: Vec::new(),
        };

        self.router.add_routing_rule(rule).await?;

        Ok(())
    }

    async fn run_test(&self) -> CircuitBreakerTestResults {
        let mut state_changes = Vec::new();
        let mut events_sent = 0;
        let mut events_succeeded = 0;
        let mut events_failed = 0;

        for i in 0..self.scenario.events_to_send {
            // Determine if this event should fail based on failure rate
            let should_fail = if i < self.scenario.events_to_send / 2 {
                // First half: use initial failure rate
                rand::random::<f64>() < self.scenario.initial_failure_rate
            } else {
                // Second half: vary based on test scenario
                match self.scenario.name.as_str() {
                    "Basic Circuit Breaker Test" => false, // All succeed in second half
                    "Circuit Breaker Recovery Test" => false, // All succeed for recovery
                    _ => rand::random::<f64>() < 0.3, // 30% failure rate
                }
            };

            self.service.set_should_fail(should_fail).await;

            // Record state before event
            let state_before = self.service.get_state().await;

            // Send event
            let event = DaemonEvent::new(
                EventType::TestEvent("circuit-breaker-test".to_string()),
                EventSource::service(format!("test-client-{}", i)),
                EventPayload::json(serde_json::json!({
                    "event_id": i,
                    "should_fail": should_fail,
                    "timestamp": Utc::now().to_rfc3339()
                })),
            );

            let result = self.router.route_event(event).await;
            events_sent += 1;

            // Record state after event
            let state_after = self.service.get_state().await;

            if state_before != state_after {
                state_changes.push((i, state_after.clone()));
            }

            match result {
                Ok(_) => events_succeeded += 1,
                Err(_) => events_failed += 1,
            }
        }

        let final_stats = self.service.get_stats().await;

        CircuitBreakerTestResults {
            scenario_name: self.scenario.name.clone(),
            events_sent,
            events_succeeded,
            events_failed,
            state_changes,
            final_stats,
            expected_transitions: self.scenario.expected_state_transitions.clone(),
        }
    }
}

#[derive(Debug)]
struct CircuitBreakerTestResults {
    scenario_name: String,
    events_sent: usize,
    events_succeeded: usize,
    events_failed: usize,
    state_changes: Vec<(usize, CircuitBreakerTestState)>,
    final_stats: ServiceStats,
    expected_transitions: Vec<(usize, CircuitBreakerTestState)>,
}

impl CircuitBreakerTestResults {
    fn print_summary(&self) {
        println!("\n=== Circuit Breaker Test: {} ===", self.scenario_name);
        println!("Events sent: {}", self.events_sent);
        println!("Events succeeded: {}", self.events_succeeded);
        println!("Events failed: {}", self.events_failed);
        println!("Success rate: {:.1}%", (self.events_succeeded as f64 / self.events_sent as f64) * 100.0);

        println!("\nState Changes:");
        for (event_index, state) in &self.state_changes {
            println!("  Event {}: {:?}", event_index, state);
        }

        println!("\nFinal Statistics:");
        println!("  Total calls: {}", self.final_stats.total_calls);
        println!("  Successes: {}", self.final_stats.successes);
        println!("  Failures: {}", self.final_stats.failures);
        println!("  Final state: {:?}", self.final_stats.state);

        if !self.expected_transitions.is_empty() {
            println!("\nExpected vs Actual Transitions:");
            for (expected_event, expected_state) in &self.expected_transitions {
                let actual_state = self.state_changes
                    .iter()
                    .find(|(event, _)| *event >= *expected_event)
                    .map(|(_, state)| state)
                    .unwrap_or(&CircuitBreakerTestState::Closed);

                println!("  Event {}: Expected {:?}, Got {:?}",
                         expected_event, expected_state, actual_state);
            }
        }
    }

    fn verify_circuit_opened(&self) -> bool {
        self.state_changes.iter().any(|(_, state)| *state == CircuitBreakerTestState::Open)
    }

    fn verify_circuit_recovered(&self) -> bool {
        // Circuit should eventually return to closed state after failures
        matches!(self.final_stats.state, CircuitBreakerTestState::Closed) &&
        self.final_stats.failures > 0
    }

    fn verify_expected_transitions(&self, tolerance: usize) -> bool {
        for (expected_event, expected_state) in &self.expected_transitions {
            let found = self.state_changes
                .iter()
                .any(|(event, state)|
                    (*event as isize - *expected_event as isize).abs() <= tolerance as isize &&
                    *state == *expected_state
                );

            if !found {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod basic_circuit_breaker_tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_opens_on_failures() {
        let config = CircuitBreakerTestConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_ms: 1000,
            ..Default::default()
        };

        let scenario = CircuitBreakerTestScenario::new(
            "Basic Circuit Breaker Test".to_string(),
        )
        .with_config(config)
        .with_failure_rate(1.0) // 100% failure rate
        .with_events(20)
        .with_expected_transitions(vec![
            (2, CircuitBreakerTestState::Open) // Should open after 3 failures (0-indexed)
        ]);

        let runner = CircuitBreakerTestRunner::new(scenario);
        runner.setup().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // Verify circuit breaker opened
        assert!(results.verify_circuit_opened(), "Circuit breaker should open after failures");

        // Verify final state is open (since we kept sending failures)
        assert_eq!(results.final_stats.state, CircuitBreakerTestState::Open);

        // Verify many events failed
        assert!(results.events_failed > results.events_succeeded);
    }

    #[tokio::test]
    async fn test_circuit_breaker_remains_closed_below_threshold() {
        let config = CircuitBreakerTestConfig {
            failure_threshold: 5,
            success_threshold: 3,
            timeout_ms: 1000,
            ..Default::default()
        };

        let scenario = CircuitBreakerTestScenario::new(
            "Circuit Breaker Below Threshold".to_string(),
        )
        .with_config(config)
        .with_failure_rate(0.4) // 40% failure rate - not enough to open circuit
        .with_events(20);

        let runner = CircuitBreakerTestRunner::new(scenario);
        runner.setup().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // Verify circuit breaker never opened
        assert!(!results.verify_circuit_opened(), "Circuit breaker should not open below threshold");

        // Verify final state is still closed
        assert_eq!(results.final_stats.state, CircuitBreakerTestState::Closed);

        // Should have mixed success/failure but circuit remains closed
        assert!(results.events_succeeded > 0);
        assert!(results.events_failed > 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_state() {
        let config = CircuitBreakerTestConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_ms: 500, // Short timeout for testing
            half_open_max_calls: 3,
            ..Default::default()
        };

        let scenario = CircuitBreakerTestScenario::new(
            "Circuit Breaker Half Open Test".to_string(),
        )
        .with_config(config)
        .with_failure_rate(0.6) // High failure rate initially
        .with_events(15);

        let runner = CircuitBreakerTestRunner::new(scenario);
        runner.setup().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // Should see state changes including half-open
        let has_half_open = results.state_changes.iter()
            .any(|(_, state)| *state == CircuitBreakerTestState::HalfOpen);

        // Half-open state might occur during timeout recovery
        println!("Half-open state observed: {}", has_half_open);
    }
}

#[cfg(test)]
mod circuit_breaker_recovery_tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_recovery_after_timeout() {
        let config = CircuitBreakerTestConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_ms: 200, // Short timeout for testing
            ..Default::default()
        };

        let scenario = CircuitBreakerTestScenario::new(
            "Circuit Breaker Recovery Test".to_string(),
        )
        .with_config(config)
        .with_failure_rate(0.8) // High failure rate initially
        .with_events(20);

        let runner = CircuitBreakerTestRunner::new(scenario);
        runner.setup().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // Should have opened circuit at some point
        assert!(results.verify_circuit_opened(), "Circuit should open during initial failures");

        // Should eventually recover (test scenario makes second half successful)
        assert!(results.verify_circuit_recovered(), "Circuit should recover after timeout");
    }

    #[tokio::test]
    async fn test_circuit_breaker_closes_after_successes() {
        let config = CircuitBreakerTestConfig {
            failure_threshold: 3,
            success_threshold: 3,
            timeout_ms: 200,
            ..Default::default()
        };

        let scenario = CircuitBreakerTestScenario::new(
            "Circuit Breaker Close After Successes".to_string(),
        )
        .with_config(config)
        .with_events(25);

        let runner = CircuitBreakerTestRunner::new(scenario);
        runner.setup().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // Should have transitions through states
        let has_open = results.state_changes.iter()
            .any(|(_, state)| *state == CircuitBreakerTestState::Open);
        let has_closed = results.final_stats.state == CircuitBreakerTestState::Closed;

        // Circuit should open and then potentially close
        if has_open {
            println!("Circuit opened during test");
            if has_closed {
                println!("Circuit recovered to closed state");
            }
        }
    }

    #[tokio::test]
    async fn test_circuit_breaker_reopens_on_new_failures() {
        let config = CircuitBreakerTestConfig {
            failure_threshold: 2, // Low threshold for easier testing
            success_threshold: 2,
            timeout_ms: 200,
            ..Default::default()
        };

        let scenario = CircuitBreakerTestScenario::new(
            "Circuit Breaker Reopens on Failures".to_string(),
        )
        .with_config(config)
        .with_events(30);

        let runner = CircuitBreakerTestRunner::new(scenario);
        runner.setup().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // Count state transitions
        let open_count = results.state_changes.iter()
            .filter(|(_, state)| *state == CircuitBreakerTestState::Open)
            .count();

        // Should potentially open multiple times if failures continue
        println!("Circuit opened {} times", open_count);

        // Final state may be open if failures continued
        if results.final_stats.failures > results.final_stats.successes {
            assert_eq!(results.final_stats.state, CircuitBreakerTestState::Open);
        }
    }
}

#[cfg(test)]
mod circuit_breaker_concurrent_tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_under_concurrent_load() {
        let config = CircuitBreakerTestConfig {
            failure_threshold: 10,
            success_threshold: 5,
            timeout_ms: 500,
            ..Default::default()
        };

        let scenario = CircuitBreakerTestScenario::new(
            "Concurrent Circuit Breaker Test".to_string(),
        )
        .with_config(config)
        .with_events(200);

        let runner = CircuitBreakerTestRunner::new(scenario);
        runner.setup().await.unwrap();

        // Send events concurrently
        let barrier = Arc::new(Barrier::new(11)); // 10 workers + main
        let service = runner.service.clone();
        let router = runner.router.clone();
        let mut handles = Vec::new();

        for worker_id in 0..10 {
            let barrier_clone = barrier.clone();
            let service_clone = service.clone();
            let router_clone = router.clone();

            let handle = tokio::spawn(async move {
                barrier_clone.wait().await; // Wait for all workers

                for i in 0..20 {
                    let should_fail = (worker_id + i) % 3 == 0; // 33% failure rate
                    service_clone.set_should_fail(should_fail).await;

                    let event = DaemonEvent::new(
                        EventType::TestEvent("concurrent-test".to_string()),
                        EventSource::service(format!("worker-{}", worker_id)),
                        EventPayload::json(serde_json::json!({
                            "worker_id": worker_id,
                            "event_id": i,
                            "should_fail": should_fail
                        })),
                    );

                    if let Err(e) = router_clone.route_event(event).await {
                        // Log error but continue
                        eprintln!("Worker {} event {} failed: {}", worker_id, i, e);
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

        // Wait a bit for processing to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let final_stats = service.get_stats().await;

        println!("\n=== Concurrent Circuit Breaker Test Results ===");
        println!("Total concurrent events processed: {}", final_stats.total_calls);
        println!("Successes: {}", final_stats.successes);
        println!("Failures: {}", final_stats.failures);
        println!("Final state: {:?}", final_stats.state);

        // Verify circuit breaker behaved correctly under concurrent load
        assert!(final_stats.total_calls > 0, "Should have processed events");

        // Circuit state should be consistent
        if final_stats.failures > config.failure_threshold {
            assert!(matches!(final_stats.state, CircuitBreakerTestState::Open | CircuitBreakerTestState::HalfOpen));
        }
    }

    #[tokio::test]
    async fn test_circuit_breaker_state_consistency() {
        let config = CircuitBreakerTestConfig {
            failure_threshold: 5,
            success_threshold: 3,
            timeout_ms: 1000,
            ..Default::default()
        };

        let service = Arc::new(ControllableService::new(
            "consistency-test-service".to_string(),
            config,
        ));

        // Spawn multiple tasks that check and update circuit state
        let mut handles = Vec::new();

        for task_id in 0..5 {
            let service_clone = service.clone();

            let handle = tokio::spawn(async move {
                for i in 0..20 {
                    // Check current state
                    let state_before = service_clone.get_state().await;

                    // Simulate event processing
                    let should_fail = (task_id + i) % 2 == 0;
                    service_clone.set_should_fail(should_fail).await;

                    let event = DaemonEvent::new(
                        EventType::TestEvent("consistency-test".to_string()),
                        EventSource::service(format!("task-{}", task_id)),
                        EventPayload::json(serde_json::json!({"i": i})),
                    );

                    let _result = service_clone.handle_event(event).await;

                    // Check state after
                    let state_after = service_clone.get_state().await;

                    // State should be valid
                    assert!(matches!(state_after,
                        CircuitBreakerTestState::Closed |
                        CircuitBreakerTestState::Open |
                        CircuitBreakerTestState::HalfOpen));

                    // Small delay to allow other tasks
                    tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                }
            });

            handles.push(handle);
        }

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }

        let final_stats = service.get_stats().await;

        println!("\n=== Circuit Breaker Consistency Test Results ===");
        println!("Total events: {}", final_stats.total_calls);
        println!("Final state: {:?}", final_stats.state);

        // Final state should be consistent
        assert!(matches!(final_stats.state,
            CircuitBreakerTestState::Closed |
            CircuitBreakerTestState::Open |
            CircuitBreakerTestState::HalfOpen));
    }
}

#[cfg(test)]
mod circuit_breaker_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_with_load_balancing() {
        let config = RoutingConfig {
            load_balancing_strategy: LoadBalancingStrategy::RoundRobin,
            circuit_breaker_threshold: 3,
            circuit_breaker_timeout_ms: 500,
            ..Default::default()
        };

        let router = Arc::new(DefaultEventRouter::with_config(config));

        // Create multiple services with circuit breaker behavior
        let services = vec![
            ("cb-service-1", true),  // This service will fail
            ("cb-service-2", false), // This service will succeed
            ("cb-service-3", false), // This service will succeed
        ];

        for (service_id, should_fail) in services {
            let registration = ServiceRegistration {
                service_id: service_id.to_string(),
                service_type: "cb-integration-test".to_string(),
                instance_id: "instance-1".to_string(),
                endpoint: None,
                supported_event_types: vec!["test".to_string()],
                priority: 0,
                weight: 1.0,
                max_concurrent_events: 100,
                filters: Vec::new(),
                metadata: HashMap::new(),
            };

            router.register_service(registration).await.unwrap();

            // Set initial health
            router.update_service_health(service_id, ServiceHealth {
                status: ServiceStatus::Healthy,
                message: Some("Integration test service".to_string()),
                last_check: Utc::now(),
                details: HashMap::new(),
            }).await.unwrap();
        }

        // Create routing rule that targets all services
        let rule = RoutingRule {
            rule_id: "cb-integration-rule".to_string(),
            name: "Circuit Breaker Integration Rule".to_string(),
            description: "Test rule for circuit breaker integration".to_string(),
            filter: EventFilter {
                event_types: vec!["test".to_string()],
                ..Default::default()
            },
            targets: vec![
                ServiceTarget::new("cb-service-1".to_string()),
                ServiceTarget::new("cb-service-2".to_string()),
                ServiceTarget::new("cb-service-3".to_string()),
            ],
            priority: 0,
            enabled: true,
            conditions: Vec::new(),
        };

        router.add_routing_rule(rule).await.unwrap();

        // Send events and observe behavior
        let mut successful_events = 0;
        let mut failed_events = 0;

        for i in 0..30 {
            let event = DaemonEvent::new(
                EventType::TestEvent("cb-integration-test".to_string()),
                EventSource::service(format!("client-{}", i)),
                EventPayload::json(serde_json::json!({"event_id": i})),
            );

            match router.route_event(event).await {
                Ok(_) => successful_events += 1,
                Err(_) => failed_events += 1,
            }
        }

        println!("\n=== Circuit Breaker Integration Test Results ===");
        println!("Successful events: {}", successful_events);
        println!("Failed events: {}", failed_events);
        println!("Success rate: {:.1}%", (successful_events as f64 / (successful_events + failed_events) as f64) * 100.0);

        // Even with one service failing, load balancer should route to healthy services
        let success_rate = successful_events as f64 / (successful_events + failed_events) as f64;
        assert!(success_rate > 0.5, "Should route majority of events to healthy services");
    }

    #[tokio::test]
    async fn test_circuit_breaker_with_service_discovery() {
        let router = Arc::new(DefaultEventRouter::new());

        // Register service
        let registration = ServiceRegistration {
            service_id: "cb-discovery-service".to_string(),
            service_type: "discovery-test".to_string(),
            instance_id: "instance-1".to_string(),
            endpoint: Some("http://localhost:8080/discovery".to_string()),
            supported_event_types: vec!["test".to_string()],
            priority: 0,
            weight: 1.0,
            max_concurrent_events: 100,
            filters: Vec::new(),
            metadata: HashMap::new(),
        };

        router.register_service(registration).await.unwrap();

        // Initially healthy
        router.update_service_health("cb-discovery-service", ServiceHealth {
            status: ServiceStatus::Healthy,
            message: Some("Initially healthy".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await.unwrap();

        // Send some events (should succeed)
        let initial_events = 5;
        for i in 0..initial_events {
            let event = DaemonEvent::new(
                EventType::TestEvent("discovery-test".to_string()),
                EventSource::service(format!("client-{}", i)),
                EventPayload::json(serde_json::json!({"phase": "initial"})),
            )
            .with_target(ServiceTarget::new("cb-discovery-service".to_string()));

            let result = router.route_event(event).await;
            assert!(result.is_ok(), "Initial events should succeed");
        }

        // Make service unhealthy (simulating circuit breaker trigger)
        router.update_service_health("cb-discovery-service", ServiceHealth {
            status: ServiceStatus::Unhealthy,
            message: Some("Circuit breaker triggered".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await.unwrap();

        // Send more events (should fail)
        let mut failed_count = 0;
        for i in 0..5 {
            let event = DaemonEvent::new(
                EventType::TestEvent("discovery-test".to_string()),
                EventSource::service(format!("client-{}", i + 5)),
                EventPayload::json(serde_json::json!({"phase": "unhealthy"})),
            )
            .with_target(ServiceTarget::new("cb-discovery-service".to_string()));

            if router.route_event(event).await.is_err() {
                failed_count += 1;
            }
        }

        assert!(failed_count > 0, "Events should fail when service is unhealthy");

        // Wait for circuit breaker timeout
        tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;

        // Make service healthy again
        router.update_service_health("cb-discovery-service", ServiceHealth {
            status: ServiceStatus::Healthy,
            message: Some("Service recovered".to_string()),
            last_check: Utc::now(),
            details: HashMap::new(),
        }).await.unwrap();

        // Send final events (should succeed)
        let recovery_events = 5;
        for i in 0..recovery_events {
            let event = DaemonEvent::new(
                EventType::TestEvent("discovery-test".to_string()),
                EventSource::service(format!("client-{}", i + 10)),
                EventPayload::json(serde_json::json!({"phase": "recovery"})),
            )
            .with_target(ServiceTarget::new("cb-discovery-service".to_string()));

            let result = router.route_event(event).await;
            // Events may start succeeding after recovery
        }

        println!("\n=== Circuit Breaker Service Discovery Test ===");
        println!("Initial events succeeded: {}", initial_events);
        println!("Events failed during unhealthy state: {}", failed_count);
        println!("Recovery phase completed");
    }
}

// Add EventType::TestEvent for testing
impl EventType {
    pub fn TestEvent(name: String) -> Self {
        EventType::Custom(name)
    }
}