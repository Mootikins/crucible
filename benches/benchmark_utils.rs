//! Utility functions and shared components for comprehensive benchmarking

use criterion::{BenchmarkId, Throughput};
use crucible_tools::{ScriptEngine, RuneRegistry, ToolRegistry};
use crucible_services::plugin_events::{EventBridge, SubscriptionManager};
use crucible_core::types::{Document, Event};
use std::sync::Arc;
use tokio::runtime::Runtime;
use tempfile::TempDir;
use std::time::Duration;

/// Test data generator for benchmarks
pub struct TestDataGenerator {
    temp_dir: TempDir,
}

impl TestDataGenerator {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            temp_dir: TempDir::new()?,
        })
    }

    /// Generate test documents of varying sizes
    pub fn generate_documents(&self, count: usize, size_kb: usize) -> Vec<Document> {
        let mut documents = Vec::with_capacity(count);
        let base_content = "x".repeat(size_kb * 1024);

        for i in 0..count {
            let document = Document {
                id: format!("doc_{}", i),
                title: format!("Test Document {}", i),
                content: format!("{} - {}", base_content, i),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                tags: vec!["test".to_string(), format!("batch_{}", i / 100)],
                metadata: serde_json::json!({
                    "size": size_kb,
                    "batch": i / 100
                }),
            };
            documents.push(document);
        }
        documents
    }

    /// Generate test events of varying types
    pub fn generate_events(&self, count: usize, event_types: &[&str]) -> Vec<Event> {
        let mut events = Vec::with_capacity(count);

        for i in 0..count {
            let event_type = event_types[i % event_types.len()];
            let event = Event {
                id: format!("event_{}", i),
                event_type: event_type.to_string(),
                data: serde_json::json!({
                    "payload": format!("test data for event {}", i),
                    "timestamp": chrono::Utc::now().timestamp_millis(),
                    "size": i * 10
                }),
                timestamp: chrono::Utc::now(),
                source: "benchmark".to_string(),
            };
            events.push(event);
        }
        events
    }

    /// Create test files for file operation benchmarks
    pub fn create_test_files(&self, file_count: usize, file_size_kb: usize) -> anyhow::Result<Vec<std::path::PathBuf>> {
        let mut file_paths = Vec::with_capacity(file_count);
        let content = "x".repeat(file_size_kb * 1024);

        for i in 0..file_count {
            let file_path = self.temp_dir.path().join(format!("test_file_{}.txt", i));
            std::fs::write(&file_path, &content)?;
            file_paths.push(file_path);
        }

        Ok(file_paths)
    }

    pub fn temp_dir(&self) -> &std::path::Path {
        self.temp_dir.path()
    }
}

/// Performance test configuration
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub small_dataset: usize,
    pub medium_dataset: usize,
    pub large_dataset: usize,
    pub iterations: u32,
    pub warmup_iterations: u32,
    pub sample_size: u32,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            small_dataset: 10,
            medium_dataset: 100,
            large_dataset: 1000,
            iterations: 100,
            warmup_iterations: 10,
            sample_size: 50,
        }
    }
}

/// Resource usage monitor
pub struct ResourceMonitor {
    start_time: std::time::Instant,
    start_memory: Option<usize>,
}

impl ResourceMonitor {
    pub fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            start_memory: Self::get_memory_usage(),
        }
    }

    fn get_memory_usage() -> Option<usize> {
        // Simple memory monitoring implementation
        // In a real scenario, you might use more sophisticated methods
        #[cfg(target_os = "linux")]
        {
            use std::fs;
            if let Ok(status) = fs::read_to_string("/proc/self/status") {
                for line in status.lines() {
                    if line.starts_with("VmRSS:") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            if let Ok(kb) = parts[1].parse::<usize>() {
                                return Some(kb * 1024); // Convert to bytes
                            }
                        }
                    }
                }
            }
        }
        None
    }

    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn memory_diff(&self) -> Option<isize> {
        let current = Self::get_memory_usage()?;
        let start = self.start_memory?;
        Some(current as isize - start as isize)
    }
}

/// Benchmark runner for async operations
pub fn run_async_benchmark<F, Fut>(rt: &Runtime, operation: F) -> Fut::Output
where
    F: Fn() -> Fut,
    Fut: std::future::Future,
{
    rt.block_on(operation())
}

/// Tool complexity levels for benchmarking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolComplexity {
    Simple,
    Medium,
    Complex,
}

impl ToolComplexity {
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolComplexity::Simple => "simple",
            ToolComplexity::Medium => "medium",
            ToolComplexity::Complex => "complex",
        }
    }
}

/// Concurrency levels for testing
pub struct ConcurrencyLevels;

impl ConcurrencyLevels {
    pub const SINGLE: usize = 1;
    pub const LOW: usize = 4;
    pub const MEDIUM: usize = 16;
    pub const HIGH: usize = 64;
}

/// Benchmark result collector
#[derive(Debug)]
pub struct BenchmarkResult {
    pub name: String,
    pub avg_time: Duration,
    pub min_time: Duration,
    pub max_time: Duration,
    pub memory_usage: Option<usize>,
    pub throughput: Option<f64>,
}

impl BenchmarkResult {
    pub fn new(name: String) -> Self {
        Self {
            name,
            avg_time: Duration::ZERO,
            min_time: Duration::MAX,
            max_time: Duration::ZERO,
            memory_usage: None,
            throughput: None,
        }
    }
}

/// Generate performance comparison report
pub struct PerformanceReport {
    pub results: Vec<BenchmarkResult>,
    pub baseline_comparison: Option<BaselineComparison>,
}

#[derive(Debug)]
pub struct BaselineComparison {
    pub old_architecture: Vec<BenchmarkResult>,
    pub improvement_percentages: Vec<f64>,
}

impl PerformanceReport {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
            baseline_comparison: None,
        }
    }

    pub fn add_result(&mut self, result: BenchmarkResult) {
        self.results.push(result);
    }

    pub fn generate_markdown(&self) -> String {
        let mut report = String::new();
        report.push_str("# Phase 6.1 Performance Benchmarking Results\n\n");

        report.push_str("## Summary\n\n");
        report.push_str("Comprehensive performance benchmarks for the new Crucible architecture.\n\n");

        if let Some(comparison) = &self.baseline_comparison {
            report.push_str("### Architecture Improvements\n\n");
            report.push_str("| Metric | Old Architecture | New Architecture | Improvement |\n");
            report.push_str("|--------|------------------|------------------|-------------|\n");

            for (i, improvement) in comparison.improvement_percentages.iter().enumerate() {
                let old_result = &comparison.old_architecture[i];
                let new_result = &self.results.get(i).unwrap_or(old_result);

                report.push_str(&format!(
                    "| {} | {:?} | {:?} | {:.1}% |\n",
                    old_result.name,
                    old_result.avg_time,
                    new_result.avg_time,
                    improvement
                ));
            }
            report.push_str("\n");
        }

        report.push_str("## Detailed Results\n\n");
        for result in &self.results {
            report.push_str(&format!(
                "### {}\n\n",
                result.name
            ));
            report.push_str(&format!(
                "- **Average Time**: {:?}\n",
                result.avg_time
            ));
            report.push_str(&format!(
                "- **Min Time**: {:?}\n",
                result.min_time
            ));
            report.push_str(&format!(
                "- **Max Time**: {:?}\n",
                result.max_time
            ));
            if let Some(memory) = result.memory_usage {
                report.push_str(&format!(
                    "- **Memory Usage**: {} bytes\n",
                    memory
                ));
            }
            if let Some(throughput) = result.throughput {
                report.push_str(&format!(
                    "- **Throughput**: {:.2} ops/sec\n",
                    throughput
                ));
            }
            report.push_str("\n");
        }

        report
    }
}