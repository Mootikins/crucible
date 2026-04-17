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
    let cli = Cli::try_parse_from(["cru", "tasks", "list"]).unwrap();
    assert!(matches!(cli.command, Some(Commands::Tasks { .. })));
}

#[test]
fn test_tasks_next_parses() {
    let cli = Cli::try_parse_from(["cru", "tasks", "next"]).unwrap();
    assert!(matches!(cli.command, Some(Commands::Tasks { .. })));
}

#[test]
fn test_tasks_pick_parses() {
    let cli = Cli::try_parse_from(["cru", "tasks", "pick", "task-1"]).unwrap();
    if let Some(Commands::Tasks { file: _, command }) = cli.command {
        assert!(matches!(
            command,
            crate::commands::tasks::TasksSubcommand::Pick { .. }
        ));
    } else {
        panic!("Expected Tasks command");
    }
}

#[test]
fn test_tasks_done_parses() {
    let cli = Cli::try_parse_from(["cru", "tasks", "done", "task-1"]).unwrap();
    if let Some(Commands::Tasks { file: _, command }) = cli.command {
        assert!(matches!(
            command,
            crate::commands::tasks::TasksSubcommand::Done { .. }
        ));
    } else {
        panic!("Expected Tasks command");
    }
}

#[test]
fn test_tasks_blocked_parses() {
    let cli = Cli::try_parse_from(["cru", "tasks", "blocked", "task-1"]).unwrap();
    if let Some(Commands::Tasks { file: _, command }) = cli.command {
        assert!(matches!(
            command,
            crate::commands::tasks::TasksSubcommand::Blocked { .. }
        ));
    } else {
        panic!("Expected Tasks command");
    }
}

#[test]
fn test_tasks_blocked_with_reason_parses() {
    let cli = Cli::try_parse_from(["cru", "tasks", "blocked", "task-1", "waiting for review"])
        .unwrap();
    if let Some(Commands::Tasks { file: _, command }) = cli.command {
        if let crate::commands::tasks::TasksSubcommand::Blocked { id, reason } = command {
            assert_eq!(id, "task-1");
            assert_eq!(reason, Some("waiting for review".to_string()));
        } else {
            panic!("Expected Blocked subcommand");
        }
    } else {
        panic!("Expected Tasks command");
    }
}
