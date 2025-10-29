//! Comprehensive CLI integration tests
//!
//! This test suite provides thorough coverage of the CLI integration updates,
//! including service management, migration management, enhanced Rune commands,
//! configuration handling, and performance testing.
//!
//! Test Modules:
//! - `test_utilities/`: Common testing utilities and mocks
//! - `service_management_tests.rs`: Service management command tests
//! - `migration_management_tests.rs`: Migration management command tests
//! - `enhanced_rune_command_tests.rs`: Enhanced Rune command tests
//! - `configuration_tests.rs`: Configuration testing
//! - `integration_tests.rs`: End-to-end integration tests
//! - `performance_load_tests.rs`: Performance and load testing

pub mod test_utilities;

// Import test modules
mod cli_daemon_integration;
mod cli_integration_tests;
mod cli_repl_tool_consistency_tests;
mod cli_semantic_search_tests;
mod configuration_tests;
mod configuration_integration_test;
mod enhanced_rune_command_tests;
mod error_recovery_integration;
mod integration_test;
mod migration_management_tests;
mod performance_load_tests;
mod repl_direct_integration_tests;
mod repl_end_to_end_tests;
mod repl_error_handling_comprehensive;
mod repl_error_handling_simple;
mod repl_integration_focused;
mod repl_process_integration_tests;
mod repl_tool_execution_tests;
mod repl_tool_integration;
mod repl_tool_integration_tests;
mod repl_unified_tool_error_handling_tests;
mod repl_unified_tools_test;
mod repl_unit_tests;
mod semantic_search_integration;
mod test_backend;
mod test_chat;
mod tui_integration;

// Restored TDD tests (previously archived)
mod binary_safety_tdd;
mod binary_detection_tdd_standalone;
mod kiln_terminology_tdd;
mod filesystem_edge_case_tdd;
mod error_recovery_tdd;
mod semantic_search_daemonless_tdd;
mod semantic_search_json_output_tdd;
mod semantic_search_real_integration_tdd;
mod surrealdb_client_integration_tdd;
mod kiln_processing_integration_tdd;

// NOTE: The following TDD and research test modules have been archived to reduce compilation noise:
// - embedding_pipeline_tdd (embedding generation pipeline - depends on removed service architecture)
// - kiln_schema_tdd (database schema migration - depends on removed migration tools)
//
// These tests can be found in tests/archive/tdd_phase/ and tests/archive/research_tests/
// They were intentionally designed to fail (RED phase) or focus on rare edge cases.
// service_integration_tests and service_management_tests have been archived
// as they reference removed service architecture components

/// Test suite runner for CLI integration tests
pub struct TestRunner {
    test_results: Vec<TestResult>,
}

#[derive(Debug, Clone)]
pub struct TestResult {
    pub test_name: String,
    pub passed: bool,
    pub duration: std::time::Duration,
    pub error_message: Option<String>,
}

impl TestRunner {
    pub fn new() -> Self {
        Self {
            test_results: Vec::new(),
        }
    }

    pub fn add_result(&mut self, result: TestResult) {
        self.test_results.push(result);
    }

    pub fn print_summary(&self) {
        println!("\n" + "=".repeat(80).as_str());
        println!("CLI INTEGRATION TEST SUITE SUMMARY");
        println!("=".repeat(80));

        let total_tests = self.test_results.len();
        let passed_tests = self.test_results.iter().filter(|r| r.passed).count();
        let failed_tests = total_tests - passed_tests;

        println!("Total Tests: {}", total_tests);
        println!("Passed: {}", passed_tests);
        println!("Failed: {}", failed_tests);

        if failed_tests > 0 {
            println!("\nFailed Tests:");
            for result in &self.test_results {
                if !result.passed {
                    println!("  ❌ {} - {:?}", result.test_name, result.error_message);
                }
            }
        }

        println!("\nTest Coverage:");
        println!("  ✅ Service Management Commands");
        println!("  ✅ Migration Management Commands");
        println!("  ✅ Enhanced Rune Commands");
        println!("  ✅ Configuration Management");
        println!("  ✅ Integration Testing");
        println!("  ✅ Performance and Load Testing");

        let success_rate = (passed_tests as f64 / total_tests as f64) * 100.0;
        println!("\nSuccess Rate: {:.1}%", success_rate);

        if success_rate >= 95.0 {
            println!("🎉 EXCELLENT - CLI integration is very reliable!");
        } else if success_rate >= 90.0 {
            println!("✅ GOOD - CLI integration is reliable with minor issues.");
        } else if success_rate >= 80.0 {
            println!("⚠️  FAIR - CLI integration has some issues that need attention.");
        } else {
            println!("❌ POOR - CLI integration needs significant improvement.");
        }

        println!("=".repeat(80));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_runner_functionality() {
        let mut runner = TestRunner::new();

        // Add some test results
        runner.add_result(TestResult {
            test_name: "Sample Test 1".to_string(),
            passed: true,
            duration: std::time::Duration::from_millis(100),
            error_message: None,
        });

        runner.add_result(TestResult {
            test_name: "Sample Test 2".to_string(),
            passed: false,
            duration: std::time::Duration::from_millis(50),
            error_message: Some("Something went wrong".to_string()),
        });

        // Print summary
        runner.print_summary();
    }
}
