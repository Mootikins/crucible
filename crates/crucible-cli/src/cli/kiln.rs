use clap::Subcommand;
use std::path::PathBuf;

/// Kiln registry subcommands.
///
/// Only `register` for now, and it exists because two daemon refusals name it
/// as the remedy: `session.create` telling a caller that kilns are addressed by
/// the name of a `[kilns]` entry, and the registry telling a user that every
/// disambiguation of a derived name is taken. An error that names a command
/// which does not exist is worse than one that names nothing.
#[derive(Subcommand)]
pub enum KilnCommands {
    /// Give a directory a name, so sessions can attach it by that name
    #[command(
        long_about = "Register a directory as a kiln under a name you choose.\n\nEverything else in Crucible addresses a kiln by this name — `session.create`, the session's stored metadata, the agent's prompt — so registering is what makes a directory referable without its path travelling with it.\n\nThe name must be lower-case `[a-z0-9._-]`, at most 64 characters, and must not start with a dot. Registering the same name and path again is a no-op; pointing an existing name at a different directory is refused, because sessions that already stored that name would silently open a different corpus.\n\nExamples:\n  # Name a directory\n  cru kiln register notes ~/vault/notes\n\n  # Names are case-folded, so this is refused rather than becoming a second kiln\n  cru kiln register Notes ~/vault/notes"
    )]
    Register {
        /// Name to register the kiln under (`[a-z0-9._-]`, max 64 chars)
        #[arg(value_name = "NAME")]
        name: String,

        /// Directory to register
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },
}
