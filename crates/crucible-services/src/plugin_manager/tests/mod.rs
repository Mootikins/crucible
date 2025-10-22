//! # Plugin Manager Test Suite
//!
//! Comprehensive test suite for the PluginManager system organized into:
//! - Unit tests: Individual component testing
//! - Integration tests: Component interaction testing
//! - Performance tests: Performance and scalability validation
//! - Security tests: Security validation and penetration testing
//! - E2E tests: End-to-end system validation

// Common test utilities and fixtures
pub mod common;

// Test modules organized by type
pub mod unit;
pub mod integration;
pub mod performance;
pub mod security;
pub mod e2e;

// Re-export common test utilities
pub use common::*;