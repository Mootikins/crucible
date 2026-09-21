use crate::formatting::TextFormat;
use clap::Subcommand;
use std::path::PathBuf;

/// Diffset subcommands.
///
/// The daemon computes each diffset. These commands ask for one and print it.
#[derive(Subcommand)]
pub enum DiffCommands {
    /// Show the changes of this branch since its merge base with the default branch
    #[command(
        long_about = "Show the changes of a branch since its merge base with a base branch.\n\nThe daemon finds the merge base of HEAD with the base branch and lists each added, modified, deleted and renamed file. Without --head, the new side is the working tree.\n\nThe root must be a registered project, the workspace of a session or a path inside a registered kiln. It must also be the top level of its git repository. Register a repository with `cru project register`.\n\nExamples:\n  # The working tree against the default branch\n  cru diff branch\n\n  # Against a named base\n  cru diff branch --base develop\n\n  # One commit against the base, with no working-tree changes\n  cru diff branch --head HEAD\n\n  # The file list and the counts only\n  cru diff branch --stat"
    )]
    Branch {
        /// The base branch. The default branch of the repository when omitted
        #[arg(long, value_name = "REF")]
        base: Option<String>,

        /// The new side. The working tree when omitted
        #[arg(long, value_name = "REF")]
        head: Option<String>,

        /// The repository. The git top level above the working directory when omitted
        #[arg(long, value_name = "PATH")]
        root: Option<PathBuf>,

        /// Show only the file list and the counts
        #[arg(long)]
        stat: bool,

        /// Output format
        #[arg(short = 'f', long, default_value_t)]
        format: TextFormat,
    },
}
