use crate::formatting::OutputFormat;
use clap::Subcommand;

/// Model subcommands.
///
/// `cru models` with no subcommand keeps its old behaviour — the chat models
/// the configured provider offers — because scripts already call it that way.
#[derive(Subcommand)]
pub enum ModelsCommands {
    /// Local embedding models: the catalog, what is on disk, what is configured
    Embeddings {
        /// Output format. Defaults to a table on a terminal, plain lines when
        /// piped or redirected.
        #[arg(short = 'f', long)]
        format: Option<OutputFormat>,

        #[command(subcommand)]
        command: Option<EmbeddingsCommands>,
    },
}

/// Embedding model subcommands.
#[derive(Subcommand)]
pub enum EmbeddingsCommands {
    /// Fetch a model into the daemon's model cache
    Download {
        /// The catalog name of the model
        name: String,
    },

    /// Write the model into the config file, for the next reprocess
    #[command(name = "use")]
    Use {
        /// The catalog name of the model
        name: String,
    },
}
