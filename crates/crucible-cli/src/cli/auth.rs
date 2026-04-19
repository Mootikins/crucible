use clap::Subcommand;

#[derive(Subcommand)]
pub enum AuthCommands {
    /// Store an API key for a provider
    Login {
        /// Provider name (openai, anthropic, etc.)
        #[arg(short, long)]
        provider: Option<String>,

        /// API key value
        #[arg(short, long)]
        key: Option<String>,
    },

    /// Remove a stored credential
    Logout {
        /// Provider name to remove
        #[arg(short, long)]
        provider: Option<String>,
    },

    /// Show all configured credentials and their sources
    List,

    /// Authenticate with GitHub Copilot using OAuth device flow
    #[command(
        long_about = "Authenticate with GitHub Copilot using OAuth device flow.\n\nThis command starts the OAuth device flow and stores the long-lived OAuth token for use with GitHub Copilot.\n\nExamples:\n  # Authenticate with GitHub Copilot\n  cru auth copilot\n\n  # Force re-authentication\n  cru auth copilot --force"
    )]
    Copilot {
        /// Force re-authentication even if token exists
        #[arg(long)]
        force: bool,
    },
}
