//! Test module declarations for daemon integration tests

pub mod daemon_event_integration_tests;

// Re-export test utilities for other test modules
pub use daemon_event_integration_tests::*;