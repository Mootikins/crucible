use clap::Subcommand;
use std::path::PathBuf;

/// Kiln registry subcommands.
///
/// `register` exists because two daemon refusals name it as the remedy:
/// `session.create` telling a caller that kilns are addressed by the name of a
/// registry entry, and the registry telling a user that every disambiguation of
/// a derived name is taken. An error that names a command which does not exist
/// is worse than one that names nothing.
///
/// `list` and `forget` exist because Crucible holds kiln names in two layers —
/// the config the user wrote, and the state the daemon was told — and a split
/// ownership model is only tolerable while the user can see which side owns an
/// entry. `list` is that view. `forget` is the only removal, because a config
/// edit never deletes a registration: absence is not intent in a language with
/// conditionals.
#[derive(Subcommand)]
pub enum KilnCommands {
    /// Give a directory a name, so sessions can attach it by that name
    #[command(
        long_about = "Register a directory as a kiln under a name you choose.\n\nEverything else in Crucible addresses a kiln by this name — `session.create`, the session's stored metadata, the agent's prompt — so registering is what makes a directory referable without its path travelling with it.\n\nThe name holds `[A-Za-z0-9._- ]`, at most 64 characters, must not start with a dot, and must not be padded with spaces. It keeps the case and the spaces you write; two names that differ only in case are one kiln. Registering the same name and path again is a no-op; pointing an existing name at a different directory is refused, because sessions that already stored that name would silently open a different corpus.\n\nExamples:\n  # Name a directory\n  cru kiln register notes ~/vault/notes\n\n  # A name with a space and capitals, quoted for the shell\n  cru kiln register \"Crucible Help\" ~/crucible/docs\n\n  # Case does not make a second kiln, so this is refused as a duplicate\n  cru kiln register Notes ~/vault/notes\n\n  # Name it and make it the kiln every command uses by default\n  cru kiln register --default \"Crucible Help\" ~/crucible/docs"
    )]
    Register {
        /// Name to register the kiln under (`[A-Za-z0-9._- ]`, max 64 chars)
        #[arg(value_name = "NAME")]
        name: String,

        /// Directory to register
        #[arg(value_name = "PATH")]
        path: PathBuf,

        /// Make this the kiln used when none is named
        #[arg(long = "default")]
        make_default: bool,
    },

    /// Show every kiln name Crucible knows, and which layer owns it
    #[command(
        long_about = "List every kiln name Crucible knows.\n\nThe `origin` column says which layer owns the name:\n\n  config      declared in your config file\n  registered  written by `cru kiln register` into the daemon's state file\n  discovered  a directory this daemon opened and named for itself; the name works until the daemon restarts, and attaching it writes it down\n\nThe config layer out-ranks the state layer. An entry marked `shadows` is a name both layers claim for different directories: the config wins, and the state entry does nothing until you run `cru kiln forget`.\n\n`*` marks the default kiln. `(missing)` marks a registration whose directory is gone."
    )]
    List,

    /// Remove a kiln registration from the daemon's state
    #[command(
        long_about = "Remove one kiln registration.\n\nDeleting a kiln from your config does NOT remove a registration — the daemon cannot tell a deleted line from a branch that did not run, so absence never deletes. This command is the removal.\n\nA name your config declares is refused: there is nothing in the state file to forget, and the fix is to edit the config. A name BOTH layers claim is forgotten, which clears the conflict.\n\nThe removal takes effect at the next daemon start."
    )]
    Forget {
        /// Name of the kiln to forget
        #[arg(value_name = "NAME")]
        name: String,
    },
}
