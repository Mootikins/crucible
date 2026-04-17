use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum StorageCommands {
    /// Show current storage mode and quick status
    Mode,

    /// Show detailed storage statistics
    Stats {
        /// Output format (table, json, plain)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,

        /// Show per-backend breakdown
        #[arg(long)]
        by_backend: bool,

        /// Include deduplication statistics
        #[arg(long)]
        deduplication: bool,
    },

    /// Verify content integrity
    Verify {
        /// Path to verify (optional - verifies all storage if omitted)
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        /// Repair any inconsistencies found
        #[arg(long)]
        repair: bool,

        /// Output format (plain, json)
        #[arg(short = 'f', long, default_value = "plain")]
        format: String,
    },

    /// Perform maintenance operations
    Cleanup {
        /// Run garbage collection
        #[arg(long)]
        gc: bool,

        /// Rebuild indexes
        #[arg(long)]
        rebuild_indexes: bool,

        /// Optimize storage layout
        #[arg(long)]
        optimize: bool,

        /// Force cleanup even if system is busy
        #[arg(long)]
        force: bool,

        /// Dry run - show what would be done
        #[arg(long)]
        dry_run: bool,
    },

    /// Export or backup storage data
    Backup {
        /// Backup destination path
        #[arg(value_name = "DEST")]
        dest: PathBuf,

        /// Include content blocks
        #[arg(long)]
        include_content: bool,

        /// Compress backup
        #[arg(long)]
        compress: bool,

        /// Verify backup after creation
        #[arg(long)]
        verify: bool,

        /// Export format (json, binary)
        #[arg(short = 'f', long, default_value = "json")]
        format: String,
    },

    /// Import or restore storage data
    Restore {
        /// Backup source path
        #[arg(value_name = "SOURCE")]
        source: PathBuf,

        /// Merge with existing data
        #[arg(long)]
        merge: bool,

        /// Skip verification during import
        #[arg(long)]
        skip_verify: bool,

        /// Import format (json, binary)
        #[arg(short = 'f', long, default_value = "json")]
        format: String,
    },
}
