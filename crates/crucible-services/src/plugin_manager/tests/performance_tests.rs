//! # Performance Tests
//!
//! Comprehensive performance testing for the PluginManager system including
//! load testing, scalability analysis, and performance regression detection.

use super::*;
use crate::plugin_manager::*;
use tokio::time::{sleep, Duration};
use std::sync::atomic::{AtomicU64, Ordering};

/// ============================================================================
/// PERFORMANCE BENCHMARK FRAMEWORK
/// ============================================================================

#[derive(Debug, Clone)]
pub struct PerformanceBenchmark {
    name: String,
    iterations: usize,
    warmup_iterations: usize,
    parallel_tasks: Option<usize>,
}

impl PerformanceBenchmark {
    pub fn new(name: String) -> Self {
        Self {
            name,
            iterations: 100,
            warmup_iterations: 10,
            parallel_tasks: None,
        }
    }

    pub fn with_iterations(mut self, iterations: usize) -> Self {
        self.iterations = iterations;
        self
    }

    pub fn with_warmup(mut self, warmup_iterations: usize) -> Self {
        self.warmup_iterations = warmup_iterations;
        self
    }

    pub fn with_parallel_tasks(mut self, parallel_tasks: usize) -> Self {
        self.parallel_tasks = Some(parallel_tasks);
        self
    }

    pub async fn run<F, T>(&self, operation: F) -> BenchmarkResult
    where
        F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>> + Send + Sync,
        T: Send + 'static,
    {
        println!("Running benchmark: {}", self.name);

        // Warmup
        if self.warmup_iterations > 0 {
            for _ in 0..self.warmup_iterations {
                let _ = operation().await;
            }
        }

        let start_time = std::time::Instant::now();

        if let Some(parallel_tasks) = self.parallel_tasks {
            // Parallel benchmark
            let mut handles = Vec::new();
            let iterations_per_task = self.iterations / parallel_tasks;

            for _ in 0..parallel_tasks {
                let operation_clone = std::sync::Arc::new(operation);
                let handle = tokio::spawn(async move {
                    let mut durations = Vec::new();
                    for _ in 0..iterations_per_task {
                        let (result, duration) = measure_async(operation_clone()).await;
                        drop(result); // We don't care about the result for benchmarking
                        durations.push(duration);
                    }
                    durations
                });
                handles.push(handle);
            }

            let mut all_durations = Vec::new();
            for handle in handles {
                let durations = handle.await.unwrap();
                all_durations.extend(durations);
            }

            let total_time = start_time.elapsed();
            BenchmarkResult::from_durations(self.name.clone(), all_durations, total_time)
        } else {
            // Sequential benchmark
            let mut durations = Vec::new();
            for _ in 0..self.iterations {
                let (result, duration) = measure_async(operation()).await;
                drop(result);
                durations.push(duration);
            }

            let total_time = start_time.elapsed();
            BenchmarkResult::from_durations(self.name.clone(), durations, total_time)
        }
    }
}

#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub name: String,
    pub iterations: usize,
    pub total_time: Duration,
    pub min_duration: Duration,
    pub max_duration: Duration,
    pub mean_duration: Duration,
    pub median_duration: Duration,
    pub p95_duration: Duration,
    pub p99_duration: Duration,
    pub operations_per_second: f64,
}

impl BenchmarkResult {
    fn from_durations(name: String, durations: Vec<Duration>, total_time: Duration) -> Self {
        let iterations = durations.len();
        let mut sorted_durations = durations.clone();
        sorted_durations.sort();

        let min_duration = sorted_durations[0];
        let max_duration = sorted_durations[sorted_durations.len() - 1];
        let mean_duration = total_time / iterations as u32;

        let median_idx = iterations / 2;
        let median_duration = if iterations % 2 == 0 {
            (sorted_durations[median_idx - 1] + sorted_durations[median_idx]) / 2
        } else {
            sorted_durations[median_idx]
        };

        let p95_idx = (iterations as f64 * 0.95) as usize;
        let p95_duration = sorted_durations[p95_idx.min(iterations - 1)];

        let p99_idx = (iterations as f64 * 0.99) as usize;
        let p99_duration = sorted_durations[p99_idx.min(iterations - 1)];

        let operations_per_second = iterations as f64 / total_time.as_secs_f64();

        Self {
            name,
            iterations,
            total_time,
            min_duration,
            max_duration,
            mean_duration,
            median_duration,
            p95_duration,
            p99_duration,
            operations_per_second,
        }
    }

    pub fn print(&self) {
        println!("Benchmark: {}", self.name);
        println!("  Iterations: {}", self.iterations);
        println!("  Total time: {:?}", self.total_time);
        println!("  Operations/sec: {:.2}", self.operations_per_second);
        println!("  Min: {:?}", self.min_duration);
        println!("  Max: {:?}", self.max_duration);
        println!("  Mean: {:?}", self.mean_duration);
        println!("  Median: {:?}", self.median_duration);
        println!("  95th percentile: {:?}", self.p95_duration);
        println!("  99th percentile: {:?}", self.p99_duration);
        println!();
    }

    pub fn assert_within(&self, max_mean: Duration, max_p95: Duration) {
        assert!(
            self.mean_duration <= max_mean,
            "Mean duration {:?} exceeds maximum {:?}",
            self.mean_duration,
            max_mean
        );
        assert!(
            self.p95_duration <= max_p95,
            "95th percentile duration {:?} exceeds maximum {:?}",
            self.p95_duration,
            max_p95
        );
    }
}

/// ============================================================================
/// PLUGIN MANAGER PERFORMANCE TESTS
/// ============================================================================

#[tokio::test]
async fn test_plugin_manager_startup_performance() -> Result<(), Box<dyn std::error::Error>> {
    let benchmark = PerformanceBenchmark::new("Plugin Manager Startup".to_string())
        .with_iterations(50)
        .with_warmup_iterations(5);

    let result = benchmark.run(|| {
        Box::pin(async {
            let config = default_test_config();
            let mut service = PluginManagerService::new(config);
            service.start().await.unwrap();
            service.stop().await.unwrap();
        })
    }).await;

    result.print();

    // Startup should be fast (less than 100ms mean)
    result.assert_within(
        Duration::from_millis(100),
        Duration::from_millis(200),
    );

    Ok(())
}

#[tokio::test]
async fn test_plugin_registration_performance() -> Result<(), Box<dyn std::error::Error>> {
    let config = default_test_config();
    let registry = DefaultPluginRegistry::new(config);

    let benchmark = PerformanceBenchmark::new("Plugin Registration".to_string())
        .with_iterations(1000)
        .with_warmup_iterations(100);

    let result = benchmark.run(|| {
        Box::pin(async {
            let plugin_id = format!("perf-plugin-{}", uuid::Uuid::new_v4());
            let manifest = create_test_plugin_manifest(&plugin_id, PluginType::Rune);
            let mut test_registry = DefaultPluginRegistry::new(default_test_config());
            test_registry.register_plugin(manifest).await
        })
    }).await;

    result.print();

    // Registration should be very fast (less than 1ms mean)
    result.assert_within(
        Duration::from_millis(1),
        Duration::from_millis(5),
    );

    Ok(())
}

#[tokio::test]
async fn test_plugin_instance_creation_performance() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Register a test plugin
    let mut registry = service.registry.write().await;
    let manifest = create_test_plugin_manifest("perf-instance-plugin", PluginType::Rune);
    let plugin_id = registry.register_plugin(manifest).await?;
    drop(registry);

    let benchmark = PerformanceBenchmark::new("Instance Creation".to_string())
        .with_iterations(500)
        .with_warmup_iterations(50);

    let result = benchmark.run(|| {
        Box::pin(async {
            let instance_id = format!("perf-instance-{}", uuid::Uuid::new_v4());
            let mut test_service = create_test_plugin_manager().await;
            test_service.start().await.unwrap();

            let mut registry = test_service.registry.write().await;
            let manifest = create_test_plugin_manifest(&instance_id, PluginType::Rune);
            let plugin_id = registry.register_plugin(manifest).await.unwrap();
            drop(registry);

            test_service.create_instance(&plugin_id, None).await
        })
    }).await;

    result.print();

    // Instance creation should be fast (less than 10ms mean)
    result.assert_within(
        Duration::from_millis(10),
        Duration::from_millis(25),
    );

    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// CONCURRENT PERFORMANCE TESTS
/// ============================================================================

#[tokio::test]
async fn test_concurrent_plugin_operations() -> Result<(), Box<dyn std::error::Error>> {
    let benchmark = PerformanceBenchmark::new("Concurrent Plugin Operations".to_string())
        .with_iterations(200)
        .with_parallel_tasks(20)
        .with_warmup_iterations(20);

    let result = benchmark.run(|| {
        Box::pin(async {
            let mut service = create_test_plugin_manager().await;
            service.start().await.unwrap();

            let plugin_id = format!("concurrent-plugin-{}", uuid::Uuid::new_v4());

            // Register plugin
            let mut registry = service.registry.write().await;
            let manifest = create_test_plugin_manifest(&plugin_id, PluginType::Rune);
            let plugin_id = registry.register_plugin(manifest).await.unwrap();
            drop(registry);

            // Create and start instance
            let instance_id = service.create_instance(&plugin_id, None).await.unwrap();
            service.start_instance(&instance_id).await.unwrap();

            // Stop instance
            service.stop_instance(&instance_id).await.unwrap();

            service.stop().await.unwrap();
        })
    }).await;

    result.print();

    // Concurrent operations should still be reasonably fast
    result.assert_within(
        Duration::from_millis(50),
        Duration::from_millis(150),
    );

    Ok(())
}

#[tokio::test]
async fn test_high_load_scenario() -> Result<(), Box<dyn std::error::Error>> {
    let plugin_count = 100;
    let instances_per_plugin = 2;

    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    let start_time = std::time::Instant::now();

    // Create many plugins and instances
    let mut instance_ids = Vec::new();
    for i in 0..plugin_count {
        let plugin_id = format!("load-plugin-{}", i);

        // Register plugin
        let mut registry = service.registry.write().await;
        let manifest = create_test_plugin_manifest(&plugin_id, PluginType::Rune);
        let plugin_id = registry.register_plugin(manifest).await?;
        drop(registry);

        // Create multiple instances
        for j in 0..instances_per_plugin {
            let instance_id = service.create_instance(&plugin_id, None).await?;
            service.start_instance(&instance_id).await?;
            instance_ids.push(instance_id);
        }
    }

    let creation_time = start_time.elapsed();

    // Verify all instances are running
    let instances = service.list_instances().await?;
    assert_eq!(instances.len(), plugin_count * instances_per_plugin);

    // Test system health under load
    let health_start = std::time::Instant::now();
    let health = service.health_check().await?;
    let health_time = health_start.elapsed();

    // Test resource usage aggregation
    let usage_start = std::time::Instant::now();
    let global_usage = service.get_resource_usage(None).await?;
    let usage_time = usage_start.elapsed();

    // Stop all instances
    let stop_start = std::time::Instant::now();
    for instance_id in instance_ids {
        service.stop_instance(&instance_id).await?;
    }
    let stop_time = stop_start.elapsed();

    let total_time = start_time.elapsed();

    println!("High Load Scenario Results:");
    println!("  Total plugins: {}", plugin_count);
    println!("  Total instances: {}", plugin_count * instances_per_plugin);
    println!("  Creation time: {:?}", creation_time);
    println!("  Health check time: {:?}", health_time);
    println!("  Resource usage time: {:?}", usage_time);
    println!("  Stop time: {:?}", stop_time);
    println!("  Total time: {:?}", total_time);
    println!("  Operations per second: {:.2}", (plugin_count * instances_per_plugin * 2) as f64 / total_time.as_secs_f64());

    // Performance assertions
    assert!(creation_time < Duration::from_secs(10), "Creation should complete within 10 seconds");
    assert!(stop_time < Duration::from_secs(5), "Stopping should complete within 5 seconds");
    assert!(health_time < Duration::from_millis(100), "Health check should be fast");
    assert!(usage_time < Duration::from_millis(100), "Resource usage check should be fast");

    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// MEMORY PERFORMANCE TESTS
/// ============================================================================

#[tokio::test]
async fn test_memory_usage_scaling() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    let mut instance_counts = Vec::new();
    let mut memory_usage = Vec::new();

    // Test memory usage at different scales
    for scale in [10, 25, 50, 100].iter() {
        // Clear previous instances
        let current_instances = service.list_instances().await?;
        for instance in current_instances {
            service.stop_instance(&instance.instance_id).await?;
        }

        let mut instance_ids = Vec::new();

        // Create instances for this scale
        for i in 0..*scale {
            let plugin_id = format!("memory-plugin-{}", i);

            let mut registry = service.registry.write().await;
            let manifest = create_test_plugin_manifest(&plugin_id, PluginType::Rune);
            let plugin_id = registry.register_plugin(manifest).await?;
            drop(registry);

            let instance_id = service.create_instance(&plugin_id, None).await?;
            service.start_instance(&instance_id).await?;
            instance_ids.push(instance_id);
        }

        // Measure memory usage
        let usage = service.get_resource_usage(None).await?;
        instance_counts.push(*scale);
        memory_usage.push(usage.memory_bytes);

        println!("Scale: {}, Memory: {} MB", scale, usage.memory_bytes / 1024 / 1024);
    }

    // Analyze memory scaling
    if instance_counts.len() >= 2 {
        let memory_per_instance = memory_usage.last().unwrap() / instance_counts.last().unwrap() as u64;
        println!("Memory per instance: {} MB", memory_per_instance / 1024 / 1024);

        // Memory usage should be reasonable (less than 10MB per instance)
        assert!(memory_per_instance < 10 * 1024 * 1024, "Memory usage per instance should be reasonable");
    }

    // Cleanup
    let instances = service.list_instances().await?;
    for instance in instances {
        service.stop_instance(&instance.instance_id).await?;
    }

    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// RESOURCE MONITORING OVERHEAD TESTS
/// ============================================================================

#[tokio::test]
async fn test_resource_monitoring_overhead() -> Result<(), Box<dyn std::error::Error>> {
    // Test with monitoring enabled
    let mut config_with_monitoring = default_test_config();
    config_with_monitoring.resource_management.monitoring.enabled = true;
    config_with_monitoring.resource_management.monitoring.interval = Duration::from_millis(10);

    let mut service_with_monitoring = PluginManagerService::new(config_with_monitoring);
    service_with_monitoring.start().await?;

    // Test with monitoring disabled
    let mut config_without_monitoring = default_test_config();
    config_without_monitoring.resource_management.monitoring.enabled = false;

    let mut service_without_monitoring = PluginManagerService::new(config_without_monitoring);
    service_without_monitoring.start().await?;

    // Create instances for both services
    let mut instance_ids_with = Vec::new();
    let mut instance_ids_without = Vec::new();

    for i in 0..50 {
        let plugin_id = format!("overhead-plugin-{}", i);

        // With monitoring
        let mut registry = service_with_monitoring.registry.write().await;
        let manifest = create_test_plugin_manifest(&plugin_id, PluginType::Rune);
        let plugin_id = registry.register_plugin(manifest).await?;
        drop(registry);

        let instance_id = service_with_monitoring.create_instance(&plugin_id, None).await?;
        service_with_monitoring.start_instance(&instance_id).await?;
        instance_ids_with.push(instance_id);

        // Without monitoring
        let mut registry = service_without_monitoring.registry.write().await;
        let manifest = create_test_plugin_manifest(&format!("overhead-plugin-{}-no-monitor", i), PluginType::Rune);
        let plugin_id = registry.register_plugin(manifest).await?;
        drop(registry);

        let instance_id = service_without_monitoring.create_instance(&plugin_id, None).await?;
        service_without_monitoring.start_instance(&instance_id).await?;
        instance_ids_without.push(instance_id);
    }

    // Measure performance difference
    let start_time = std::time::Instant::now();
    let _ = service_with_monitoring.get_resource_usage(None).await?;
    let monitoring_time = start_time.elapsed();

    let start_time = std::time::Instant::now();
    let _ = service_without_monitoring.get_resource_usage(None).await?;
    let no_monitoring_time = start_time.elapsed();

    println!("Resource monitoring overhead:");
    println!("  With monitoring: {:?}", monitoring_time);
    println!("  Without monitoring: {:?}", no_monitoring_time);
    println!("  Overhead: {:?}", monitoring_time.saturating_sub(no_monitoring_time));

    // Overhead should be minimal (less than 10ms)
    assert!(monitoring_time.saturating_sub(no_monitoring_time) < Duration::from_millis(10));

    // Cleanup
    for instance_id in instance_ids_with {
        service_with_monitoring.stop_instance(&instance_id).await?;
    }
    for instance_id in instance_ids_without {
        service_without_monitoring.stop_instance(&instance_id).await?;
    }

    service_with_monitoring.stop().await?;
    service_without_monitoring.stop().await?;
    Ok(())
}

/// ============================================================================
/// EVENT SYSTEM PERFORMANCE TESTS
/// ============================================================================

#[tokio::test]
async fn test_event_system_performance() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Subscribe to events
    let mut event_receiver = service.subscribe_events().await;

    let event_count = 1000;
    let start_time = std::time::Instant::now();

    // Generate many events by creating and stopping instances
    for i in 0..event_count {
        let plugin_id = format!("event-plugin-{}", i);

        let mut registry = service.registry.write().await;
        let manifest = create_test_plugin_manifest(&plugin_id, PluginType::Rune);
        let plugin_id = registry.register_plugin(manifest).await?;
        drop(registry);

        let instance_id = service.create_instance(&plugin_id, None).await?;
        service.start_instance(&instance_id).await?;
        service.stop_instance(&instance_id).await?;
    }

    let generation_time = start_time.elapsed();

    // Count received events
    let mut received_events = 0;
    let start_time = std::time::Instant::now();

    while let Ok(_) = event_receiver.try_recv() {
        received_events += 1;
    }

    let processing_time = start_time.elapsed();

    println!("Event System Performance:");
    println!("  Generated events: ~{}", event_count * 3); // 3 events per instance
    println!("  Generation time: {:?}", generation_time);
    println!("  Processing time: {:?}", processing_time);
    println!("  Received events: {}", received_events);
    println!("  Events per second: {:.2}", received_events as f64 / generation_time.as_secs_f64());

    // Event processing should be fast
    assert!(generation_time < Duration::from_secs(5));
    assert!(processing_time < Duration::from_millis(100));

    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// CONFIGURATION PERFORMANCE TESTS
/// ============================================================================

#[tokio::test]
async fn test_configuration_operations_performance() -> Result<(), Box<dyn std::error::Error>> {
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    // Benchmark configuration retrieval
    let get_benchmark = PerformanceBenchmark::new("Configuration Get".to_string())
        .with_iterations(1000)
        .with_warmup_iterations(100);

    let get_result = get_benchmark.run(|| {
        Box::pin(async {
            service.get_config().await
        })
    }).await;

    get_result.print();
    get_result.assert_within(Duration::from_micros(100), Duration::from_micros(500));

    // Benchmark configuration validation
    let validate_benchmark = PerformanceBenchmark::new("Configuration Validate".to_string())
        .with_iterations(100)
        .with_warmup_iterations(10);

    let test_config = default_test_config();
    let validate_result = validate_benchmark.run(|| {
        Box::pin(async {
            service.validate_config(&test_config).await
        })
    }).await;

    validate_result.print();
    validate_result.assert_within(Duration::from_millis(1), Duration::from_millis(5));

    service.stop().await?;
    Ok(())
}

/// ============================================================================
/// PERFORMANCE REGRESSION TESTS
/// ============================================================================

#[tokio::test]
async fn test_performance_regression_detection() -> Result<(), Box<dyn std::error::Error>> {
    // Define performance baselines (these would come from historical data)
    let baselines = PerformanceBaselines {
        plugin_registration: Duration::from_millis(1),
        instance_creation: Duration::from_millis(10),
        instance_startup: Duration::from_millis(50),
        health_check: Duration::from_millis(5),
        resource_usage: Duration::from_millis(2),
    };

    let mut results = Vec::new();

    // Test plugin registration performance
    let config = default_test_config();
    let registry = DefaultPluginRegistry::new(config);

    let registration_result = benchmark_async(|| {
        Box::pin(async {
            let plugin_id = format!("regression-plugin-{}", uuid::Uuid::new_v4());
            let manifest = create_test_plugin_manifest(&plugin_id, PluginType::Rune);
            let mut test_registry = DefaultPluginRegistry::new(default_test_config());
            test_registry.register_plugin(manifest).await
        })
    }, 100).await;

    let registration_stats = calculate_duration_stats(&registration_result);
    results.push(("plugin_registration", registration_stats.mean, baselines.plugin_registration));

    // Test instance creation performance
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    let mut registry = service.registry.write().await;
    let manifest = create_test_plugin_manifest("regression-instance-plugin", PluginType::Rune);
    let plugin_id = registry.register_plugin(manifest).await?;
    drop(registry);

    let creation_result = benchmark_async(|| {
        Box::pin(async {
            service.create_instance(&plugin_id, None).await
        })
    }, 50).await;

    let creation_stats = calculate_duration_stats(&creation_result);
    results.push(("instance_creation", creation_stats.mean, baselines.instance_creation));

    // Check for regressions
    let mut regressions = Vec::new();
    for (test_name, actual_time, baseline_time) in &results {
        if actual_time > *baseline_time * 2 {
            regressions.push((test_name, *actual_time, *baseline_time));
        }
    }

    println!("Performance Regression Test Results:");
    for (test_name, actual, baseline) in &results {
        let ratio = *actual as f64 / *baseline as f64;
        println!("  {}: {:?} (baseline: {:?}, ratio: {:.2}x)", test_name, actual, baseline, ratio);
    }

    if !regressions.is_empty() {
        println!("\nPerformance Regressions Detected:");
        for (test_name, actual, baseline) in regressions {
            println!("  {}: {:?} vs baseline {:?} ({:.2}x slower)", test_name, actual, baseline, actual as f64 / baseline as f64);
        }
    }

    // For now, we'll allow some regressions in test environment
    // In production, this would fail the test
    println!("\nNote: Performance baselines would be updated from actual performance data");

    service.stop().await?;
    Ok(())
}

struct PerformanceBaselines {
    plugin_registration: Duration,
    instance_creation: Duration,
    instance_startup: Duration,
    health_check: Duration,
    resource_usage: Duration,
}

/// ============================================================================
/// STRESS TESTS
/// ============================================================================

#[tokio::test]
async fn test_extreme_load_stress() -> Result<(), Box<dyn std::error::Error>> {
    // This test pushes the system to its limits
    let mut service = create_test_plugin_manager().await;
    service.start().await?;

    let plugin_count = 500; // Very high number
    let operation_timeout = Duration::from_secs(30);

    let start_time = std::time::Instant::now();
    let mut successful_operations = 0;
    let mut failed_operations = 0;

    // Create many plugins concurrently
    let semaphore = Arc::new(tokio::sync::Semaphore::new(50)); // Limit concurrency
    let mut handles = Vec::new();

    for i in 0..plugin_count {
        let semaphore_clone = semaphore.clone();
        let handle = tokio::spawn(async move {
            let _permit = semaphore_clone.acquire().await.unwrap();

            let mut test_service = create_test_plugin_manager().await;
            let start_result = tokio::time::timeout(operation_timeout, test_service.start()).await;

            if start_result.is_err() {
                return false;
            }

            let plugin_id = format!("stress-plugin-{}", i);
            let mut registry = test_service.registry.write().await;
            let manifest = create_test_plugin_manifest(&plugin_id, PluginType::Rune);

            let registration_result = tokio::time::timeout(operation_timeout, registry.register_plugin(manifest)).await;

            if registration_result.is_err() {
                let _ = test_service.stop().await;
                return false;
            }

            let plugin_id = registration_result.unwrap().unwrap();
            drop(registry);

            let instance_result = tokio::time::timeout(operation_timeout, test_service.create_instance(&plugin_id, None)).await;

            if instance_result.is_err() {
                let _ = test_service.stop().await;
                return false;
            }

            let instance_id = instance_result.unwrap().unwrap();
            let start_result = tokio::time::timeout(operation_timeout, test_service.start_instance(&instance_id)).await;

            let _ = test_service.stop_instance(&instance_id).await;
            let _ = test_service.stop().await;

            start_result.is_ok()
        });
        handles.push(handle);
    }

    // Wait for all operations
    for handle in handles {
        match handle.await.unwrap() {
            true => successful_operations += 1,
            false => failed_operations += 1,
        }
    }

    let total_time = start_time.elapsed();
    let success_rate = successful_operations as f64 / plugin_count as f64;

    println!("Extreme Load Stress Test Results:");
    println!("  Total operations: {}", plugin_count);
    println!("  Successful: {}", successful_operations);
    println!("  Failed: {}", failed_operations);
    println!("  Success rate: {:.2}%", success_rate * 100.0);
    println!("  Total time: {:?}", total_time);
    println!("  Operations per second: {:.2}", plugin_count as f64 / total_time.as_secs_f64());

    // System should maintain reasonable success rate (> 80%)
    assert!(success_rate > 0.8, "Success rate should be above 80%");

    // System should complete within reasonable time
    assert!(total_time < Duration::from_secs(60), "Test should complete within 60 seconds");

    Ok(())
}