//! Test suite for Crucible knowledge management system
//!
//! This module organizes the comprehensive test suite for the Crucible system,
//! including search validation, integration tests, and performance benchmarks.

// Core test modules
pub mod common;
pub mod test_utilities;
pub mod knowledge_management_tests;
pub mod performance_validation_tests;
pub mod search_validation_comprehensive;
pub mod search_validation_extended;
pub mod script_execution_tests;
pub mod concurrent_user_tests;
pub mod database_integration_tests;
pub mod error_scenarios;
pub mod resilience_tests;
pub mod phase8_integration_tests;
pub mod phase8_main_test_runner;
pub mod phase8_final_report;
pub mod cross_component_integration_tests;
pub mod workload_simulator;

// Comprehensive integration workflow test modules
pub mod comprehensive_integration_workflow_tests;
pub mod cli_workflow_integration_tests;
pub mod repl_interactive_workflow_tests;
pub mod tool_api_integration_tests;
pub mod cross_interface_consistency_tests;
pub mod real_world_usage_scenario_tests;
pub mod integration_workflow_test_runner;

// Re-export common test utilities for easier access
pub use common::*;
pub use test_utilities::*;

// Re-export integration workflow test runners for easier access
pub use integration_workflow_test_runner::{IntegrationWorkflowTestRunner, TestRunnerConfig, run_integration_tests};