mod agents;
mod chat;
mod init;
mod misc;
mod session;
mod tasks;

use crate::cli::{Cli, Commands};
use clap::Parser;

/// Parse CLI args and unwrap straight to the top-level `Commands` the
/// parser produced. Panics (failing the test) on a parse error or on a
/// missing subcommand, mirroring the old `Cli::try_parse_from(...).unwrap()`
/// + `.command` access every arg-parsing test used to repeat inline.
pub(super) fn parse(args: &[&str]) -> Commands {
    Cli::try_parse_from(args)
        .unwrap_or_else(|e| panic!("failed to parse {args:?}: {e}"))
        .command
        .unwrap_or_else(|| panic!("expected a command from {args:?}"))
}
