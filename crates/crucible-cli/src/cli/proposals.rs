use clap::Subcommand;

/// Reflection-pass proposal review subcommands.
///
/// The reflection pass stages proposed notes in `KILN/.crucible/proposals/`,
/// outside the indexed kiln. These commands let a human review, accept, or
/// reject them. Accepting moves a proposal into the kiln (where the daemon's
/// file watcher then indexes it); rejecting deletes it.
#[derive(Subcommand)]
pub enum ProposalsCommands {
    /// List pending proposals
    List {
        /// Output format (table, json, plain)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,
    },
    /// Show a proposal's full content
    Show {
        /// Proposal id (the file name without extension)
        id: String,
    },
    /// Accept a proposal: move it into the kiln and drop provenance frontmatter
    Accept {
        /// Proposal id (the file name without extension)
        id: String,
    },
    /// Reject a proposal: delete it from the staging area
    Reject {
        /// Proposal id (the file name without extension)
        id: String,
    },
}
