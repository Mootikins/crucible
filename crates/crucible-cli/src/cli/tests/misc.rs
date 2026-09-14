use crate::cli::*;
use clap::Parser;

#[test]
fn test_storage_mode_parses() {
    let cli = Cli::try_parse_from(["cru", "storage", "mode"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Commands::Storage(StorageCommands::Mode))
    ));
}
