use clap::Subcommand;

/// Agent card management subcommands
#[derive(Subcommand)]
pub enum AgentsCommands {
    /// List all registered agent cards (default when no subcommand given)
    #[command(name = "list")]
    List {
        /// Filter by tag
        #[arg(short = 't', long)]
        tag: Option<String>,

        /// Output format (table, json)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,
    },

    /// Show details of a specific agent card
    Show {
        /// Name of the agent card to show
        name: String,

        /// Output format (table, json)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,

        /// Show full system prompt (not truncated)
        #[arg(long)]
        full: bool,
    },

    /// Validate all agent cards in configured directories
    Validate {
        /// Show detailed output for each file
        #[arg(long)]
        verbose: bool,
    },
}
