use super::parse;
use crate::cli::*;
use clap::Parser;

#[test]
fn test_agents_with_tag_filter() {
    let Commands::Agents { tag, command, .. } = parse(&["cru", "agents", "-t", "documentation"])
    else {
        panic!("Expected Agents command");
    };
    assert_eq!(tag, Some("documentation".to_string()));
    assert!(command.is_none());
}

#[test]
fn test_agents_validate_parses() {
    assert!(matches!(
        parse(&["cru", "agents", "validate"]),
        Commands::Agents {
            command: Some(AgentsCommands::Validate { .. }),
            ..
        }
    ));
}

/// `list` and `show` are gone: `cru agents` alone is the one list.
#[test]
fn test_agents_has_no_list_or_show_subcommand() {
    for sub in ["list", "show"] {
        assert!(
            Cli::try_parse_from(["cru", "agents", sub]).is_err(),
            "{sub}"
        );
    }
}
