//! Help-text and subcommand-inference regression tests.
//!
//! Snapshot tests guard against accidental regressions in user-facing help
//! output. Behaviour tests assert that prefix subcommand inference and
//! suggestion-on-typo work.

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser};
use crucible_cli::cli::Cli;
use insta::assert_snapshot;

/// Clap's own audit of the arg definition: duplicate ids, colliding shorts,
/// defaults that are not valid values. The class of drift that let a global
/// `-f, --format` advertise `csv` while no code read the field starts here.
#[test]
fn clap_arg_definition_is_self_consistent() {
    Cli::command().debug_assert();
}

/// There is no global output format. Every command that renders formatted
/// output declares its own `--format`, and clap never propagated the global
/// into those anyway, so `cru -f json <cmd>` was accepted and ignored.
#[test]
fn format_before_the_subcommand_is_an_unknown_argument() {
    let err = Cli::try_parse_from(["cru", "-f", "json", "search", "query"])
        .map(|_| ())
        .expect_err("`cru -f json search` must not parse: there is no global --format");
    assert_eq!(err.kind(), ErrorKind::UnknownArgument, "{err}");
}

/// `cru storage {stats,verify,backup,restore} --format` was bound to `_format`
/// or discarded by `..` — a flag that told a script it got JSON and then
/// printed a human report. Absent is better than inert.
#[test]
fn storage_subcommands_reject_a_format_flag() {
    for args in [
        vec!["cru", "storage", "stats", "-f", "table"],
        vec!["cru", "storage", "verify", "-f", "json"],
        vec!["cru", "storage", "backup", "/tmp/dest", "-f", "binary"],
        vec!["cru", "storage", "restore", "/tmp/src", "-f", "binary"],
    ] {
        let rendered = args.join(" ");
        let err = Cli::try_parse_from(&args)
            .map(|_| ())
            .expect_err(&format!("`{rendered}` must not parse: --format is gone"));
        assert_eq!(err.kind(), ErrorKind::UnknownArgument, "{rendered}: {err}");
    }
}

/// Stage 1 removes only fictions from the help text; the values the
/// subcommands accept are untouched. The narrowing to a typed `ValueEnum`
/// vocabulary is a separate change, so `csv` still parses here even though no
/// help string offers it any more.
#[test]
fn subcommand_format_values_are_unchanged() {
    for args in [
        vec!["cru", "stats", "-f", "csv"],
        vec!["cru", "models", "-f", "csv"],
        vec!["cru", "stats", "-f", "json"],
        vec!["cru", "search", "q", "-f", "plain"],
        vec!["cru", "status", "-f", "table"],
    ] {
        let rendered = args.join(" ");
        assert!(
            Cli::try_parse_from(&args).is_ok(),
            "`{rendered}` must still parse"
        );
    }
}

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
