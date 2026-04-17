use crate::cli::*;
use clap::Parser;

#[test]
fn test_agents_list_parses() {
    let cli = Cli::try_parse_from(["cru", "agents", "list"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Agents {
            command: Some(AgentsCommands::List { .. })
        })
    ));
}

#[test]
fn test_agents_list_with_tag_filter() {
    let cli = Cli::try_parse_from(["cru", "agents", "list", "-t", "documentation"]).unwrap();
    if let Some(Commands::Agents {
        command: Some(AgentsCommands::List { tag, .. }),
    }) = cli.command
    {
        assert_eq!(tag, Some("documentation".to_string()));
    } else {
        panic!("Expected Agents List command");
    }
}

#[test]
fn test_agents_show_parses() {
    let cli = Cli::try_parse_from(["cru", "agents", "show", "General Assistant"]).unwrap();
    if let Some(Commands::Agents {
        command: Some(AgentsCommands::Show { name, .. }),
    }) = cli.command
    {
        assert_eq!(name, "General Assistant");
    } else {
        panic!("Expected Agents Show command");
    }
}

#[test]
fn test_agents_validate_parses() {
    let cli = Cli::try_parse_from(["cru", "agents", "validate"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Agents {
            command: Some(AgentsCommands::Validate { .. })
        })
    ));
}

#[test]
fn test_agents_defaults_to_list() {
    // Per design decision: `cru agents` defaults to `list`
    // When no subcommand is given, command is None, which we treat as List
    let cli = Cli::try_parse_from(["cru", "agents"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Agents { command: None })
    ));
}
