//! Help-text and subcommand-inference regression tests.
//!
//! Snapshot tests guard against accidental regressions in user-facing help
//! output. Behaviour tests assert that prefix subcommand inference and
//! suggestion-on-typo work.

use clap::{CommandFactory, Parser};
use crucible_cli::cli::Cli;
use insta::assert_snapshot;

/// The top-level `cru --help` output is the front door of the CLI; lock it
/// down so future changes are reviewed deliberately.
#[test]
fn top_level_help_snapshot() {
    let help = Cli::command().render_long_help().to_string();
    assert_snapshot!("top_level_help", help);
}

/// `cru chat --help` is the most-used subcommand — protect the long_about
/// example block.
#[test]
fn chat_subcommand_help_snapshot() {
    let mut cmd = Cli::command();
    let chat = cmd
        .find_subcommand_mut("chat")
        .expect("chat subcommand exists");
    let help = chat.render_long_help().to_string();
    assert_snapshot!("chat_subcommand_help", help);
}

/// `cru session --help` is the second most-used surface (multi-session
/// scripting). Lock the long_about's lifecycle example.
#[test]
fn session_subcommand_help_snapshot() {
    let mut cmd = Cli::command();
    let session = cmd
        .find_subcommand_mut("session")
        .expect("session subcommand exists");
    let help = session.render_long_help().to_string();
    assert_snapshot!("session_subcommand_help", help);
}

/// Prefix inference: `cru con show` should resolve to `cru config show`.
/// `con` has no alias, so this test fails until `infer_subcommands = true`
/// is set on the top-level command. (`session` has aliases `s`/`sess`, so
/// inference must be tested with a prefix that does not double as an
/// alias.)
#[test]
fn prefix_infers_unique_subcommand() {
    let result = Cli::try_parse_from(["cru", "con", "show"]);
    let kind = result.as_ref().err().map(|e| e.kind());
    assert!(
        result.is_ok(),
        "expected `cru con show` to infer `config show`, error kind: {kind:?}"
    );
}

/// Unknown subcommand should produce a "did you mean" style suggestion.
/// Clap 4.x emits suggestions automatically; this test guards the
/// machinery so a future change to suggestion settings is caught.
#[test]
fn unknown_subcommand_suggests_close_match() {
    let result = Cli::try_parse_from(["cru", "stauts"]);
    let err = match result {
        Ok(_) => panic!("`cru stauts` is not a real subcommand"),
        Err(e) => e,
    };
    let msg = err.to_string();
    assert!(
        msg.contains("status") || msg.to_lowercase().contains("did you mean"),
        "expected suggestion mentioning 'status' or 'did you mean'; got:\n{msg}"
    );
}
