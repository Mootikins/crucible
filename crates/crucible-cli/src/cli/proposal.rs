use crate::formatting::TextFormat;
use clap::Subcommand;
use crucible_core::proposal::ProposalId;
use std::path::PathBuf;

/// Proposal subcommands.
///
/// The daemon owns each proposal. These commands read a proposal or send the
/// decision of the user.
#[derive(Subcommand)]
pub enum ProposalCommands {
    /// List the proposals in the Inbox
    #[command(
        long_about = "List the proposals in the Inbox: each open, stale, conflicted or superseded proposal.\n\nExamples:\n  # The proposals that wait for a decision\n  cru proposal list\n\n  # Every stored proposal, with the decided ones\n  cru proposal list --all\n\n  # As JSON\n  cru proposal list -f json"
    )]
    List {
        /// Also list the accepted, rejected and dismissed proposals
        #[arg(long)]
        all: bool,

        /// Output format
        #[arg(short = 'f', long, default_value_t)]
        format: TextFormat,
    },

    /// Show a proposal and the diff of each file
    #[command(
        long_about = "Show a proposal: its title, author, state and the diff of each file.\n\nFor a conflicted proposal, the command prints each conflicted file with the markers `<<<<<<< proposal`, `=======` and `>>>>>>> disk`. Edit that text, then give it to `cru proposal resolve`.\n\nExamples:\n  cru proposal show <id>\n\n  # Keep the text of a conflicted file, to resolve it\n  cru proposal show <id> --conflict notes/a.md > a.md"
    )]
    Show {
        /// The proposal id (a UUID)
        id: ProposalId,

        /// Print only the text with conflict markers of this conflicted file
        #[arg(long, value_name = "PATH")]
        conflict: Option<String>,

        /// Kiln root for a path that occurs in several kilns
        #[arg(long, requires = "conflict")]
        root: Option<PathBuf>,

        /// Output format
        #[arg(short = 'f', long, default_value_t)]
        format: TextFormat,
    },

    /// Write every file of a proposal
    #[command(
        long_about = "Accept a proposal: the daemon writes every file of it.\n\nWhen a file changed on disk, the daemon merges. When a merge has a conflict, the daemon writes no file and the proposal becomes conflicted. Then run `cru proposal show <id>` and `cru proposal resolve`."
    )]
    Accept {
        /// The proposal id (a UUID)
        id: ProposalId,
    },

    /// Reject a proposal. The files do not change
    Reject {
        /// The proposal id (a UUID)
        id: ProposalId,

        /// Why the proposal is wrong. The reviewers read it before they propose again
        #[arg(long)]
        reason: Option<String>,
    },

    /// Take a proposal out of the Inbox with no decision
    Dismiss {
        /// The proposal id (a UUID)
        id: ProposalId,
    },

    /// Give the settled text of one conflicted file
    #[command(
        long_about = "Resolve one conflicted file of a proposal with a text that you settled.\n\nThe daemon writes the files of the proposal only when every conflicted file has a settled text. The command refuses a text that still holds a `<<<<<<< proposal` or `>>>>>>> disk` marker line.\n\nExamples:\n  cru proposal show <id> --conflict notes/a.md > a.md\n  $EDITOR a.md\n  cru proposal resolve <id> notes/a.md --from a.md\n\n  # Read the text from stdin\n  cru proposal resolve <id> notes/a.md --from - < a.md"
    )]
    Resolve {
        /// The proposal id (a UUID)
        id: ProposalId,

        /// The path of the file, relative to its kiln root, as the proposal names it
        path: String,

        /// Kiln root for a path that occurs in several kilns
        #[arg(long)]
        root: Option<PathBuf>,

        /// The file that holds the settled text. `-` reads stdin
        #[arg(long, value_name = "FILE")]
        from: PathBuf,
    },
}
