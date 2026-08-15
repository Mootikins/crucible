use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum StorageCommands {
    /// Show current storage mode and quick status
    Mode,

    /// Show detailed storage statistics
    Stats,

    /// Verify content integrity
    Verify {
        /// Path to verify (optional - verifies all storage if omitted)
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
    },

    /// Perform maintenance operations
    Cleanup,

    /// Export or backup storage data
    Backup {
        /// Backup destination path
        #[arg(value_name = "DEST")]
        dest: PathBuf,
    },

    /// Import or restore storage data
    Restore {
        /// Backup source path
        #[arg(value_name = "SOURCE")]
        source: PathBuf,
    },
}
