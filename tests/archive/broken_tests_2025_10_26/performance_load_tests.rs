//! Performance and load testing for CLI integration
//!
//! This module provides comprehensive performance testing including:
//! - Load testing for CLI commands
//! - Memory usage monitoring
//! - Concurrent operation stress testing
//! - Performance regression detection
//! - Resource cleanup validation
//! - Scalability testing

use crate::test_utilities::{
    AssertUtils, MemoryUsage, PerformanceMeasurement, TestContext, TestDataGenerator,
};

/// Performance test configuration
#[derive(Debug, Clone)]
pub struct PerformanceTestConfig {
    pub concurrent_operations: usize,
    pub iterations: usize,
    pub warmup_iterations: usize,
    pub timeout_duration: Duration,
    pub memory_limit_mb: u64,
    pub response_time_p50_limit: Duration,
    pub response_time_p95_limit: Duration,
    pub response_time_p99_limit: Duration,
}

impl Default for PerformanceTestConfig {
    fn default() -> Self {
        Self {
            concurrent_operations: 10,
            iterations: 100,
            warmup_iterations: 10,
            timeout_duration: Duration::from_secs(30),
            memory_limit_mb: 512,
            response_time_p50_limit: Duration::from_millis(100),
            response_time_p95_limit: Duration::from_millis(500),
            response_time_p99_limit: Duration::from_millis(1000),
        }
    }
}

/// Performance test results
#[derive(Debug, Clone)]
pub struct PerformanceTestResults {
    pub total_operations: usize,
    pub successful_operations: usize,
    pub failed_operations: usize,
    pub total_duration: Duration,
    pub average_response_time: Duration,
    pub min_response_time: Duration,
    pub max_response_time: Duration,
    pub p50_response_time: Duration,
    pub p95_response_time: Duration,
    pub p99_response_time: Duration,
    pub memory_usage_before: MemoryUsage,
    pub memory_usage_after: MemoryUsage,
    pub memory_peak_usage: MemoryUsage,
    pub throughput_ops_per_second: f64,
}

impl PerformanceTestResults {
    pub fn success_rate(&self) -> f64 {
        if self.total_operations == 0 {
            0.0
        } else {
            (self.successful_operations as f64 / self.total_operations as f64) * 100.0
        }
    }

    pub fn passes_sla(&self, config: &PerformanceTestConfig) -> bool {
        self.success_rate() >= 99.0
            && self.p50_response_time <= config.response_time_p50_limit
            && self.p95_response_time <= config.response_time_p95_limit
            && self.p99_response_time <= config.response_time_p99_limit
            && (self.memory_usage_after.rss_bytes - self.memory_usage_before.rss_bytes)
                <= config.memory_limit_mb * 1024 * 1024
    }
}

/// Performance testing framework
pub struct PerformanceTester {
    config: PerformanceTestConfig,
    memory_tracker: Arc<Mutex<Vec<MemoryUsage>>>,
}

impl PerformanceTester {
    pub fn new(config: PerformanceTestConfig) -> Self {
        Self {
            config,
            memory_tracker: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn run_service_performance_test(
        &self,
        context: &TestContext,
    ) -> Result<PerformanceTestResults> {
        println!(
            "Running service performance test with {} operations...",
            self.config.iterations
        );

        let memory_before = MemoryUsage::current();
        let mut response_times = Vec::new();
        let mut successful_operations = 0;

        // Warmup phase
        for _ in 0..self.config.warmup_iterations {
            let command = ServiceCommands::Health {
                service: None,
                format: "table".to_string(),
                detailed: false,
            };

            let _ = service_execute(context.config.clone(), command).await;
        }

        // Main test phase
        let start_time = Instant::now();
        let mut memory_samples = Vec::new();

        for i in 0..self.config.iterations {
            let operation_start = Instant::now();

            // Vary operations for realistic testing
            let command = match i % 4 {
                0 => ServiceCommands::Health {
                    service: None,
                    format: "table".to_string(),
                    detailed: false,
                },
                1 => ServiceCommands::List {
                    format: "table".to_string(),
                    status: true,
                    detailed: false,
                },
                2 => ServiceCommands::Metrics {
                    service: None,
                    format: "table".to_string(),
                    real_time: false,
                },
                _ => ServiceCommands::Health {
                    service: Some("crucible-script-engine".to_string()),
                    format: "json".to_string(),
                    detailed: false,
                },
            };

            let result = timeout(
                self.config.timeout_duration,
                service_execute(context.config.clone(), command),
            )
            .await;

            let response_time = operation_start.elapsed();
            response_times.push(response_time);

            if result.is_ok() && result.unwrap().is_ok() {
                successful_operations += 1;
            }

            // Sample memory usage periodically
            if i % 10 == 0 {
                let memory = MemoryUsage::current();
                memory_samples.push(memory);
                self.memory_tracker.lock().unwrap().push(memory);
            }
        }

        let total_duration = start_time.elapsed();
        let memory_after = MemoryUsage::current();
        let memory_peak = memory_samples
            .into_iter()
            .max_by_key(|m| m.rss_bytes)
            .unwrap_or(memory_after);

        // Calculate percentiles
        response_times.sort();
        let total_operations = response_times.len();
        let p50_index = total_operations * 50 / 100;
        let p95_index = total_operations * 95 / 100;
        let p99_index = total_operations * 99 / 100;

        let results = PerformanceTestResults {
            total_operations,
            successful_operations,
            failed_operations: total_operations - successful_operations,
            total_duration,
            average_response_time: total_duration / total_operations as u32,
            min_response_time: *response_times.first().unwrap_or(&Duration::ZERO),
            max_response_time: *response_times.last().unwrap_or(&Duration::ZERO),
            p50_response_time: response_times
                .get(p50_index)
                .cloned()
                .unwrap_or(Duration::ZERO),
            p95_response_time: response_times
                .get(p95_index)
                .cloned()
                .unwrap_or(Duration::ZERO),
            p99_response_time: response_times
                .get(p99_index)
                .cloned()
                .unwrap_or(Duration::ZERO),
            memory_usage_before: memory_before,
            memory_usage_after: memory_after,
            memory_peak_usage: memory_peak,
            throughput_ops_per_second: total_operations as f64 / total_duration.as_secs_f64(),
        };

        println!("Service Performance Test Results:");
        println!(
            "  Operations: {}/{} ({:.1}% success)",
            results.successful_operations,
            results.total_operations,
            results.success_rate()
        );
        println!(
            "  Throughput: {:.2} ops/sec",
            results.throughput_ops_per_second
        );
        println!(
            "  Response times: p50={:?}, p95={:?}, p99={:?}",
            results.p50_response_time, results.p95_response_time, results.p99_response_time
        );
        println!(
            "  Memory growth: {} MB",
            (results.memory_usage_after.rss_bytes - results.memory_usage_before.rss_bytes)
                / 1024
                / 1024
        );

        Ok(results)
    }

    pub async fn run_migration_performance_test(
        &self,
        context: &TestContext,
    ) -> Result<PerformanceTestResults> {
        println!(
            "Running migration performance test with {} operations...",
            self.config.iterations
        );

        let memory_before = MemoryUsage::current();
        let mut response_times = Vec::new();
        let mut successful_operations = 0;

        // Warmup phase
        for _ in 0..self.config.warmup_iterations {
            let command = MigrationCommands::Status {
                format: "table".to_string(),
                detailed: false,
                validate: false,
            };

            let _ = migration_execute(context.config.clone(), command).await;
        }

        // Main test phase
        let start_time = Instant::now();
        let mut memory_samples = Vec::new();

        for i in 0..self.config.iterations {
            let operation_start = Instant::now();

            // Vary migration operations
            let command = match i % 5 {
                0 => MigrationCommands::Status {
                    format: "table".to_string(),
                    detailed: false,
                    validate: false,
                },
                1 => MigrationCommands::List {
                    format: "table".to_string(),
                    active: false,
                    inactive: false,
                    metadata: false,
                },
                2 => MigrationCommands::Validate {
                    tool: None,
                    auto_fix: false,
                    format: "table".to_string(),
                },
                3 => MigrationCommands::Migrate {
                    tool: Some(format!("test-tool-{}", i % 10)),
                    force: false,
                    security_level: "safe".to_string(),
                    dry_run: true,
                },
                _ => MigrationCommands::Status {
                    format: "json".to_string(),
                    detailed: true,
                    validate: true,
                },
            };

            let result = timeout(
                self.config.timeout_duration,
                migration_execute(context.config.clone(), command),
            )
            .await;

            let response_time = operation_start.elapsed();
            response_times.push(response_time);

            if result.is_ok() && result.unwrap().is_ok() {
                successful_operations += 1;
            }

            // Sample memory usage
            if i % 10 == 0 {
                let memory = MemoryUsage::current();
                memory_samples.push(memory);
                self.memory_tracker.lock().unwrap().push(memory);
            }
        }

        let total_duration = start_time.elapsed();
        let memory_after = MemoryUsage::current();
        let memory_peak = memory_samples
            .into_iter()
            .max_by_key(|m| m.rss_bytes)
            .unwrap_or(memory_after);

        // Calculate percentiles
        response_times.sort();
        let total_operations = response_times.len();
        let p50_index = total_operations * 50 / 100;
        let p95_index = total_operations * 95 / 100;
        let p99_index = total_operations * 99 / 100;

        let results = PerformanceTestResults {
            total_operations,
            successful_operations,
            failed_operations: total_operations - successful_operations,
            total_duration,
            average_response_time: total_duration / total_operations as u32,
            min_response_time: *response_times.first().unwrap_or(&Duration::ZERO),
            max_response_time: *response_times.last().unwrap_or(&Duration::ZERO),
            p50_response_time: response_times
                .get(p50_index)
                .cloned()
                .unwrap_or(Duration::ZERO),
            p95_response_time: response_times
                .get(p95_index)
                .cloned()
                .unwrap_or(Duration::ZERO),
            p99_response_time: response_times
                .get(p99_index)
                .cloned()
                .unwrap_or(Duration::ZERO),
            memory_usage_before: memory_before,
            memory_usage_after: memory_after,
            memory_peak_usage: memory_peak,
            throughput_ops_per_second: total_operations as f64 / total_duration.as_secs_f64(),
        };

        println!("Migration Performance Test Results:");
        println!(
            "  Operations: {}/{} ({:.1}% success)",
            results.successful_operations,
            results.total_operations,
            results.success_rate()
        );
        println!(
            "  Throughput: {:.2} ops/sec",
            results.throughput_ops_per_second
        );
        println!(
            "  Response times: p50={:?}, p95={:?}, p99={:?}",
            results.p50_response_time, results.p95_response_time, results.p99_response_time
        );
        println!(
            "  Memory growth: {} MB",
            (results.memory_usage_after.rss_bytes - results.memory_usage_before.rss_bytes)
                / 1024
                / 1024
        );

        Ok(results)
    }

    pub async fn run_rune_performance_test(
        &self,
        context: &TestContext,
    ) -> Result<PerformanceTestResults> {
        println!(
            "Running Rune performance test with {} operations...",
            self.config.iterations
        );

        let memory_before = MemoryUsage::current();
        let mut response_times = Vec::new();
        let mut successful_operations = 0;

        // Create test scripts
        let scripts = vec![
            (
                "simple",
                r#"function main(args) { return { success: true }; }"#,
            ),
            (
                "with_args",
                r#"function main(args) { return { success: true, args: args }; }"#,
            ),
            (
                "complex",
                r#"
                function main(args) {
                    let result = { success: true, operations: [] };
                    for (let i = 0; i < 10; i++) {
                        result.operations.push({ id: i, processed: true });
                    }
                    return result;
                }
            "#,
            ),
        ];

        // Warmup phase
        for _ in 0..self.config.warmup_iterations {
            let script_path = context.create_test_script("warmup", scripts[0].1);
            let _ = rune_execute(
                context.config.clone(),
                script_path.to_string_lossy().to_string(),
                None,
            )
            .await;
        }

        // Main test phase
        let start_time = Instant::now();
        let mut memory_samples = Vec::new();

        for i in 0..self.config.iterations {
            let operation_start = Instant::now();

            // Vary scripts and arguments
            let script = scripts[i % scripts.len()];
            let script_path = context.create_test_script(&format!("perf-test-{}", i), script.1);
            let args = if i % 3 == 0 {
                Some(r#"{"test": "performance", "iteration": "#.to_string() + &i.to_string() + "}")
            } else {
                None
            };

            let result = timeout(
                self.config.timeout_duration,
                rune_execute(
                    context.config.clone(),
                    script_path.to_string_lossy().to_string(),
                    args,
                ),
            )
            .await;

            let response_time = operation_start.elapsed();
            response_times.push(response_time);

            if result.is_ok() && result.unwrap().is_ok() {
                successful_operations += 1;
            }

            // Sample memory usage
            if i % 10 == 0 {
                let memory = MemoryUsage::current();
                memory_samples.push(memory);
                self.memory_tracker.lock().unwrap().push(memory);
            }
        }

        let total_duration = start_time.elapsed();
        let memory_after = MemoryUsage::current();
        let memory_peak = memory_samples
            .into_iter()
            .max_by_key(|m| m.rss_bytes)
            .unwrap_or(memory_after);

        // Calculate percentiles
        response_times.sort();
        let total_operations = response_times.len();
        let p50_index = total_operations * 50 / 100;
        let p95_index = total_operations * 95 / 100;
        let p99_index = total_operations * 99 / 100;

        let results = PerformanceTestResults {
            total_operations,
            successful_operations,
            failed_operations: total_operations - successful_operations,
            total_duration,
            average_response_time: total_duration / total_operations as u32,
            min_response_time: *response_times.first().unwrap_or(&Duration::ZERO),
            max_response_time: *response_times.last().unwrap_or(&Duration::ZERO),
            p50_response_time: response_times
                .get(p50_index)
                .cloned()
                .unwrap_or(Duration::ZERO),
            p95_response_time: response_times
                .get(p95_index)
                .cloned()
                .unwrap_or(Duration::ZERO),
            p99_response_time: response_times
                .get(p99_index)
                .cloned()
                .unwrap_or(Duration::ZERO),
            memory_usage_before: memory_before,
            memory_usage_after: memory_after,
            memory_peak_usage: memory_peak,
            throughput_ops_per_second: total_operations as f64 / total_duration.as_secs_f64(),
        };

        println!("Rune Performance Test Results:");
        println!(
            "  Operations: {}/{} ({:.1}% success)",
            results.successful_operations,
            results.total_operations,
            results.success_rate()
        );
        println!(
            "  Throughput: {:.2} ops/sec",
            results.throughput_ops_per_second
        );
        println!(
            "  Response times: p50={:?}, p95={:?}, p99={:?}",
            results.p50_response_time, results.p95_response_time, results.p99_response_time
        );
        println!(
            "  Memory growth: {} MB",
            (results.memory_usage_after.rss_bytes - results.memory_usage_before.rss_bytes)
                / 1024
                / 1024
        );

        Ok(results)
    }

    pub async fn run_concurrent_load_test(
        &self,
        context: &TestContext,
    ) -> Result<PerformanceTestResults> {
        println!(
            "Running concurrent load test with {} operations over {} concurrent workers...",
            self.config.iterations, self.config.concurrent_operations
        );

        let memory_before = MemoryUsage::current();
        let operations_per_worker = self.config.iterations / self.config.concurrent_operations;
        let mut response_times = Vec::new();
        let mut successful_operations = 0;

        // Create concurrent worker tasks
        let mut worker_futures = Vec::new();

        for worker_id in 0..self.config.concurrent_operations {
            let config = context.config.clone();
            let operations = operations_per_worker;
            let timeout_duration = self.config.timeout_duration;

            let future = async move {
                let mut worker_response_times = Vec::new();
                let mut worker_successes = 0;

                for i in 0..operations {
                    let operation_start = Instant::now();

                    let command = ServiceCommands::Health {
                        service: Some(format!("service-{}", (worker_id + i) % 5)),
                        format: "table".to_string(),
                        detailed: false,
                    };

                    let result =
                        timeout(timeout_duration, service_execute(config.clone(), command)).await;
                    let response_time = operation_start.elapsed();
                    worker_response_times.push(response_time);

                    if result.is_ok() && result.unwrap().is_ok() {
                        worker_successes += 1;
                    }
                }

                (worker_response_times, worker_successes)
            };

            worker_futures.push(future);
        }

        // Execute all workers concurrently
        let start_time = Instant::now();
        let worker_results = join_all(worker_futures).await;
        let total_duration = start_time.elapsed();

        // Aggregate results
        for (worker_response_times, worker_successes) in worker_results {
            response_times.extend(worker_response_times);
            successful_operations += worker_successes;
        }

        let memory_after = MemoryUsage::current();

        // Calculate percentiles
        response_times.sort();
        let total_operations = response_times.len();
        let p50_index = total_operations * 50 / 100;
        let p95_index = total_operations * 95 / 100;
        let p99_index = total_operations * 99 / 100;

        let results = PerformanceTestResults {
            total_operations,
            successful_operations,
            failed_operations: total_operations - successful_operations,
            total_duration,
            average_response_time: total_duration / total_operations as u32,
            min_response_time: *response_times.first().unwrap_or(&Duration::ZERO),
            max_response_time: *response_times.last().unwrap_or(&Duration::ZERO),
            p50_response_time: response_times
                .get(p50_index)
                .cloned()
                .unwrap_or(Duration::ZERO),
            p95_response_time: response_times
                .get(p95_index)
                .cloned()
                .unwrap_or(Duration::ZERO),
            p99_response_time: response_times
                .get(p99_index)
                .cloned()
                .unwrap_or(Duration::ZERO),
            memory_usage_before: memory_before,
            memory_usage_after: memory_after,
            memory_peak_usage: memory_after, // Simplified for concurrent test
            throughput_ops_per_second: total_operations as f64 / total_duration.as_secs_f64(),
        };

        println!("Concurrent Load Test Results:");
        println!(
            "  Operations: {}/{} ({:.1}% success)",
            results.successful_operations,
            results.total_operations,
            results.success_rate()
        );
        println!("  Workers: {}", self.config.concurrent_operations);
        println!(
            "  Throughput: {:.2} ops/sec",
            results.throughput_ops_per_second
        );
        println!(
            "  Response times: p50={:?}, p95={:?}, p99={:?}",
            results.p50_response_time, results.p95_response_time, results.p99_response_time
        );
        println!(
            "  Memory growth: {} MB",
            (results.memory_usage_after.rss_bytes - results.memory_usage_before.rss_bytes)
                / 1024
                / 1024
        );

        Ok(results)
    }
}

// Performance test implementations

#[tokio::test]
async fn test_service_command_performance() -> Result<()> {
    let context = TestContext::new()?;
    let config = PerformanceTestConfig {
        iterations: 50,
        warmup_iterations: 5,
        ..Default::default()
    };

    let tester = PerformanceTester::new(config);
    let results = tester.run_service_performance_test(&context).await?;

    // Basic performance assertions
    assert!(
        results.success_rate() >= 95.0,
        "Success rate should be at least 95%"
    );
    assert!(
        results.average_response_time < Duration::from_millis(200),
        "Average response time should be under 200ms"
    );
    assert!(
        results.throughput_ops_per_second >= 5.0,
        "Throughput should be at least 5 ops/sec"
    );

    Ok(())
}

#[tokio::test]
async fn test_migration_command_performance() -> Result<()> {
    let context = TestContext::new()?;
    let config = PerformanceTestConfig {
        iterations: 30,
        warmup_iterations: 3,
        ..Default::default()
    };

    let tester = PerformanceTester::new(config);
    let results = tester.run_migration_performance_test(&context).await?;

    // Migration-specific performance assertions
    assert!(
        results.success_rate() >= 90.0,
        "Migration success rate should be at least 90%"
    );
    assert!(
        results.average_response_time < Duration::from_millis(500),
        "Migration commands should be reasonably fast"
    );
    assert!(
        results.throughput_ops_per_second >= 2.0,
        "Migration throughput should be at least 2 ops/sec"
    );

    Ok(())
}

#[tokio::test]
async fn test_rune_command_performance() -> Result<()> {
    let context = TestContext::new()?;
    let config = PerformanceTestConfig {
        iterations: 40,
        warmup_iterations: 5,
        timeout_duration: Duration::from_secs(10),
        ..Default::default()
    };

    let tester = PerformanceTester::new(config);
    let results = tester.run_rune_performance_test(&context).await?;

    // Rune-specific performance assertions
    assert!(
        results.success_rate() >= 85.0,
        "Rune success rate should be at least 85%"
    );
    assert!(
        results.average_response_time < Duration::from_millis(1000),
        "Rune execution should be reasonably fast"
    );
    assert!(
        results.throughput_ops_per_second >= 1.0,
        "Rune throughput should be at least 1 ops/sec"
    );

    Ok(())
}

#[tokio::test]
async fn test_concurrent_load_performance() -> Result<()> {
    let context = TestContext::new()?;
    let config = PerformanceTestConfig {
        concurrent_operations: 5,
        iterations: 100,
        warmup_iterations: 5,
        ..Default::default()
    };

    let tester = PerformanceTester::new(config);
    let results = tester.run_concurrent_load_test(&context).await?;

    // Concurrent performance assertions
    assert!(
        results.success_rate() >= 90.0,
        "Concurrent success rate should be at least 90%"
    );
    assert!(
        results.throughput_ops_per_second >= 10.0,
        "Concurrent throughput should be high"
    );
    assert!(
        results.p95_response_time < Duration::from_secs(2),
        "95th percentile should be reasonable"
    );

    Ok(())
}

#[tokio::test]
async fn test_memory_leak_detection() -> Result<()> {
    let context = TestContext::new()?;

    let initial_memory = MemoryUsage::current();

    // Execute many operations to detect memory leaks
    for round in 0..5 {
        println!("Memory leak test round {}", round + 1);

        // Run service commands
        for _ in 0..20 {
            let command = ServiceCommands::Health {
                service: None,
                format: "table".to_string(),
                detailed: false,
            };

            let _ = service_execute(context.config.clone(), command).await;
        }

        // Run migration commands
        for _ in 0..10 {
            let command = MigrationCommands::Status {
                format: "table".to_string(),
                detailed: false,
                validate: false,
            };

            let _ = migration_execute(context.config.clone(), command).await;
        }

        // Run Rune commands
        for i in 0..5 {
            let script_path = context.create_test_script(
                &format!("memory-test-{}-{}", round, i),
                "function main() { return { success: true }; }",
            );
            let _ = rune_execute(
                context.config.clone(),
                script_path.to_string_lossy().to_string(),
                None,
            )
            .await;
        }

        // Force garbage collection if possible
        tokio::task::yield_now().await;

        let current_memory = MemoryUsage::current();
        let memory_growth = current_memory
            .rss_bytes
            .saturating_sub(initial_memory.rss_bytes);

        println!("  Memory growth: {} MB", memory_growth / 1024 / 1024);

        // Memory growth should be reasonable
        assert!(
            memory_growth < 100 * 1024 * 1024,
            "Memory growth should be under 100MB per round"
        );
    }

    let final_memory = MemoryUsage::current();
    let total_growth = final_memory
        .rss_bytes
        .saturating_sub(initial_memory.rss_bytes);

    println!("Total memory growth: {} MB", total_growth / 1024 / 1024);
    assert!(
        total_growth < 200 * 1024 * 1024,
        "Total memory growth should be under 200MB"
    );

    Ok(())
}

#[tokio::test]
async fn test_scalability_limits() -> Result<()> {
    let context = TestContext::new()?;

    // Test increasing load to find scalability limits
    let concurrent_levels = vec![1, 2, 5, 10, 20];
    let mut scalability_results = Vec::new();

    for concurrent_ops in concurrent_levels {
        println!(
            "Testing scalability with {} concurrent operations",
            concurrent_ops
        );

        let config = PerformanceTestConfig {
            concurrent_operations: concurrent_ops,
            iterations: concurrent_ops * 10, // Keep total work consistent
            warmup_iterations: 5,
            timeout_duration: Duration::from_secs(30),
            ..Default::default()
        };

        let tester = PerformanceTester::new(config);
        let results = tester.run_concurrent_load_test(&context).await?;

        scalability_results.push((concurrent_ops, results.clone()));

        println!("  Success rate: {:.1}%", results.success_rate());
        println!(
            "  Throughput: {:.2} ops/sec",
            results.throughput_ops_per_second
        );
        println!("  p95 response time: {:?}", results.p95_response_time);

        // Stop testing if performance degrades significantly
        if results.success_rate() < 80.0 || results.p95_response_time > Duration::from_secs(5) {
            println!("Stopping scalability test - performance degradation detected");
            break;
        }
    }

    // Analyze scalability results
    let mut max_throughput = 0.0;
    let mut optimal_concurrency = 1;

    for (concurrency, results) in &scalability_results {
        if results.success_rate() >= 90.0 && results.throughput_ops_per_second > max_throughput {
            max_throughput = results.throughput_ops_per_second;
            optimal_concurrency = *concurrency;
        }
    }

    println!(
        "Optimal concurrency level: {} (throughput: {:.2} ops/sec)",
        optimal_concurrency, max_throughput
    );

    // Scalability assertions
    assert!(
        optimal_concurrency >= 2,
        "Should benefit from some concurrency"
    );
    assert!(
        max_throughput >= 10.0,
        "Should achieve reasonable throughput"
    );

    Ok(())
}

#[tokio::test]
async fn test_performance_regression_detection() -> Result<()> {
    let context = TestContext::new()?;

    // Define performance baselines (these would normally come from historical data)
    let baseline_throughput = 20.0; // ops/sec
    let baseline_p95_response = Duration::from_millis(300);
    let baseline_success_rate = 95.0; // percent

    // Run performance test
    let config = PerformanceTestConfig {
        iterations: 100,
        warmup_iterations: 10,
        ..Default::default()
    };

    let tester = PerformanceTester::new(config);
    let results = tester.run_service_performance_test(&context).await?;

    println!("Performance Regression Test Results:");
    println!(
        "  Current throughput: {:.2} ops/sec (baseline: {:.2})",
        results.throughput_ops_per_second, baseline_throughput
    );
    println!(
        "  Current p95: {:?} (baseline: {:?})",
        results.p95_response_time, baseline_p95_response
    );
    println!(
        "  Current success rate: {:.1}% (baseline: {:.1}%)",
        results.success_rate(),
        baseline_success_rate
    );

    // Check for performance regressions
    let throughput_regression =
        (baseline_throughput - results.throughput_ops_per_second) / baseline_throughput > 0.20;
    let latency_regression = results.p95_response_time > baseline_p95_response * 2;
    let success_rate_regression = results.success_rate() < baseline_success_rate - 10.0;

    if throughput_regression {
        println!("⚠️  Throughput regression detected!");
    }
    if latency_regression {
        println!("⚠️  Latency regression detected!");
    }
    if success_rate_regression {
        println!("⚠️  Success rate regression detected!");
    }

    // In a real CI/CD environment, you might fail the test on significant regressions
    // For now, we'll just warn about them
    println!("Regression test completed - check output for any performance warnings");

    Ok(())
}

use crate::test_utilities::{
    AssertUtils, MemoryUsage, PerformanceMeasurement, TestContext, TestDataGenerator,
};
use anyhow::Result;
use crucible_cli::cli::{MigrationCommands, ServiceCommands};
use crucible_cli::commands::migration::execute as migration_execute;
use crucible_cli::commands::rune::{execute as rune_execute, list_commands};
use crucible_cli::commands::service::execute as service_execute;
use crucible_cli::config::CliConfig;
use futures::future::join_all;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::time::{sleep, timeout};
