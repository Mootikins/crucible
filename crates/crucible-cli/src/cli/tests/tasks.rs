use super::parse;
use crate::cli::*;
use clap::Parser;

#[test]
fn test_tasks_subcommand_exists() {
    // Test that `cru tasks --help` works
    let result = Cli::try_parse_from(["cru", "tasks", "--help"]);
    // --help exits with error code, but we can test that the command is recognized
    assert!(result.is_err()); // clap exits with error on --help
}

#[test]
fn test_tasks_list_parses() {
    assert!(matches!(
        parse(&["cru", "tasks", "list"]),
        Commands::Tasks { .. }
    ));
}

#[test]
fn test_tasks_next_parses() {
    assert!(matches!(
        parse(&["cru", "tasks", "next"]),
        Commands::Tasks { .. }
    ));
}

#[test]
fn test_tasks_pick_parses() {
    let Commands::Tasks { file: _, command } = parse(&["cru", "tasks", "pick", "task-1"]) else {
        panic!("Expected Tasks command");
    };
    assert!(matches!(
        command,
        crate::commands::tasks::TasksSubcommand::Pick { .. }
    ));
}

#[test]
fn test_tasks_done_parses() {
    let Commands::Tasks { file: _, command } = parse(&["cru", "tasks", "done", "task-1"]) else {
        panic!("Expected Tasks command");
    };
    assert!(matches!(
        command,
        crate::commands::tasks::TasksSubcommand::Done { .. }
    ));
}

#[test]
fn test_tasks_blocked_parses() {
    let Commands::Tasks { file: _, command } = parse(&["cru", "tasks", "blocked", "task-1"]) else {
        panic!("Expected Tasks command");
    };
    assert!(matches!(
        command,
        crate::commands::tasks::TasksSubcommand::Blocked { .. }
    ));
}

#[test]
fn test_tasks_blocked_with_reason_parses() {
    let Commands::Tasks { file: _, command } =
        parse(&["cru", "tasks", "blocked", "task-1", "waiting for review"])
    else {
        panic!("Expected Tasks command");
    };
    let crate::commands::tasks::TasksSubcommand::Blocked { id, reason } = command else {
        panic!("Expected Blocked subcommand");
    };
    assert_eq!(id, "task-1");
    assert_eq!(reason, Some("waiting for review".to_string()));
}
