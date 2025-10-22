//! # Integration Test Module Organization
//!
//! This module organizes all integration tests in the crucible-services crate.
//! It provides a clear structure for different test categories and makes it easy
//! to run specific types of tests.

// Core integration tests
pub mod service_integration_tests;
pub mod consolidated_integration_tests;

// Event system tests (Phase 1 - Keep these as they are comprehensive)
pub mod event_circuit_breaker_tests;
pub mod event_concurrent_tests;
pub mod event_core_tests;
pub mod event_error_handling_tests;
pub mod event_filtering_tests;
pub mod event_load_balancing_tests;
pub mod event_performance_tests;
pub mod event_property_based_tests;
pub mod event_routing_integration_tests;
pub mod event_test_utilities;

// Test utilities and frameworks
pub mod integration_test_runner;
pub mod test_utilities;
pub mod mock_services;
pub mod event_validation;
pub mod performance_benchmarks;

// Legacy Phase 2 tests (marked for consolidation/deprecation)
// These will be replaced by consolidated_integration_tests.rs
pub mod phase2_integration_tests;
pub mod phase2_main_test;
pub mod phase2_simple_validation;
pub mod phase2_test_runner;
pub mod phase2_validation_tests;

// Unit tests (basic coverage)
pub mod unit_tests;

// Re-export main test runners for easier access
pub use service_integration_tests::*;
pub use consolidated_integration_tests::*;
pub use integration_test_runner::*;
pub use test_utilities::*;
pub use mock_services::*;
pub use event_validation::*;
pub use performance_benchmarks::*;

// Re-export legacy Phase 2 components (for backward compatibility)
pub use phase2_integration_tests::*;
pub use phase2_test_runner::*;
pub use phase2_validation_tests::*;

// Re-export event system tests
pub use event_circuit_breaker_tests::*;
pub use event_concurrent_tests::*;
pub use event_core_tests::*;
pub use event_error_handling_tests::*;
pub use event_filtering_tests::*;
pub use event_load_balancing_tests::*;
pub use event_performance_tests::*;
pub use event_property_based_tests::*;
pub use event_routing_integration_tests::*;
pub use event_test_utilities::*;