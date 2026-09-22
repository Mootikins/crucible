use super::parse;
use crate::cli::{Cli, Commands, ProposalCommands};
use crate::formatting::TextFormat;
use clap::Parser;
use std::path::PathBuf;

const ID: &str = "6f1c1d2e-3b4a-4c5d-8e9f-0a1b2c3d4e5f";

fn proposal(args: &[&str]) -> ProposalCommands {
    let mut full = vec!["cru", "proposal"];
    full.extend_from_slice(args);
    match parse(&full) {
        Commands::Proposal { command } => command,
        _ => panic!("expected `proposal` from {args:?}"),
    }
}

#[test]
fn proposal_list_parses_with_and_without_all() {
    match proposal(&["list"]) {
        ProposalCommands::List { all, format } => {
            assert!(!all, "the Inbox only");
            assert_eq!(format, TextFormat::Text);
        }
        _ => panic!("expected `proposal list`"),
    }
    match proposal(&["list", "--all", "-f", "json"]) {
        ProposalCommands::List { all, format } => {
            assert!(all);
            assert_eq!(format, TextFormat::Json);
        }
        _ => panic!("expected `proposal list --all`"),
    }
}

#[test]
fn proposal_show_takes_an_id_and_an_optional_conflict_path() {
    match proposal(&["show", ID]) {
        ProposalCommands::Show {
            id,
            conflict,
            format,
        } => {
            assert_eq!(id.to_string(), ID);
            assert_eq!(conflict, None);
            assert_eq!(format, TextFormat::Text);
        }
        _ => panic!("expected `proposal show`"),
    }
    match proposal(&["show", ID, "--conflict", "notes/a.md"]) {
        ProposalCommands::Show { conflict, .. } => {
            assert_eq!(conflict.as_deref(), Some("notes/a.md"));
        }
        _ => panic!("expected `proposal show --conflict`"),
    }
}

#[test]
fn proposal_decisions_parse() {
    match proposal(&["accept", ID]) {
        ProposalCommands::Accept { id } => assert_eq!(id.to_string(), ID),
        _ => panic!("expected `proposal accept`"),
    }
    match proposal(&["reject", ID]) {
        ProposalCommands::Reject { id, reason } => {
            assert_eq!(id.to_string(), ID);
            assert_eq!(reason, None);
        }
        _ => panic!("expected `proposal reject`"),
    }
    match proposal(&["reject", ID, "--reason", "wrong note"]) {
        ProposalCommands::Reject { reason, .. } => {
            assert_eq!(reason.as_deref(), Some("wrong note"));
        }
        _ => panic!("expected `proposal reject --reason`"),
    }
    match proposal(&["dismiss", ID]) {
        ProposalCommands::Dismiss { id } => assert_eq!(id.to_string(), ID),
        _ => panic!("expected `proposal dismiss`"),
    }
}

#[test]
fn proposal_resolve_needs_a_path_and_a_file() {
    match proposal(&["resolve", ID, "notes/a.md", "--from", "a.md"]) {
        ProposalCommands::Resolve { id, path, from } => {
            assert_eq!(id.to_string(), ID);
            assert_eq!(path, "notes/a.md");
            assert_eq!(from, PathBuf::from("a.md"));
        }
        _ => panic!("expected `proposal resolve`"),
    }
    assert!(
        Cli::try_parse_from(["cru", "proposal", "resolve", ID, "notes/a.md"]).is_err(),
        "--from is required"
    );
}

#[test]
fn a_proposal_id_that_is_not_a_uuid_is_refused() {
    for sub in ["show", "accept", "reject", "dismiss"] {
        assert!(
            Cli::try_parse_from(["cru", "proposal", sub, "not-a-uuid"]).is_err(),
            "{sub}"
        );
    }
}
