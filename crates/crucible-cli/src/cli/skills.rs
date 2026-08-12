use crate::formatting::OutputFormat;
use clap::Subcommand;

/// Skills management subcommands
#[derive(Subcommand)]
pub enum SkillsCommands {
    /// List discovered skills
    List {
        /// Filter by scope (personal, workspace, kiln)
        #[arg(long)]
        scope: Option<String>,
        /// Output format. Defaults to a table on a terminal, plain lines when
        /// piped or redirected.
        #[arg(short = 'f', long)]
        format: Option<OutputFormat>,
    },
    /// Show skill details
    Show {
        /// Skill name
        name: String,
    },
    /// Search skills by query
    Search {
        /// Search query
        query: String,
        /// Maximum results
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
    },
}
