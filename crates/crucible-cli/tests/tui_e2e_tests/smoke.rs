//! Smoke tests: basic `--version`/`--help` and subcommand help output.

use super::shared::require_binary;
use super::tui_e2e_harness::{TuiTestConfig, TuiTestSession};

// =============================================================================
// Smoke Tests
// =============================================================================

/// Test that `cru --version` works
///
/// This test runs by default (no #[ignore]) and skips gracefully if the binary
/// isn't built. This allows CI to catch version string issues when the binary
/// is available.
#[test]
fn smoke_version() {
    require_binary!();

    let config = TuiTestConfig::new("--version");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect("cru").expect("Should show cru in version");
    session.expect_eof().expect("Should exit");
}

/// Test that `cru --help` works
///
/// Runs by default and skips gracefully if binary isn't built.
#[test]
fn smoke_help() {
    require_binary!();

    let config = TuiTestConfig::new("--help");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect("Usage").expect("Should show usage");
    session.expect_eof().expect("Should exit");
}

// =============================================================================
// Subcommand Help Output Tests
// =============================================================================

/// Test that `cru config --help` shows available subcommands
#[test]
fn smoke_config_help() {
    require_binary!();

    let config = TuiTestConfig::new("config --help");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect("init").expect("Should show init subcommand");
    session.expect("show").expect("Should show show subcommand");
    session.expect_eof().expect("Should exit");
}

/// Test that `cru daemon --help` shows daemon management options
#[test]
fn smoke_daemon_help() {
    require_binary!();

    let config = TuiTestConfig::new("daemon --help");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session
        .expect("start")
        .expect("Should show start subcommand");
    session.expect("stop").expect("Should show stop subcommand");
    session
        .expect("status")
        .expect("Should show status subcommand");
    session.expect_eof().expect("Should exit");
}

/// Test that `cru storage --help` shows storage management options
#[test]
fn smoke_storage_help() {
    require_binary!();

    let config = TuiTestConfig::new("storage --help");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect("Usage").expect("Should show usage");
    session.expect_eof().expect("Should exit");
}

/// Test that `cru process --help` shows processing options
#[test]
fn smoke_process_help() {
    require_binary!();

    let config = TuiTestConfig::new("process --help");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect("Usage").expect("Should show usage");
    session.expect_eof().expect("Should exit");
}

/// Test that `cru stats --help` shows stats options
#[test]
fn smoke_stats_help() {
    require_binary!();

    let config = TuiTestConfig::new("stats --help");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect("Usage").expect("Should show usage");
    session.expect_eof().expect("Should exit");
}

/// Test that `cru mcp --help` shows MCP server options
#[test]
fn smoke_mcp_help() {
    require_binary!();

    let config = TuiTestConfig::new("mcp --help");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect("Usage").expect("Should show usage");
    session.expect_eof().expect("Should exit");
}

/// Test that `cru chat --help` shows chat options
#[test]
fn smoke_chat_help() {
    require_binary!();

    let config = TuiTestConfig::new("chat --help");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session.expect("Usage").expect("Should show usage");
    session.expect_eof().expect("Should exit");
}
