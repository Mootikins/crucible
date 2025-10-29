//! Performance benchmarks for the unified tool system
//!
//! This module provides comprehensive benchmarks to measure and validate
//! the performance improvements from lazy loading and caching optimizations.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::{debug, info};

use super::tool_group::ToolGroupCacheConfig;
use super::unified_registry::UnifiedToolRegistry;

/// Benchmark configuration
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    /// Number of iterations for each benchmark
    pub iterations: usize,
    /// Number of warmup iterations (not measured)
    pub warmup_iterations: usize,
    /// Whether to include detailed metrics collection
    pub detailed_metrics: bool,
    /// Cache configuration to test
    pub cache_config: ToolGroupCacheConfig,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            iterations: 10,
            warmup_iterations: 3,
            detailed_metrics: true,
            cache_config: ToolGroupCacheConfig::default(),
        }
    }
}

/// Benchmark results
#[derive(Debug, Clone)]
pub struct BenchmarkResults {
    /// Registry initialization time
    pub initialization_time_ms: u64,
    /// Tool discovery times
    pub discovery_times_ms: Vec<u64>,
    /// Tool execution times
    pub execution_times_ms: Vec<u64>,
    /// Cache hit rates
    pub cache_hit_rates: Vec<f64>,
    /// Memory usage samples
    pub memory_usage_bytes: Vec<usize>,
    /// Summary statistics
    pub summary: BenchmarkSummary,
}

/// Summary statistics for benchmarks
#[derive(Debug, Clone)]
pub struct BenchmarkSummary {
    /// Average discovery time (ms)
    pub avg_discovery_time_ms: f64,
    /// Median discovery time (ms)
    pub median_discovery_time_ms: f64,
    /// P95 discovery time (ms)
    pub p95_discovery_time_ms: f64,
    /// Average execution time (ms)
    pub avg_execution_time_ms: f64,
    /// Median execution time (ms)
    pub median_execution_time_ms: f64,
    /// P95 execution time (ms)
    pub p95_execution_time_ms: f64,
    /// Average cache hit rate
    pub avg_cache_hit_rate: f64,
    /// Total benchmark time (ms)
    pub total_time_ms: u64,
}

/// Performance benchmark suite
pub struct PerformanceBenchmarks {
    config: BenchmarkConfig,
}

impl PerformanceBenchmarks {
    /// Create a new benchmark suite
    pub fn new(config: BenchmarkConfig) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self { config })
    }

    /// Run all benchmarks
    pub async fn run_all_benchmarks(
        &mut self,
    ) -> Result<BenchmarkResults, Box<dyn std::error::Error>> {
        info!(
            "Starting performance benchmarks with {} iterations",
            self.config.iterations
        );

        let start_time = Instant::now();
        let mut results = BenchmarkResults {
            initialization_time_ms: 0,
            discovery_times_ms: Vec::new(),
            execution_times_ms: Vec::new(),
            cache_hit_rates: Vec::new(),
            memory_usage_bytes: Vec::new(),
            summary: BenchmarkSummary {
                avg_discovery_time_ms: 0.0,
                median_discovery_time_ms: 0.0,
                p95_discovery_time_ms: 0.0,
                avg_execution_time_ms: 0.0,
                median_execution_time_ms: 0.0,
                p95_execution_time_ms: 0.0,
                avg_cache_hit_rate: 0.0,
                total_time_ms: 0,
            },
        };

        // Benchmark 1: Registry initialization
        results.initialization_time_ms = self.benchmark_registry_initialization().await?;

        // Benchmark 2: Tool discovery
        results.discovery_times_ms = self.benchmark_tool_discovery().await?;

        // Benchmark 3: Tool execution
        results.execution_times_ms = self.benchmark_tool_execution().await?;

        // Benchmark 4: Cache performance
        results.cache_hit_rates = self.benchmark_cache_performance().await?;

        // Calculate summary statistics
        results.summary = self.calculate_summary(&results, start_time.elapsed());

        let total_time = start_time.elapsed();
        info!("All benchmarks completed in {}ms", total_time.as_millis());

        Ok(results)
    }

    /// Benchmark registry initialization time
    async fn benchmark_registry_initialization(&self) -> Result<u64, Box<dyn std::error::Error>> {
        info!("Benchmarking registry initialization...");

        let mut times = Vec::new();

        for i in 0..self.config.iterations + self.config.warmup_iterations {
            let start_time = Instant::now();

            let tool_dir = PathBuf::from("/tmp/test_tools");
            let registry =
                UnifiedToolRegistry::with_cache_config(tool_dir, self.config.cache_config.clone())
                    .await?;

            let duration = start_time.elapsed().as_millis() as u64;

            if i >= self.config.warmup_iterations {
                times.push(duration);
                debug!(
                    "Initialization iteration {}: {}ms",
                    i - self.config.warmup_iterations + 1,
                    duration
                );
            }

            // Cleanup
            drop(registry);
        }

        let avg_time = times.iter().sum::<u64>() as f64 / times.len() as f64;
        info!("Registry initialization: {:.2}ms average", avg_time);

        Ok(avg_time as u64)
    }

    /// Benchmark tool discovery performance
    async fn benchmark_tool_discovery(&mut self) -> Result<Vec<u64>, Box<dyn std::error::Error>> {
        info!("Benchmarking tool discovery...");

        let tool_dir = PathBuf::from("/tmp/test_tools");
        let registry =
            UnifiedToolRegistry::with_cache_config(tool_dir, self.config.cache_config.clone())
                .await?;

        let mut times = Vec::new();

        // Warmup
        for _ in 0..self.config.warmup_iterations {
            let _tools = registry.list_tools().await;
        }

        // Actual benchmark
        for i in 0..self.config.iterations {
            let start_time = Instant::now();
            let _tools = registry.list_tools().await;
            let duration = start_time.elapsed().as_millis() as u64;

            times.push(duration);
            debug!("Discovery iteration {}: {}ms", i + 1, duration);

            // Small delay between iterations
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let avg_time = times.iter().sum::<u64>() as f64 / times.len() as f64;
        info!("Tool discovery: {:.2}ms average", avg_time);

        Ok(times)
    }

    /// Benchmark tool execution performance
    async fn benchmark_tool_execution(&mut self) -> Result<Vec<u64>, Box<dyn std::error::Error>> {
        info!("Benchmarking tool execution...");

        let tool_dir = PathBuf::from("/tmp/test_tools");
        let registry =
            UnifiedToolRegistry::with_cache_config(tool_dir, self.config.cache_config.clone())
                .await?;

        let mut times = Vec::new();

        // Test tools that should be available
        let test_tools = vec![("system_info", vec![]), ("get_kiln_stats", vec![])];

        // Warmup
        for _ in 0..self.config.warmup_iterations {
            for (tool_name, args) in &test_tools {
                let _result = registry.execute_tool(tool_name, args).await;
            }
        }

        // Actual benchmark
        for i in 0..self.config.iterations {
            for (tool_name, args) in &test_tools {
                let start_time = Instant::now();
                let _result = registry.execute_tool(tool_name, args).await;
                let duration = start_time.elapsed().as_millis() as u64;

                times.push(duration);
                debug!(
                    "Execution iteration {} ({}): {}ms",
                    i + 1,
                    tool_name,
                    duration
                );
            }

            // Small delay between iterations
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let avg_time = times.iter().sum::<u64>() as f64 / times.len() as f64;
        info!("Tool execution: {:.2}ms average", avg_time);

        Ok(times)
    }

    /// Benchmark cache performance
    async fn benchmark_cache_performance(
        &mut self,
    ) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
        info!("Benchmarking cache performance...");

        let tool_dir = PathBuf::from("/tmp/test_tools");
        let registry =
            UnifiedToolRegistry::with_cache_config(tool_dir, self.config.cache_config.clone())
                .await?;

        let mut hit_rates = Vec::new();

        // Perform multiple discovery operations to test cache
        for i in 0..self.config.iterations {
            // First discovery (cache miss)
            let _tools1 = registry.list_tools().await;

            // Second discovery (cache hit)
            let _tools2 = registry.list_tools().await;

            // Get metrics to calculate hit rate
            let metrics = registry.get_performance_metrics().await;
            let total_hits: u64 = metrics.group_metrics.values().map(|m| m.cache_hits).sum();
            let total_requests: u64 = metrics
                .group_metrics
                .values()
                .map(|m| m.cache_hits + m.cache_misses)
                .sum();

            let hit_rate = if total_requests > 0 {
                total_hits as f64 / total_requests as f64
            } else {
                0.0
            };

            hit_rates.push(hit_rate);
            debug!(
                "Cache iteration {}: {:.2}% hit rate",
                i + 1,
                hit_rate * 100.0
            );

            // Wait a moment between iterations
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let avg_hit_rate = hit_rates.iter().sum::<f64>() / hit_rates.len() as f64;
        info!(
            "Cache performance: {:.2}% average hit rate",
            avg_hit_rate * 100.0
        );

        Ok(hit_rates)
    }

    /// Calculate summary statistics
    fn calculate_summary(
        &self,
        results: &BenchmarkResults,
        total_duration: Duration,
    ) -> BenchmarkSummary {
        let mut sorted_times = results.discovery_times_ms.clone();
        sorted_times.sort_unstable();

        let avg_discovery = if !results.discovery_times_ms.is_empty() {
            results.discovery_times_ms.iter().sum::<u64>() as f64
                / results.discovery_times_ms.len() as f64
        } else {
            0.0
        };

        let median_discovery = if !sorted_times.is_empty() {
            let mid = sorted_times.len() / 2;
            if sorted_times.len() % 2 == 0 {
                (sorted_times[mid - 1] + sorted_times[mid]) as f64 / 2.0
            } else {
                sorted_times[mid] as f64
            }
        } else {
            0.0
        };

        let p95_discovery = if !sorted_times.is_empty() {
            let p95_index =
                ((sorted_times.len() as f64 * 0.95) as usize).min(sorted_times.len() - 1);
            sorted_times[p95_index] as f64
        } else {
            0.0
        };

        let mut sorted_exec_times = results.execution_times_ms.clone();
        sorted_exec_times.sort_unstable();

        let avg_execution = if !results.execution_times_ms.is_empty() {
            results.execution_times_ms.iter().sum::<u64>() as f64
                / results.execution_times_ms.len() as f64
        } else {
            0.0
        };

        let median_execution = if !sorted_exec_times.is_empty() {
            let mid = sorted_exec_times.len() / 2;
            if sorted_exec_times.len() % 2 == 0 {
                (sorted_exec_times[mid - 1] + sorted_exec_times[mid]) as f64 / 2.0
            } else {
                sorted_exec_times[mid] as f64
            }
        } else {
            0.0
        };

        let p95_execution = if !sorted_exec_times.is_empty() {
            let p95_index =
                ((sorted_exec_times.len() as f64 * 0.95) as usize).min(sorted_exec_times.len() - 1);
            sorted_exec_times[p95_index] as f64
        } else {
            0.0
        };

        let avg_cache_hit_rate = if !results.cache_hit_rates.is_empty() {
            results.cache_hit_rates.iter().sum::<f64>() / results.cache_hit_rates.len() as f64
        } else {
            0.0
        };

        BenchmarkSummary {
            avg_discovery_time_ms: avg_discovery,
            median_discovery_time_ms: median_discovery,
            p95_discovery_time_ms: p95_discovery,
            avg_execution_time_ms: avg_execution,
            median_execution_time_ms: median_execution,
            p95_execution_time_ms: p95_execution,
            avg_cache_hit_rate,
            total_time_ms: total_duration.as_millis() as u64,
        }
    }

    /// Compare performance between different cache configurations
    pub async fn compare_cache_configurations(
        &mut self,
    ) -> Result<HashMap<String, BenchmarkResults>, Box<dyn std::error::Error>> {
        info!("Comparing different cache configurations...");

        let configs = vec![
            ("no_caching", ToolGroupCacheConfig::no_caching()),
            ("default_caching", ToolGroupCacheConfig::default()),
            ("fast_caching", ToolGroupCacheConfig::fast_cache()),
        ];

        let mut results = HashMap::new();

        for (name, cache_config) in configs {
            info!("Testing configuration: {}", name);
            let mut benchmark = PerformanceBenchmarks::new(BenchmarkConfig {
                cache_config,
                ..self.config.clone()
            })?;

            let result = benchmark.run_all_benchmarks().await?;
            results.insert(name.to_string(), result);

            // Wait between configurations
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        Ok(results)
    }
}

/// Print benchmark results in a formatted way
pub fn print_benchmark_results(results: &BenchmarkResults) {
    println!("\n=== Performance Benchmark Results ===\n");

    println!("Initialization Time: {}ms", results.initialization_time_ms);
    println!("Total Benchmark Time: {}ms", results.summary.total_time_ms);
    println!();

    println!("Tool Discovery Performance:");
    println!("  Average: {:.2}ms", results.summary.avg_discovery_time_ms);
    println!(
        "  Median:  {:.2}ms",
        results.summary.median_discovery_time_ms
    );
    println!("  P95:     {:.2}ms", results.summary.p95_discovery_time_ms);
    println!();

    println!("Tool Execution Performance:");
    println!("  Average: {:.2}ms", results.summary.avg_execution_time_ms);
    println!(
        "  Median:  {:.2}ms",
        results.summary.median_execution_time_ms
    );
    println!("  P95:     {:.2}ms", results.summary.p95_execution_time_ms);
    println!();

    println!("Cache Performance:");
    println!(
        "  Hit Rate: {:.2}%",
        results.summary.avg_cache_hit_rate * 100.0
    );
    println!();
}

/// Print comparison results
pub fn print_comparison_results(results: &HashMap<String, BenchmarkResults>) {
    println!("\n=== Cache Configuration Comparison ===\n");

    for (config_name, benchmark_results) in results {
        println!("Configuration: {}", config_name);
        println!(
            "  Initialization: {}ms",
            benchmark_results.initialization_time_ms
        );
        println!(
            "  Discovery Avg:  {:.2}ms",
            benchmark_results.summary.avg_discovery_time_ms
        );
        println!(
            "  Execution Avg:  {:.2}ms",
            benchmark_results.summary.avg_execution_time_ms
        );
        println!(
            "  Cache Hit Rate: {:.2}%",
            benchmark_results.summary.avg_cache_hit_rate * 100.0
        );
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_benchmarks() {
        let config = BenchmarkConfig {
            iterations: 3,
            warmup_iterations: 1,
            detailed_metrics: false,
            cache_config: ToolGroupCacheConfig::no_caching(),
        };

        let mut benchmarks = PerformanceBenchmarks::new(config).unwrap();
        let results = benchmarks.run_all_benchmarks().await.unwrap();

        assert!(results.initialization_time_ms > 0);
        assert!(!results.discovery_times_ms.is_empty());
        assert!(!results.execution_times_ms.is_empty());
    }
}
