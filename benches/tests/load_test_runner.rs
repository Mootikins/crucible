#!/usr/bin/env rust-script

//! Load testing framework test runner
//!
//! Comprehensive test runner for validating the ScriptEngine load testing framework

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct LoadTestSuiteResults {
    suite_name: String,
    total_tests: usize,
    passed_tests: usize,
    failed_tests: usize,
    execution_time: Duration,
    test_categories: Vec<TestCategoryResults>,
    framework_validation: FrameworkValidationResults,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestCategoryResults {
    category_name: String,
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

#[derive(Debug, Serialize, Deserialize)]
struct FrameworkValidationResults {
    compilation_success: bool,
    dependency_validation: bool,
    benchmark_structure: bool,
    framework_structure: bool,
    file_integrity: bool,
}

struct LoadTestRunner {
    output_dir: PathBuf,
    verbose: bool,
}

impl LoadTestRunner {
    fn new(output_dir: PathBuf, verbose: bool) -> Self {
        Self { output_dir, verbose }
    }

    fn run_all_tests(&self) -> Result<LoadTestSuiteResults> {
        println!("🧪 Running comprehensive load testing framework tests...\n");

        std::fs::create_dir_all(&self.output_dir)?;

        let test_categories = vec![
            ("load_testing_framework_tests", "Load Testing Framework Unit Tests"),
            ("load_testing_integration_tests", "Load Testing Integration Tests"),
            ("benchmark_integration_tests", "Benchmark Integration Tests"),
            ("configuration_tests", "Configuration Validation Tests"),
        ];

        let mut all_results = Vec::new();
        let start_time = Instant::now();

        // First, validate the framework
        let framework_validation = self.validate_framework()?;
        if !framework_validation.compilation_success {
            println!("❌ Framework validation failed. Skipping tests.");
            return Ok(LoadTestSuiteResults {
                suite_name: "Load Testing Framework Tests".to_string(),
                total_tests: 0,
                passed_tests: 0,
                failed_tests: 0,
                execution_time: start_time.elapsed(),
                test_categories: vec![],
                framework_validation,
            });
        }

        println!("✅ Framework validation passed.\n");

        for (suite_file, suite_name) in test_categories {
            println!("📋 Running {} tests...", suite_name);
            let results = self.run_test_category(suite_file, suite_name)?;

            self.print_category_results(&results);
            all_results.push(results);

            println!();
        }

        let total_execution_time = start_time.elapsed();
        let mut total_tests = 0;
        let mut passed_tests = 0;
        let mut failed_tests = 0;

        for category in &all_results {
            total_tests += category.total_tests;
            passed_tests += category.passed_tests;
            failed_tests += category.failed_tests;
        }

        let suite_results = LoadTestSuiteResults {
            suite_name: "Load Testing Framework Tests".to_string(),
            total_tests,
            passed_tests,
            failed_tests,
            execution_time: total_execution_time,
            test_categories: all_results,
            framework_validation,
        };

        self.generate_suite_report(&suite_results)?;
        self.generate_json_report(&suite_results)?;

        Ok(suite_results)
    }

    fn validate_framework(&self) -> Result<FrameworkValidationResults> {
        println!("🔍 Validating load testing framework...");

        let mut validation = FrameworkValidationResults {
            compilation_success: false,
            dependency_validation: false,
            benchmark_structure: false,
            framework_structure: false,
            file_integrity: false,
        };

        // Test compilation
        println!("  🔨 Testing compilation...");
        let compile_output = Command::new("cargo")
            .args(&["check", "--package", "crucible-benchmarks"])
            .output()?;

        validation.compilation_success = compile_output.status.success();
        if self.verbose && !validation.compilation_success {
            println!("    Compilation errors: {}", String::from_utf8_lossy(&compile_output.stderr));
        }

        // Test dependencies
        println!("  📦 Testing dependencies...");
        let dep_output = Command::new("cargo")
            .args(&["tree", "--package", "crucible-benchmarks"])
            .output()?;

        let dep_tree = String::from_utf8_lossy(&dep_output.stdout);
        let required_deps = vec!["criterion", "tokio", "futures", "serde", "rand"];
        validation.dependency_validation = required_deps.iter().all(|dep| dep_tree.contains(dep));

        // Test benchmark structure
        println!("  📊 Testing benchmark structure...");
        validation.benchmark_structure = self.validate_benchmark_structure()?;

        // Test framework structure
        println!("  🏗️ Testing framework structure...");
        validation.framework_structure = self.validate_framework_structure()?;

        // Test file integrity
        println!("  📁 Testing file integrity...");
        validation.file_integrity = self.validate_file_integrity()?;

        let all_passed = validation.compilation_success &&
                       validation.dependency_validation &&
                       validation.benchmark_structure &&
                       validation.framework_structure &&
                       validation.file_integrity;

        if all_passed {
            println!("✅ Framework validation passed.\n");
        } else {
            println!("❌ Framework validation failed:");
            if !validation.compilation_success {
                println!("  ❌ Compilation failed");
            }
            if !validation.dependency_validation {
                println!("  ❌ Dependencies missing");
            }
            if !validation.benchmark_structure {
                println!("  ❌ Benchmark structure invalid");
            }
            if !validation.framework_structure {
                println!("  ❌ Framework structure invalid");
            }
            if !validation.file_integrity {
                println!("  ❌ File integrity check failed");
            }
            println!();
        }

        Ok(validation)
    }

    fn validate_benchmark_structure(&self) -> Result<bool> {
        let benchmark_file = std::fs::read_to_string("benches/script_engine_load_tests.rs")?;

        let required_functions = vec![
            "fn bench_concurrent_tool_execution",
            "fn bench_sustained_load",
            "fn bench_mixed_workload",
            "fn bench_resource_usage_under_load",
            "fn bench_error_handling_under_load",
        ];

        let has_all_functions = required_functions.iter().all(|func| benchmark_file.contains(func));
        let has_criterion_setup = benchmark_file.contains("criterion_group!") &&
                               benchmark_file.contains("criterion_main!");

        Ok(has_all_functions && has_criterion_setup)
    }

    fn validate_framework_structure(&self) -> Result<bool> {
        let framework_file = std::fs::read_to_string("benches/load_testing_framework.rs")?;

        let required_structs = vec![
            "pub struct LoadTestConfig",
            "pub struct ScriptEngineLoadTester",
            "pub struct MockScriptEngine",
            "pub struct MetricsCollector",
        ];

        let required_methods = vec![
            "pub async fn run_load_test",
            "pub async fn execute_tool",
            "pub fn record_operation",
        ];

        let has_all_structs = required_structs.iter().all(|struct_name| framework_file.contains(struct_name));
        let has_all_methods = required_methods.iter().all(|method| framework_file.contains(method));
        let has_configurations = framework_file.contains("pub fn light_load_test") &&
                               framework_file.contains("pub fn stress_test");

        Ok(has_all_structs && has_all_methods && has_configurations)
    }

    fn validate_file_integrity(&self) -> Result<bool> {
        let required_files = vec![
            "benches/script_engine_load_tests.rs",
            "benches/load_testing_framework.rs",
            "benches/Cargo.toml",
        ];

        let all_files_exist = required_files.iter().all(|file| Path::new(file).exists());

        // Check Cargo.toml has required entries
        let cargo_toml = std::fs::read_to_string("benches/Cargo.toml")?;
        let has_required_entries = cargo_toml.contains("script_engine_load_tests") &&
                                cargo_toml.contains("rand") &&
                                cargo_toml.contains("serde");

        Ok(all_files_exist && has_required_entries)
    }

    fn run_test_category(&self, suite_file: &str, suite_name: &str) -> Result<TestCategoryResults> {
        let start_time = Instant::now();
        let mut category_results = TestCategoryResults {
            category_name: suite_name.to_string(),
            total_tests: 0,
            passed_tests: 0,
            failed_tests: 0,
            execution_time: Duration::ZERO,
            test_details: Vec::new(),
        };

        // Run the test suite
        let mut cmd = Command::new("cargo");
        cmd.args(&["test", "--package", "crucible-benchmarks"])
            .args(&["--", "--nocapture", suite_file])
            .current_dir(env::current_dir()?);

        if self.verbose {
            cmd.env("RUST_LOG", "debug");
        }

        let output = cmd.output()
            .with_context(|| format!("Failed to run test suite: {}", suite_file))?;

        let execution_time = start_time.elapsed();
        category_results.execution_time = execution_time;

        // Parse test output
        let output_str = String::from_utf8_lossy(&output.stdout);
        let error_str = String::from_utf8_lossy(&output.stderr);

        if self.verbose {
            println!("STDOUT:\n{}", output_str);
            if !error_str.is_empty() {
                println!("STDERR:\n{}", error_str);
            }
        }

        // Parse individual test results
        for line in output_str.lines() {
            if line.contains("test") && (line.contains("ok") || line.contains("FAILED")) {
                if let Some(test_result) = self.parse_test_result(line, execution_time) {
                    category_results.total_tests += 1;
                    if test_result.passed {
                        category_results.passed_tests += 1;
                    } else {
                        category_results.failed_tests += 1;
                    }
                    category_results.test_details.push(test_result);
                }
            }
        }

        // If we couldn't parse individual tests, record overall success
        if category_results.total_tests == 0 {
            let overall_success = output.status.success();
            category_results.total_tests = 1;
            if overall_success {
                category_results.passed_tests = 1;
            } else {
                category_results.failed_tests = 1;
            }

            category_results.test_details.push(TestDetail {
                name: format!("{}_overall", suite_file),
                passed: overall_success,
                execution_time,
                error_message: if overall_success { None } else { Some("Test suite failed".to_string()) },
            });
        }

        Ok(category_results)
    }

    fn parse_test_result(&self, line: &str, total_time: Duration) -> Option<TestDetail> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            return None;
        }

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

        Some(TestDetail {
            name: test_name,
            passed,
            execution_time,
            error_message: error,
        })
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

    fn print_category_results(&self, results: &TestCategoryResults) {
        println!("  ✅ Passed: {}", results.passed_tests);
        println!("  ❌ Failed: {}", results.failed_tests);
        println!("  ⏱️  Time: {:?}", results.execution_time);
        println!("  📊 Success Rate: {:.1}%", (results.passed_tests as f64 / results.total_tests as f64) * 100.0);

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

    fn generate_suite_report(&self, results: &LoadTestSuiteResults) -> Result<()> {
        let report_path = self.output_dir.join("LOAD_TEST_FRAMEWORK_TEST_REPORT.md");
        let mut report = String::new();

        report.push_str("# Phase 6.6 Load Testing Framework Test Report\n\n");
        report.push_str(&format!("Generated: {}\n\n", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));

        // Framework validation summary
        report.push_str("## Framework Validation\n\n");
        report.push_str("| Check | Status |\n");
        report.push_str("|-------|--------|\n");
        report.push_str(&format!("| Compilation | {} |\n",
            if results.framework_validation.compilation_success { "✅ PASS" } else { "❌ FAIL" }));
        report.push_str(&format!("| Dependencies | {} |\n",
            if results.framework_validation.dependency_validation { "✅ PASS" } else { "❌ FAIL" }));
        report.push_str(&format!("| Benchmark Structure | {} |\n",
            if results.framework_validation.benchmark_structure { "✅ PASS" } else { "❌ FAIL" }));
        report.push_str(&format!("| Framework Structure | {} |\n",
            if results.framework_validation.framework_structure { "✅ PASS" } else { "❌ FAIL" }));
        report.push_str(&format!("| File Integrity | {} |\n",
            if results.framework_validation.file_integrity { "✅ PASS" } else { "❌ FAIL" }));

        // Overall statistics
        let success_rate = if results.total_tests > 0 {
            (results.passed_tests as f64 / results.total_tests as f64) * 100.0
        } else {
            0.0
        };

        report.push_str("\n## Overall Results\n\n");
        report.push_str("| Metric | Value |\n");
        report.push_str("|--------|-------|\n");
        report.push_str(&format!("| Total Test Categories | {} |\n", results.test_categories.len()));
        report.push_str(&format!("| Total Tests | {} |\n", results.total_tests));
        report.push_str(&format!("| Passed | {} |\n", results.passed_tests));
        report.push_str(&format!("| Failed | {} |\n", results.failed_tests));
        report.push_str(&format!("| Success Rate | {:.1}% |\n", success_rate));
        report.push_str(&format!("| Execution Time | {:?} |\n", results.execution_time));

        // Test category details
        report.push_str("\n## Test Category Results\n\n");
        report.push_str("| Category | Tests | Passed | Failed | Success Rate | Time |\n");
        report.push_str("|----------|-------|--------|--------|--------------|------|\n");

        for category in &results.test_categories {
            let category_success_rate = if category.total_tests > 0 {
                (category.passed_tests as f64 / category.total_tests as f64) * 100.0
            } else {
                0.0
            };

            report.push_str(&format!(
                "| {} | {} | {} | {} | {:.1}% | {:?} |\n",
                category.category_name,
                category.total_tests,
                category.passed_tests,
                category.failed_tests,
                category_success_rate,
                category.execution_time
            ));
        }

        // Failed tests summary
        if results.failed_tests > 0 {
            report.push_str("\n## Failed Tests\n\n");
            for category in &results.test_categories {
                for test in &category.test_details {
                    if !test.passed {
                        report.push_str(&format!(
                            "### {} - {}\n\n",
                            category.category_name, test.name
                        ));
                        if let Some(error) = &test.error_message {
                            report.push_str(&format!("**Error**: {}\n\n", error));
                        }
                    }
                }
            }
        }

        // Framework quality assessment
        report.push_str("## Framework Quality Assessment\n\n");

        let framework_score = [
            results.framework_validation.compilation_success,
            results.framework_validation.dependency_validation,
            results.framework_validation.benchmark_structure,
            results.framework_validation.framework_structure,
            results.framework_validation.file_integrity,
        ].iter().filter(|&&x| x).count() as f64 / 5.0 * 100.0;

        if framework_score >= 90.0 {
            report.push_str("✅ **Excellent**: Load testing framework validation passed with high score\n");
        } else if framework_score >= 75.0 {
            report.push_str("⚠️  **Good**: Load testing framework validation passed with acceptable score\n");
        } else {
            report.push_str("❌ **Needs Attention**: Load testing framework has validation issues\n");
        }

        report.push_str(&format!("\n**Framework Validation Score**: {:.1}%\n", framework_score));

        // Test coverage areas
        report.push_str("\n## Test Coverage Areas\n\n");
        report.push_str("- ✅ Load testing framework unit tests\n");
        report.push_str("- ✅ Load testing integration tests\n");
        report.push_str("- ✅ Benchmark integration validation\n");
        report.push_str("- ✅ Configuration validation tests\n");
        report.push_str("- ✅ Framework structure validation\n");

        // Recommendations
        report.push_str("\n## Recommendations\n\n");

        if results.failed_tests > 0 {
            report.push_str("- Review and fix failing tests\n");
            report.push_str("- Investigate potential framework issues\n");
        } else {
            report.push_str("- Load testing framework is stable and ready for use\n");
            report.push_str("- Consider adding additional edge case tests\n");
        }

        if !results.framework_validation.compilation_success {
            report.push_str("- Fix compilation issues before using the framework\n");
        }

        std::fs::write(&report_path, report)?;
        println!("📝 Test report generated: {}", report_path.display());

        Ok(())
    }

    fn generate_json_report(&self, results: &LoadTestSuiteResults) -> Result<()> {
        let json_path = self.output_dir.join("load_test_framework_results.json");
        let json_data = serde_json::json!({
            "generated_at": chrono::Utc::now(),
            "framework_validation": results.framework_validation,
            "summary": {
                "total_categories": results.test_categories.len(),
                "total_tests": results.total_tests,
                "passed_tests": results.passed_tests,
                "failed_tests": results.failed_tests,
                "success_rate": if results.total_tests > 0 {
                    (results.passed_tests as f64 / results.total_tests as f64) * 100.0
                } else {
                    0.0
                },
                "execution_time": format!("{:?}", results.execution_time),
            },
            "test_categories": results.test_categories
        });

        let json_content = serde_json::to_string_pretty(&json_data)?;
        std::fs::write(&json_path, json_content)?;
        println!("📊 JSON test results generated: {}", json_path.display());

        Ok(())
    }
}

fn print_usage() {
    println!("Usage: load_test_runner [OPTIONS]");
    println!();
    println!("Options:");
    println!("  --output-dir <DIR>    Output directory for test reports [default: load_test_results]");
    println!("  --verbose              Enable verbose output");
    println!("  --help                 Show this help message");
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    let mut output_dir = PathBuf::from("load_test_results");
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

    let runner = LoadTestRunner::new(output_dir, verbose);
    let results = runner.run_all_tests()?;

    // Determine exit code based on results
    let framework_healthy = results.framework_validation.compilation_success &&
                           results.framework_validation.dependency_validation &&
                           results.framework_validation.benchmark_structure &&
                           results.framework_validation.framework_structure &&
                           results.framework_validation.file_integrity;

    let tests_passed = results.failed_tests == 0;

    if framework_healthy && tests_passed {
        println!("\n🎉 All tests passed! Load testing framework is ready for use.");
        std::process::exit(0);
    } else {
        println!("\n❌ Framework validation or tests failed. Please review the test report.");
        std::process::exit(1);
    }
}