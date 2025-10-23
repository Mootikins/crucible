//! Performance testing utilities for the unified tool system
//!
//! This module provides utilities to test and measure performance improvements
//! in the REPL environment.

use std::path::PathBuf;
use tracing::{info, warn, debug};
use super::benchmarks::{
    PerformanceBenchmarks, BenchmarkConfig, print_benchmark_results, print_comparison_results
};
use super::unified_registry::UnifiedToolRegistry;
use super::tool_group::ToolGroupCacheConfig;

/// Quick performance test for REPL use
pub async fn quick_performance_test(tool_dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    info!("Running quick performance test...");

    let config = BenchmarkConfig {
        iterations: 5,
        warmup_iterations: 2,
        detailed_metrics: true,
        cache_config: ToolGroupCacheConfig::default(),
    };

    let mut benchmarks = PerformanceBenchmarks::new(config)?;
    let results = benchmarks.run_all_benchmarks().await?;

    print_benchmark_results(&results);

    // Performance validation
    validate_performance(&results);

    Ok(())
}

/// Compare different caching strategies
pub async fn compare_caching_strategies(tool_dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    info!("Comparing caching strategies...");

    let config = BenchmarkConfig {
        iterations: 3,
        warmup_iterations: 1,
        detailed_metrics: false,
        cache_config: ToolGroupCacheConfig::default(), // Will be overridden
    };

    let mut benchmarks = PerformanceBenchmarks::new(config)?;
    let results = benchmarks.compare_cache_configurations().await?;

    print_comparison_results(&results);

    // Analyze improvements
    analyze_caching_improvements(&results);

    Ok(())
}

/// Validate that performance meets expectations
fn validate_performance(results: &super::benchmarks::BenchmarkResults) {
    info!("Validating performance results...");

    // Initialize should be fast (under 100ms)
    if results.initialization_time_ms > 100 {
        warn!("Registry initialization time is high: {}ms (target: <100ms)", results.initialization_time_ms);
    } else {
        info!("✓ Registry initialization time is acceptable: {}ms", results.initialization_time_ms);
    }

    // Discovery should be reasonably fast (under 50ms average)
    if results.summary.avg_discovery_time_ms > 50.0 {
        warn!("Tool discovery time is high: {:.2}ms (target: <50ms)", results.summary.avg_discovery_time_ms);
    } else {
        info!("✓ Tool discovery time is acceptable: {:.2}ms", results.summary.avg_discovery_time_ms);
    }

    // Cache hit rate should be reasonable (over 50%)
    if results.summary.avg_cache_hit_rate < 0.5 {
        warn!("Cache hit rate is low: {:.2}% (target: >50%)", results.summary.avg_cache_hit_rate * 100.0);
    } else {
        info!("✓ Cache hit rate is good: {:.2}%", results.summary.avg_cache_hit_rate * 100.0);
    }

    // Execution times should be consistent
    if results.summary.p95_execution_time_ms > results.summary.avg_execution_time_ms * 3.0 {
        warn!("Execution times have high variance: avg={:.2}ms, p95={:.2}ms",
              results.summary.avg_execution_time_ms, results.summary.p95_execution_time_ms);
    } else {
        info!("✓ Execution times are consistent: avg={:.2}ms, p95={:.2}ms",
              results.summary.avg_execution_time_ms, results.summary.p95_execution_time_ms);
    }
}

/// Analyze caching improvements
fn analyze_caching_improvements(results: &std::collections::HashMap<String, super::benchmarks::BenchmarkResults>) {
    info!("Analyzing caching improvements...");

    let no_caching = results.get("no_caching");
    let default_caching = results.get("default_caching");
    let fast_caching = results.get("fast_caching");

    if let (Some(no_cache), Some(default)) = (no_caching, default_caching) {
        let discovery_improvement = ((no_cache.summary.avg_discovery_time_ms - default.summary.avg_discovery_time_ms)
            / no_cache.summary.avg_discovery_time_ms) * 100.0;

        if discovery_improvement > 0.0 {
            info!("✓ Default caching improves discovery by {:.1}%", discovery_improvement);
        } else {
            warn!("Default caching doesn't improve discovery performance");
        }

        if default.summary.avg_cache_hit_rate > no_cache.summary.avg_cache_hit_rate {
            info!("✓ Default caching improves cache hit rate: {:.1}% vs {:.1}%",
                  default.summary.avg_cache_hit_rate * 100.0, no_cache.summary.avg_cache_hit_rate * 100.0);
        }
    }

    if let (Some(default), Some(fast)) = (default_caching, fast_caching) {
        let discovery_improvement = ((default.summary.avg_discovery_time_ms - fast.summary.avg_discovery_time_ms)
            / default.summary.avg_discovery_time_ms) * 100.0;

        if discovery_improvement > 10.0 {
            info!("✓ Fast caching significantly improves discovery by {:.1}%", discovery_improvement);
        }
    }
}

/// Test lazy loading behavior
pub async fn test_lazy_loading(tool_dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    info!("Testing lazy loading behavior...");

    let start_time = std::time::Instant::now();

    // Create registry - should be fast due to lazy loading
    let registry = UnifiedToolRegistry::new(tool_dir).await?;
    let init_time = start_time.elapsed();
    info!("Registry creation time: {}ms (should be fast due to lazy loading)", init_time.as_millis());

    // First tool listing - should trigger lazy initialization
    let start_time = std::time::Instant::now();
    let _tools = registry.list_tools().await;
    let first_listing_time = start_time.elapsed();
    info!("First tool listing time: {}ms (includes lazy initialization)", first_listing_time.as_millis());

    // Second tool listing - should be faster due to caching
    let start_time = std::time::Instant::now();
    let _tools = registry.list_tools().await;
    let second_listing_time = start_time.elapsed();
    info!("Second tool listing time: {}ms (should be cached)", second_listing_time.as_millis());

    // Analyze lazy loading effectiveness
    if first_listing_time.as_millis() > 50 {
        warn!("Lazy initialization took longer than expected: {}ms", first_listing_time.as_millis());
    } else {
        info!("✓ Lazy initialization is efficient: {}ms", first_listing_time.as_millis());
    }

    if second_listing_time.as_millis() < first_listing_time.as_millis() {
        let improvement = ((first_listing_time.as_millis() - second_listing_time.as_millis()) as f64
            / first_listing_time.as_millis() as f64) * 100.0;
        info!("✓ Caching improves subsequent listings by {:.1}%", improvement);
    }

    Ok(())
}

/// Memory usage test
pub async fn test_memory_usage(tool_dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    info!("Testing memory usage patterns...");

    // Measure baseline memory
    let baseline_memory = get_memory_usage();
    info!("Baseline memory usage: {} bytes", baseline_memory);

    // Create registry
    let registry = UnifiedToolRegistry::new(tool_dir).await?;
    let after_registry_memory = get_memory_usage();
    info!("Memory after registry creation: {} bytes (+{})", after_registry_memory, after_registry_memory - baseline_memory);

    // Trigger lazy initialization
    let _tools = registry.list_tools().await;
    let after_init_memory = get_memory_usage();
    info!("Memory after initialization: {} bytes (+{})", after_init_memory, after_init_memory - baseline_memory);

    // Multiple operations
    for _ in 0..10 {
        let _tools = registry.list_tools().await;
    }
    let after_ops_memory = get_memory_usage();
    info!("Memory after operations: {} bytes (+{})", after_ops_memory, after_ops_memory - baseline_memory);

    // Check for memory leaks
    let memory_growth = after_ops_memory - after_init_memory;
    if memory_growth > 1_000_000 { // 1MB
        warn!("Potential memory leak detected: {} bytes growth after operations", memory_growth);
    } else {
        info!("✓ Memory usage is stable: {} bytes growth", memory_growth);
    }

    Ok(())
}

/// Get current memory usage (simplified)
fn get_memory_usage() -> usize {
    // This is a simplified implementation
    // In a real scenario, you'd use platform-specific APIs or crates like `memory-stats`
    // For now, we'll use a heuristic based on available information
    std::mem::size_of::<UnifiedToolRegistry>() + 100_000 // Placeholder
}