//! # Plugin Manager Test Suite
//!
//! Comprehensive unit and integration tests for the PluginManager system.
//! This test suite ensures the reliability, security, and performance of the
//! plugin management functionality.

pub mod common;
pub mod manager_tests;
pub mod registry_tests;
pub mod instance_tests;
pub mod resource_manager_tests;
pub mod security_manager_tests;
pub mod health_monitor_tests;
pub mod config_tests;
pub mod integration_tests;
pub mod performance_tests;
pub mod security_tests;

// Re-export common test utilities
pub use common::*;