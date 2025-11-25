use clap::{Parser, Subcommand};
use std::path::PathBuf;


#[derive(Parser)]
#[command(name = "cru")]
#[command(about = "cru - Crucible CLI - Interactive knowledge management with semantic search")]
#[command(version)]
#[command(arg_required_else_help = false)]
pub struct Cli {
    /// Subcommand to execute (defaults to chat if not provided)
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Config file path (defaults to ~/.config/crucible/config.toml)
    #[arg(short = 'C', long, global = true)]
    pub config: Option<PathBuf>,

    /// Embedding service URL (overrides config file)
    #[arg(long, global = true)]
    pub embedding_url: Option<String>,

    /// Embedding model name (overrides config file)
    #[arg(long, global = true)]
    pub embedding_model: Option<String>,

    /// Database path to use (overrides config)
    #[arg(long, global = true)]
    pub db_path: Option<String>,

    /// (Deprecated) Tool directory path - Rune removed from MVP
    #[arg(long, global = true)]
    pub tool_dir: Option<String>,

    /// Set output format (table, json, csv)
    #[arg(short = 'f', long, global = true, default_value = "table")]
    pub format: String,

  
    /// Skip file processing on startup (useful for quick commands with potentially stale data)
    #[arg(long = "no-process", global = true)]
    pub no_process: bool,

    /// File processing timeout in seconds (default: 300, 0 = no timeout)
    #[arg(long = "process-timeout", global = true, default_value = "300")]
    pub process_timeout: u64,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Natural language chat interface (toggleable plan/act modes)
    Chat {
        /// Optional one-shot query (if omitted, starts interactive mode)
        query: Option<String>,

        /// Preferred agent to use (claude-code, gemini-cli, codex)
        #[arg(short = 'a', long)]
        agent: Option<String>,

        /// Skip context enrichment (faster, but agent has no knowledge base access)
        #[arg(long)]
        no_context: bool,

        /// Number of context results to include (default: 5)
        #[arg(long, default_value = "5")]
        context_size: usize,

        /// Start in act mode (write-enabled) instead of plan mode (read-only)
        /// Can be toggled during session with /plan and /act commands
        #[arg(long)]
        act: bool,
    },

    /// Start MCP server exposing Crucible tools via stdio
    Mcp,

    /// Process files through the pipeline (parse, enrich, store)
    Process {
        /// Specific file or directory to process (if omitted, processes entire kiln)
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        /// Force reprocess all files (ignore change detection)
        #[arg(long)]
        force: bool,

        /// Watch for changes and reprocess automatically
        #[arg(short = 'w', long)]
        watch: bool,

        /// Preview what would be processed without making database changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Unified search through kiln notes (text and fuzzy search)
    Search {
        /// Search query (optional - opens interactive picker if omitted)
        query: Option<String>,

        /// Search mode: auto, fuzzy, text [default: auto]
        #[arg(long = "mode", default_value = "auto")]
        mode: String,

        /// Number of results to show
        #[arg(short = 'n', long, default_value = "20")]
        limit: u32,

        /// Output format (plain, json, table)
        #[arg(short = 'f', long, default_value = "plain")]
        format: String,

        /// Show content preview in results
        #[arg(short = 'c', long)]
        show_content: bool,
    },

    /// (Deprecated) Fuzzy search - use 'cru search' instead
    #[command(hide = true)] // Hide from help but keep for backwards compatibility
    Fuzzy {
        /// Search query (optional - starts with all results if omitted)
        query: Option<String>,

        /// Number of results
        #[arg(short = 'n', long, default_value = "20")]
        limit: u32,
    },

  
    
    /// Display kiln statistics
    Stats,

  
    /// Configuration management
    #[command(subcommand)]
    Config(ConfigCommands),

  
    /// Show storage status and statistics
    Status {
        /// Path to analyze (optional - shows global status if omitted)
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        /// Output format (table, json, plain)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,

        /// Show detailed block-level information
        #[arg(long)]
        detailed: bool,

        /// Include recent change activity
        #[arg(long)]
        recent: bool,
    },

    /// Storage management and operations
    #[command(subcommand)]
    Storage(StorageCommands),

    /// Parse and analyze files
    Parse {
        /// File or directory to parse
        #[arg(value_name = "PATH")]
        path: PathBuf,

        /// Output format (plain, json, detailed)
        #[arg(short = 'f', long, default_value = "plain")]
        format: String,

        /// Show Merkle tree information
        #[arg(short = 't', long)]
        show_tree: bool,

        /// Display content blocks and hashes
        #[arg(short = 'b', long)]
        show_blocks: bool,

        /// Maximum recursion depth for directories
        #[arg(short = 'd', long, default_value = "5")]
        max_depth: usize,

        /// Continue processing on errors
        #[arg(short = 'c', long)]
        continue_on_error: bool,
    },
}

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
    },

    /// Dump default configuration to stdout (useful for creating example config)
    Dump {
        /// Output format (toml, json)
        #[arg(short = 'f', long, default_value = "toml")]
        format: String,
    },
}

#[derive(Subcommand)]
pub enum StorageCommands {
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

