use clap::{Parser, Subcommand};
use std::path::PathBuf;

// Import process command types
use crate::commands::process::ProcessCommands;

#[derive(Parser)]
#[command(name = "cru")]
#[command(about = "cru - Crucible CLI - Interactive knowledge management with semantic search")]
#[command(version)]
#[command(arg_required_else_help = false)]
pub struct Cli {
    /// Subcommand to execute (defaults to REPL if not provided)
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

    /// Run REPL in non-interactive mode (reads from stdin, useful for testing/scripting)
    #[arg(long, global = true)]
    pub non_interactive: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Interactive search through kiln notes (fuzzy finder)
    Search {
        /// Search query (optional - opens picker if omitted)
        query: Option<String>,

        /// Number of results to show
        #[arg(short = 'n', long, default_value = "10")]
        limit: u32,

        /// Output format (plain, json, table)
        #[arg(short = 'f', long, default_value = "plain")]
        format: String,

        /// Show content preview in results
        #[arg(short = 'c', long)]
        show_content: bool,
    },

    /// Fuzzy search across all metadata (tags, properties, content)
    Fuzzy {
        /// Search query (optional - starts with all results if omitted)
        query: Option<String>,

        /// Search in content
        #[arg(long, default_value = "true")]
        content: bool,

        /// Search in tags
        #[arg(long, default_value = "true")]
        tags: bool,

        /// Search in file paths
        #[arg(long, default_value = "true")]
        paths: bool,

        /// Number of results
        #[arg(short = 'n', long, default_value = "20")]
        limit: u32,
    },

    /// Semantic search using embeddings across kiln content
    Semantic {
        /// Search query
        query: String,

        /// Number of results
        #[arg(short = 'n', long, default_value = "10")]
        top_k: u32,

        /// Output format (plain, json, table)
        #[arg(short = 'f', long, default_value = "plain")]
        format: String,

        /// Show similarity scores
        #[arg(short = 's', long)]
        show_scores: bool,
    },

    /// Note operations
    #[command(subcommand)]
    Note(NoteCommands),

    /// Display kiln statistics
    Stats,

    /// Test tool loading and execution
    Test,

    /// Configuration management
    #[command(subcommand)]
    Config(ConfigCommands),

    /// Kiln processing management
    #[command(subcommand)]
    Process(ProcessCommands),
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
}

#[derive(Subcommand)]
pub enum NoteCommands {
    /// Get a note by path or ID
    Get {
        /// File path or ID
        path: String,

        /// Output format (plain, json)
        #[arg(short = 'f', long, default_value = "plain")]
        format: String,
    },

    /// Create a new note
    Create {
        /// Path for the new note
        path: String,

        /// Note content
        #[arg(short, long)]
        content: Option<String>,

        /// Open in $EDITOR after creation
        #[arg(short, long)]
        edit: bool,
    },

    /// Update note properties
    Update {
        /// File path
        path: String,

        /// Properties as JSON object
        #[arg(short = 'p', long)]
        properties: String,
    },

    /// List all notes
    List {
        /// Output format (plain, json, table)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,
    },
}
