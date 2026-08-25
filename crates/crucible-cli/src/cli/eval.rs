//! `cru eval` — measurement commands.

use clap::Subcommand;
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum EvalCommands {
    /// Score precognition retrieval against a golden query set.
    ///
    /// Runs each question through the daemon's embed + vector-search path
    /// (the same one live injection uses) and reports hit@1, hit@k, MRR and
    /// recall@k. See assets/fixtures/precognition_eval for the fixture format.
    Precognition {
        /// Path to the golden-set TOML file
        #[arg(short = 'g', long)]
        golden: PathBuf,
    },
}

impl EvalCommands {
    pub async fn execute(&self, config: crate::config::CliConfig) -> anyhow::Result<()> {
        match self {
            EvalCommands::Precognition { golden } => {
                super::super::commands::eval::execute(config, golden.clone()).await
            }
        }
    }
}
