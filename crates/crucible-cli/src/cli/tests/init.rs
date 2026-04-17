use crate::cli::*;
use clap::Parser;

#[test]
fn test_init_parses() {
    let cli = Cli::try_parse_from(["cru", "init"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Init {
            path: None,
            force: false,
            yes: false,
        })
    ));
}

#[test]
fn test_init_with_path_parses() {
    let cli = Cli::try_parse_from(["cru", "init", "--path", "/tmp/test"]).unwrap();
    if let Some(Commands::Init { path, force, yes }) = cli.command {
        assert_eq!(path, Some(std::path::PathBuf::from("/tmp/test")));
        assert!(!force);
        assert!(!yes);
    } else {
        panic!("Expected Init command");
    }
}

#[test]
fn test_init_with_force_parses() {
    let cli = Cli::try_parse_from(["cru", "init", "--force"]).unwrap();
    if let Some(Commands::Init { path, force, yes }) = cli.command {
        assert_eq!(path, None);
        assert!(force);
        assert!(!yes);
    } else {
        panic!("Expected Init command");
    }
}

#[test]
fn test_init_with_short_flags_parses() {
    let cli = Cli::try_parse_from(["cru", "init", "-p", "/tmp/test", "-F"]).unwrap();
    if let Some(Commands::Init { path, force, yes }) = cli.command {
        assert_eq!(path, Some(std::path::PathBuf::from("/tmp/test")));
        assert!(force);
        assert!(!yes);
    } else {
        panic!("Expected Init command");
    }
}

#[test]
fn test_init_with_yes_flag_parses() {
    let cli = Cli::try_parse_from(["cru", "init", "--yes"]).unwrap();
    if let Some(Commands::Init { path, force, yes }) = cli.command {
        assert_eq!(path, None);
        assert!(!force);
        assert!(yes);
    } else {
        panic!("Expected Init command");
    }
}

#[test]
fn test_init_with_yes_short_flag_parses() {
    let cli = Cli::try_parse_from(["cru", "init", "-y"]).unwrap();
    if let Some(Commands::Init { path, force, yes }) = cli.command {
        assert_eq!(path, None);
        assert!(!force);
        assert!(yes);
    } else {
        panic!("Expected Init command");
    }
}
