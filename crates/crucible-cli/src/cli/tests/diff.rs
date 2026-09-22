use super::parse;
use crate::cli::{Commands, CommentFormat, DiffCommands};
use crate::formatting::TextFormat;
use clap::Parser;
use std::path::PathBuf;

#[test]
fn diff_branch_parses_with_no_flags() {
    match parse(&["cru", "diff", "branch"]) {
        Commands::Diff {
            command:
                DiffCommands::Branch {
                    base,
                    head,
                    root,
                    stat,
                    format,
                },
        } => {
            assert_eq!(base, None, "the daemon picks the default branch");
            assert_eq!(head, None, "the working tree");
            assert_eq!(root, None, "the working directory");
            assert!(!stat);
            assert_eq!(format, TextFormat::Text);
        }
        _ => panic!("expected `diff branch`"),
    }
}

#[test]
fn diff_branch_parses_every_flag() {
    match parse(&[
        "cru", "diff", "branch", "--base", "develop", "--head", "HEAD", "--root", "/repo",
        "--stat", "-f", "json",
    ]) {
        Commands::Diff {
            command:
                DiffCommands::Branch {
                    base,
                    head,
                    root,
                    stat,
                    format,
                },
        } => {
            assert_eq!(base.as_deref(), Some("develop"));
            assert_eq!(head.as_deref(), Some("HEAD"));
            assert_eq!(root, Some(PathBuf::from("/repo")));
            assert!(stat);
            assert_eq!(format, TextFormat::Json);
        }
        _ => panic!("expected `diff branch`"),
    }
}

#[test]
fn diff_needs_a_subcommand() {
    assert!(crate::cli::Cli::try_parse_from(["cru", "diff"]).is_err());
}

#[test]
fn diff_comments_prints_quickfix_by_default() {
    match parse(&["cru", "diff", "comments", "session-chat-1"]) {
        Commands::Diff {
            command:
                DiffCommands::Comments {
                    diffset,
                    base,
                    head,
                    root,
                    format,
                },
        } => {
            assert_eq!(diffset, "session-chat-1");
            assert_eq!((base, head, root), (None, None, None));
            assert_eq!(format, CommentFormat::Quickfix);
        }
        _ => panic!("expected `diff comments`"),
    }
}

#[test]
fn diff_comments_parses_every_flag() {
    match parse(&[
        "cru", "diff", "comments", "branch", "--base", "develop", "--head", "HEAD", "--root",
        "/repo", "--format", "json",
    ]) {
        Commands::Diff {
            command:
                DiffCommands::Comments {
                    diffset,
                    base,
                    head,
                    root,
                    format,
                },
        } => {
            assert_eq!(diffset, "branch");
            assert_eq!(base.as_deref(), Some("develop"));
            assert_eq!(head.as_deref(), Some("HEAD"));
            assert_eq!(root, Some(PathBuf::from("/repo")));
            assert_eq!(format, CommentFormat::Json);
        }
        _ => panic!("expected `diff comments`"),
    }
    assert!(
        crate::cli::Cli::try_parse_from(["cru", "diff", "comments"]).is_err(),
        "the diffset is required"
    );
}
