use clap::Subcommand;
use std::path::PathBuf;

/// Project registry subcommands.
///
/// A project is where work OUTPUT goes; a kiln is where knowledge goes. The two
/// registries are separate files with separate lifecycles, so they get separate
/// commands rather than one generic state tool — `cru project forget` reads
/// better than `cru state rm project/<name>`, and the domain noun is the point.
#[derive(Subcommand)]
pub enum ProjectCommands {
    /// List the registered projects
    List,

    /// Register a directory as a project
    Register {
        /// Directory to register (defaults to the working directory)
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
    },

    /// Remove a project registration
    #[command(
        long_about = "Remove one project registration from the daemon's registry.\n\nThe directory is not touched. Only the registration goes."
    )]
    Forget {
        /// Path of the project to forget
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },
}
