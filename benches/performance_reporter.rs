//! Performance reporting and analysis tools
//!
//! This module provides comprehensive performance analysis, reporting,
//! and visualization capabilities for the benchmarking framework.

use std::collections::HashMap;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Comprehensive performance benchmark result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkMetric {
    pub name: String,
    pub category: String,
    pub subcategory: Option<String>,
    pub value: f64,
    pub unit: String,
    pub iterations: u32,
    pub sample_size: u32,
    pub std_deviation: Option<f64>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub percentile_95: Option<f64>,
    pub memory_usage_mb: Option<f64>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Benchmark suite results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSuite {
    pub name: String,
    pub version: String,
    pub commit_hash: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub system_info: SystemInfo,
    pub metrics: Vec<BenchmarkMetric>,
}

/// System information for benchmark context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
    pub cpu_cores: usize,
    pub memory_gb: f64,
    pub rust_version: String,
    pub compiler_flags: String,
}

/// Performance comparison between architectures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureComparison {
    pub baseline_metrics: Vec<BenchmarkMetric>,
    pub new_metrics: Vec<BenchmarkMetric>,
    pub improvements: Vec<PerformanceImprovement>,
}

/// Performance improvement calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceImprovement {
    pub metric_name: String,
    pub baseline_value: f64,
    pub new_value: f64,
    pub improvement_percentage: f64,
    pub significance_level: Option<f64>,
    pub confidence_interval: Option<(f64, f64)>,
}

/// Performance analyzer and reporter
pub struct PerformanceReporter {
    results: Vec<BenchmarkSuite>,
    comparisons: Vec<ArchitectureComparison>,
}

impl PerformanceReporter {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
            comparisons: Vec::new(),
        }
    }

    /// Add benchmark suite results
    pub fn add_suite(&mut self, suite: BenchmarkSuite) {
        self.results.push(suite);
    }

    /// Add architecture comparison
    pub fn add_comparison(&mut self, comparison: ArchitectureComparison) {
        self.comparisons.push(comparison);
    }

    /// Generate comprehensive performance report
    pub fn generate_comprehensive_report(&self) -> String {
        let mut report = String::new();

        report.push_str("# Phase 6.1: Comprehensive Performance Benchmarking Report\n\n");
        report.push_str(&format!("Generated: {}\n\n", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));

        // Executive Summary
        report.push_str("## Executive Summary\n\n");
        report.push_str(&self.generate_executive_summary());

        // Architecture Comparison
        if !self.comparisons.is_empty() {
            report.push_str("## Architecture Performance Comparison\n\n");
            report.push_str(&self.generate_architecture_comparison());
        }

        // Detailed Results by Category
        report.push_str("## Detailed Benchmark Results\n\n");
        report.push_str(&self.generate_detailed_results());

        // Performance Analysis
        report.push_str("## Performance Analysis\n\n");
        report.push_str(&self.generate_performance_analysis());

        // Recommendations
        report.push_str("## Optimization Recommendations\n\n");
        report.push_str(&self.generate_recommendations());

        // Statistical Analysis
        report.push_str("## Statistical Analysis\n\n");
        report.push_str(&self.generate_statistical_analysis());

        report
    }

    /// Generate executive summary
    fn generate_executive_summary(&self) -> String {
        let mut summary = String::new();

        if let Some(latest_suite) = self.results.last() {
            summary.push_str(&format!("**Benchmark Suite**: {} v{}\n", latest_suite.name, latest_suite.version));
            summary.push_str(&format!("**Commit Hash**: {}\n", latest_suite.commit_hash));
            summary.push_str(&format!("**System**: {} ({}), {} cores, {:.1}GB RAM\n",
                latest_suite.system_info.os,
                latest_suite.system_info.arch,
                latest_suite.system_info.cpu_cores,
                latest_suite.system_info.memory_gb
            ));

            // Calculate overall metrics
            let total_metrics = latest_suite.metrics.len();
            let categories: std::collections::HashSet<_> = latest_suite.metrics.iter()
                .map(|m| &m.category)
                .collect();

            summary.push_str(&format!("**Total Benchmarks**: {} across {} categories\n\n", total_metrics, categories.len()));

            // Key findings
            summary.push_str("### Key Performance Findings\n\n");

            // Find best and worst performers
            let mut execution_times: Vec<_> = latest_suite.metrics.iter()
                .filter(|m| m.unit == "ms" && m.category.contains("tool_execution"))
                .collect();

            if !execution_times.is_empty() {
                execution_times.sort_by(|a, b| a.value.partial_cmp(&b.value).unwrap());
                let fastest = execution_times.first().unwrap();
                let slowest = execution_times.last().unwrap();

                summary.push_str(&format!("- **Tool Execution**: Fastest {} ({:.2}ms), Slowest {} ({:.2}ms)\n",
                    fastest.name, fastest.value, slowest.name, slowest.value));
            }

            // Memory usage analysis
            let memory_metrics: Vec<_> = latest_suite.metrics.iter()
                .filter(|m| m.memory_usage_mb.is_some())
                .collect();

            if !memory_metrics.is_empty() {
                let avg_memory = memory_metrics.iter()
                    .map(|m| m.memory_usage_mb.unwrap())
                    .sum::<f64>() / memory_metrics.len() as f64;

                summary.push_str(&format!("- **Average Memory Usage**: {:.1}MB across benchmarks\n", avg_memory));
            }
        }

        if !self.comparisons.is_empty() {
            summary.push_str("\n### Architecture Improvements\n\n");
            let comparison = &self.comparisons[0];

            for improvement in &comparison.improvements {
                if improvement.improvement_percentage > 0.0 {
                    summary.push_str(&format!("- **{}**: {:.1}% improvement ({:.2} → {:.2})\n",
                        improvement.metric_name,
                        improvement.improvement_percentage,
                        improvement.baseline_value,
                        improvement.new_value
                    ));
                }
            }
        }

        summary.push_str("\n");
        summary
    }

    /// Generate architecture comparison section
    fn generate_architecture_comparison(&self) -> String {
        let mut comparison_section = String::new();

        for comparison in &self.comparisons {
            comparison_section.push_str("### Performance Improvements\n\n");

            comparison_section.push_str("| Metric | Old Architecture | New Architecture | Improvement |\n");
            comparison_section.push_str("|--------|------------------|------------------|-------------|\n");

            for improvement in &comparison.improvements {
                comparison_section.push_str(&format!(
                    "| {} | {:.2} | {:.2} | {:.1}% |\n",
                    improvement.metric_name,
                    improvement.baseline_value,
                    improvement.new_value,
                    improvement.improvement_percentage
                ));
            }

            comparison_section.push_str("\n#### Validation of Phase 5 Claims\n\n");

            // Validate specific Phase 5 claims
            let tool_execution_improvement = comparison.improvements.iter()
                .find(|i| i.metric_name.contains("tool_execution"));

            if let Some(improvement) = tool_execution_improvement {
                let claimed_improvement = 82.0;
                let actual_improvement = improvement.improvement_percentage;

                comparison_section.push_str(&format!(
                    "- **Tool Execution Speed**: Claimed {:.0}%, Measured {:.1}% {}\n",
                    claimed_improvement,
                    actual_improvement,
                    if actual_improvement >= claimed_improvement * 0.9 { "✅ Validated" } else { "⚠️  Difference" }
                ));
            }

            let memory_improvement = comparison.improvements.iter()
                .find(|i| i.metric_name.contains("memory"));

            if let Some(improvement) = memory_improvement {
                let claimed_improvement = 58.0;
                let actual_improvement = improvement.improvement_percentage;

                comparison_section.push_str(&format!(
                    "- **Memory Reduction**: Claimed {:.0}%, Measured {:.1}% {}\n",
                    claimed_improvement,
                    actual_improvement,
                    if actual_improvement >= claimed_improvement * 0.9 { "✅ Validated" } else { "⚠️  Difference" }
                ));
            }

            comparison_section.push_str("\n");
        }

        comparison_section
    }

    /// Generate detailed results section
    fn generate_detailed_results(&self) -> String {
        let mut detailed = String::new();

        if let Some(latest_suite) = self.results.last() {
            // Group metrics by category
            let mut categories: HashMap<String, Vec<&BenchmarkMetric>> = HashMap::new();
            for metric in &latest_suite.metrics {
                categories.entry(metric.category.clone()).or_insert_with(Vec::new).push(metric);
            }

            for (category, metrics) in categories {
                detailed.push_str(&format!("### {}\n\n", category));

                detailed.push_str("| Benchmark | Value | Unit | Memory (MB) | Iterations |\n");
                detailed.push_str("|-----------|-------|------|------------|------------|\n");

                for metric in metrics {
                    let memory_str = metric.memory_usage_mb
                        .map(|m| format!("{:.1}", m))
                        .unwrap_or_else(|| "N/A".to_string());

                    detailed.push_str(&format!(
                        "| {} | {:.2} | {} | {} | {} |\n",
                        metric.name,
                        metric.value,
                        metric.unit,
                        memory_str,
                        metric.iterations
                    ));
                }

                detailed.push_str("\n");
            }
        }

        detailed
    }

    /// Generate performance analysis
    fn generate_performance_analysis(&self) -> String {
        let mut analysis = String::new();

        if let Some(latest_suite) = self.results.last() {
            analysis.push_str("### Performance Characteristics\n\n");

            // Analyze performance by category
            let mut category_performance: HashMap<String, Vec<f64>> = HashMap::new();
            for metric in &latest_suite.metrics {
                if metric.unit == "ms" {
                    category_performance.entry(metric.category.clone())
                        .or_insert_with(Vec::new)
                        .push(metric.value);
                }
            }

            for (category, values) in category_performance {
                if !values.is_empty() {
                    let avg = values.iter().sum::<f64>() / values.len() as f64;
                    let min = values.iter().min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
                    let max = values.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();

                    analysis.push_str(&format!(
                        "- **{}**: Avg {:.2}ms (Range: {:.2}ms - {:.2}ms)\n",
                        category, avg, min, max
                    ));
                }
            }

            analysis.push_str("\n### Performance Bottlenecks\n\n");

            // Identify potential bottlenecks
            let mut slow_benchmarks: Vec<_> = latest_suite.metrics.iter()
                .filter(|m| m.unit == "ms" && m.value > 100.0)
                .collect();

            slow_benchmarks.sort_by(|a, b| b.value.partial_cmp(&a.value).unwrap());

            for benchmark in slow_benchmarks.iter().take(5) {
                analysis.push_str(&format!(
                    "- **{}**: {:.2}ms - Consider optimization\n",
                    benchmark.name, benchmark.value
                ));
            }

            if slow_benchmarks.is_empty() {
                analysis.push_str("- No significant performance bottlenecks identified\n");
            }

            analysis.push_str("\n");
        }

        analysis
    }

    /// Generate optimization recommendations
    fn generate_recommendations(&self) -> String {
        let mut recommendations = String::new();

        recommendations.push_str("### Performance Optimization Recommendations\n\n");

        recommendations.push_str("#### High Priority\n\n");
        recommendations.push_str("- Focus on optimizing the slowest 10% of benchmarks\n");
        recommendations.push_str("- Implement memory pooling for frequently allocated objects\n");
        recommendations.push_str("- Consider lazy loading for non-critical components\n\n");

        recommendations.push_str("#### Medium Priority\n\n");
        recommendations.push_str("- Optimize hot paths identified in the benchmarks\n");
        recommendations.push_str("- Implement better caching strategies for repeated operations\n");
        recommendations.push_str("- Review and optimize database queries\n\n");

        recommendations.push_str("#### Low Priority\n\n");
        recommendations.push_str("- Fine-tune compilation flags for specific workloads\n");
        recommendations.push_str("- Consider platform-specific optimizations\n");
        recommendations.push_str("- Implement advanced memory management techniques\n\n");

        recommendations.push_str("### Continuous Monitoring\n\n");
        recommendations.push_str("- Set up automated regression testing with these benchmarks\n");
        recommendations.push_str("- Establish performance budgets for critical operations\n");
        recommendations.push_str("- Monitor trends over time to detect performance degradation\n\n");

        recommendations
    }

    /// Generate statistical analysis
    fn generate_statistical_analysis(&self) -> String {
        let mut stats = String::new();

        if let Some(latest_suite) = self.results.last() {
            stats.push_str("### Statistical Summary\n\n");

            // Calculate statistics across all timing metrics
            let timing_metrics: Vec<f64> = latest_suite.metrics.iter()
                .filter(|m| m.unit == "ms")
                .map(|m| m.value)
                .collect();

            if !timing_metrics.is_empty() {
                let mean = timing_metrics.iter().sum::<f64>() / timing_metrics.len() as f64;
                let variance = timing_metrics.iter()
                    .map(|x| (x - mean).powi(2))
                    .sum::<f64>() / timing_metrics.len() as f64;
                let std_dev = variance.sqrt();

                let mut sorted = timing_metrics.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let median = if sorted.len() % 2 == 0 {
                    (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
                } else {
                    sorted[sorted.len() / 2]
                };

                stats.push_str(&format!(
                    "- **Mean**: {:.2}ms\n", mean
                ));
                stats.push_str(&format!(
                    "- **Median**: {:.2}ms\n", median
                ));
                stats.push_str(&format!(
                    "- **Standard Deviation**: {:.2}ms\n", std_dev
                ));
                stats.push_str(&format!(
                    "- **Range**: {:.2}ms - {:.2}ms\n",
                    sorted.first().unwrap(),
                    sorted.last().unwrap()
                ));
            }

            stats.push_str("\n### Benchmark Reliability\n\n");
            stats.push_str(&format!(
                "- **Total Samples**: {}\n",
                latest_suite.metrics.iter().map(|m| m.sample_size).sum::<u32>()
            ));
            stats.push_str(&format!(
                "- **Average Sample Size**: {:.1}\n",
                latest_suite.metrics.iter().map(|m| m.sample_size as f64).sum::<f64>() / latest_suite.metrics.len() as f64
            ));
            stats.push_str("- **Confidence Level**: 95% (default)\n\n");
        }

        stats
    }

    /// Export results to JSON
    pub fn export_json(&self, path: &Path) -> anyhow::Result<()> {
        let data = serde_json::json!({
            "suites": self.results,
            "comparisons": self.comparisons,
            "generated_at": chrono::Utc::now()
        });

        let json = serde_json::to_string_pretty(&data)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Generate CSV export for analysis
    pub fn export_csv(&self, path: &Path) -> anyhow::Result<()> {
        let mut csv_content = String::new();
        csv_content.push_str("name,category,subcategory,value,unit,iterations,sample_size,std_deviation,min_value,max_value,percentile_95,memory_usage_mb,timestamp\n");

        if let Some(latest_suite) = self.results.last() {
            for metric in &latest_suite.metrics {
                csv_content.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{},{},{},{}\n",
                    metric.name,
                    metric.category,
                    metric.subcategory.as_deref().unwrap_or(""),
                    metric.value,
                    metric.unit,
                    metric.iterations,
                    metric.sample_size,
                    metric.std_deviation.map(|d| d.to_string()).unwrap_or_else(|| "".to_string()),
                    metric.min_value.map(|d| d.to_string()).unwrap_or_else(|| "".to_string()),
                    metric.max_value.map(|d| d.to_string()).unwrap_or_else(|| "".to_string()),
                    metric.percentile_95.map(|d| d.to_string()).unwrap_or_else(|| "".to_string()),
                    metric.memory_usage_mb.map(|d| d.to_string()).unwrap_or_else(|| "".to_string()),
                    metric.timestamp.format("%Y-%m-%d %H:%M:%S")
                ));
            }
        }

        fs::write(path, csv_content)?;
        Ok(())
    }

    /// Generate performance trend analysis (if multiple runs available)
    pub fn generate_trend_analysis(&self) -> String {
        let mut trends = String::new();

        if self.results.len() < 2 {
            trends.push_str("Insufficient data for trend analysis (requires at least 2 benchmark runs).\n");
            return trends;
        }

        trends.push_str("## Performance Trend Analysis\n\n");

        // Analyze trends over time
        let first_run = &self.results[0];
        let latest_run = &self.results[self.results.len() - 1];

        let time_diff = latest_run.timestamp.signed_duration_since(first_run.timestamp);
        trends.push_str(&format!("**Analysis Period**: {} to {} ({})\n\n",
            first_run.timestamp.format("%Y-%m-%d"),
            latest_run.timestamp.format("%Y-%m-%d"),
            time_diff.num_days()
        ));

        // Compare key metrics over time
        trends.push_str("### Key Metric Trends\n\n");

        // Find common metrics between runs
        let mut common_metrics = Vec::new();
        for first_metric in &first_run.metrics {
            if let Some(latest_metric) = latest_run.metrics.iter()
                .find(|m| m.name == first_metric.name && m.category == first_metric.category) {
                common_metrics.push((first_metric, latest_metric));
            }
        }

        // Sort by performance change
        common_metrics.sort_by(|a, b| {
            let change_a = ((b.1.value - a.0.value) / a.0.value) * 100.0;
            let change_b = ((b.1.value - a.0.value) / a.0.value) * 100.0;
            change_a.partial_cmp(&change_b).unwrap()
        });

        trends.push_str("| Metric | First Run | Latest Run | Change % |\n");
        trends.push_str("|--------|-----------|------------|----------|\n");

        for (first, latest) in common_metrics.iter().take(10) {
            let change_percent = ((latest.value - first.value) / first.value) * 100.0;
            let trend_emoji = if change_percent > 5.0 { "📈" } else if change_percent < -5.0 { "📉" } else { "➡️" };

            trends.push_str(&format!(
                "| {} {} | {:.2} | {:.2} | {:.1}% |\n",
                trend_emoji, first.name, first.value, latest.value, change_percent
            ));
        }

        trends.push_str("\n");
        trends
    }
}

impl Default for PerformanceReporter {
    fn default() -> Self {
        Self::new()
    }
}

/// Create system information
pub fn create_system_info() -> SystemInfo {
    SystemInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_cores: num_cpus::get(),
        memory_gb: {
            #[cfg(target_os = "linux")]
            {
                if let Ok(info) = std::fs::read_to_string("/proc/meminfo") {
                    for line in info.lines() {
                        if line.starts_with("MemTotal:") {
                            if let Some(kb_str) = line.split_whitespace().nth(1) {
                                if let Ok(kb) = kb_str.parse::<f64>() {
                                    return kb / 1024.0 / 1024.0; // Convert KB to GB
                                }
                            }
                        }
                    }
                }
                8.0 // Default fallback
            }
            #[cfg(not(target_os = "linux"))]
            {
                8.0 // Default fallback for non-Linux systems
            }
        },
        rust_version: rustc_version::version().unwrap().to_string(),
        compiler_flags: "-O3 -C target-cpu=native".to_string(),
    }
}

/// Generate benchmark metric from raw data
pub fn create_metric(
    name: String,
    category: String,
    value: f64,
    unit: String,
    iterations: u32,
    sample_size: u32,
) -> BenchmarkMetric {
    BenchmarkMetric {
        name,
        category,
        subcategory: None,
        value,
        unit,
        iterations,
        sample_size,
        std_deviation: None,
        min_value: None,
        max_value: None,
        percentile_95: None,
        memory_usage_mb: None,
        timestamp: chrono::Utc::now(),
    }
}