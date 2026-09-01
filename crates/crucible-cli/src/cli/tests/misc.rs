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

/// `cru proposals reject` keeps the file in `rejected/`. The long help must
/// say so, because it is the first text a user reads before the command runs.
#[test]
fn proposals_help_says_reject_keeps_the_file() {
    use clap::CommandFactory;

    let cli = Cli::command();
    let proposals = cli
        .find_subcommand("proposals")
        .expect("the proposals subcommand exists");
    let long_about = proposals
        .get_long_about()
        .expect("proposals has a long_about")
        .to_string();

    assert!(
        !long_about.to_lowercase().contains("delete"),
        "reject no longer deletes; help said: {long_about}"
    );
    assert!(
        long_about.contains("rejected/"),
        "help must name the rejected/ directory; help said: {long_about}"
    );
}
