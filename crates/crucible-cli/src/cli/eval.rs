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
    ///
    /// Pass --golden-dir instead of --golden to score a directory of class
    /// files: one row per class plus a TOTAL row.
    Precognition {
        /// Path to the golden-set TOML file
        #[arg(short = 'g', long, conflicts_with = "golden_dir")]
        golden: Option<PathBuf>,

        /// Directory of golden-set TOMLs; each file is scored as its own class
        #[arg(short = 'd', long)]
        golden_dir: Option<PathBuf>,

        /// Kiln to score against; defaults to the configured kiln_path
        #[arg(long)]
        kiln: Option<PathBuf>,
    },
}

impl EvalCommands {
    pub async fn execute(&self, config: crate::config::CliConfig) -> anyhow::Result<()> {
        match self {
            EvalCommands::Precognition {
                golden,
                golden_dir,
                kiln,
            } => {
                let mut config = config;
                if let Some(path) = kiln {
                    config.kiln_path = path.clone();
                }
                super::super::commands::eval::execute(config, golden.clone(), golden_dir.clone())
                    .await
            }
        }
    }
}
