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

    /// Tool directory path for Rune scripts
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

    /// Run a Rune script as a command
    Run {
        /// Path to .rn script
        script: String,

        /// Arguments to pass to the script (as JSON object)
        #[arg(short, long)]
        args: Option<String>,
    },

    /// List available Rune commands
    Commands,

    /// Configuration management
    #[command(subcommand)]
    Config(ConfigCommands),

    /// Service management and monitoring
    #[command(subcommand)]
    Service(ServiceCommands),

    /// Kiln processing management
    #[command(subcommand)]
    Process(ProcessCommands),

    /// Migration management for tool migration
    #[command(subcommand)]
    Migration(MigrationCommands),

    /// Interactive chat mode with AI agents
    Chat {
        /// Agent name to use for conversation
        #[arg(short, long, default_value = "default")]
        agent: String,

        /// Model to use for chat (overrides config)
        #[arg(short = 'm', long)]
        model: Option<String>,

        /// Temperature for chat responses (0.0-2.0)
        #[arg(short = 't', long)]
        temperature: Option<f32>,

        /// Maximum tokens in responses
        #[arg(long)]
        max_tokens: Option<u32>,

        /// Disable streaming responses
        #[arg(long)]
        no_stream: bool,

        /// Start with a specific message
        #[arg(short = 's', long)]
        start_message: Option<String>,

        /// Load conversation history from file
        #[arg(long)]
        history: Option<PathBuf>,
    },
    // /// Enhanced chat mode with intelligent agent management // Temporarily disabled
    // EnhancedChat {
    //     /// Agent name to use for conversation
    //     #[arg(short, long, default_value = "default")]
    //     agent: String,

    //     /// Model to use for chat (overrides config)
    //     #[arg(short = 'm', long)]
    //     model: Option<String>,

    //     /// Temperature for chat responses (0.0-2.0)
    //     #[arg(short = 't', long)]
    //     temperature: Option<f32>,

    //     /// Maximum tokens in responses
    //     #[arg(long)]
    //     max_tokens: Option<u32>,

    //     /// Enable performance tracking and learning
    //     #[arg(long)]
    //     performance_tracking: bool,

    //     /// Start with a specific message
    //     #[arg(short = 's', long)]
    //     start_message: Option<String>,

    //     /// Load conversation history from file
    //     #[arg(long)]
    //     history: Option<PathBuf>,
    // },

    // /// Agent management commands // Temporarily disabled
    // #[command(subcommand)]
    // Agent(AgentCommands),
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

    /// Migrate environment variable configuration to config file
    MigrateEnvVars {
        /// Path for the output config file (defaults to ~/.config/crucible/config.toml)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Show what would be migrated without writing the file
        #[arg(short = 'n', long)]
        dry_run: bool,
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

#[derive(Subcommand)]
pub enum AgentCommands {
    /// List all available agents with performance metrics
    List {
        /// Output format (plain, json, table)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,

        /// Show detailed information
        #[arg(long)]
        detailed: bool,
    },

    /// Show agent rankings by performance
    Rankings {
        /// Number of top agents to show
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,

        /// Sort by specific metric (success, satisfaction, specialization)
        #[arg(short = 's', long, default_value = "success")]
        sort_by: String,
    },

    /// Show performance insights for an agent
    Performance {
        /// Agent name
        agent_name: String,

        /// Show learning insights
        #[arg(long)]
        insights: bool,
    },

    /// Suggest agents for a task
    Suggest {
        /// Task description
        task: String,

        /// Required capabilities
        #[arg(short = 'c', long)]
        capabilities: Vec<String>,

        /// Number of suggestions
        #[arg(short = 'n', long, default_value = "3")]
        limit: usize,
    },

    /// Show collaboration statistics
    CollabStats {
        /// Show active sessions
        #[arg(long)]
        active: bool,

        /// Show detailed breakdown
        #[arg(short = 'd', long)]
        detailed: bool,
    },

    /// List available workflow templates
    Workflows {
        /// Show workflow details
        #[arg(short = 'd', long)]
        detailed: bool,
    },
}

/// Service management commands
#[derive(Subcommand, Debug)]
pub enum ServiceCommands {
    /// Show service health status
    Health {
        /// Service name (optional - shows all services if not specified)
        service: Option<String>,

        /// Output format (table, json)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,

        /// Show detailed health information
        #[arg(short = 'd', long)]
        detailed: bool,
    },

    /// Show service metrics
    Metrics {
        /// Service name (optional - shows all services if not specified)
        service: Option<String>,

        /// Output format (table, json)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,

        /// Show real-time metrics
        #[arg(short = 'r', long)]
        real_time: bool,
    },

    /// Start a service
    Start {
        /// Service name
        service: String,

        /// Wait for service to be ready
        #[arg(short, long)]
        wait: bool,
    },

    /// Stop a service
    Stop {
        /// Service name
        service: String,

        /// Force stop (graceful shutdown if false)
        #[arg(short, long)]
        force: bool,
    },

    /// Restart a service
    Restart {
        /// Service name
        service: String,

        /// Wait for service to be ready
        #[arg(short, long)]
        wait: bool,
    },

    /// List all services
    List {
        /// Output format (table, json)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,

        /// Show service status
        #[arg(short = 's', long)]
        status: bool,

        /// Show detailed information
        #[arg(short = 'd', long)]
        detailed: bool,
    },

    /// Show service logs
    Logs {
        /// Service name
        service: Option<String>,

        /// Number of lines to show
        #[arg(short = 'n', long, default_value = "100")]
        lines: usize,

        /// Follow log output
        #[arg(short = 'f', long)]
        follow: bool,

        /// Show only errors
        #[arg(long)]
        errors: bool,
    },
}

/// Migration management commands
#[derive(Subcommand, Debug)]
pub enum MigrationCommands {
    /// Start migration of Rune tools to ScriptEngine service
    Migrate {
        /// Tool name to migrate (migrates all if not specified)
        tool: Option<String>,

        /// Force migration even if tool already exists
        #[arg(short, long)]
        force: bool,

        /// Security level for migrated tools
        #[arg(long, default_value = "safe")]
        security_level: String,

        /// Dry run - show what would be migrated without doing it
        #[arg(long)]
        dry_run: bool,
    },

    /// Show migration status and statistics
    Status {
        /// Output format (table, json)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,

        /// Show detailed migration information
        #[arg(short = 'd', long)]
        detailed: bool,

        /// Validate migration integrity
        #[arg(long)]
        validate: bool,
    },

    /// Rollback migrated tools
    Rollback {
        /// Tool name to rollback (rollbacks all if not specified)
        tool: Option<String>,

        /// Confirm rollback without prompt
        #[arg(short, long)]
        confirm: bool,

        /// Keep backup of migrated tools
        #[arg(long)]
        backup: bool,
    },

    /// List migrated tools
    List {
        /// Output format (table, json)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,

        /// Show only active tools
        #[arg(long)]
        active: bool,

        /// Show only inactive tools
        #[arg(long)]
        inactive: bool,

        /// Show migration metadata
        #[arg(short = 'm', long)]
        metadata: bool,
    },

    /// Validate migration integrity
    Validate {
        /// Tool name to validate (validates all if not specified)
        tool: Option<String>,

        /// Fix issues automatically if possible
        #[arg(long)]
        auto_fix: bool,

        /// Output format (table, json)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,
    },

    /// Reload a migrated tool from its original source
    Reload {
        /// Tool name to reload
        tool: String,

        /// Force reload even if source unchanged
        #[arg(short, long)]
        force: bool,
    },

    /// Clean up migration artifacts
    Cleanup {
        /// Remove inactive migrations
        #[arg(long)]
        inactive: bool,

        /// Remove failed migrations
        #[arg(long)]
        failed: bool,

        /// Confirm cleanup without prompt
        #[arg(short, long)]
        confirm: bool,
    },
}
