use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Initialize a new config file
    Init {
        /// Path for the config file (defaults to ~/.config/crucible/config.toml)
        #[arg(short, long)]
        path: Option<PathBuf>,

        /// Overwrite existing config file
        #[arg(short = 'F', long)]
        force: bool,
    },

    /// Show the current effective configuration
    Show {
        /// Output format (toml, json)
        #[arg(short = 'f', long, default_value = "toml")]
        format: String,

        /// Show where each value came from (file, env, cli, default)
        #[arg(long, visible_alias = "trace")]
        sources: bool,
    },

    /// Dump default configuration to stdout (useful for creating example config)
    Dump {
        /// Output format (toml, json)
        #[arg(short = 'f', long, default_value = "toml")]
        format: String,
    },
}
