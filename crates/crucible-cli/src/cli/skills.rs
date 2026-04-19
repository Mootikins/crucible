use clap::Subcommand;

/// Skills management subcommands
#[derive(Subcommand)]
pub enum SkillsCommands {
    /// List discovered skills
    List {
        /// Filter by scope (personal, workspace, kiln)
        #[arg(long)]
        scope: Option<String>,
        /// Output format (table, json, plain)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,
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
