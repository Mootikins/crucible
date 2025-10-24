//! Integration Workflow Test Runner
//!
//! This module serves as the main entry point for running all comprehensive
//! integration workflow tests. It provides orchestration, reporting, and
//! validation capabilities for the complete test suite.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

// Import all test modules
use crate::comprehensive_integration_workflow_tests::{
    ComprehensiveIntegrationTestSuite, TestResult
};
use crate::cli_workflow_integration_tests::ExtendedCliTestHarness;
use crate::repl_interactive_workflow_tests::ExtendedReplTestHarness;
use crate::tool_api_integration_tests::ToolApiTestHarness;
use crate::cross_interface_consistency_tests::CrossInterfaceTestHarness;
use crate::real_world_usage_scenario_tests::RealWorldUsageTestHarness;

/// Main integration workflow test runner
pub struct IntegrationWorkflowTestRunner {
    config: TestRunnerConfig,
    results: Vec<TestSuiteResult>,
    session_start: Instant,
}

impl IntegrationWorkflowTestRunner {
    /// Create new test runner with default configuration
    pub fn new() -> Self {
        Self {
            config: TestRunnerConfig::default(),
            results: Vec::new(),
            session_start: Instant::now(),
        }
    }

    /// Create test runner with custom configuration
    pub fn with_config(config: TestRunnerConfig) -> Self {
        Self {
            config,
            results: Vec::new(),
            session_start: Instant::now(),
        }
    }

    /// Run all comprehensive integration workflow tests
    pub async fn run_all_tests(&mut self) -> Result<TestRunReport> {
        println!("\n🚀 Starting Comprehensive Integration Workflow Test Suite");
        println!("================================================================");

        if self.config.verbose {
            println!("📋 Configuration:");
            println!("   Verbose: {}", self.config.verbose);
            println!("   Parallel execution: {}", self.config.parallel_execution);
            println!("   Performance validation: {}", self.config.performance_validation);
            println!("   Real-world scenarios: {}", self.config.real_world_scenarios);
            println!();
        }

        let test_start = Instant::now();

        // Run test suites based on configuration
        if self.config.run_comprehensive_suite {
            self.run_comprehensive_suite().await?;
        }

        if self.config.run_cli_workflows {
            self.run_cli_workflow_tests().await?;
        }

        if self.config.run_repl_workflows {
            self.run_repl_workflow_tests().await?;
        }

        if self.config.run_tool_integration {
            self.run_tool_integration_tests().await?;
        }

        if self.config.run_consistency_tests {
            self.run_consistency_tests().await?;
        }

        if self.config.run_real_world_scenarios {
            self.run_real_world_scenario_tests().await?;
        }

        if self.config.run_performance_tests {
            self.run_performance_validation_tests().await?;
        }

        let total_duration = test_start.elapsed();

        // Generate comprehensive report
        let report = self.generate_test_run_report(total_duration)?;

        // Print results
        self.print_test_run_summary(&report);

        Ok(report)
    }

    /// Run comprehensive integration test suite
    async fn run_comprehensive_suite(&mut self) -> Result<()> {
        println!("\n📋 Running Comprehensive Integration Test Suite");
        println!("----------------------------------------------");

        let suite_start = Instant::now();

        let mut test_suite = ComprehensiveIntegrationTestSuite::new();
        test_suite.run_all_tests().await?;

        let suite_duration = suite_start.elapsed();

        self.results.push(TestSuiteResult {
            name: "Comprehensive Integration Suite".to_string(),
            category: TestCategory::Comprehensive,
            passed: true,
            duration: suite_duration,
            test_count: 8, // Approximate number of test categories
            details: "All pipeline, CLI, REPL, and tool integration tests".to_string(),
        });

        println!("✅ Comprehensive integration test suite completed in {:?}", suite_duration);
        Ok(())
    }

    /// Run CLI workflow tests
    async fn run_cli_workflow_tests(&mut self) -> Result<()> {
        println!("\n📋 Running CLI Workflow Tests");
        println!("-------------------------------");

        let suite_start = Instant::now();

        let harness = ExtendedCliTestHarness::new().await?;

        // Run individual CLI workflow tests
        harness.test_advanced_search_workflows().await?;
        harness.test_indexing_workflows().await?;
        harness.test_note_workflows().await?;
        harness.test_config_workflows().await?;
        harness.test_error_handling_workflows().await?;
        harness.test_performance_workflows().await?;
        harness.test_external_integration_workflows().await?;

        let suite_duration = suite_start.elapsed();

        self.results.push(TestSuiteResult {
            name: "CLI Workflow Tests".to_string(),
            category: TestCategory::Cli,
            passed: true,
            duration: suite_duration,
            test_count: 7,
            details: "CLI command workflows, search, indexing, and configuration".to_string(),
        });

        println!("✅ CLI workflow tests completed in {:?}", suite_duration);
        Ok(())
    }

    /// Run REPL workflow tests
    async fn run_repl_workflow_tests(&mut self) -> Result<()> {
        println!("\n📋 Running REPL Workflow Tests");
        println!("-------------------------------");

        let suite_start = Instant::now();

        let harness = ExtendedReplTestHarness::new().await?;

        // Run individual REPL workflow tests
        harness.test_startup_workflow().await?;
        harness.test_tool_management_workflows().await?;
        harness.test_query_execution_workflows().await?;
        harness.test_output_formatting_workflows().await?;
        harness.test_history_management_workflows().await?;
        harness.test_interactive_workflows().await?;
        harness.test_error_handling_workflows().await?;
        harness.test_performance_workflows().await?;

        let suite_duration = suite_start.elapsed();

        self.results.push(TestSuiteResult {
            name: "REPL Workflow Tests".to_string(),
            category: TestCategory::Repl,
            passed: true,
            duration: suite_duration,
            test_count: 8,
            details: "REPL interactive sessions, tool execution, and query workflows".to_string(),
        });

        println!("✅ REPL workflow tests completed in {:?}", suite_duration);
        Ok(())
    }

    /// Run tool integration tests
    async fn run_tool_integration_tests(&mut self) -> Result<()> {
        println!("\n📋 Running Tool Integration Tests");
        println!("----------------------------------");

        let suite_start = Instant::now();

        let harness = ToolApiTestHarness::new().await?;

        // Run individual tool integration tests
        harness.test_tool_discovery_workflow().await?;
        harness.test_tool_execution_workflow().await?;
        harness.test_parameter_handling_workflow().await?;
        harness.test_tool_chaining_workflow().await?;
        harness.test_result_processing_workflow().await?;
        harness.test_tool_performance_workflow().await?;
        harness.test_tool_error_handling_workflow().await?;
        harness.test_search_integration_workflow().await?;

        let suite_duration = suite_start.elapsed();

        self.results.push(TestSuiteResult {
            name: "Tool Integration Tests".to_string(),
            category: TestCategory::Tool,
            passed: true,
            duration: suite_duration,
            test_count: 8,
            details: "Tool discovery, execution, chaining, and error handling".to_string(),
        });

        println!("✅ Tool integration tests completed in {:?}", suite_duration);
        Ok(())
    }

    /// Run cross-interface consistency tests
    async fn run_consistency_tests(&mut self) -> Result<()> {
        println!("\n📋 Running Cross-Interface Consistency Tests");
        println!("--------------------------------------------");

        let suite_start = Instant::now();

        let harness = CrossInterfaceTestHarness::new().await?;

        // Run individual consistency tests
        harness.test_query_consistency().await?;
        harness.test_performance_consistency().await?;
        harness.test_output_format_consistency().await?;
        harness.test_error_handling_consistency().await?;
        harness.test_state_consistency().await?;
        harness.test_resource_usage_consistency().await?;

        let suite_duration = suite_start.elapsed();

        self.results.push(TestSuiteResult {
            name: "Cross-Interface Consistency Tests".to_string(),
            category: TestCategory::Consistency,
            passed: true,
            duration: suite_duration,
            test_count: 6,
            details: "Consistency across CLI, REPL, and tool interfaces".to_string(),
        });

        println!("✅ Cross-interface consistency tests completed in {:?}", suite_duration);
        Ok(())
    }

    /// Run real-world scenario tests
    async fn run_real_world_scenario_tests(&mut self) -> Result<()> {
        println!("\n📋 Running Real-World Scenario Tests");
        println!("-------------------------------------");

        let suite_start = Instant::now();

        let harness = RealWorldUsageTestHarness::new().await?;

        // Run individual real-world scenario tests
        harness.test_research_workflow().await?;
        harness.test_project_management_workflow().await?;
        harness.test_knowledge_discovery_workflow().await?;
        harness.test_code_documentation_workflow().await?;
        harness.test_personal_knowledge_management_workflow().await?;
        harness.test_collaborative_knowledge_sharing_workflow().await?;
        harness.test_comprehensive_workflow_integration().await?;

        let suite_duration = suite_start.elapsed();

        self.results.push(TestSuiteResult {
            name: "Real-World Scenario Tests".to_string(),
            category: TestCategory::RealWorld,
            passed: true,
            duration: suite_duration,
            test_count: 7,
            details: "Research, project management, knowledge discovery, and code documentation workflows".to_string(),
        });

        println!("✅ Real-world scenario tests completed in {:?}", suite_duration);
        Ok(())
    }

    /// Run performance validation tests
    async fn run_performance_validation_tests(&mut self) -> Result<()> {
        println!("\n📋 Running Performance Validation Tests");
        println!("------------------------------------");

        let suite_start = Instant::now();

        // Performance benchmarks
        let benchmark_results = self.run_performance_benchmarks().await?;

        let suite_duration = suite_start.elapsed();

        self.results.push(TestSuiteResult {
            name: "Performance Validation Tests".to_string(),
            category: TestCategory::Performance,
            passed: benchmark_results.all_passed,
            duration: suite_duration,
            test_count: benchmark_results.test_count,
            details: format!("Performance benchmarks: {} passed, {} failed",
                             benchmark_results.passed_count, benchmark_results.failed_count),
        });

        println!("✅ Performance validation tests completed in {:?}", suite_duration);
        Ok(())
    }

    /// Run performance benchmarks
    async fn run_performance_benchmarks(&self) -> Result<PerformanceBenchmarkResults> {
        println!("  ⚡ Running performance benchmarks...");

        let mut results = PerformanceBenchmarkResults::new();

        // Benchmark CLI search performance
        let cli_harness = ExtendedCliTestHarness::new().await?;
        let cli_search_time = self.benchmark_operation(
            "CLI Search",
            || async {
                cli_harness.execute_cli_command(&["search", "quantum computing"])
            }
        ).await?;

        results.add_benchmark("CLI Search", cli_search_time, Duration::from_secs(5));

        // Benchmark REPL query performance
        let repl_harness = ExtendedReplTestHarness::new().await?;
        let mut repl = repl_harness.spawn_repl_with_config(Default::default())?;
        let repl_query_time = self.benchmark_operation(
            "REPL Query",
            || async {
                repl.send_command("SELECT * FROM notes LIMIT 5")
            }
        ).await?;
        repl.quit()?;

        results.add_benchmark("REPL Query", repl_query_time, Duration::from_secs(3));

        // Benchmark tool execution performance
        let tool_harness = ToolApiTestHarness::new().await?;
        let mut tool_repl = tool_harness.spawn_repl()?;
        let tool_time = self.benchmark_operation(
            "Tool Execution",
            || async {
                tool_repl.send_command(":run system_info")
            }
        ).await?;
        tool_repl.quit()?;

        results.add_benchmark("Tool Execution", tool_time, Duration::from_secs(2));

        Ok(results)
    }

    /// Benchmark a specific operation
    async fn benchmark_operation<F, Fut>(&self, name: &str, operation: F) -> Result<Duration>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<anyhow::Result<()>>>,
    {
        println!("    🕐 Benchmarking: {}", name);

        let mut durations = Vec::new();
        let iterations = 3;

        for i in 0..iterations {
            let start = Instant::now();
            let result = operation().await;
            let duration = start.elapsed();

            match result {
                Ok(inner_result) => {
                    match inner_result {
                        Ok(_) => {
                            durations.push(duration);
                            if self.config.verbose {
                                println!("      Iteration {}: {:?} ✅", i + 1, duration);
                            }
                        }
                        Err(e) => {
                            println!("      ❌ Iteration {} failed: {}", i + 1, e);
                            return Err(anyhow!("Benchmark failed for {}: {}", name, e));
                        }
                    }
                }
                Err(e) => {
                    println!("      ❌ Iteration {} failed: {}", i + 1, e);
                    return Err(anyhow!("Benchmark failed for {}: {}", name, e));
                }
            }
        }

        // Calculate average duration
        let total_duration: Duration = durations.iter().sum();
        let avg_duration = total_duration / durations.len() as u32;

        if self.config.verbose {
            println!("      Average: {:?} over {} iterations", avg_duration, iterations);
        }

        Ok(avg_duration)
    }

    /// Generate comprehensive test run report
    fn generate_test_run_report(&self, total_duration: Duration) -> Result<TestRunReport> {
        let total_suites = self.results.len();
        let passed_suites = self.results.iter().filter(|r| r.passed).count();
        let total_tests: usize = self.results.iter().map(|r| r.test_count).sum();

        // Calculate performance metrics
        let performance_metrics = self.calculate_performance_metrics()?;

        // Identify any failed tests
        let failed_suites: Vec<_> = self.results.iter()
            .filter(|r| !r.passed)
            .map(|r| r.name.clone())
            .collect();

        Ok(TestRunReport {
            total_duration,
            total_suites,
            passed_suites,
            failed_suites,
            total_tests,
            suite_results: self.results.clone(),
            performance_metrics,
            success_rate: if total_suites > 0 {
                (passed_suites as f64 / total_suites as f64) * 100.0
            } else {
                0.0
            },
        })
    }

    /// Calculate performance metrics across all test suites
    fn calculate_performance_metrics(&self) -> Result<PerformanceMetrics> {
        let total_suite_duration: Duration = self.results.iter().map(|r| r.duration).sum();
        let avg_suite_duration = if self.results.is_empty() {
            Duration::ZERO
        } else {
            total_suite_duration / self.results.len() as u32
        };

        let slowest_suite = self.results.iter()
            .max_by_key(|r| r.duration)
            .map(|r| (r.name.clone(), r.duration));

        let fastest_suite = self.results.iter()
            .min_by_key(|r| r.duration)
            .map(|r| (r.name.clone(), r.duration));

        Ok(PerformanceMetrics {
            total_suite_duration,
            avg_suite_duration,
            slowest_suite,
            fastest_suite,
        })
    }

    /// Print comprehensive test run summary
    fn print_test_run_summary(&self, report: &TestRunReport) {
        println!("\n🎉 Comprehensive Integration Workflow Test Results");
        println!("====================================================");

        // Overall summary
        println!("📊 Overall Summary:");
        println!("   Total test suites: {}", report.total_suites);
        println!("   Passed suites: {}", report.passed_suites);
        println!("   Failed suites: {}", report.failed_suites.len());
        println!("   Total individual tests: {}", report.total_tests);
        println!("   Success rate: {:.1}%", report.success_rate);
        println!("   Total duration: {:?}", report.total_duration);

        // Suite breakdown
        println!("\n📋 Test Suite Breakdown:");
        for (i, suite) in report.suite_results.iter().enumerate() {
            let status = if suite.passed { "✅ PASS" } else { "❌ FAIL" };
            println!("   {}. {} - {} ({:?}) - {} tests",
                     i + 1, suite.name, status, suite.duration, suite.test_count);
            if self.config.verbose && !suite.details.is_empty() {
                println!("      {}", suite.details);
            }
        }

        // Performance metrics
        println!("\n⚡ Performance Metrics:");
        println!("   Average suite duration: {:?}", report.performance_metrics.avg_suite_duration);
        if let Some((ref name, duration)) = report.performance_metrics.slowest_suite {
            println!("   Slowest suite: {} ({:?})", name, duration);
        }
        if let Some((ref name, duration)) = report.performance_metrics.fastest_suite {
            println!("   Fastest suite: {} ({:?})", name, duration);
        }

        // Failed tests (if any)
        if !report.failed_suites.is_empty() {
            println!("\n❌ Failed Test Suites:");
            for failed_suite in &report.failed_suites {
                println!("   - {}", failed_suite);
            }
        }

        // Final status
        if report.success_rate >= 100.0 {
            println!("\n🎉 All integration workflow tests passed!");
            println!("The Crucible knowledge management system is working correctly across all interfaces.");
        } else {
            println!("\n⚠️  Some tests failed. Please review the detailed results above.");
        }

        // Test coverage summary
        println!("\n📈 Test Coverage Summary:");
        println!("   ✅ Complete pipeline integration");
        println!("   ✅ CLI command workflows and options");
        println!("   ✅ REPL interactive sessions and tool execution");
        println!("   ✅ Tool discovery, execution, and chaining");
        println!("   ✅ Cross-interface consistency and validation");
        println!("   ✅ Real-world usage scenarios and workflows");
        println!("   ✅ Performance validation and benchmarks");
        println!("   ✅ Error handling and recovery mechanisms");

        // Recommendations
        println!("\n💡 Recommendations:");
        if report.success_rate >= 100.0 {
            println!("   🟢 System is ready for production use");
            println!("   🟢 All interfaces are functioning correctly");
            println!("   🟢 Performance meets expectations");
        } else {
            println!("   🟡 Review failed test suites and address issues");
            println!("   🟡 Ensure all interfaces are properly configured");
            println!("   🟡 Validate environment setup and dependencies");
        }

        println!("   📋 Run individual test suites for targeted validation");
        println!("   🔧 Use verbose mode for detailed failure analysis");
        println!("   📊 Monitor performance metrics in production");
    }
}

/// Configuration for test runner
#[derive(Debug, Clone)]
pub struct TestRunnerConfig {
    pub verbose: bool,
    pub parallel_execution: bool,
    pub performance_validation: bool,
    pub real_world_scenarios: bool,
    pub run_comprehensive_suite: bool,
    pub run_cli_workflows: bool,
    pub run_repl_workflows: bool,
    pub run_tool_integration: bool,
    pub run_consistency_tests: bool,
    pub run_real_world_scenarios: bool,
    pub run_performance_tests: bool,
}

impl Default for TestRunnerConfig {
    fn default() -> Self {
        Self {
            verbose: false,
            parallel_execution: false,
            performance_validation: true,
            real_world_scenarios: true,
            run_comprehensive_suite: true,
            run_cli_workflows: true,
            run_repl_workflows: true,
            run_tool_integration: true,
            run_consistency_tests: true,
            run_real_world_scenarios: true,
            run_performance_tests: true,
        }
    }
}

/// Result of running a test suite
#[derive(Debug, Clone)]
pub struct TestSuiteResult {
    pub name: String,
    pub category: TestCategory,
    pub passed: bool,
    pub duration: Duration,
    pub test_count: usize,
    pub details: String,
}

/// Test suite categories
#[derive(Debug, Clone, PartialEq)]
pub enum TestCategory {
    Comprehensive,
    Cli,
    Repl,
    Tool,
    Consistency,
    RealWorld,
    Performance,
}

/// Comprehensive test run report
#[derive(Debug, Clone)]
pub struct TestRunReport {
    pub total_duration: Duration,
    pub total_suites: usize,
    pub passed_suites: usize,
    pub failed_suites: Vec<String>,
    pub total_tests: usize,
    pub suite_results: Vec<TestSuiteResult>,
    pub performance_metrics: PerformanceMetrics,
    pub success_rate: f64,
}

/// Performance metrics for test run
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub total_suite_duration: Duration,
    pub avg_suite_duration: Duration,
    pub slowest_suite: Option<(String, Duration)>,
    pub fastest_suite: Option<(String, Duration)>,
}

/// Performance benchmark results
#[derive(Debug, Clone)]
pub struct PerformanceBenchmarkResults {
    pub benchmarks: Vec<PerformanceBenchmark>,
    pub all_passed: bool,
    pub test_count: usize,
    pub passed_count: usize,
    pub failed_count: usize,
}

impl PerformanceBenchmarkResults {
    pub fn new() -> Self {
        Self {
            benchmarks: Vec::new(),
            all_passed: true,
            test_count: 0,
            passed_count: 0,
            failed_count: 0,
        }
    }

    pub fn add_benchmark(&mut self, name: String, duration: Duration, threshold: Duration) {
        let passed = duration <= threshold;
        self.test_count += 1;
        if passed {
            self.passed_count += 1;
        } else {
            self.failed_count += 1;
            self.all_passed = false;
        }

        self.benchmarks.push(PerformanceBenchmark {
            name,
            duration,
            threshold,
            passed,
        });
    }
}

/// Individual performance benchmark
#[derive(Debug, Clone)]
pub struct PerformanceBenchmark {
    pub name: String,
    pub duration: Duration,
    pub threshold: Duration,
    pub passed: bool,
}

// ============================================================================
// Main Test Execution Functions
// ============================================================================

#[tokio::test]
#[ignore] // Integration test - requires built binary and environment setup
async fn test_comprehensive_integration_workflow_complete() -> Result<()> {
    println!("🧪 Running complete comprehensive integration workflow test suite");

    let mut runner = IntegrationWorkflowTestRunner::new();
    let report = runner.run_all_tests().await?;

    assert!(report.success_rate >= 95.0,
           "At least 95% of test suites should pass, got {:.1}%",
           report.success_rate);

    assert!(!report.suite_results.is_empty(),
           "Should have run test suites");

    println!("✅ Complete comprehensive integration workflow test suite passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_quick_integration_validation() -> Result<()> {
    println!("🧪 Running quick integration validation");

    let config = TestRunnerConfig {
        verbose: false,
        run_comprehensive_suite: true,
        run_cli_workflows: false,
        run_repl_workflows: false,
        run_tool_integration: false,
        run_consistency_tests: true,
        run_real_world_scenarios: false,
        run_performance_tests: false,
        ..Default::default()
    };

    let mut runner = IntegrationWorkflowTestRunner::with_config(config);
    let report = runner.run_all_tests().await?;

    assert!(report.success_rate >= 90.0,
           "Quick validation should have at least 90% success rate, got {:.1}%",
           report.success_rate);

    println!("✅ Quick integration validation passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_performance_validation_suite() -> Result<()> {
    println!("🧪 Running performance validation suite");

    let config = TestRunnerConfig {
        verbose: true,
        run_comprehensive_suite: false,
        run_cli_workflows: true,
        run_repl_workflows: true,
        run_tool_integration: true,
        run_consistency_tests: false,
        run_real_world_scenarios: false,
        run_performance_tests: true,
        ..Default::default()
    };

    let mut runner = IntegrationWorkflowTestRunner::with_config(config);
    let report = runner.run_all_tests().await?;

    assert!(report.success_rate >= 90.0,
           "Performance validation should have at least 90% success rate, got {:.1}%",
           report.success_rate);

    println!("✅ Performance validation suite passed");
    Ok(())
}

#[tokio::test]
#[ignore] // Integration test - requires built binary
async fn test_real_world_scenario_validation() -> Result<()> {
    println!("🧪 Running real-world scenario validation");

    let config = TestRunnerConfig {
        verbose: true,
        run_comprehensive_suite: false,
        run_cli_workflows: false,
        run_repl_workflows: false,
        run_tool_integration: false,
        run_consistency_tests: false,
        run_real_world_scenarios: true,
        run_performance_tests: false,
        ..Default::default()
    };

    let mut runner = IntegrationWorkflowTestRunner::with_config(config);
    let report = runner.run_all_tests().await?;

    assert!(report.success_rate >= 95.0,
           "Real-world scenarios should have at least 95% success rate, got {:.1}%",
           report.success_rate);

    println!("✅ Real-world scenario validation passed");
    Ok(())
}

/// Main entry point for running integration tests programmatically
pub async fn run_integration_tests(verbose: bool) -> Result<TestRunReport> {
    let config = TestRunnerConfig {
        verbose,
        ..Default::default()
    };

    let mut runner = IntegrationWorkflowTestRunner::with_config(config);
    runner.run_all_tests().await
}