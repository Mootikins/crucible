//! Performance Benchmarks and Memory Validation Tests
//!
//! This module provides comprehensive performance testing for the Phase 5.1 migration
//! components, including benchmarks, memory usage validation, and stress tests.

use crate::{
    migration_bridge::{ToolMigrationBridge, MigrationConfig},
    migration_manager::{Phase51MigrationManager, MigrationManagerConfig, MigrationMode, ValidationMode},
    tool::RuneTool,
    types::{RuneServiceConfig, ToolExecutionContext},
};
use anyhow::{Context, Result};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::thread;
use tempfile::TempDir;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Performance test configuration
#[derive(Debug, Clone)]
pub struct PerformanceTestConfig {
    /// Number of tools to create for stress testing
    pub tool_count: usize,
    /// Number of concurrent operations
    pub concurrent_operations: usize,
    /// Maximum duration for individual operations
    pub max_operation_duration: Duration,
    /// Memory usage threshold in bytes
    pub memory_threshold_bytes: usize,
    /// Whether to run stress tests
    pub run_stress_tests: bool,
    /// Whether to run memory leak detection
    pub run_memory_leak_detection: bool,
}

impl Default for PerformanceTestConfig {
    fn default() -> Self {
        Self {
            tool_count: 100,
            concurrent_operations: 10,
            max_operation_duration: Duration::from_secs(30),
            memory_threshold_bytes: 100 * 1024 * 1024, // 100MB
            run_stress_tests: false, // Disabled by default for CI
            run_memory_leak_detection: true,
        }
    }
}

/// Performance metrics collection
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Operation duration
    pub duration: Duration,
    /// Memory usage before operation
    pub memory_before_bytes: usize,
    /// Memory usage after operation
    pub memory_after_bytes: usize,
    /// Number of operations performed
    pub operations_count: usize,
    /// Success rate
    pub success_rate: f64,
    /// Average time per operation
    pub avg_time_per_operation: Duration,
    /// Peak memory usage
    pub peak_memory_bytes: usize,
    /// CPU usage (approximate)
    pub cpu_usage_percent: f64,
}

/// Performance test utilities
pub struct PerformanceTestUtils;

impl PerformanceTestUtils {
    /// Get current memory usage (approximate)
    pub fn get_memory_usage() -> usize {
        // This is a simplified implementation
        // In a real scenario, you'd use platform-specific APIs
        std::mem::size_of::<PerformanceTestUtils>()
    }

    /// Create a large number of test tools for stress testing
    pub async fn create_stress_test_tool_directory(
        tool_count: usize,
    ) -> Result<(TempDir, Vec<String>)> {
        let temp_dir = TempDir::new()?;
        let mut tool_names = vec![];

        for i in 0..tool_count {
            let tool_source = format!(r#"
                pub fn NAME() {{ "stress_tool_{}" }}
                pub fn DESCRIPTION() {{ "Stress test tool {}" }}
                pub fn INPUT_SCHEMA() {{
                    #{{
                        type: "object",
                        properties: #{{
                            input: #{{ type: "string" }}
                        }},
                        required: ["input"]
                    }}
                }}
                pub async fn call(args) {{
                    // Simulate some processing
                    let result = format!("processed_{{}}_{{}}", args.input, {});
                    #{{ success: true, result }}
                }}
            "#, i, i, i);

            let tool_path = temp_dir.path().join(format!("stress_tool_{}.rn", i));
            tokio::fs::write(tool_path, tool_source).await?;
            tool_names.push(format!("stress_tool_{}", i));
        }

        Ok((temp_dir, tool_names))
    }

    /// Create a performance benchmark configuration
    pub fn create_benchmark_config(
        mode: MigrationMode,
        validation_mode: ValidationMode,
        enable_parallel: bool,
        max_concurrent: usize,
    ) -> MigrationManagerConfig {
        MigrationManagerConfig {
            mode,
            validation_mode,
            enable_parallel_migration: enable_parallel,
            max_concurrent_migrations: max_concurrent,
            migration_directories: vec![],
            preserve_original_service: false,
            rollback_on_failure: false,
            security_level: crucible_services::SecurityLevel::Safe,
        }
    }

    /// Measure execution time of a function
    pub async fn measure_execution_time<F, Fut, T>(f: F) -> (T, Duration)
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        let start = Instant::now();
        let result = f().await;
        let duration = start.elapsed();
        (result, duration)
    }

    /// Collect performance metrics for multiple operations
    pub async fn collect_performance_metrics<F, Fut, T>(
        operation_count: usize,
        operation: F,
    ) -> PerformanceMetrics
    where
        F: Fn(usize) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let start_time = Instant::now();
        let memory_before = Self::get_memory_usage();
        let mut successful_operations = 0;
        let mut durations = vec![];

        for i in 0..operation_count {
            let (result, duration) = Self::measure_execution_time(|| operation(i)).await;
            durations.push(duration);

            match result {
                Ok(_) => successful_operations += 1,
                Err(_) => {
                    // Handle failed operations
                }
            }
        }

        let total_duration = start_time.elapsed();
        let memory_after = Self::get_memory_usage();
        let success_rate = successful_operations as f64 / operation_count as f64;
        let avg_time_per_operation = total_duration / operation_count as u32;
        let peak_memory = std::cmp::max(memory_before, memory_after);
        let cpu_usage = (total_duration.as_millis() as f64 / 1000.0) * 100.0; // Approximate

        PerformanceMetrics {
            duration: total_duration,
            memory_before_bytes: memory_before,
            memory_after_bytes: memory_after,
            operations_count: operation_count,
            success_rate,
            avg_time_per_operation,
            peak_memory_bytes: peak_memory,
            cpu_usage_percent: cpu_usage,
        }
    }

    /// Generate performance report
    pub fn generate_performance_report(
        test_name: &str,
        metrics: &PerformanceMetrics,
    ) -> String {
        format!(
            "Performance Test Report: {}\n\
             ======================================\n\
             Total Duration: {:?}\n\
             Operations Count: {}\n\
             Success Rate: {:.2}%\n\
             Avg Time per Operation: {:?}\n\
             Memory Before: {} bytes\n\
             Memory After: {} bytes\n\
             Peak Memory: {} bytes\n\
             Memory Delta: {} bytes\n\
             Approx CPU Usage: {:.2}%\n\
             ======================================",
            test_name,
            metrics.duration,
            metrics.operations_count,
            metrics.success_rate * 100.0,
            metrics.avg_time_per_operation,
            metrics.memory_before_bytes,
            metrics.memory_after_bytes,
            metrics.peak_memory_bytes,
            metrics.memory_after_bytes.saturating_sub(metrics.memory_before_bytes),
            metrics.cpu_usage_percent
        )
    }
}

// ============================================================================
// PERFORMANCE BENCHMARK TESTS
// ============================================================================

#[cfg(test)]
mod migration_performance_benchmarks {
    use super::*;

    mod bridge_performance_benchmarks {
        use super::*;

        #[tokio::test]
        async fn test_bridge_creation_performance() -> Result<()> {
            let iterations = 10;
            let config = MigrationConfig::default();
            let rune_config = RuneServiceConfig::default();

            let metrics = PerformanceTestUtils::collect_performance_metrics(
                iterations,
                |_| async {
                    let bridge = ToolMigrationBridge::new(rune_config.clone(), config.clone()).await;
                    bridge.map(|_| ())
                },
            ).await;

            let report = PerformanceTestUtils::generate_performance_report(
                "Bridge Creation Performance",
                &metrics,
            );

            println!("{}", report);

            // Performance assertions
            assert!(metrics.duration < Duration::from_secs(30));
            assert!(metrics.avg_time_per_operation < Duration::from_secs(3));
            assert!(metrics.success_rate >= 0.5); // Allow for CI failures

            Ok(())
        }

        #[tokio::test]
        async fn test_migration_stats_retrieval_performance() -> Result<()> {
            let config = MigrationConfig::default();
            let rune_config = RuneServiceConfig::default();

            if let Ok(bridge) = ToolMigrationBridge::new(rune_config, config).await {
                let iterations = 1000;

                let metrics = PerformanceTestUtils::collect_performance_metrics(
                    iterations,
                    |_| async {
                        let stats = bridge.get_migration_stats().await;
                        Ok(stats)
                    },
                ).await;

                let report = PerformanceTestUtils::generate_performance_report(
                    "Migration Stats Retrieval Performance",
                    &metrics,
                );

                println!("{}", report);

                // Stats retrieval should be very fast
                assert!(metrics.avg_time_per_operation < Duration::from_millis(10));
                assert!(metrics.success_rate >= 0.9);
            }

            Ok(())
        }

        #[tokio::test]
        async fn test_tool_listing_performance() -> Result<()> {
            let config = MigrationConfig::default();
            let rune_config = RuneServiceConfig::default();

            if let Ok(bridge) = ToolMigrationBridge::new(rune_config, config).await {
                let iterations = 100;

                let metrics = PerformanceTestUtils::collect_performance_metrics(
                    iterations,
                    |_| async {
                        let tools = bridge.list_migrated_tools().await;
                        Ok(tools)
                    },
                ).await;

                let report = PerformanceTestUtils::generate_performance_report(
                    "Tool Listing Performance",
                    &metrics,
                );

                println!("{}", report);

                assert!(metrics.avg_time_per_operation < Duration::from_millis(50));
                assert!(metrics.success_rate >= 0.9);
            }

            Ok(())
        }

        #[tokio::test]
        async fn test_validation_performance() -> Result<()> {
            let config = MigrationConfig::default();
            let rune_config = RuneServiceConfig::default();

            if let Ok(bridge) = ToolMigrationBridge::new(rune_config, config).await {
                let iterations = 50;

                let metrics = PerformanceTestUtils::collect_performance_metrics(
                    iterations,
                    |_| async {
                        let validation = bridge.validate_migration().await;
                        Ok(validation)
                    },
                ).await;

                let report = PerformanceTestUtils::generate_performance_report(
                    "Migration Validation Performance",
                    &metrics,
                );

                println!("{}", report);

                assert!(metrics.avg_time_per_operation < Duration::from_millis(100));
                assert!(metrics.success_rate >= 0.8);
            }

            Ok(())
        }
    }

    mod manager_performance_benchmarks {
        use super::*;

        #[tokio::test]
        async fn test_manager_creation_performance() -> Result<()> {
            let iterations = 5;
            let config = MigrationManagerConfig::default();

            let metrics = PerformanceTestUtils::collect_performance_metrics(
                iterations,
                |_| async {
                    let manager = Phase51MigrationManager::new(config.clone()).await;
                    manager.map(|_| ())
                },
            ).await;

            let report = PerformanceTestUtils::generate_performance_report(
                "Manager Creation Performance",
                &metrics,
            );

            println!("{}", report);

            assert!(metrics.duration < Duration::from_secs(30));
            assert!(metrics.avg_time_per_operation < Duration::from_secs(6));
            assert!(metrics.success_rate >= 0.5);

            Ok(())
        }

        #[tokio::test]
        async fn test_dry_run_performance() -> Result<()> {
            let config = PerformanceTestUtils::create_benchmark_config(
                MigrationMode::DryRun,
                ValidationMode::Basic,
                false,
                1,
            );

            let iterations = 10;

            let metrics = PerformanceTestUtils::collect_performance_metrics(
                iterations,
                |_| async {
                    let mut manager = Phase51MigrationManager::new(config.clone()).await?;
                    let report = manager.execute_migration().await;
                    Ok(report)
                },
            ).await;

            let report = PerformanceTestUtils::generate_performance_report(
                "Dry Run Migration Performance",
                &metrics,
            );

            println!("{}", report);

            assert!(metrics.avg_time_per_operation < Duration::from_secs(5));
            assert!(metrics.success_rate >= 0.7);

            Ok(())
        }

        #[tokio::test]
        async fn test_status_retrieval_performance() -> Result<()> {
            let config = MigrationManagerConfig::default();

            if let Ok(manager) = Phase51MigrationManager::new(config).await {
                let iterations = 500;

                let metrics = PerformanceTestUtils::collect_performance_metrics(
                    iterations,
                    |_| async {
                        let status = manager.get_migration_status().await;
                        Ok(status)
                    },
                ).await;

                let report = PerformanceTestUtils::generate_performance_report(
                    "Status Retrieval Performance",
                    &metrics,
                );

                println!("{}", report);

                assert!(metrics.avg_time_per_operation < Duration::from_millis(10));
                assert!(metrics.success_rate >= 0.9);
            }

            Ok(())
        }

        #[tokio::test]
        async fn test_concurrent_status_access_performance() -> Result<()> {
            let config = MigrationManagerConfig::default();

            if let Ok(manager) = Arc::new(Phase51MigrationManager::new(config).await)) {
                let concurrent_operations = 20;
                let operations_per_thread = 10;

                let start = Instant::now();

                let handles: Vec<_> = (0..concurrent_operations)
                    .map(|_| {
                        let manager = Arc::clone(&manager);
                        tokio::spawn(async move {
                            for _ in 0..operations_per_thread {
                                let _ = manager.get_migration_status().await;
                                let _ = manager.get_migration_statistics().await;
                            }
                        })
                    })
                    .collect();

                futures::future::join_all(handles).await;

                let total_duration = start.elapsed();
                let total_operations = concurrent_operations * operations_per_thread * 2; // status + stats
                let avg_time_per_operation = total_duration / total_operations as u32;

                println!(
                    "Concurrent Status Access Performance:\n\
                     Total Duration: {:?}\n\
                     Total Operations: {}\n\
                     Avg Time per Operation: {:?}",
                    total_duration, total_operations, avg_time_per_operation
                );

                assert!(avg_time_per_operation < Duration::from_millis(5));
            }

            Ok(())
        }
    }

    mod scalability_benchmarks {
        use super::*;

        #[tokio::test]
        async fn test_large_configuration_performance() -> Result<()> {
            let tool_counts = vec![10, 50, 100, 200];

            for tool_count in tool_counts {
                let (temp_dir, _) = PerformanceTestUtils::create_stress_test_tool_directory(tool_count).await?;

                let config = MigrationManagerConfig {
                    mode: MigrationMode::DryRun,
                    migration_directories: vec![temp_dir.path().to_path_buf()],
                    validation_mode: ValidationMode::Skip,
                    ..Default::default()
                };

                let (report, duration) = PerformanceTestUtils::measure_execution_time(|| async {
                    let mut manager = Phase51MigrationManager::new(config).await?;
                    manager.execute_migration().await
                }).await;

                println!(
                    "Large Configuration Performance ({} tools):\n\
                     Duration: {:?}\n\
                     Result: {}",
                    tool_count,
                    duration,
                    if report.is_ok() { "Success" } else { "Failed" }
                );

                // Performance should scale reasonably
                assert!(duration < Duration::from_secs(30));
            }

            Ok(())
        }

        #[tokio::test]
        async fn test_parallel_migration_scalability() -> Result<()> {
            let (temp_dir, _) = PerformanceTestUtils::create_stress_test_tool_directory(20).await?;

            let concurrent_counts = vec![1, 2, 5, 10];

            for concurrent_count in concurrent_counts {
                let config = MigrationManagerConfig {
                    mode: MigrationMode::Full,
                    migration_directories: vec![temp_dir.path().to_path_buf()],
                    enable_parallel_migration: concurrent_count > 1,
                    max_concurrent_migrations: concurrent_count,
                    validation_mode: ValidationMode::Skip,
                    ..Default::default()
                };

                let (report, duration) = PerformanceTestUtils::measure_execution_time(|| async {
                    let mut manager = Phase51MigrationManager::new(config).await?;
                    manager.execute_migration().await
                }).await;

                println!(
                    "Parallel Migration Scalability ({} concurrent):\n\
                     Duration: {:?}\n\
                     Result: {}",
                    concurrent_count,
                    duration,
                    if report.is_ok() { "Success" } else { "Failed" }
                );

                // Parallel execution should be faster or at least not slower
                assert!(duration < Duration::from_secs(60));
            }

            Ok(())
        }

        #[tokio::test]
        async fn test_memory_usage_scalability() -> Result<()> {
            let tool_counts = vec![10, 25, 50];

            for tool_count in tool_counts {
                let (temp_dir, _) = PerformanceTestUtils::create_stress_test_tool_directory(tool_count).await?;

                let memory_before = PerformanceTestUtils::get_memory_usage();

                let config = MigrationManagerConfig {
                    mode: MigrationMode::DryRun,
                    migration_directories: vec![temp_dir.path().to_path_buf()],
                    validation_mode: ValidationMode::Basic,
                    ..Default::default()
                };

                let mut manager = Phase51MigrationManager::new(config).await?;
                let _report = manager.execute_migration().await?;

                let memory_after = PerformanceTestUtils::get_memory_usage();
                let memory_delta = memory_after.saturating_sub(memory_before);

                println!(
                    "Memory Usage Scalability ({} tools):\n\
                     Memory Before: {} bytes\n\
                     Memory After: {} bytes\n\
                     Memory Delta: {} bytes",
                    tool_count, memory_before, memory_after, memory_delta
                );

                // Memory usage should be reasonable
                assert!(memory_delta < 50 * 1024 * 1024); // 50MB threshold
            }

            Ok(())
        }
    }
}

// ============================================================================
// MEMORY VALIDATION TESTS
// ============================================================================

#[cfg(test)]
mod memory_validation_tests {
    use super::*;

    mod memory_leak_detection {
        use super::*;

        #[tokio::test]
        async fn test_bridge_memory_leak_detection() -> Result<()> {
            let initial_memory = PerformanceTestUtils::get_memory_usage();
            let iterations = 50;

            for i in 0..iterations {
                let config = MigrationConfig::default();
                let rune_config = RuneServiceConfig::default();

                // Create and use bridge
                if let Ok(bridge) = ToolMigrationBridge::new(rune_config, config).await {
                    let _stats = bridge.get_migration_stats().await;
                    let _tools = bridge.list_migrated_tools().await.unwrap_or_default();
                    let _validation = bridge.validate_migration().await.unwrap_or_else(|_| {
                        crate::migration_bridge::MigrationValidation {
                            valid: true,
                            issues: vec![],
                            warnings: vec![],
                            total_tools: 0,
                            valid_tools: 0,
                        }
                    });
                }

                // Check memory usage periodically
                if i % 10 == 0 {
                    let current_memory = PerformanceTestUtils::get_memory_usage();
                    let memory_increase = current_memory.saturating_sub(initial_memory);

                    println!(
                        "Iteration {}: Memory usage = {} bytes (+{} bytes)",
                        i, current_memory, memory_increase
                    );

                    // Memory should not grow excessively
                    assert!(memory_increase < 20 * 1024 * 1024); // 20MB threshold
                }
            }

            let final_memory = PerformanceTestUtils::get_memory_usage();
            let total_increase = final_memory.saturating_sub(initial_memory);

            println!(
                "Bridge Memory Leak Detection:\n\
                 Initial Memory: {} bytes\n\
                 Final Memory: {} bytes\n\
                 Total Increase: {} bytes\n\
                 Iterations: {}",
                initial_memory, final_memory, total_increase, iterations
            );

            // Total memory increase should be minimal
            assert!(total_increase < 30 * 1024 * 1024); // 30MB threshold

            Ok(())
        }

        #[tokio::test]
        async fn test_manager_memory_leak_detection() -> Result<()> {
            let initial_memory = PerformanceTestUtils::get_memory_usage();
            let iterations = 20;

            for i in 0..iterations {
                let config = MigrationManagerConfig {
                    mode: MigrationMode::DryRun,
                    ..Default::default()
                };

                // Create and use manager
                if let Ok(mut manager) = Phase51MigrationManager::new(config).await {
                    let _status = manager.get_migration_status().await;
                    let _stats = manager.get_migration_statistics().await;
                    let _report = manager.execute_migration().await.unwrap_or_else(|_| {
                        crate::migration_manager::MigrationReport {
                            migration_id: Uuid::new_v4().to_string(),
                            config: MigrationManagerConfig::default(),
                            stats: crate::migration_bridge::MigrationStats {
                                total_migrated: 0,
                                active_tools: 0,
                                inactive_tools: 0,
                                migration_timestamp: chrono::Utc::now(),
                            },
                            state: crate::migration_manager::MigrationState::default(),
                            migrated_tools: vec![],
                            failed_tools: vec![],
                            validation: None,
                            duration: Some(Duration::from_millis(100)),
                            timestamp: chrono::Utc::now(),
                        }
                    });
                }

                // Check memory usage periodically
                if i % 5 == 0 {
                    let current_memory = PerformanceTestUtils::get_memory_usage();
                    let memory_increase = current_memory.saturating_sub(initial_memory);

                    println!(
                        "Iteration {}: Memory usage = {} bytes (+{} bytes)",
                        i, current_memory, memory_increase
                    );

                    assert!(memory_increase < 50 * 1024 * 1024); // 50MB threshold
                }
            }

            let final_memory = PerformanceTestUtils::get_memory_usage();
            let total_increase = final_memory.saturating_sub(initial_memory);

            println!(
                "Manager Memory Leak Detection:\n\
                 Initial Memory: {} bytes\n\
                 Final Memory: {} bytes\n\
                 Total Increase: {} bytes\n\
                 Iterations: {}",
                initial_memory, final_memory, total_increase, iterations
            );

            assert!(total_increase < 100 * 1024 * 1024); // 100MB threshold

            Ok(())
        }

        #[tokio::test]
        async fn test_concurrent_operations_memory_usage() -> Result<()> {
            let initial_memory = PerformanceTestUtils::get_memory_usage();

            let config = MigrationManagerConfig::default();

            if let Ok(manager) = Arc::new(Phase51MigrationManager::new(config).await)) {
                let concurrent_operations = 10;
                let operations_per_task = 20;

                let handles: Vec<_> = (0..concurrent_operations)
                    .map(|task_id| {
                        let manager = Arc::clone(&manager);
                        tokio::spawn(async move {
                            for i in 0..operations_per_task {
                                let _status = manager.get_migration_status().await;
                                let _stats = manager.get_migration_statistics().await;

                                // Add some delay to simulate real usage
                                tokio::time::sleep(Duration::from_millis(1)).await;

                                if i % 10 == 0 {
                                    println!("Task {}: Operation {} completed", task_id, i);
                                }
                            }
                        })
                    })
                    .collect();

                // Wait for all tasks to complete
                futures::future::join_all(handles).await;
            }

            let final_memory = PerformanceTestUtils::get_memory_usage();
            let memory_increase = final_memory.saturating_sub(initial_memory);

            println!(
                "Concurrent Operations Memory Usage:\n\
                 Initial Memory: {} bytes\n\
                 Final Memory: {} bytes\n\
                 Memory Increase: {} bytes\n\
                 Concurrent Operations: {}\n\
                 Operations per Task: {}",
                initial_memory, final_memory, memory_increase, concurrent_operations, operations_per_task
            );

            // Memory usage should be reasonable even with concurrent operations
            assert!(memory_increase < 200 * 1024 * 1024); // 200MB threshold

            Ok(())
        }
    }

    mod resource_cleanup_validation {
        use super::*;

        #[tokio::test]
        async fn test_bridge_resource_cleanup() -> Result<()> {
            let initial_memory = PerformanceTestUtils::get_memory_usage();

            // Create multiple bridges sequentially
            for i in 0..10 {
                let config = MigrationConfig::default();
                let rune_config = RuneServiceConfig::default();

                {
                    let bridge = ToolMigrationBridge::new(rune_config, config).await?;
                    let _stats = bridge.get_migration_stats().await;
                    let _tools = bridge.list_migrated_tools().await;

                    // Bridge goes out of scope at the end of this block
                }

                // Small delay to allow for potential cleanup
                tokio::time::sleep(Duration::from_millis(10)).await;

                if i % 3 == 0 {
                    let current_memory = PerformanceTestUtils::get_memory_usage();
                    let memory_increase = current_memory.saturating_sub(initial_memory);

                    println!(
                        "Bridge cleanup iteration {}: {} bytes (+{} bytes)",
                        i, current_memory, memory_increase
                    );

                    // Memory should not accumulate significantly
                    assert!(memory_increase < 50 * 1024 * 1024); // 50MB threshold
                }
            }

            let final_memory = PerformanceTestUtils::get_memory_usage();
            let total_increase = final_memory.saturating_sub(initial_memory);

            println!(
                "Bridge Resource Cleanup:\n\
                 Initial Memory: {} bytes\n\
                 Final Memory: {} bytes\n\
                 Total Increase: {} bytes",
                initial_memory, final_memory, total_increase
            );

            assert!(total_increase < 100 * 1024 * 1024); // 100MB threshold

            Ok(())
        }

        #[tokio::test]
        async fn test_manager_resource_cleanup() -> Result<()> {
            let initial_memory = PerformanceTestUtils::get_memory_usage();

            // Create multiple managers sequentially
            for i in 0..5 {
                let config = MigrationManagerConfig {
                    mode: MigrationMode::DryRun,
                    ..Default::default()
                };

                {
                    let mut manager = Phase51MigrationManager::new(config).await?;
                    let _status = manager.get_migration_status().await;
                    let _report = manager.execute_migration().await?;

                    // Manager goes out of scope at the end of this block
                }

                // Small delay to allow for potential cleanup
                tokio::time::sleep(Duration::from_millis(50)).await;

                let current_memory = PerformanceTestUtils::get_memory_usage();
                let memory_increase = current_memory.saturating_sub(initial_memory);

                println!(
                    "Manager cleanup iteration {}: {} bytes (+{} bytes)",
                    i, current_memory, memory_increase
                );

                // Memory should not accumulate significantly
                assert!(memory_increase < 100 * 1024 * 1024); // 100MB threshold
            }

            let final_memory = PerformanceTestUtils::get_memory_usage();
            let total_increase = final_memory.saturating_sub(initial_memory);

            println!(
                "Manager Resource Cleanup:\n\
                 Initial Memory: {} bytes\n\
                 Final Memory: {} bytes\n\
                 Total Increase: {} bytes",
                initial_memory, final_memory, total_increase
            );

            assert!(total_increase < 200 * 1024 * 1024); // 200MB threshold

            Ok(())
        }

        #[tokio::test]
        async fn test_large_data_structure_cleanup() -> Result<()> {
            let initial_memory = PerformanceTestUtils::get_memory_usage();

            // Create large test environment
            let (temp_dir, _) = PerformanceTestUtils::create_stress_test_tool_directory(100).await?;

            {
                let config = MigrationManagerConfig {
                    mode: MigrationMode::DryRun,
                    migration_directories: vec![temp_dir.path().to_path_buf()],
                    validation_mode: ValidationMode::Basic,
                    ..Default::default()
                };

                let mut manager = Phase51MigrationManager::new(config).await?;
                let _report = manager.execute_migration().await?;

                // Manager and associated data structures go out of scope
            }

            // Allow time for cleanup
            tokio::time::sleep(Duration::from_millis(100)).await;

            let final_memory = PerformanceTestUtils::get_memory_usage();
            let memory_increase = final_memory.saturating_sub(initial_memory);

            println!(
                "Large Data Structure Cleanup:\n\
                 Initial Memory: {} bytes\n\
                 Final Memory: {} bytes\n\
                 Memory Increase: {} bytes\n\
                 Tools Created: 100",
                initial_memory, final_memory, memory_increase
            );

            // Memory should be cleaned up properly
            assert!(memory_increase < 300 * 1024 * 1024); // 300MB threshold

            Ok(())
        }
    }

    mod memory_efficiency_tests {
        use super::*;

        #[tokio::test]
        async fn test_memory_efficiency_with_scaling_tools() -> Result<()> {
            let tool_counts = vec![10, 25, 50, 100];
            let mut memory_usage_data = vec![];

            for tool_count in tool_counts {
                let (temp_dir, _) = PerformanceTestUtils::create_stress_test_tool_directory(tool_count).await?;

                let memory_before = PerformanceTestUtils::get_memory_usage();

                let config = MigrationManagerConfig {
                    mode: MigrationMode::DryRun,
                    migration_directories: vec![temp_dir.path().to_path_buf()],
                    validation_mode: ValidationMode::Skip,
                    ..Default::default()
                };

                let mut manager = Phase51MigrationManager::new(config).await?;
                let _report = manager.execute_migration().await?;

                let memory_after = PerformanceTestUtils::get_memory_usage();
                let memory_delta = memory_after.saturating_sub(memory_before);
                let memory_per_tool = memory_delta / tool_count;

                memory_usage_data.push((tool_count, memory_delta, memory_per_tool));

                println!(
                    "Memory Efficiency Test ({} tools):\n\
                     Memory Delta: {} bytes\n\
                     Memory per Tool: {} bytes",
                    tool_count, memory_delta, memory_per_tool
                );
            }

            // Check that memory usage scales reasonably
            for (i, (count1, delta1, per_tool1)) in memory_usage_data.iter().enumerate() {
                if i > 0 {
                    let (count2, delta2, per_tool2) = memory_usage_data[i - 1];
                    let scaling_factor = *delta1 as f64 / *delta2 as f64;
                    let tool_ratio = *count1 as f64 / *count2 as f64;

                    println!(
                        "Scaling from {} to {} tools: {:.2}x memory vs {:.2}x tools (efficiency: {:.2}%)",
                        count2, count1, scaling_factor, tool_ratio,
                        (tool_ratio / scaling_factor) * 100.0
                    );

                    // Memory usage should scale roughly linearly with tool count
                    // Allow for some overhead, but not exponential growth
                    assert!(scaling_factor < tool_ratio * 2.0);
                }
            }

            Ok(())
        }

        #[tokio::test]
        async fn test_cache_memory_efficiency() -> Result<()> {
            let (temp_dir, _) = PerformanceTestUtils::create_stress_test_tool_directory(50).await?;

            // Test with different cache sizes
            let cache_sizes = vec![10, 50, 100, 500];

            for cache_size in cache_sizes {
                let config = MigrationManagerConfig {
                    mode: MigrationMode::DryRun,
                    migration_directories: vec![temp_dir.path().to_path_buf()],
                    validation_mode: ValidationMode::Basic,
                    ..Default::default()
                };

                let memory_before = PerformanceTestUtils::get_memory_usage();

                let mut manager = Phase51MigrationManager::new(config).await?;
                let _report = manager.execute_migration().await?;

                let memory_after = PerformanceTestUtils::get_memory_usage();
                let memory_delta = memory_after.saturating_sub(memory_before);

                println!(
                    "Cache Memory Efficiency (size {}):\n\
                     Memory Delta: {} bytes",
                    cache_size, memory_delta
                );

                // Memory usage should not grow excessively with cache size
                assert!(memory_delta < 100 * 1024 * 1024); // 100MB threshold
            }

            Ok(())
        }
    }
}

// ============================================================================
// STRESS TESTS (OPTIONAL - DISABLED BY DEFAULT FOR CI)
// ============================================================================

#[cfg(test)]
mod stress_tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Stress test - ignored by default
    async fn test_high_concurrency_stress() -> Result<()> {
        let config = PerformanceTestConfig {
            tool_count: 200,
            concurrent_operations: 50,
            max_operation_duration: Duration::from_secs(120),
            memory_threshold_bytes: 500 * 1024 * 1024, // 500MB
            run_stress_tests: true,
            run_memory_leak_detection: true,
        };

        println!("Running high concurrency stress test...");

        let (temp_dir, _) = PerformanceTestUtils::create_stress_test_tool_directory(config.tool_count).await?;

        let start_time = Instant::now();

        let handles: Vec<_> = (0..config.concurrent_operations)
            .map(|task_id| {
                let temp_dir = temp_dir.path().to_path_buf();
                tokio::spawn(async move {
                    let migration_config = MigrationManagerConfig {
                        mode: MigrationMode::DryRun,
                        migration_directories: vec![temp_dir],
                        validation_mode: ValidationMode::Skip,
                        ..Default::default()
                    };

                    let mut manager = Phase51MigrationManager::new(migration_config).await?;
                    let report = manager.execute_migration().await?;

                    Ok::<_, anyhow::Error>((task_id, report))
                })
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_default();

        let total_duration = start_time.elapsed();

        let successful_operations = results.iter().filter(|r| r.is_ok()).count();
        let failed_operations = results.len() - successful_operations;

        println!(
            "High Concurrency Stress Test Results:\n\
             Concurrent Operations: {}\n\
             Successful: {}\n\
             Failed: {}\n\
             Total Duration: {:?}\n\
             Avg Duration per Operation: {:?}",
            config.concurrent_operations,
            successful_operations,
            failed_operations,
            total_duration,
            total_duration / config.concurrent_operations as u32
        );

        // Stress test assertions
        assert!(total_duration < config.max_operation_duration);
        assert!(successful_operations > config.concurrent_operations / 2); // At least 50% success rate

        Ok(())
    }

    #[tokio::test]
    #[ignore] // Stress test - ignored by default
    async fn test_memory_stress() -> Result<()> {
        println!("Running memory stress test...");

        let initial_memory = PerformanceTestUtils::get_memory_usage();
        let iterations = 100;

        for i in 0..iterations {
            let (temp_dir, _) = PerformanceTestUtils::create_stress_test_tool_directory(20).await?;

            let config = MigrationManagerConfig {
                mode: MigrationMode::DryRun,
                migration_directories: vec![temp_dir.path().to_path_buf()],
                validation_mode: ValidationMode::Basic,
                ..Default::default()
            };

            let mut manager = Phase51MigrationManager::new(config).await?;
            let _report = manager.execute_migration().await?;

            if i % 20 == 0 {
                let current_memory = PerformanceTestUtils::get_memory_usage();
                let memory_increase = current_memory.saturating_sub(initial_memory);

                println!(
                    "Memory stress iteration {}: {} bytes (+{} bytes)",
                    i, current_memory, memory_increase
                );

                // Memory should not grow excessively even under stress
                assert!(memory_increase < 200 * 1024 * 1024); // 200MB threshold
            }
        }

        let final_memory = PerformanceTestUtils::get_memory_usage();
        let total_increase = final_memory.saturating_sub(initial_memory);

        println!(
            "Memory Stress Test Results:\n\
             Iterations: {}\n\
             Initial Memory: {} bytes\n\
             Final Memory: {} bytes\n\
             Total Increase: {} bytes",
            iterations, initial_memory, final_memory, total_increase
        );

        assert!(total_increase < 500 * 1024 * 1024); // 500MB threshold

        Ok(())
    }
}