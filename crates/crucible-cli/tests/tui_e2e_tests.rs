//! TUI End-to-End Tests
//!
//! These tests use expectrl to spawn real `cru` processes in PTY sessions,
//! enabling multi-turn interaction testing.
//!
//! # Running Tests
//!
//! ```bash
//! # Build first
//! cargo build --release
//!
//! # Run e2e tests (marked as ignored by default)
//! cargo test -p crucible-cli tui_e2e -- --ignored
//! ```
//!
//! # Test Categories
//!
//! - **Smoke tests**: Basic startup/shutdown
//! - **Navigation tests**: Key sequences, mode switching
//! - **Multi-turn tests**: Full conversation flows
//! - **Command tests**: Slash command behavior
//!
//! # vt100 Assertion Patterns
//!
//! Tests use a vt100 terminal emulator (`TuiTestSession.vt_parser`) that parses
//! all PTY output into a queryable screen buffer. Prefer these over raw string
//! matching on `capture_screen()` output:
//!
//! ```rust,ignore
//! // Wait for the TUI to render expected content (polls with timeout):
//! session.wait_for_text("Plan", Duration::from_secs(3)).expect("mode visible");
//!
//! // Assert on parsed screen state (no ANSI escape codes):
//! assert_screen_contains(session.screen(), "Plan");
//! assert_screen_not_contains(session.screen(), "Error");
//!
//! // Custom predicates for complex conditions:
//! session.wait_until(
//!     |s| s.contents().contains("Act") || s.contents().contains("Plan"),
//!     Duration::from_secs(3),
//! ).expect("mode indicator visible");
//! ```
//!
//! See `tui_e2e_harness.rs` for the full set of assertion helpers:
//! `assert_screen_contains`, `assert_screen_not_contains`, `assert_row_contains`,
//! `assert_region_contains`, `assert_cursor_at`, `assert_cell_bold`.

// Include the harness module
#[path = "tui_e2e_harness.rs"]
mod tui_e2e_harness;

#[path = "tui_e2e_tests/shared.rs"]
mod shared;

#[path = "tui_e2e_tests/smoke.rs"]
mod smoke;
#[path = "tui_e2e_tests/chat.rs"]
mod chat;
#[path = "tui_e2e_tests/oil.rs"]
mod oil;
#[path = "tui_e2e_tests/model.rs"]
mod model;
#[path = "tui_e2e_tests/popup.rs"]
mod popup;
#[path = "tui_e2e_tests/terminal.rs"]
mod terminal;
#[path = "tui_e2e_tests/errors.rs"]
mod errors;
#[path = "tui_e2e_tests/vt100.rs"]
mod vt100;
