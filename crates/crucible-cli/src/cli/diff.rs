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

    /// Print the open comments of a diffset, one quickfix entry for each
    #[command(
        long_about = "Print the open comments of a diffset.\n\nThe quickfix form gives one entry for each comment: `path:start: [start-end] text`. Vim reads it with its default errorformat. The path is relative to the root of the comment, so run Vim in that root.\n\nThe diffset is `session-<id>` for the record of a session, or `branch` for the branch diff that --root, --base and --head name. A `branch-<hex>` id also works when those flags give the same id.\n\nExamples:\n  # Open each comment of a session record in Vim\n  vim -q <(cru diff comments session-<id>)\n\n  # The comments of this branch diff, as JSON\n  cru diff comments branch --format json"
    )]
    Comments {
        /// The diffset: `session-<id>`, `proposal-<uuid>`, `branch` or `branch-<hex>`
        #[arg(value_name = "DIFFSET")]
        diffset: String,

        /// The base branch of a branch diffset. The default branch when omitted
        #[arg(long, value_name = "REF")]
        base: Option<String>,

        /// The new side of a branch diffset. The working tree when omitted
        #[arg(long, value_name = "REF")]
        head: Option<String>,

        /// The repository of a branch diffset. The git top level above the working directory when omitted
        #[arg(long, value_name = "PATH")]
        root: Option<PathBuf>,

        /// Output format
        #[arg(short = 'f', long, value_enum, default_value = "quickfix")]
        format: CommentFormat,
    },
}

/// The output format of `cru diff comments`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CommentFormat {
    /// One `path:line: [start-end] text` entry for each comment, for `vim -q`.
    Quickfix,
    /// The listed comments as JSON, with the `outdated` flag.
    Json,
}
