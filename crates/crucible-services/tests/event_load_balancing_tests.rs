//! Comprehensive load balancing tests for all routing strategies

use crucible_services::events::core::*;
use crucible_services::events::routing::*;
use crucible_services::events::errors::EventResult;
use crucible_services::types::{ServiceHealth, ServiceStatus};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Test service tracker for load balancing verification
struct ServiceEventTracker {
    service_id: String,
    events_received: Arc<AtomicUsize>,
    current_connections: Arc<AtomicUsize>,
    max_connections: usize,
    health_status: Arc<RwLock<ServiceStatus>>,
    weight: f64,
    priority: u8,
}

impl ServiceEventTracker {
    fn new(service_id: String, max_connections: usize, weight: f64, priority: u8) -> Self {
        Self {
            service_id,
            events_received: Arc::new(AtomicUsize::new(0)),
            current_connections: Arc::new(AtomicUsize::new(0)),
            max_connections,
            health_status: Arc::new(RwLock::new(ServiceStatus::Healthy)),
            weight,
            priority,
        }
    }

    async fn simulate_event_processing(&self) -> EventResult<()> {
        // Check health status
        let health = *self.health_status.read().await;
        if health != ServiceStatus::Healthy {
            return Err(EventError::delivery_error(
                self.service_id.clone(),
                format!("Service is {:?}", health),
            ));
        }

        // Check connection limit
        let current = self.current_connections.fetch_add(1, Ordering::SeqCst);
        if current >= self.max_connections {
            self.current_connections.fetch_sub(1, Ordering::SeqCst);
            return Err(EventError::delivery_error(
                self.service_id.clone(),
                "Connection limit exceeded".to_string(),
            ));
        }

        // Record event
        self.events_received.fetch_add(1, Ordering::SeqCst);

        // Simulate processing time
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Release connection
        self.current_connections.fetch_sub(1, Ordering::SeqCst);

        Ok(())
    }

    async fn get_event_count(&self) -> usize {
        self.events_received.load(Ordering::SeqCst)
    }

    async fn get_current_connections(&self) -> usize {
        self.current_connections.load(Ordering::SeqCst)
    }

    async fn set_health_status(&self, status: ServiceStatus) {
        *self.health_status.write().await = status;
    }

    async fn get_health_status(&self) -> ServiceStatus {
        *self.health_status.read().await
    }
}

/// Load balancing test scenario
struct LoadBalancingTestScenario {
    name: String,
    strategy: LoadBalancingStrategy,
    service_configs: Vec<ServiceConfig>,
    event_count: usize,
    expected_distribution: Option<Vec<f64>>, // Expected percentage distribution
}

#[derive(Clone)]
struct ServiceConfig {
    id: String,
    weight: f64,
    priority: u8,
    max_connections: usize,
    initial_health: ServiceStatus,
}

impl LoadBalancingTestScenario {
    fn new(name: String, strategy: LoadBalancingStrategy) -> Self {
        Self {
            name,
            strategy,
            service_configs: Vec::new(),
            event_count: 1000,
            expected_distribution: None,
        }
    }

    fn with_services(mut self, configs: Vec<ServiceConfig>) -> Self {
        self.service_configs = configs;
        self
    }

    fn with_event_count(mut self, count: usize) -> Self {
        self.event_count = count;
        self
    }

    fn with_expected_distribution(mut self, distribution: Vec<f64>) -> Self {
        self.expected_distribution = Some(distribution);
        self
    }
}

/// Load balancing test runner
struct LoadBalancingTestRunner {
    scenario: LoadBalancingTestScenario,
    router: Arc<DefaultEventRouter>,
    service_trackers: HashMap<String, ServiceEventTracker>,
}

impl LoadBalancingTestRunner {
    fn new(scenario: LoadBalancingTestScenario) -> Self {
        let config = RoutingConfig {
            load_balancing_strategy: scenario.strategy.clone(),
            ..Default::default()
        };

        let router = Arc::new(DefaultEventRouter::with_config(config));
        let mut service_trackers = HashMap::new();

        Self {
            scenario,
            router,
            service_trackers,
        }
    }

    async fn setup_services(&mut self) -> EventResult<()> {
        for service_config in &self.scenario.service_configs {
            // Create service tracker
            let tracker = ServiceEventTracker::new(
                service_config.id.clone(),
                service_config.max_connections,
                service_config.weight,
                service_config.priority,
            );

            // Set initial health
            tracker.set_health_status(service_config.initial_health.clone()).await;

            self.service_trackers.insert(service_config.id.clone(), tracker);

            // Register service with router
            let registration = ServiceRegistration {
                service_id: service_config.id.clone(),
                service_type: "load-balance-test".to_string(),
                instance_id: format!("{}-instance-1", service_config.id),
                endpoint: Some(format!("http://localhost:8080/{}", service_config.id)),
                supported_event_types: vec!["test".to_string(), "custom".to_string()],
                priority: service_config.priority,
                weight: service_config.weight,
                max_concurrent_events: service_config.max_connections,
                filters: Vec::new(),
                metadata: HashMap::new(),
            };

            self.router.register_service(registration).await?;

            // Set initial health in router
            self.router.update_service_health(&service_config.id, ServiceHealth {
                status: service_config.initial_health.clone(),
                message: Some(format!("Initial status: {:?}", service_config.initial_health)),
                last_check: Utc::now(),
                details: HashMap::new(),
            }).await?;
        }

        Ok(())
    }

    async fn setup_routing_rules(&self) -> EventResult<()> {
        // Create a routing rule that targets all test services
        let targets: Vec<ServiceTarget> = self.scenario.service_configs
            .iter()
            .map(|config| ServiceTarget::new(config.id.clone()))
            .collect();

        let rule = RoutingRule {
            rule_id: "load-balance-test-rule".to_string(),
            name: "Load Balance Test Rule".to_string(),
            description: "Rule for load balancing tests".to_string(),
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

    async fn run_test(&self) -> LoadBalancingTestResults {
        let start_time = std::time::Instant::now();

        // Route events
        for i in 0..self.scenario.event_count {
            let event = DaemonEvent::new(
                EventType::TestEvent("load-balance-test".to_string()),
                EventSource::service(format!("test-client-{}", i % 10)),
                EventPayload::json(serde_json::json!({
                    "event_id": i,
                    "timestamp": Utc::now().to_rfc3339(),
                    "test_type": "load_balancing"
                })),
            );

            if let Err(e) = self.router.route_event(event).await {
                eprintln!("Failed to route event {}: {}", i, e);
            }
        }

        // Wait for events to be processed
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let duration = start_time.elapsed();

        // Collect results
        let mut distribution = HashMap::new();
        let mut total_events = 0;

        for (service_id, tracker) in &self.service_trackers {
            let event_count = tracker.get_event_count().await;
            distribution.insert(service_id.clone(), event_count);
            total_events += event_count;
        }

        // Calculate distribution percentages
        let mut percentages = HashMap::new();
        if total_events > 0 {
            for (service_id, count) in &distribution {
                percentages.insert(service_id.clone(), *count as f64 / total_events as f64 * 100.0);
            }
        }

        LoadBalancingTestResults {
            scenario_name: self.scenario.name.clone(),
            strategy: self.scenario.strategy.clone(),
            event_count: self.scenario.event_count,
            successful_events: total_events,
            duration,
            distribution,
            percentages,
            expected_distribution: self.scenario.expected_distribution.clone(),
        }
    }

    async fn simulate_service_health_changes(&self) {
        // Simulate some health changes during the test for health-based routing
        if matches!(self.scenario.strategy, LoadBalancingStrategy::HealthBased) {
            // Make one service unhealthy halfway through
            if let Some((service_id, _)) = self.service_trackers.iter().next() {
                if let Some(tracker) = self.service_trackers.get(service_id) {
                    tracker.set_health_status(ServiceStatus::Unhealthy).await;

                    self.router.update_service_health(service_id, ServiceHealth {
                        status: ServiceStatus::Unhealthy,
                        message: Some("Simulated health degradation".to_string()),
                        last_check: Utc::now(),
                        details: HashMap::new(),
                    }).await.ok();
                }
            }
        }
    }
}

#[derive(Debug)]
struct LoadBalancingTestResults {
    scenario_name: String,
    strategy: LoadBalancingStrategy,
    event_count: usize,
    successful_events: usize,
    duration: std::time::Duration,
    distribution: HashMap<String, usize>,
    percentages: HashMap<String, f64>,
    expected_distribution: Option<Vec<f64>>,
}

impl LoadBalancingTestResults {
    fn print_summary(&self) {
        println!("\n=== Load Balancing Test: {} ===", self.scenario_name);
        println!("Strategy: {:?}", self.strategy);
        println!("Events routed: {} / {}", self.successful_events, self.event_count);
        println!("Duration: {:?}", self.duration);
        println!("Events/sec: {:.2}", self.successful_events as f64 / self.duration.as_secs_f64());

        println!("\nEvent Distribution:");
        for (service_id, count) in &self.distribution {
            let percentage = self.percentages.get(service_id).unwrap_or(&0.0);
            println!("  {}: {} events ({:.1}%)", service_id, count, percentage);
        }

        if let Some(expected) = &self.expected_distribution {
            println!("\nExpected vs Actual Distribution:");
            let actual_percentages: Vec<f64> = self.scenario.service_configs
                .iter()
                .map(|config| *self.percentages.get(&config.id).unwrap_or(&0.0))
                .collect();

            for (i, service_config) in self.scenario.service_configs.iter().enumerate() {
                let actual = actual_percentages.get(i).unwrap_or(&0.0);
                let expected = expected.get(i).unwrap_or(&0.0);
                let diff = (actual - expected).abs();
                println!("  {}: Expected {:.1}%, Got {:.1}% (diff: {:.1}%)",
                         service_config.id, expected, actual, diff);
            }
        }
    }

    fn verify_distribution(&self, tolerance: f64) -> bool {
        if let Some(expected) = &self.expected_distribution {
            let actual_percentages: Vec<f64> = self.scenario.service_configs
                .iter()
                .map(|config| *self.percentages.get(&config.id).unwrap_or(&0.0))
                .collect();

            for (i, expected_pct) in expected.iter().enumerate() {
                let actual_pct = actual_percentages.get(i).unwrap_or(&0.0);
                if (actual_pct - expected_pct).abs() > tolerance {
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod round_robin_tests {
    use super::*;

    #[tokio::test]
    async fn test_round_robin_basic_distribution() {
        let scenario = LoadBalancingTestScenario::new(
            "Round Robin Basic".to_string(),
            LoadBalancingStrategy::RoundRobin,
        )
        .with_services(vec![
            ServiceConfig {
                id: "rr-service-1".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "rr-service-2".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "rr-service-3".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
        ])
        .with_event_count(300) // Divisible by 3 for even distribution
        .with_expected_distribution(vec![33.3, 33.3, 33.4]); // Allow small variance

        let mut runner = LoadBalancingTestRunner::new(scenario);
        runner.setup_services().await.unwrap();
        runner.setup_routing_rules().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // Verify even distribution (within tolerance)
        assert!(results.verify_distribution(5.0), "Round robin should distribute events evenly");
        assert_eq!(results.successful_events, 300, "All events should be routed successfully");
    }

    #[tokio::test]
    async fn test_round_robin_with_unhealthy_service() {
        let scenario = LoadBalancingTestScenario::new(
            "Round Robin with Unhealthy Service".to_string(),
            LoadBalancingStrategy::RoundRobin,
        )
        .with_services(vec![
            ServiceConfig {
                id: "rr-healthy-1".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "rr-unhealthy".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Unhealthy,
            },
            ServiceConfig {
                id: "rr-healthy-2".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
        ])
        .with_event_count(200)
        .with_expected_distribution(vec![50.0, 0.0, 50.0]); // Unhealthy service should get no events

        let mut runner = LoadBalancingTestRunner::new(scenario);
        runner.setup_services().await.unwrap();
        runner.setup_routing_rules().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // Verify unhealthy service gets no events
        assert_eq!(results.distribution.get("rr-unhealthy").unwrap_or(&0), &0);
        // Verify healthy services share the load
        assert!(results.verify_distribution(5.0));
    }

    #[tokio::test]
    async fn test_round_robin_with_single_service() {
        let scenario = LoadBalancingTestScenario::new(
            "Round Robin Single Service".to_string(),
            LoadBalancingStrategy::RoundRobin,
        )
        .with_services(vec![
            ServiceConfig {
                id: "rr-single".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
        ])
        .with_event_count(100)
        .with_expected_distribution(vec![100.0]);

        let mut runner = LoadBalancingTestRunner::new(scenario);
        runner.setup_services().await.unwrap();
        runner.setup_routing_rules().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        assert_eq!(results.successful_events, 100);
        assert_eq!(results.distribution.get("rr-single").unwrap_or(&0), &100);
    }
}

#[cfg(test)]
mod least_connections_tests {
    use super::*;

    #[tokio::test]
    async fn test_least_connections_basic() {
        let scenario = LoadBalancingTestScenario::new(
            "Least Connections Basic".to_string(),
            LoadBalancingStrategy::LeastConnections,
        )
        .with_services(vec![
            ServiceConfig {
                id: "lc-service-1".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 50, // Limited connections
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "lc-service-2".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100, // More connections available
                initial_health: ServiceStatus::Healthy,
            },
        ])
        .with_event_count(200);

        let mut runner = LoadBalancingTestRunner::new(scenario);
        runner.setup_services().await.unwrap();
        runner.setup_routing_rules().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // Service with more available connections should get more events
        let service1_events = results.distribution.get("lc-service-1").unwrap_or(&0);
        let service2_events = results.distribution.get("lc-service-2").unwrap_or(&0);

        assert!(service2_events > service1_events, "Service with more capacity should get more events");
        assert_eq!(results.successful_events, 200);
    }

    #[tokio::test]
    async fn test_least_connections_with_varied_load() {
        let scenario = LoadBalancingTestScenario::new(
            "Least Connections Varied Load".to_string(),
            LoadBalancingStrategy::LeastConnections,
        )
        .with_services(vec![
            ServiceConfig {
                id: "lc-low-load".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 200, // High capacity
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "lc-medium-load".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100, // Medium capacity
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "lc-high-load".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 50, // Low capacity
                initial_health: ServiceStatus::Healthy,
            },
        ])
        .with_event_count(300);

        let mut runner = LoadBalancingTestRunner::new(scenario);
        runner.setup_services().await.unwrap();
        runner.setup_routing_rules().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // Services should be prioritized by available capacity
        let low_load_events = results.distribution.get("lc-low-load").unwrap_or(&0);
        let medium_load_events = results.distribution.get("lc-medium-load").unwrap_or(&0);
        let high_load_events = results.distribution.get("lc-high-load").unwrap_or(&0);

        assert!(low_load_events >= medium_load_events);
        assert!(medium_load_events >= high_load_events);
    }
}

#[cfg(test)]
mod weighted_random_tests {
    use super::*;

    #[tokio::test]
    async fn test_weighted_random_distribution() {
        let scenario = LoadBalancingTestScenario::new(
            "Weighted Random Distribution".to_string(),
            LoadBalancingStrategy::WeightedRandom,
        )
        .with_services(vec![
            ServiceConfig {
                id: "wr-light".to_string(),
                weight: 0.2, // 20% weight
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "wr-medium".to_string(),
                weight: 0.3, // 30% weight
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "wr-heavy".to_string(),
                weight: 0.5, // 50% weight
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
        ])
        .with_event_count(1000) // Large sample for statistical significance
        .with_expected_distribution(vec![20.0, 30.0, 50.0]); // Expected based on weights

        let mut runner = LoadBalancingTestRunner::new(scenario);
        runner.setup_services().await.unwrap();
        runner.setup_routing_rules().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // Verify weighted distribution (with higher tolerance due to randomness)
        assert!(results.verify_distribution(15.0), "Weighted random should follow weight distribution");

        // Verify heavier weight service gets more events
        let light_events = results.distribution.get("wr-light").unwrap_or(&0);
        let medium_events = results.distribution.get("wr-medium").unwrap_or(&0);
        let heavy_events = results.distribution.get("wr-heavy").unwrap_or(&0);

        assert!(heavy_events > medium_events);
        assert!(medium_events > light_events);
    }

    #[tokio::test]
    async fn test_weighted_random_equal_weights() {
        let scenario = LoadBalancingTestScenario::new(
            "Weighted Random Equal Weights".to_string(),
            LoadBalancingStrategy::WeightedRandom,
        )
        .with_services(vec![
            ServiceConfig {
                id: "wr-equal-1".to_string(),
                weight: 1.0, // Equal weights
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "wr-equal-2".to_string(),
                weight: 1.0, // Equal weights
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
        ])
        .with_event_count(1000)
        .with_expected_distribution(vec![50.0, 50.0]); // Should be roughly equal

        let mut runner = LoadBalancingTestRunner::new(scenario);
        runner.setup_services().await.unwrap();
        runner.setup_routing_rules().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // With equal weights, distribution should be roughly equal
        assert!(results.verify_distribution(20.0), "Equal weights should result in similar distribution");
    }
}

#[cfg(test)]
mod health_based_tests {
    use super::*;

    #[tokio::test]
    async fn test_health_based_prefer_healthy() {
        let scenario = LoadBalancingTestScenario::new(
            "Health Based Prefer Healthy".to_string(),
            LoadBalancingStrategy::HealthBased,
        )
        .with_services(vec![
            ServiceConfig {
                id: "hb-healthy".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "hb-degraded".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Degraded,
            },
            ServiceConfig {
                id: "hb-unhealthy".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Unhealthy,
            },
        ])
        .with_event_count(200)
        .with_expected_distribution(vec![100.0, 0.0, 0.0]); // Only healthy should get events

        let mut runner = LoadBalancingTestRunner::new(scenario);
        runner.setup_services().await.unwrap();
        runner.setup_routing_rules().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // Only healthy services should get events
        assert_eq!(results.distribution.get("hb-healthy").unwrap_or(&0), &200);
        assert_eq!(results.distribution.get("hb-degraded").unwrap_or(&0), &0);
        assert_eq!(results.distribution.get("hb-unhealthy").unwrap_or(&0), &0);
    }

    #[tokio::test]
    async fn test_health_based_fallback_to_degraded() {
        let scenario = LoadBalancingTestScenario::new(
            "Health Based Fallback to Degraded".to_string(),
            LoadBalancingStrategy::HealthBased,
        )
        .with_services(vec![
            ServiceConfig {
                id: "hb-unhealthy-only".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Unhealthy,
            },
            ServiceConfig {
                id: "hb-degraded-only".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Degraded,
            },
        ])
        .with_event_count(200)
        .with_expected_distribution(vec![0.0, 100.0]); // Only degraded should get events

        let mut runner = LoadBalancingTestRunner::new(scenario);
        runner.setup_services().await.unwrap();
        runner.setup_routing_rules().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // When no healthy services, should fallback to degraded
        assert_eq!(results.distribution.get("hb-unhealthy-only").unwrap_or(&0), &0);
        assert_eq!(results.distribution.get("hb-degraded-only").unwrap_or(&0), &200);
    }

    #[tokio::test]
    async fn test_health_based_multiple_healthy() {
        let scenario = LoadBalancingTestScenario::new(
            "Health Based Multiple Healthy".to_string(),
            LoadBalancingStrategy::HealthBased,
        )
        .with_services(vec![
            ServiceConfig {
                id: "hb-healthy-1".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "hb-healthy-2".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "hb-unhealthy".to_string(),
                weight: 1.0,
                priority: 0,
                max_connections: 100,
                initial_health: ServiceStatus::Unhealthy,
            },
        ])
        .with_event_count(300)
        .with_expected_distribution(vec![50.0, 50.0, 0.0]); // Should distribute between healthy services

        let mut runner = LoadBalancingTestRunner::new(scenario);
        runner.setup_services().await.unwrap();
        runner.setup_routing_rules().await.unwrap();

        let results = runner.run_test().await;
        results.print_summary();

        // Should distribute between healthy services only
        assert_eq!(results.distribution.get("hb-unhealthy").unwrap_or(&0), &0);

        let healthy1_events = results.distribution.get("hb-healthy-1").unwrap_or(&0);
        let healthy2_events = results.distribution.get("hb-healthy-2").unwrap_or(&0);
        assert_eq!(healthy1_events + healthy2_events, 300);

        // Should be roughly even between healthy services
        let diff = (*healthy1_events as i32 - *healthy2_events as i32).abs();
        assert!(diff < 100, "Healthy services should have roughly equal load");
    }
}

#[cfg(test)]
mod priority_based_tests {
    use super::*;

    #[tokio::test]
    async fn test_priority_based_high_priority_events() {
        let scenario = LoadBalancingTestScenario::new(
            "Priority Based High Priority Events".to_string(),
            LoadBalancingStrategy::PriorityBased,
        )
        .with_services(vec![
            ServiceConfig {
                id: "pb-high-priority".to_string(),
                weight: 1.0,
                priority: 1, // High priority (lower number = higher priority)
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "pb-low-priority".to_string(),
                weight: 1.0,
                priority: 10, // Low priority (higher number = lower priority)
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
        ])
        .with_event_count(200)
        .with_expected_distribution(vec![100.0, 0.0]); // High priority service should get all events

        let mut runner = LoadBalancingTestRunner::new(scenario);
        runner.setup_services().await.unwrap();
        runner.setup_routing_rules().await.unwrap();

        // Create high priority events
        let event_count = scenario.event_count;
        for i in 0..event_count {
            let event = DaemonEvent::new(
                EventType::TestEvent("priority-test".to_string()),
                EventSource::service(format!("client-{}", i)),
                EventPayload::json(serde_json::json!({"event_id": i})),
            )
            .with_priority(EventPriority::Critical); // High priority

            runner.router.route_event(event).await.ok();
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let results = runner.run_test().await;
        results.print_summary();

        // High priority events should go to high priority service
        let high_priority_events = runner.service_trackers
            .get("pb-high-priority")
            .unwrap()
            .get_event_count().await;
        let low_priority_events = runner.service_trackers
            .get("pb-low-priority")
            .unwrap()
            .get_event_count().await;

        assert!(high_priority_events > low_priority_events);
    }

    #[tokio::test]
    async fn test_priority_based_normal_priority_events() {
        let scenario = LoadBalancingTestScenario::new(
            "Priority Based Normal Priority Events".to_string(),
            LoadBalancingStrategy::PriorityBased,
        )
        .with_services(vec![
            ServiceConfig {
                id: "pb-service-1".to_string(),
                weight: 1.0,
                priority: 1,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "pb-service-2".to_string(),
                weight: 1.0,
                priority: 2,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
        ])
        .with_event_count(200);

        let mut runner = LoadBalancingTestRunner::new(scenario);
        runner.setup_services().await.unwrap();
        runner.setup_routing_rules().await.unwrap();

        // Create normal priority events
        let event_count = scenario.event_count;
        for i in 0..event_count {
            let event = DaemonEvent::new(
                EventType::TestEvent("priority-normal-test".to_string()),
                EventSource::service(format!("client-{}", i)),
                EventPayload::json(serde_json::json!({"event_id": i})),
            )
            .with_priority(EventPriority::Normal); // Normal priority

            runner.router.route_event(event).await.ok();
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let results = runner.run_test().await;
        results.print_summary();

        // Normal priority events can go to any service, but should prefer higher priority
        let service1_events = runner.service_trackers
            .get("pb-service-1")
            .unwrap()
            .get_event_count().await;
        let service2_events = runner.service_trackers
            .get("pb-service-2")
            .unwrap()
            .get_event_count().await;

        assert!(service1_events + service2_events > 0);
        // May distribute between both services for normal priority events
    }
}

#[cfg(test)]
mod load_balancing_comparative_tests {
    use super::*;

    #[tokio::test]
    async fn compare_all_strategies() {
        println!("\n=== Load Balancing Strategy Comparison ===");

        let service_configs = vec![
            ServiceConfig {
                id: "comp-service-1".to_string(),
                weight: 0.5,
                priority: 1,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "comp-service-2".to_string(),
                weight: 1.0,
                priority: 2,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "comp-service-3".to_string(),
                weight: 1.5,
                priority: 3,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
        ];

        let strategies = vec![
            LoadBalancingStrategy::RoundRobin,
            LoadBalancingStrategy::LeastConnections,
            LoadBalancingStrategy::WeightedRandom,
            LoadBalancingStrategy::HealthBased,
            LoadBalancingStrategy::PriorityBased,
        ];

        for strategy in strategies {
            let scenario = LoadBalancingTestScenario::new(
                format!("Strategy Comparison: {:?}", strategy),
                strategy,
            )
            .with_services(service_configs.clone())
            .with_event_count(600); // 200 events per service expected

            let mut runner = LoadBalancingTestRunner::new(scenario);
            runner.setup_services().await.unwrap();
            runner.setup_routing_rules().await.unwrap();

            let results = runner.run_test().await;
            results.print_summary();

            // All strategies should route all events successfully
            assert_eq!(results.successful_events, 600);
            assert!(results.duration.as_millis() < 5000, "All strategies should complete in reasonable time");
        }
    }

    #[tokio::test]
    async fn test_load_balancing_under_stress() {
        println!("\n=== Load Balancing Under Stress ===");

        let service_configs = vec![
            ServiceConfig {
                id: "stress-service-1".to_string(),
                weight: 1.0,
                priority: 1,
                max_connections: 50, // Limited capacity
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "stress-service-2".to_string(),
                weight: 1.0,
                priority: 2,
                max_connections: 50, // Limited capacity
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "stress-service-3".to_string(),
                weight: 1.0,
                priority: 3,
                max_connections: 50, // Limited capacity
                initial_health: ServiceStatus::Healthy,
            },
        ];

        let strategies = vec![
            LoadBalancingStrategy::RoundRobin,
            LoadBalancingStrategy::LeastConnections,
        ];

        for strategy in strategies {
            let scenario = LoadBalancingTestScenario::new(
                format!("Stress Test: {:?}", strategy),
                strategy,
            )
            .with_services(service_configs.clone())
            .with_event_count(3000); // High volume with limited capacity

            let mut runner = LoadBalancingTestRunner::new(scenario);
            runner.setup_services().await.unwrap();
            runner.setup_routing_rules().await.unwrap();

            let results = runner.run_test().await;
            results.print_summary();

            // Under stress, some events may fail due to capacity limits
            let success_rate = results.successful_events as f64 / results.event_count as f64;
            assert!(success_rate > 0.7, "Success rate should be > 70% even under stress");

            // Verify events are distributed across services
            let mut services_with_events = 0;
            for (_, count) in &results.distribution {
                if *count > 0 {
                    services_with_events += 1;
                }
            }
            assert!(services_with_events >= 2, "Should use multiple services under stress");
        }
    }

    #[tokio::test]
    async fn test_load_balancing_with_service_failures() {
        println!("\n=== Load Balancing with Service Failures ===");

        // Start with all services healthy
        let service_configs = vec![
            ServiceConfig {
                id: "failure-service-1".to_string(),
                weight: 1.0,
                priority: 1,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "failure-service-2".to_string(),
                weight: 1.0,
                priority: 2,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
            ServiceConfig {
                id: "failure-service-3".to_string(),
                weight: 1.0,
                priority: 3,
                max_connections: 100,
                initial_health: ServiceStatus::Healthy,
            },
        ];

        let scenario = LoadBalancingTestScenario::new(
            "Service Failure Test".to_string(),
            LoadBalancingStrategy::HealthBased,
        )
        .with_services(service_configs)
        .with_event_count(900);

        let mut runner = LoadBalancingTestRunner::new(scenario);
        runner.setup_services().await.unwrap();
        runner.setup_routing_rules().await.unwrap();

        // Make one service unhealthy partway through
        tokio::spawn({
            let service_2 = runner.service_trackers.get("failure-service-2").unwrap().clone();
            let router = runner.router.clone();
            async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

                // Mark service as unhealthy
                service_2.set_health_status(ServiceStatus::Unhealthy).await;
                router.update_service_health("failure-service-2", ServiceHealth {
                    status: ServiceStatus::Unhealthy,
                    message: Some("Simulated failure".to_string()),
                    last_check: Utc::now(),
                    details: HashMap::new(),
                }).await.ok();
            }
        });

        let results = runner.run_test().await;
        results.print_summary();

        // Verify that unhealthy service received fewer events
        let service1_events = results.distribution.get("failure-service-1").unwrap_or(&0);
        let service2_events = results.distribution.get("failure-service-2").unwrap_or(&0);
        let service3_events = results.distribution.get("failure-service-3").unwrap_or(&0);

        assert!(service2_events < service1_events);
        assert!(service2_events < service3_events);

        // Total events should still be distributed to healthy services
        assert!(service1_events + service3_events > 600);
    }
}

// Add EventType::TestEvent for testing
impl EventType {
    pub fn TestEvent(name: String) -> Self {
        EventType::Custom(name)
    }
}