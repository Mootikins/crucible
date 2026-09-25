use clap::Subcommand;

/// Agent card subcommands. `cru agents` alone lists the cards.
#[derive(Subcommand)]
pub enum AgentsCommands {
    /// Validate all agent cards in configured directories
    Validate {
        /// Show detailed output for each file
        #[arg(long)]
        verbose: bool,
    },
}
