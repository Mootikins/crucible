//! TUI testing infrastructure
//!
//! Provides a test harness and fixtures for testing TUI components without
//! requiring a real terminal.
//!
//! # Overview
//!
//! The testing module enables "write failing test first" workflow:
//!
//! 1. Write a test describing expected behavior
//! 2. Run test, confirm it fails
//! 3. Fix implementation
//! 4. Test passes → spec now exists
//!
//! # Components
//!
//! - [`Harness`] - Simulated TUI environment for testing
//! - [`TestStateBuilder`] - Fluent builder for TuiState
//! - [`fixtures`] - Reusable test data (sessions, registries, events)
//!
//! # Example
//!
//! ```ignore
//! use crate::tui::testing::{Harness, fixtures::sessions};
//! use crossterm::event::KeyCode;
//!
//! #[test]
//! fn popup_filters_as_you_type() {
//!     let mut h = Harness::new(80, 24);
//!     h.key(KeyCode::Char('/'));
//!     assert!(h.has_popup());
//!
//!     h.keys("sea");
//!     assert_eq!(h.popup_query(), Some("sea"));
//! }
//!
//! #[test]
//! fn conversation_shows_history() {
//!     let h = Harness::new(80, 24)
//!         .with_session(sessions::basic_exchange());
//!
//!     assert_eq!(h.conversation_len(), 2);
//!     insta::assert_snapshot!(h.render());
//! }
//! ```

#[cfg(test)]
mod code_block_tests;
#[cfg(test)]
mod e2e_flow_tests;
pub mod fixtures;
mod harness;
#[cfg(test)]
mod harness_tests;
#[cfg(test)]
mod popup_snapshot_tests;
#[cfg(test)]
mod resize_tests;
mod state_builder;
#[cfg(test)]
mod table_tests;
#[cfg(test)]
mod tool_call_tests;

pub use harness::Harness;
pub use state_builder::{
    render_to_terminal, test_terminal, test_terminal_sized, TestStateBuilder, TEST_HEIGHT,
    TEST_WIDTH,
};
