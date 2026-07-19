use super::parse;
use crate::cli::*;

#[test]
fn test_agents_list_parses() {
    assert!(matches!(
        parse(&["cru", "agents", "list"]),
        Commands::Agents {
            command: Some(AgentsCommands::List { .. })
        }
    ));
}

#[test]
fn test_agents_list_with_tag_filter() {
    let Commands::Agents {
        command: Some(AgentsCommands::List { tag, .. }),
    } = parse(&["cru", "agents", "list", "-t", "documentation"])
    else {
        panic!("Expected Agents List command");
    };
    assert_eq!(tag, Some("documentation".to_string()));
}

#[test]
fn test_agents_show_parses() {
    let Commands::Agents {
        command: Some(AgentsCommands::Show { name, .. }),
    } = parse(&["cru", "agents", "show", "General Assistant"])
    else {
        panic!("Expected Agents Show command");
    };
    assert_eq!(name, "General Assistant");
}

#[test]
fn test_agents_validate_parses() {
    assert!(matches!(
        parse(&["cru", "agents", "validate"]),
        Commands::Agents {
            command: Some(AgentsCommands::Validate { .. })
        }
    ));
}

#[test]
fn test_agents_defaults_to_list() {
    // Per design decision: `cru agents` defaults to `list`
    // When no subcommand is given, command is None, which we treat as List
    assert!(matches!(
        parse(&["cru", "agents"]),
        Commands::Agents { command: None }
    ));
}
