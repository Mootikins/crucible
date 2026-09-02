use crate::formatting::OutputFormat;
use clap::Subcommand;

/// Reflection-pass proposal review subcommands.
///
/// The reflection pass stages proposed notes in `KILN/.crucible/proposals/`,
/// outside the indexed kiln. These commands let a human review, accept, or
/// reject them. Accepting moves a proposal into the kiln (where the daemon's
/// file watcher then indexes it); rejecting moves it into the `rejected/`
/// directory, so the reviewer does not repeat it.
#[derive(Subcommand)]
pub enum ProposalsCommands {
    /// List pending proposals
    List {
        /// Output format. Defaults to a table on a terminal, plain lines when
        /// piped or redirected.
        #[arg(short = 'f', long)]
        format: Option<OutputFormat>,
    },
    /// Show a proposal's full content
    Show {
        /// Proposal id (the file name without extension)
        id: String,
    },
    /// Accept a proposal: a note moves into the kiln, an update replaces its target, a skill lands under .crucible/skills
    Accept {
        /// Proposal id (the file name without extension)
        id: String,
    },
    /// Reject a proposal: move it to the rejected directory so the reviewer does not repeat it
    Reject {
        /// Proposal id (the file name without extension)
        id: String,
    },
}
