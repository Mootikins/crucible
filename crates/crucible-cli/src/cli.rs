use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "crucible")]
#[command(about = "Crucible CLI - Knowledge management with semantic search")]
#[command(version)]
#[command(arg_required_else_help = true)]
pub struct Cli {
    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Config file path (defaults to ~/.config/crucible/config.toml)
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    /// Vault path (overrides config file)
    #[arg(short = 'p', long, global = true, env = "CRUCIBLE_VAULT_PATH")]
    pub vault_path: Option<String>,

    /// Embedding service URL (overrides config file)
    #[arg(long, global = true)]
    pub embedding_url: Option<String>,

    /// Embedding model name (overrides config file)
    #[arg(long, global = true)]
    pub embedding_model: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Interactive search through notes (fuzzy finder)
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

    /// Semantic search using embeddings
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

    /// Index vault for search and embeddings
    Index {
        /// Path to vault (defaults to current directory or --vault-path)
        path: Option<String>,

        /// Force re-indexing of all files
        #[arg(short, long)]
        force: bool,

        /// File pattern to match (e.g., "**/*.md")
        #[arg(short = 'g', long, default_value = "**/*.md")]
        glob: String,
    },

    /// Display vault statistics
    Stats,

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
        #[arg(short, long)]
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
