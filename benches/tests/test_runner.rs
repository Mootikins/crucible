#!/usr/bin/env rust-script

//! Test runner for the benchmarking framework
//!
//! This script provides a comprehensive test runner that can execute
//! different categories of tests and generate detailed reports on
//! test results and framework reliability.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct TestResults {
    suite_name: String,
    total_tests: usize,
    passed_tests: usize,
    failed_tests: usize,
    execution_time: Duration,
    test_details: Vec<TestDetail>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestDetail {
    name: String,
    passed: bool,
    execution_time: Duration,
    error_message: Option<String>,
}

impl TestResults {
    fn new(suite_name: String) -> Self {
        Self {
            suite_name,
            total_tests: 0,
            passed_tests: 0,
            failed_tests: 0,
            execution_time: Duration::ZERO,
            test_details: Vec::new(),
        }
    }

    fn add_test_result(&mut self, name: String, passed: bool, execution_time: Duration, error: Option<String>) {
        self.total_tests += 1;
        if passed {
            self.passed_tests += 1;
        } else {
            self.failed_tests += 1;
        }
        self.test_details.push(TestDetail {
            name,
            passed,
            execution_time,
            error_message: error,
        });
    }

    fn success_rate(&self) -> f64 {
        if self.total_tests == 0 {
            0.0
        } else {
            (self.passed_tests as f64 / self.total_tests as f64) * 100.0
        }
    }
}

struct TestRunner {
    output_dir: PathBuf,
    verbose: bool,
}

impl TestRunner {
    fn new(output_dir: PathBuf, verbose: bool) -> Self {
        Self { output_dir, verbose }
    }

    fn run_all_tests(&self) -> Result<Vec<TestResults>> {
        println!("🧪 Running comprehensive benchmarking framework tests...\n");

        std::fs::create_dir_all(&self.output_dir)?;

        let test_suites = vec![
            ("benchmark_utils_tests", "Benchmark utilities"),
            ("performance_reporter_tests", "Performance reporter"),
            ("benchmark_runner_tests", "Benchmark runner"),
            ("individual_benchmark_tests", "Individual benchmarks"),
            ("benchmark_integration_tests", "Integration tests"),
            ("edge_case_error_tests", "Edge cases and errors"),
            ("framework_performance_tests", "Framework performance"),
        ];

        let mut all_results = Vec::new();

        for (suite_file, suite_name) in test_suites {
            println!("📋 Running {} tests...", suite_name);
            let results = self.run_test_suite(suite_file, suite_name)?;

            self.print_suite_results(&results);
            all_results.push(results);

            println!();
        }

        self.generate_summary_report(&all_results)?;
        self.generate_json_report(&all_results)?;

        Ok(all_results)
    }

    fn run_test_suite(&self, suite_file: &str, suite_name: &str) -> Result<TestResults> {
        let start_time = Instant::now();
        let mut results = TestResults::new(suite_name.to_string());

        // Run the test suite using cargo test
        let mut cmd = Command::new("cargo");
        cmd.args(&["test", "--bench", "comprehensive_benchmarks"])
            .args(&["--", "--nocapture", suite_file])
            .current_dir(env::current_dir()?);

        if self.verbose {
            cmd.env("RUST_LOG", "debug");
        }

        let output = cmd.output()
            .with_context(|| format!("Failed to run test suite: {}", suite_file))?;

        let execution_time = start_time.elapsed();
        results.execution_time = execution_time;

        // Parse test output to extract individual test results
        let output_str = String::from_utf8_lossy(&output.stdout);
        let error_str = String::from_utf8_lossy(&output.stderr);

        if self.verbose {
            println!("STDOUT:\n{}", output_str);
            if !error_str.is_empty() {
                println!("STDERR:\n{}", error_str);
            }
        }

        // Simple parsing of test results
        for line in output_str.lines() {
            if line.contains("test") && (line.contains("ok") || line.contains("FAILED")) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    let test_name = parts[1].to_string();
                    let passed = line.contains("ok") && !line.contains("FAILED");

                    // Extract execution time if available
                    let execution_time = if line.contains("<") {
                        // Parse time like "test_name ... ok <10ms>"
                        if let Some(time_part) = line.split('<').nth(1) {
                            if let Some(time_str) = time_part.split('>').next() {
                                self.parse_duration(time_str).unwrap_or(Duration::from_millis(1))
                            } else {
                                Duration::from_millis(1)
                            }
                        } else {
                            Duration::from_millis(1)
                        }
                    } else {
                        Duration::from_millis(1)
                    };

                    let error = if !passed {
                        Some("Test failed".to_string())
                    } else {
                        None
                    };

                    results.add_test_result(test_name, passed, execution_time, error);
                }
            }
        }

        // If we couldn't parse individual tests, at least record overall success
        if results.total_tests == 0 {
            let overall_success = output.status.success();
            results.add_test_result(
                format!("{}_overall", suite_file),
                overall_success,
                execution_time,
                if overall_success { None } else { Some("Test suite failed".to_string()) },
            );
        }

        Ok(results)
    }

    fn parse_duration(&self, duration_str: &str) -> Option<Duration> {
        let duration_str = duration_str.trim();

        if duration_str.ends_with("ms") {
            let num_str = &duration_str[..duration_str.len()-2];
            num_str.parse::<f64>().ok().map(|ms| Duration::from_millis(ms as u64))
        } else if duration_str.ends_with("ns") {
            let num_str = &duration_str[..duration_str.len()-2];
            num_str.parse::<f64>().ok().map(|ns| Duration::from_nanos(ns as u64))
        } else if duration_str.ends_with("μs") || duration_str.ends_with("µs") {
            let num_str = &duration_str[..duration_str.len()-2];
            num_str.parse::<f64>().ok().map(|us| Duration::from_micros(us as u64))
        } else if duration_str.ends_with("s") {
            let num_str = &duration_str[..duration_str.len()-1];
            num_str.parse::<f64>().ok().map(|s| Duration::from_secs_f64(s))
        } else {
            duration_str.parse::<f64>().ok().map(|ms| Duration::from_millis(ms as u64))
        }
    }

    fn print_suite_results(&self, results: &TestResults) {
        println!("  ✅ Passed: {}", results.passed_tests);
        println!("  ❌ Failed: {}", results.failed_tests);
        println!("  ⏱️  Time: {:?}", results.execution_time);
        println!("  📊 Success Rate: {:.1}%", results.success_rate());

        if results.failed_tests > 0 {
            println!("  🔍 Failed Tests:");
            for test in &results.test_details {
                if !test.passed {
                    println!("    - {}", test.name);
                    if let Some(error) = &test.error_message {
                        println!("      Error: {}", error);
                    }
                }
            }
        }
    }

    fn generate_summary_report(&self, all_results: &[TestResults]) -> Result<()> {
        let report_path = self.output_dir.join("TEST_SUMMARY.md");
        let mut report = String::new();

        report.push_str("# Phase 6.2 Benchmarking Framework Test Summary\n\n");
        report.push_str(&format!("Generated: {}\n\n", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));

        // Overall statistics
        let total_tests: usize = all_results.iter().map(|r| r.total_tests).sum();
        let total_passed: usize = all_results.iter().map(|r| r.passed_tests).sum();
        let total_failed: usize = all_results.iter().map(|r| r.failed_tests).sum();
        let overall_success_rate = if total_tests > 0 {
            (total_passed as f64 / total_tests as f64) * 100.0
        } else {
            0.0
        };

        report.push_str("## Overall Results\n\n");
        report.push_str("| Metric | Value |\n");
        report.push_str("|--------|-------|\n");
        report.push_str(&format!("| Total Test Suites | {} |\n", all_results.len()));
        report.push_str(&format!("| Total Tests | {} |\n", total_tests));
        report.push_str(&format!("| Passed | {} |\n", total_passed));
        report.push_str(&format!("| Failed | {} |\n", total_failed));
        report.push_str(&format!("| Success Rate | {:.1}% |\n", overall_success_rate));

        // Test suite details
        report.push_str("\n## Test Suite Details\n\n");
        report.push_str("| Suite | Tests | Passed | Failed | Success Rate | Time |\n");
        report.push_str("|-------|-------|--------|--------|--------------|------|\n");

        for results in all_results {
            report.push_str(&format!(
                "| {} | {} | {} | {} | {:.1}% | {:?} |\n",
                results.suite_name,
                results.total_tests,
                results.passed_tests,
                results.failed_tests,
                results.success_rate(),
                results.execution_time
            ));
        }

        // Failed tests summary
        if total_failed > 0 {
            report.push_str("\n## Failed Tests\n\n");
            for results in all_results {
                for test in &results.test_details {
                    if !test.passed {
                        report.push_str(&format!(
                            "### {} - {}\n\n",
                            results.suite_name, test.name
                        ));
                        if let Some(error) = &test.error_message {
                            report.push_str(&format!("**Error**: {}\n\n", error));
                        }
                    }
                }
            }
        }

        // Performance analysis
        report.push_str("## Performance Analysis\n\n");

        let total_time: Duration = all_results.iter().map(|r| r.execution_time).sum();
        let avg_suite_time = total_time / all_results.len() as u32;

        report.push_str(&format!("- **Total Execution Time**: {:?}\n", total_time));
        report.push_str(&format!("- **Average Suite Time**: {:?}\n", avg_suite_time));

        // Find slowest and fastest suites
        if let Some(slowest) = all_results.iter().max_by_key(|r| r.execution_time) {
            report.push_str(&format!("- **Slowest Suite**: {} ({:?})\n", slowest.suite_name, slowest.execution_time));
        }

        if let Some(fastest) = all_results.iter().min_by_key(|r| r.execution_time) {
            report.push_str(&format!("- **Fastest Suite**: {} ({:?})\n", fastest.suite_name, fastest.execution_time));
        }

        // Framework quality assessment
        report.push_str("\n## Framework Quality Assessment\n\n");

        if overall_success_rate >= 95.0 {
            report.push_str("✅ **Excellent**: Framework test suite passes with high success rate\n");
        } else if overall_success_rate >= 90.0 {
            report.push_str("⚠️  **Good**: Framework test suite passes with acceptable success rate\n");
        } else {
            report.push_str("❌ **Needs Attention**: Framework test suite has low success rate\n");
        }

        report.push_str("\n### Test Coverage Areas\n\n");
        report.push_str("- ✅ Benchmark utilities and data generation\n");
        report.push_str("- ✅ Performance reporting and statistical analysis\n");
        report.push_str("- ✅ Benchmark runner orchestration\n");
        report.push_str("- ✅ Individual benchmark modules\n");
        report.push_str("- ✅ End-to-end integration workflows\n");
        report.push_str("- ✅ Edge cases and error handling\n");
        report.push_str("- ✅ Framework performance characteristics\n");

        report.push_str("\n### Recommendations\n\n");

        if total_failed > 0 {
            report.push_str("- Review and fix failing tests\n");
            report.push_str("- Investigate potential regressions\n");
        } else {
            report.push_str("- Framework is stable and ready for production use\n");
            report.push_str("- Consider adding additional edge case tests\n");
        }

        if avg_suite_time > Duration::from_secs(10) {
            report.push_str("- Optimize slow test suites for faster CI/CD\n");
        }

        std::fs::write(&report_path, report)?;
        println!("📝 Test summary report generated: {}", report_path.display());

        Ok(())
    }

    fn generate_json_report(&self, all_results: &[TestResults]) -> Result<()> {
        let json_path = self.output_dir.join("test_results.json");
        let json_data = serde_json::json!({
            "generated_at": chrono::Utc::now(),
            "summary": {
                "total_suites": all_results.len(),
                "total_tests": all_results.iter().map(|r| r.total_tests).sum::<usize>(),
                "total_passed": all_results.iter().map(|r| r.passed_tests).sum::<usize>(),
                "total_failed": all_results.iter().map(|r| r.failed_tests).sum::<usize>(),
                "total_time": format!("{:?}", all_results.iter().map(|r| r.execution_time).sum::<Duration>()),
            },
            "suites": all_results
        });

        let json_content = serde_json::to_string_pretty(&json_data)?;
        std::fs::write(&json_path, json_content)?;
        println!("📊 JSON test results generated: {}", json_path.display());

        Ok(())
    }
}

fn print_usage() {
    println!("Usage: test_runner [OPTIONS]");
    println!();
    println!("Options:");
    println!("  --output-dir <DIR>    Output directory for test reports [default: test_results]");
    println!("  --verbose              Enable verbose output");
    println!("  --help                 Show this help message");
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    let mut output_dir = PathBuf::from("test_results");
    let mut verbose = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--output-dir" => {
                if i + 1 < args.len() {
                    output_dir = PathBuf::from(&args[i + 1]);
                    i += 1;
                } else {
                    eprintln!("Error: --output-dir requires a directory path");
                    print_usage();
                    return Ok(());
                }
            }
            "--verbose" => {
                verbose = true;
            }
            "--help" => {
                print_usage();
                return Ok(());
            }
            _ => {
                eprintln!("Error: Unknown option {}", args[i]);
                print_usage();
                return Ok(());
            }
        }
        i += 1;
    }

    let runner = TestRunner::new(output_dir, verbose);
    let results = runner.run_all_tests()?;

    // Determine exit code based on test results
    let total_failed: usize = results.iter().map(|r| r.failed_tests).sum();

    if total_failed == 0 {
        println!("\n🎉 All tests passed! Benchmarking framework is ready for use.");
        std::process::exit(0);
    } else {
        println!("\n❌ {} tests failed. Please review the test report.", total_failed);
        std::process::exit(1);
    }
}