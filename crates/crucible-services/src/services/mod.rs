//! # Services Unit Tests Module
//!
//! This module contains unit tests for all core services in the crucible-services crate.
//! Each service has dedicated tests covering individual components, methods, and error scenarios.

// Import all service unit test modules
// pub mod script_engine_tests;
// pub mod data_store_tests;
// pub mod mcp_gateway_tests;
pub mod inference_engine_tests;

// Re-export test modules for easier access
// pub use script_engine_tests::*;
// pub use data_store_tests::*;
// pub use mcp_gateway_tests::*;
pub use inference_engine_tests::*;