use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use tracing_subscriber::filter::LevelFilter;

/// Log level options for CLI
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LogLevel {
    /// No logging output
    Off,
    /// Error messages only
    Error,
    /// Warnings and errors
    Warn,
    /// Informational messages (default for verbose)
    Info,
    /// Debug messages
    Debug,
    /// Trace-level messages (most verbose)
    Trace,
}

impl From<LogLevel> for LevelFilter {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Off => LevelFilter::OFF,
            LogLevel::Error => LevelFilter::ERROR,
            LogLevel::Warn => LevelFilter::WARN,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Trace => LevelFilter::TRACE,
        }
    }
}

#[derive(Parser)]
#[command(name = "cru")]
#[command(about = "cru - Crucible CLI - Interactive knowledge management with semantic search")]
#[command(version)]
#[command(arg_required_else_help = false)]
pub struct Cli {
    /// Subcommand to execute (defaults to chat if not provided)
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Set log level (off, error, warn, info, debug, trace)
    /// If not specified, uses config file value or defaults to 'off'
    #[arg(short = 'l', long, global = true, value_enum)]
    pub log_level: Option<LogLevel>,

    /// Enable verbose logging (shortcut for --log-level=debug)
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

    /// Set output format (table, json, csv)
    #[arg(short = 'f', long, global = true, default_value = "table")]
    pub format: String,

    /// Run with an in-process daemon (no background server required).
    /// Useful for single-session use, restricted environments, or testing.
    /// Data persists to the kiln's .crucible/ directory.
    #[arg(long, global = true)]
    pub standalone: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Interactive AI chat with session persistence and tool access
    #[command(
        long_about = "Interactive AI chat with session persistence and tool access.\n\nExamples:\n  # Interactive chat session\n  cru chat\n\n  # One-shot query\n  cru chat \"Explain the architecture\"\n\n  # Resume previous session\n  cru chat --resume chat-20250102-1430-a1b2\n\n  # Use specific agent\n  cru chat --agent claude-code\n\n  # Plan mode (read-only)\n  cru chat --plan",
        visible_alias = "c"
    )]
    Chat {
        /// Optional one-shot query (if omitted, starts interactive mode)
        query: Option<String>,

        /// Preferred ACP agent to use (claude-code, gemini-cli, codex, or custom profile)
        #[arg(short = 'a', long)]
        agent: Option<String>,

        /// Resume a previous session by ID (format: chat-YYYYMMDD-HHMM-xxxx)
        #[arg(short = 'r', long)]
        resume: Option<String>,

        /// Environment variables to pass to the ACP agent (can be repeated)
        /// Format: KEY=VALUE
        /// Example: --env ANTHROPIC_BASE_URL=http://localhost:4000
        #[arg(short = 'e', long = "env", value_name = "KEY=VALUE")]
        env: Vec<String>,

        /// Session configuration overrides in vim-style format (can be repeated)
        /// Same syntax as TUI :set — examples: --set model=llama3 --set temperature=0.5
        /// Use --set key for boolean flags (e.g. --set perm.autoconfirm_session)
        #[arg(long = "set", value_name = "KEY[=VALUE]")]
        set_overrides: Vec<String>,

        /// LLM provider to use (from config [llm.providers])
        #[arg(long)]
        provider: Option<String>,

        /// Maximum context window tokens for internal agent (default: 16384)
        #[arg(long, default_value = "16384")]
        max_context: usize,

        /// Skip context enrichment (faster, but agent has no knowledge base access)
        #[arg(long)]
        no_context: bool,

        /// Number of context results to include (default: 5)
        #[arg(long, default_value = "5")]
        context_size: usize,

        /// Start in plan mode (read-only) instead of normal mode (full access)
        /// Can be toggled during session with /plan and /normal commands
        #[arg(long)]
        plan: bool,

        /// Record TUI session to a JSONL file for later replay
        #[arg(long, conflicts_with = "replay")]
        record: Option<PathBuf>,

        /// Replay a previously recorded JSONL session
        #[arg(long, conflicts_with = "record")]
        replay: Option<PathBuf>,

        /// Playback speed multiplier for replay (default: 1.0)
        #[arg(long, default_value = "1.0")]
        replay_speed: f64,

        /// Auto-exit after replay completes. Optional value is delay in milliseconds (default: 2000).
        #[arg(long, value_name = "DELAY_MS", num_args = 0..=1, default_missing_value = "2000")]
        replay_auto_exit: Option<u64>,
    },

    /// Start MCP server exposing Crucible tools for external AI agents
    #[command(
        name = "mcp",
        long_about = "Start MCP server exposing Crucible tools for external AI agents.\n\nSupports both SSE (Server-Sent Events) and stdio transports. Default is SSE on port 3847.\n\nExamples:\n  # Start SSE server on default port\n  cru mcp\n\n  # Start SSE on custom port\n  cru mcp --port 4000\n\n  # Use stdio transport (for Claude Desktop)\n  cru mcp --stdio\n\n  # Custom kiln path\n  cru mcp --kiln-path ~/my-kiln\n\n  # Disable Just tools\n  cru mcp --no-just"
    )]
    Mcp {
        /// Use stdio transport instead of SSE (default: SSE)
        #[arg(long)]
        stdio: bool,

        /// SSE server port (default: 3847)
        #[arg(long, default_value = "3847")]
        port: u16,

        /// Override kiln path
        #[arg(long)]
        kiln_path: Option<std::path::PathBuf>,

        /// Override justfile directory (default: PWD)
        #[arg(long)]
        just_dir: Option<std::path::PathBuf>,

        /// Disable Just tools
        #[arg(long)]
        no_just: bool,

        /// Log file path (default: ~/.crucible/logs/mcp.log for stdio mode)
        #[arg(long)]
        log_file: Option<std::path::PathBuf>,
    },

    /// Process markdown files through the pipeline (parse, enrich, store)
    #[command(
        long_about = "Process markdown files through the pipeline: parse, enrich with embeddings, and store in the knowledge graph.\n\nExamples:\n  # Process entire kiln\n  cru process\n\n  # Process specific file\n  cru process docs/notes.md\n\n  # Watch for changes\n  cru process --watch\n\n  # Force reprocess all files\n  cru process --force\n\n  # Dry run to preview changes\n  cru process --dry-run\n\n  # Use 4 parallel workers\n  cru process --parallel 4",
        visible_alias = "p"
    )]
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

        /// Number of parallel workers for processing (default: num_cpus / 2)
        #[arg(short = 'j', long = "parallel")]
        parallel: Option<usize>,
    },

    /// Search kiln notes using semantic and/or text search
    #[command(
        long_about = "Search kiln notes using semantic similarity, text matching, or both.\n\nExamples:\n  # Search with default (semantic + text)\n  cru search \"wikilinks\"\n\n  # Semantic search only\n  cru search \"how do links work\" --type semantic\n\n  # Text search only, JSON output\n  cru search \"wikilink\" --type text -f json\n\n  # Limit results\n  cru search \"architecture\" --limit 5"
    )]
    Search {
        /// Search query
        query: String,

        /// Maximum number of results
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,

        /// Search type: semantic, text, or both
        #[arg(long, default_value = "both")]
        r#type: String,

        /// Output format (table, json, plain)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,
    },

    /// Display kiln statistics
    #[command(
        long_about = "Display comprehensive kiln statistics including note count, embeddings status, and storage metrics.\n\nShows overview of your knowledge base with format options for different output styles.\n\nExamples:\n  # Show statistics in table format\n  cru stats\n\n  # JSON output for scripting\n  cru stats -f json\n\n  # CSV format for spreadsheets\n  cru stats -f csv"
    )]
    Stats {
        /// Output format (table, json, plain)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,
    },

    /// List available models from configured LLM provider
    #[command(
        long_about = "List available models from the configured LLM provider.\n\nQueries the provider (Ollama, OpenAI, Anthropic, etc.) to show available models and their capabilities.\n\nExamples:\n  # List models from configured provider\n  cru models\n\n  # JSON output for scripting\n  cru models -f json\n\n  # CSV format for spreadsheets\n  cru models -f csv"
    )]
    Models {
        /// Output format (table, json, plain)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,
    },

    /// Manage Crucible configuration (initialize, view, export)
    #[command(
        subcommand,
        long_about = "Manage Crucible configuration - initialize, view, and export settings.\n\nExamples:\n  # Initialize config\n  cru config init\n\n  # Show current config\n  cru config show\n\n  # Show config as JSON\n  cru config show -f json\n\n  # Dump default config\n  cru config dump > default-config.toml",
        visible_alias = "cfg"
    )]
    Config(ConfigCommands),

    /// Display storage status and statistics for the knowledge base
    #[command(
        long_about = "Display storage status and statistics for the knowledge base.\n\nShows current storage mode, usage metrics, and recent activity. Supports detailed analysis and format options.\n\nExamples:\n  # Show global storage status\n  cru status\n\n  # Analyze specific path\n  cru status docs/\n\n  # Detailed block-level information\n  cru status --detailed\n\n  # Include recent changes\n  cru status --recent\n\n  # JSON output\n  cru status -f json"
    )]
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

    #[command(long_about = "Run bounded installation diagnostics for Crucible.

Performs exactly five checks: daemon reachability, config validity, provider connectivity, kiln accessibility, and embedding backend availability.

Examples:
  # Run all doctor checks
  cru doctor

  # Output as JSON
  cru doctor -f json

  # Show help for diagnostics
  cru doctor --help")]
    Doctor {
        /// Output format (table, json)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,
    },

    /// Manage storage operations (migration, verification, backup, cleanup)
    #[command(
        subcommand,
        long_about = "Manage storage operations including migration, verification, backup, and cleanup.\n\nSupports mode switching, integrity checks, maintenance, and data export/import.\n\nExamples:\n  # Show current storage mode\n  cru storage mode\n\n  # Display storage statistics\n  cru storage stats\n\n  # Verify content integrity\n  cru storage verify\n\n  # Backup storage data\n  cru storage backup ~/backup.json\n\n  # Restore from backup\n  cru storage restore ~/backup.json\n\n  # Cleanup and optimize\n  cru storage cleanup --gc --optimize"
    )]
    Storage(StorageCommands),

    /// Manage agent cards (list, show, validate)
    #[command(
        long_about = "Manage agent cards - list, show details, and validate configurations.\n\nAgent cards define AI assistant profiles with system prompts, capabilities, and settings.\n\nExamples:\n  # List all agent cards\n  cru agents list\n\n  # Filter by tag\n  cru agents list -t documentation\n\n  # Show agent details\n  cru agents show \"Claude Code\"\n\n  # Show full system prompt\n  cru agents show \"Claude Code\" --full\n\n  # Validate all agent cards\n  cru agents validate --verbose"
    )]
    Agents {
        #[command(subcommand)]
        command: Option<AgentsCommands>,
    },

    /// Manage tasks from a TASKS.md file (list, next, pick, done)
    #[command(
        long_about = "Manage tasks from a TASKS.md file with list, next, pick, and done subcommands.\n\nTrack work items with status, blocking, and completion tracking.\n\nExamples:\n  # List all tasks\n  cru tasks list\n\n  # Show next task\n  cru tasks next\n\n  # Pick a specific task\n  cru tasks pick task-1\n\n  # Mark task as done\n  cru tasks done task-1\n\n  # Use custom tasks file\n  cru tasks --file ~/work/TASKS.md list\n\n  # Mark task as blocked\n  cru tasks blocked task-2 \"waiting for review\""
    )]
    Tasks {
        /// Path to tasks file (default: TASKS.md in cwd)
        #[arg(long, default_value = "TASKS.md")]
        file: PathBuf,

        #[command(subcommand)]
        command: crate::commands::tasks::TasksSubcommand,
    },

    /// Manage the Crucible daemon (start, stop, status, logs)
    #[command(
        subcommand,
        long_about = "Manage the Crucible daemon server for multi-session support.\n\nStart, stop, and monitor the background daemon that handles session persistence and agent execution.\n\nExamples:\n  # Start daemon\n  cru daemon start\n\n  # Check daemon status\n  cru daemon status\n\n  # Stop daemon\n  cru daemon stop\n\n  # View daemon logs\n  cru daemon logs\n\n  # Restart daemon\n  cru daemon restart"
    )]
    Daemon(crate::commands::daemon::DaemonCommands),

    /// Discover and manage agent skills (list, show, search)
    #[command(
        subcommand,
        long_about = "Discover and manage agent skills - reusable capabilities and tools.\n\nList available skills, show details, and search by functionality.\n\nExamples:\n  # List all discovered skills\n  cru skills list\n\n  # Filter by scope\n  cru skills list --scope workspace\n\n  # Show skill details\n  cru skills show \"semantic_search\"\n\n  # Search skills by query\n  cru skills search \"search\" -n 5"
    )]
    Skills(SkillsCommands),

    /// Discover and manage tools (list, show)
    #[command(
        subcommand,
        long_about = "Discover and manage tools available to agents.\n\nList tools from MCP servers, plugins, and built-in tools.\n\nExamples:\n  # List all available tools\n  cru tools list\n\n  # List tools in permission rule format\n  cru tools list --permissions"
    )]
    Tools(ToolsCommands),

    /// Manage and develop Lua plugins
    #[command(
        subcommand,
        long_about = "Manage and develop Lua plugins.\n\nTest, scaffold, generate type stubs, and run health checks for Crucible plugins.\n\nExamples:\n  # Run plugin tests\n  cru plugin test ./my-plugin\n\n  # Scaffold a new plugin\n  cru plugin new my-plugin\n\n  # Generate LuaLS type stubs\n  cru plugin stubs\n\n  # Run health checks\n  cru plugin health ./my-plugin"
    )]
    Plugin(crate::commands::plugin::PluginCommands),

    /// Initialize a new kiln (Crucible workspace)
    #[command(
        long_about = "Initialize a new kiln (Crucible workspace) with configuration and directory structure.\n\nExamples:\n  # Initialize kiln in current directory\n  cru init\n\n  # Initialize in specific directory\n  cru init --path ~/my-kiln\n\n  # Interactive setup with provider selection\n  cru init --interactive\n\n  # Force overwrite existing kiln\n  cru init --force\n\n  # Initialize a personal kiln for session storage\n  cru init --personal --path ~/my-sessions",
        visible_alias = "i"
    )]
    Init {
        /// Path where kiln should be created (defaults to current directory)
        #[arg(short, long)]
        path: Option<PathBuf>,

        /// Overwrite existing kiln
        #[arg(short = 'F', long)]
        force: bool,

        /// Interactive provider/model selection
        #[arg(short = 'i', long)]
        interactive: bool,

        /// Initialize as a personal session kiln and update config.toml
        ///
        /// Creates the kiln directory and sets session_kiln in
        /// ~/.config/crucible/config.toml so sessions are stored here by default.
        #[arg(long)]
        personal: bool,
    },

    /// Manage chat sessions (list, show, resume, export, search)
    #[command(
        subcommand,
        long_about = "Manage chat sessions - list, show details, resume, export, and search.\n\nExamples:\n  # List recent sessions\n  cru session list\n\n  # Show session details\n  cru session show chat-20250102-1430-a1b2\n\n  # Resume a session\n  cru session resume chat-20250102-1430-a1b2\n\n  # Export session to markdown\n  cru session export chat-20250102-1430-a1b2 -o session.md\n\n  # Search sessions\n  cru session search \"rust\"",
        visible_aliases = ["s", "sess"]
    )]
    Session(SessionCommands),

    /// Manage LLM provider credentials (login, logout, list)
    #[command(
        long_about = "Manage LLM provider credentials and authentication.\n\nStore, retrieve, and manage API keys for OpenAI, Anthropic, Ollama, and other providers.\n\nExamples:\n  # List all configured credentials\n  cru auth list\n\n  # Store API key for provider\n  cru auth login --provider openai --key sk-...\n\n  # Interactive login prompt\n  cru auth login\n\n  # Remove credential\n  cru auth logout --provider anthropic"
    )]
    Auth {
        #[command(subcommand)]
        command: Option<AuthCommands>,
    },

    /// Configure a running session's settings (same syntax as TUI :set)
    ///
    /// Requires session targeting via --session or CRU_SESSION env var.
    /// Examples:
    ///   cru set model=llama3 --session chat-20260217-1030
    ///   CRU_SESSION=chat-20260217-1030 cru set temperature=0.5
    #[command(
        name = "set",
        long_about = "Configure a running session's settings remotely (same syntax as TUI :set).\n\nRequires an explicit session target via --session flag or CRU_SESSION env var.\nOnly daemon-synced settings (model, temperature, thinkingbudget, maxtokens) are supported.\nTUI-local settings (verbose, thinking, theme, etc.) must be set via `cru chat --set`.\n\nExamples:\n  # Switch model on a running session\n  cru set model=llama3 --session chat-20260217-1030\n\n  # Set temperature using env var for session\n  CRU_SESSION=chat-20260217-1030 cru set temperature=0.5\n\n  # Set multiple settings at once\n  cru set model=llama3 temperature=0.7 --session chat-20260217-1030"
    )]
    Set {
        /// Settings to apply (same syntax as TUI :set, can be repeated)
        #[arg(required = true, value_name = "KEY[=VALUE]")]
        settings: Vec<String>,

        /// Target session ID (or use CRU_SESSION env var)
        #[arg(long, value_name = "SESSION_ID")]
        session: Option<String>,
    },

    /// Generate shell completion scripts (bash, zsh)
    #[command(
        long_about = "Generate shell completion scripts for bash and zsh.\n\nOutput completion script to stdout for installation in your shell configuration.\n\nExamples:\n  # Generate bash completions\n  cru completions bash\n\n  # Generate zsh completions\n  cru completions zsh\n\n  # Install bash completions\n  cru completions bash | sudo tee /etc/bash_completion.d/cru\n\n  # Install zsh completions\n  cru completions zsh | sudo tee /usr/share/zsh/site-functions/_cru"
    )]
    Completions {
        /// Shell type (bash or zsh)
        #[arg(value_name = "SHELL")]
        shell: String,
    },

    /// Start the web UI server for browser-based chat
    #[cfg(feature = "web")]
    #[command(
        long_about = "Start the web UI server for browser-based chat interface.\n\nConnects to the Crucible daemon for session management and agent execution.\n\nExamples:\n  # Start web server with defaults from config\n  cru web\n\n  # Start on custom port\n  cru web --port 8080\n\n  # Bind to all interfaces\n  cru web --host 0.0.0.0"
    )]
    Web(crate::commands::web::WebCommand),
}

/// Session management subcommands
#[derive(Subcommand)]
pub enum SessionCommands {
    /// List recent sessions
    List {
        /// Maximum number of sessions to show (default: 20)
        #[arg(short = 'n', long, default_value = "20")]
        limit: u32,

        /// Filter by session type (chat, workflow, mcp, agent)
        #[arg(short = 't', long, value_parser = ["chat", "workflow", "mcp", "agent"])]
        session_type: Option<String>,

        /// Output format (text, json)
        #[arg(short = 'f', long, default_value = "text")]
        format: String,

        /// Filter by daemon state (active, paused, ended)
        #[arg(long)]
        state: Option<String>,

        /// Include persisted sessions from storage in addition to daemon sessions
        #[arg(long)]
        all: bool,
    },

    /// Search sessions by title
    Search {
        /// Search query
        query: String,

        /// Maximum number of results
        #[arg(short = 'n', long, default_value = "20")]
        limit: u32,

        /// Output format (text, json)
        #[arg(short = 'f', long, default_value = "text", value_parser = ["text", "json"])]
        format: String,
    },

    /// Show session details
    Show {
        /// Session ID
        #[arg(value_name = "SESSION_ID")]
        id: Option<String>,

        /// Output format (text, json, markdown)
        #[arg(short = 'f', long, default_value = "text")]
        format: String,
    },

    /// Open a previous session in the TUI (same as `cru chat --resume`)
    #[command(name = "open")]
    Open {
        /// Session ID to open
        #[arg(value_name = "SESSION_ID")]
        id: Option<String>,
    },

    /// Export session to markdown file
    Export {
        /// Session ID
        #[arg(value_name = "SESSION_ID")]
        id: Option<String>,

        /// Output file (defaults to session.md in session directory)
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,

        /// Include timestamps
        #[arg(long)]
        timestamps: bool,
    },

    /// Rebuild session index from JSONL files
    Reindex {
        /// Re-index all sessions even if already indexed
        #[arg(long)]
        force: bool,
    },

    /// Clean up old sessions
    Cleanup {
        /// Delete sessions older than this many days
        #[arg(long, default_value = "30")]
        older_than: u32,

        /// Dry run - show what would be deleted
        #[arg(long)]
        dry_run: bool,
    },

    /// Create a new daemon session
    Create {
        /// Session type (chat, workflow, mcp, agent)
        #[arg(short = 't', long, default_value = "chat", value_parser = ["chat", "workflow", "mcp", "agent"])]
        session_type: String,

        /// ACP agent profile to configure for the new session
        #[arg(short = 'a', long)]
        agent: Option<String>,

        /// Recording mode (granular, coarse)
        #[arg(long)]
        recording_mode: Option<String>,

        /// Output only the session ID (for scripting). Auto-enabled when stdout is not a TTY.
        #[arg(short = 'q', long)]
        quiet: bool,

        /// Output format (text, json)
        #[arg(short = 'f', long, default_value = "text", value_parser = ["text", "json"])]
        format: String,

        /// Set a title for the new session
        #[arg(long)]
        title: Option<String>,

        /// Working directory for the session (defaults to current directory)
        #[arg(long)]
        workspace: Option<std::path::PathBuf>,
    },

    /// Pause a daemon session
    Pause {
        /// Session ID
        #[arg(value_name = "SESSION_ID")]
        session_id: Option<String>,
        /// Output format (text, json)
        #[arg(short = 'f', long, default_value = "text", value_parser = ["text", "json"])]
        format: String,
    },

    /// Resume a paused daemon session
    #[command(name = "resume")]
    Resume {
        /// Session ID
        #[arg(value_name = "SESSION_ID")]
        session_id: Option<String>,
        /// Output format (text, json)
        #[arg(short = 'f', long, default_value = "text", value_parser = ["text", "json"])]
        format: String,
    },

    /// [DEPRECATED] Use `resume` instead
    #[command(name = "unpause", hide = true)]
    Unpause {
        /// Session ID
        #[arg(value_name = "SESSION_ID")]
        session_id: Option<String>,
    },

    /// End a daemon session
    End {
        /// Session ID
        #[arg(value_name = "SESSION_ID")]
        session_id: Option<String>,
        /// Output format (text, json)
        #[arg(short = 'f', long, default_value = "text", value_parser = ["text", "json"])]
        format: String,
    },

    /// Send a message to a session and stream response
    Send {
        /// Session ID (positional, or set CRU_SESSION env var)
        /// If only one positional is given without CRU_SESSION set, it is treated as the session ID.
        /// If CRU_SESSION is set, the single positional is treated as the message.
        #[arg(value_name = "SESSION_ID", hide = true, required = false)]
        session_id_pos: Option<String>,

        /// Message to send (reads from stdin if not provided and stdin is piped)
        #[arg(value_name = "MESSAGE")]
        message: Option<String>,

        /// [DEPRECATED] Use positional SESSION_ID instead
        #[arg(long = "session", value_name = "SESSION_ID", hide = true)]
        session_id_flag: Option<String>,

        /// Show raw events instead of formatted output
        #[arg(long)]
        raw: bool,
    },

    /// Configure agent backend (provider + model + endpoint) for a session.
    /// Use `cru set` for runtime parameter tweaks (temperature, thinking_budget, etc.)
    Configure {
        /// Session ID
        #[arg(value_name = "SESSION_ID")]
        session_id: Option<String>,

        /// Provider (ollama, openai, anthropic)
        #[arg(short, long)]
        provider: String,

        /// Model name
        #[arg(short, long)]
        model: String,

        /// Custom endpoint URL
        #[arg(short, long)]
        endpoint: Option<String>,

        /// Output format (text, json)
        #[arg(short = 'f', long, default_value = "text", value_parser = ["text", "json"])]
        format: String,
    },

    /// Subscribe to session events (for debugging)
    #[command(hide = true)]
    Subscribe {
        /// Session IDs to subscribe to
        session_ids: Vec<String>,
    },

    /// Load a persisted session from storage into daemon memory
    Load {
        /// Session ID to load
        #[arg(value_name = "SESSION_ID")]
        session_id: Option<String>,
    },

    /// Replay a recorded session
    #[command(hide = true)]
    Replay {
        /// Path to recording.jsonl file
        recording_path: String,

        /// Playback speed multiplier (default 1.0, 0 = instant)
        #[arg(long, default_value = "1.0")]
        speed: f64,

        /// Show raw JSON events instead of formatted output
        #[arg(long)]
        raw: bool,
    },
}

/// Agent card management subcommands
#[derive(Subcommand)]
pub enum AgentsCommands {
    /// List all registered agent cards (default when no subcommand given)
    #[command(name = "list")]
    List {
        /// Filter by tag
        #[arg(short = 't', long)]
        tag: Option<String>,

        /// Output format (table, json)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,
    },

    /// Show details of a specific agent card
    Show {
        /// Name of the agent card to show
        name: String,

        /// Output format (table, json)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,

        /// Show full system prompt (not truncated)
        #[arg(long)]
        full: bool,
    },

    /// Validate all agent cards in configured directories
    Validate {
        /// Show detailed output for each file
        #[arg(long)]
        verbose: bool,
    },
}

#[derive(Subcommand)]
pub enum AuthCommands {
    /// Store an API key for a provider
    Login {
        /// Provider name (openai, anthropic, etc.)
        #[arg(short, long)]
        provider: Option<String>,

        /// API key value
        #[arg(short, long)]
        key: Option<String>,
    },

    /// Remove a stored credential
    Logout {
        /// Provider name to remove
        #[arg(short, long)]
        provider: Option<String>,
    },

    /// Show all configured credentials and their sources
    List,

    /// Authenticate with GitHub Copilot using OAuth device flow
    #[command(
        long_about = "Authenticate with GitHub Copilot using OAuth device flow.\n\nThis command starts the OAuth device flow and stores the long-lived OAuth token for use with GitHub Copilot.\n\nExamples:\n  # Authenticate with GitHub Copilot\n  cru auth copilot\n\n  # Force re-authentication\n  cru auth copilot --force"
    )]
    Copilot {
        /// Force re-authentication even if token exists
        #[arg(long)]
        force: bool,
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

/// Skills management subcommands
#[derive(Subcommand)]
pub enum SkillsCommands {
    /// List discovered skills
    List {
        /// Filter by scope (personal, workspace, kiln)
        #[arg(long)]
        scope: Option<String>,
        /// Output format (table, json, plain)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,
    },
    /// Show skill details
    Show {
        /// Skill name
        name: String,
    },
    /// Search skills by query
    Search {
        /// Search query
        query: String,
        /// Maximum results
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
}

#[derive(Subcommand)]
pub enum ToolsCommands {
    /// List available tools
    List {
        /// Output in permission rule format (tool:pattern)
        #[arg(long)]
        permissions: bool,
        /// Output format (table, json, plain)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_agents_list_parses() {
        let cli = Cli::try_parse_from(["cru", "agents", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Agents {
                command: Some(AgentsCommands::List { .. })
            })
        ));
    }

    #[test]
    fn test_agents_list_with_tag_filter() {
        let cli = Cli::try_parse_from(["cru", "agents", "list", "-t", "documentation"]).unwrap();
        if let Some(Commands::Agents {
            command: Some(AgentsCommands::List { tag, .. }),
        }) = cli.command
        {
            assert_eq!(tag, Some("documentation".to_string()));
        } else {
            panic!("Expected Agents List command");
        }
    }

    #[test]
    fn test_agents_show_parses() {
        let cli = Cli::try_parse_from(["cru", "agents", "show", "General Assistant"]).unwrap();
        if let Some(Commands::Agents {
            command: Some(AgentsCommands::Show { name, .. }),
        }) = cli.command
        {
            assert_eq!(name, "General Assistant");
        } else {
            panic!("Expected Agents Show command");
        }
    }

    #[test]
    fn test_agents_validate_parses() {
        let cli = Cli::try_parse_from(["cru", "agents", "validate"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Agents {
                command: Some(AgentsCommands::Validate { .. })
            })
        ));
    }

    #[test]
    fn test_agents_defaults_to_list() {
        // Per design decision: `cru agents` defaults to `list`
        // When no subcommand is given, command is None, which we treat as List
        let cli = Cli::try_parse_from(["cru", "agents"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Agents { command: None })
        ));
    }

    #[test]
    fn test_tasks_subcommand_exists() {
        // Test that `cru tasks --help` works
        let result = Cli::try_parse_from(["cru", "tasks", "--help"]);
        // --help exits with error code, but we can test that the command is recognized
        assert!(result.is_err()); // clap exits with error on --help
    }

    #[test]
    fn test_tasks_list_parses() {
        let cli = Cli::try_parse_from(["cru", "tasks", "list"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Tasks { .. })));
    }

    #[test]
    fn test_tasks_next_parses() {
        let cli = Cli::try_parse_from(["cru", "tasks", "next"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Tasks { .. })));
    }

    #[test]
    fn test_tasks_pick_parses() {
        let cli = Cli::try_parse_from(["cru", "tasks", "pick", "task-1"]).unwrap();
        if let Some(Commands::Tasks { file: _, command }) = cli.command {
            assert!(matches!(
                command,
                crate::commands::tasks::TasksSubcommand::Pick { .. }
            ));
        } else {
            panic!("Expected Tasks command");
        }
    }

    #[test]
    fn test_tasks_done_parses() {
        let cli = Cli::try_parse_from(["cru", "tasks", "done", "task-1"]).unwrap();
        if let Some(Commands::Tasks { file: _, command }) = cli.command {
            assert!(matches!(
                command,
                crate::commands::tasks::TasksSubcommand::Done { .. }
            ));
        } else {
            panic!("Expected Tasks command");
        }
    }

    #[test]
    fn test_tasks_blocked_parses() {
        let cli = Cli::try_parse_from(["cru", "tasks", "blocked", "task-1"]).unwrap();
        if let Some(Commands::Tasks { file: _, command }) = cli.command {
            assert!(matches!(
                command,
                crate::commands::tasks::TasksSubcommand::Blocked { .. }
            ));
        } else {
            panic!("Expected Tasks command");
        }
    }

    #[test]
    fn test_tasks_blocked_with_reason_parses() {
        let cli = Cli::try_parse_from(["cru", "tasks", "blocked", "task-1", "waiting for review"])
            .unwrap();
        if let Some(Commands::Tasks { file: _, command }) = cli.command {
            if let crate::commands::tasks::TasksSubcommand::Blocked { id, reason } = command {
                assert_eq!(id, "task-1");
                assert_eq!(reason, Some("waiting for review".to_string()));
            } else {
                panic!("Expected Blocked subcommand");
            }
        } else {
            panic!("Expected Tasks command");
        }
    }

    #[test]
    fn test_chat_with_env_flag_single() {
        // Should parse --env KEY=VALUE
        let cli = Cli::try_parse_from([
            "cru",
            "chat",
            "--agent",
            "opencode",
            "--env",
            "LOCAL_ENDPOINT=http://localhost:11434",
        ])
        .unwrap();

        if let Some(Commands::Chat { agent, env, .. }) = cli.command {
            assert_eq!(agent, Some("opencode".to_string()));
            assert_eq!(env.len(), 1);
            assert_eq!(env[0], "LOCAL_ENDPOINT=http://localhost:11434");
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_chat_with_env_flag_multiple() {
        // Should parse multiple --env flags
        let cli = Cli::try_parse_from([
            "cru",
            "chat",
            "--agent",
            "claude",
            "--env",
            "ANTHROPIC_BASE_URL=http://localhost:4000",
            "--env",
            "ANTHROPIC_MODEL=claude-sonnet",
        ])
        .unwrap();

        if let Some(Commands::Chat { agent, env, .. }) = cli.command {
            assert_eq!(agent, Some("claude".to_string()));
            assert_eq!(env.len(), 2);
            assert!(env.contains(&"ANTHROPIC_BASE_URL=http://localhost:4000".to_string()));
            assert!(env.contains(&"ANTHROPIC_MODEL=claude-sonnet".to_string()));
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_chat_without_env_flag_has_empty_vec() {
        // Default should be empty vec
        let cli = Cli::try_parse_from(["cru", "chat", "--agent", "opencode"]).unwrap();

        if let Some(Commands::Chat { env, .. }) = cli.command {
            assert!(env.is_empty());
        } else {
            panic!("Expected Chat command");
        }
    }

    #[test]
    fn test_storage_mode_parses() {
        let cli = Cli::try_parse_from(["cru", "storage", "mode"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Storage(StorageCommands::Mode))
        ));
    }

    #[test]
    fn test_init_parses() {
        let cli = Cli::try_parse_from(["cru", "init"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Init {
                path: None,
                force: false,
                interactive: false,
                personal: false,
            })
        ));
    }

    #[test]
    fn test_init_with_path_parses() {
        let cli = Cli::try_parse_from(["cru", "init", "--path", "/tmp/test"]).unwrap();
        if let Some(Commands::Init {
            path,
            force,
            interactive,
            personal,
        }) = cli.command
        {
            assert_eq!(path, Some(std::path::PathBuf::from("/tmp/test")));
            assert!(!force);
            assert!(!interactive);
            assert!(!personal);
        } else {
            panic!("Expected Init command");
        }
    }

    #[test]
    fn test_init_with_force_parses() {
        let cli = Cli::try_parse_from(["cru", "init", "--force"]).unwrap();
        if let Some(Commands::Init {
            path,
            force,
            interactive,
            personal,
        }) = cli.command
        {
            assert_eq!(path, None);
            assert!(force);
            assert!(!interactive);
            assert!(!personal);
        } else {
            panic!("Expected Init command");
        }
    }

    #[test]
    fn test_init_with_short_flags_parses() {
        let cli = Cli::try_parse_from(["cru", "init", "-p", "/tmp/test", "-F"]).unwrap();
        if let Some(Commands::Init {
            path,
            force,
            interactive,
            personal,
        }) = cli.command
        {
            assert_eq!(path, Some(std::path::PathBuf::from("/tmp/test")));
            assert!(force);
            assert!(!interactive);
            assert!(!personal);
        } else {
            panic!("Expected Init command");
        }
    }

    #[test]
    fn test_init_with_interactive_flag_parses() {
        let cli = Cli::try_parse_from(["cru", "init", "--interactive"]).unwrap();
        if let Some(Commands::Init {
            path,
            force,
            interactive,
            personal,
        }) = cli.command
        {
            assert_eq!(path, None);
            assert!(!force);
            assert!(interactive);
            assert!(!personal);
        } else {
            panic!("Expected Init command");
        }
    }

    #[test]
    fn test_init_with_interactive_short_flag_parses() {
        let cli = Cli::try_parse_from(["cru", "init", "-i"]).unwrap();
        if let Some(Commands::Init {
            path,
            force,
            interactive,
            personal,
        }) = cli.command
        {
            assert_eq!(path, None);
            assert!(!force);
            assert!(interactive);
            assert!(!personal);
        } else {
            panic!("Expected Init command");
        }
    }

    #[test]
    fn test_init_with_personal_flag_parses() {
        let cli =
            Cli::try_parse_from(["cru", "init", "--personal", "--path", "/tmp/sessions"]).unwrap();
        if let Some(Commands::Init {
            path,
            force,
            interactive,
            personal,
        }) = cli.command
        {
            assert_eq!(path, Some(std::path::PathBuf::from("/tmp/sessions")));
            assert!(!force);
            assert!(!interactive);
            assert!(personal);
        } else {
            panic!("Expected Init command");
        }
    }

    #[test]
    fn test_session_list_parses() {
        let cli = Cli::try_parse_from(["cru", "session", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Session(SessionCommands::List { .. }))
        ));
    }

    #[test]
    fn test_session_list_with_options() {
        let cli =
            Cli::try_parse_from(["cru", "session", "list", "-n", "10", "-t", "chat"]).unwrap();
        if let Some(Commands::Session(SessionCommands::List {
            limit,
            session_type,
            format,
            ..
        })) = cli.command
        {
            assert_eq!(limit, 10);
            assert_eq!(session_type, Some("chat".to_string()));
            assert_eq!(format, "text");
        } else {
            panic!("Expected Session List command");
        }
    }

    #[test]
    fn test_session_show_parses() {
        let cli =
            Cli::try_parse_from(["cru", "session", "show", "chat-20260104-1530-a1b2"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Show { id, .. })) = cli.command {
            assert_eq!(id, Some("chat-20260104-1530-a1b2".to_string()));
        } else {
            panic!("Expected Session Show command");
        }
    }

    #[test]
    fn test_session_open_parses() {
        let cli =
            Cli::try_parse_from(["cru", "session", "open", "chat-20260104-1530-a1b2"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Open { id })) = cli.command {
            assert_eq!(id, Some("chat-20260104-1530-a1b2".to_string()));
        } else {
            panic!("Expected Session Open command");
        }
    }

    #[test]
    fn test_session_resume_parses() {
        let cli =
            Cli::try_parse_from(["cru", "session", "resume", "chat-20260104-1530-a1b2"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Resume { session_id, format })) = cli.command
        {
            assert_eq!(session_id, Some("chat-20260104-1530-a1b2".to_string()));
            assert_eq!(format, "text");
        } else {
            panic!("Expected Session Resume command");
        }
    }

    #[test]
    fn test_session_export_parses() {
        let cli = Cli::try_parse_from([
            "cru",
            "session",
            "export",
            "chat-20260104-1530-a1b2",
            "--timestamps",
        ])
        .unwrap();
        if let Some(Commands::Session(SessionCommands::Export { id, timestamps, .. })) = cli.command
        {
            assert_eq!(id, Some("chat-20260104-1530-a1b2".to_string()));
            assert!(timestamps);
        } else {
            panic!("Expected Session Export command");
        }
    }

    #[test]
    fn test_session_search_parses() {
        let cli = Cli::try_parse_from(["cru", "session", "search", "rust"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Search { query, .. })) = cli.command {
            assert_eq!(query, "rust");
        } else {
            panic!("Expected Session Search command");
        }
    }

    #[test]
    fn test_session_cleanup_parses() {
        let cli = Cli::try_parse_from([
            "cru",
            "session",
            "cleanup",
            "--older-than",
            "60",
            "--dry-run",
        ])
        .unwrap();
        if let Some(Commands::Session(SessionCommands::Cleanup {
            older_than,
            dry_run,
        })) = cli.command
        {
            assert_eq!(older_than, 60);
            assert!(dry_run);
        } else {
            panic!("Expected Session Cleanup command");
        }
    }

    #[test]
    fn test_session_reindex_parses() {
        let cli = Cli::try_parse_from(["cru", "session", "reindex"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Session(SessionCommands::Reindex { force: false }))
        ));
    }

    #[test]
    fn test_session_list_with_state_parses() {
        let cli = Cli::try_parse_from(["cru", "session", "list", "--state", "active"]).unwrap();
        if let Some(Commands::Session(SessionCommands::List { state, .. })) = cli.command {
            assert_eq!(state, Some("active".to_string()));
        } else {
            panic!("Expected Session List command");
        }
    }

    #[test]
    fn test_session_list_with_all_flag_parses() {
        let cli = Cli::try_parse_from(["cru", "session", "list", "--all"]).unwrap();
        if let Some(Commands::Session(SessionCommands::List { all, .. })) = cli.command {
            assert!(all);
        } else {
            panic!("Expected Session List command with --all flag");
        }
    }

    #[test]
    fn test_session_list_accepts_agent_type() {
        let cli = Cli::try_parse_from(["cru", "session", "list", "-t", "agent"]).unwrap();
        if let Some(Commands::Session(SessionCommands::List { session_type, .. })) = cli.command {
            assert_eq!(session_type, Some("agent".to_string()));
        } else {
            panic!("Expected Session List command");
        }
    }

    #[test]
    fn test_session_create_parses() {
        let cli = Cli::try_parse_from(["cru", "session", "create"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Create {
            session_type,
            agent,
            recording_mode,
            quiet,
            format,
            title,
            workspace,
        })) = cli.command
        {
            assert_eq!(session_type, "chat");
            assert_eq!(agent, None);
            assert_eq!(recording_mode, None);
            assert!(!quiet);
            assert_eq!(format, "text");
            assert_eq!(title, None);
            assert_eq!(workspace, None);
        } else {
            panic!("Expected Session Create command");
        }
    }

    #[test]
    fn test_session_create_with_type_parses() {
        let cli = Cli::try_parse_from(["cru", "session", "create", "-t", "workflow"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Create {
            session_type,
            agent,
            recording_mode,
            quiet,
            format,
            title,
            workspace,
        })) = cli.command
        {
            assert_eq!(session_type, "workflow");
            assert_eq!(agent, None);
            assert_eq!(recording_mode, None);
            assert!(!quiet);
            assert_eq!(format, "text");
            assert_eq!(title, None);
            assert_eq!(workspace, None);
        } else {
            panic!("Expected Session Create command");
        }
    }

    #[test]
    fn test_session_create_accepts_mcp_type() {
        let cli = Cli::try_parse_from(["cru", "session", "create", "-t", "mcp"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Create { session_type, .. })) = cli.command {
            assert_eq!(session_type, "mcp");
        } else {
            panic!("Expected Session Create command");
        }
    }

    #[test]
    fn test_session_create_with_quiet_flag() {
        let cli = Cli::try_parse_from(["cru", "session", "create", "-q"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Create { quiet, .. })) = cli.command {
            assert!(quiet);
        } else {
            panic!("Expected Session Create command");
        }
    }

    #[test]
    fn test_session_create_with_format_json() {
        let cli = Cli::try_parse_from(["cru", "session", "create", "-f", "json"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Create { format, .. })) = cli.command {
            assert_eq!(format, "json");
        } else {
            panic!("Expected Session Create command");
        }
    }

    #[test]
    fn test_session_create_with_title() {
        let cli =
            Cli::try_parse_from(["cru", "session", "create", "--title", "My Session"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Create { title, .. })) = cli.command {
            assert_eq!(title, Some("My Session".to_string()));
        } else {
            panic!("Expected Session Create command");
        }
    }

    #[test]
    fn test_session_create_with_workspace() {
        let cli = Cli::try_parse_from(["cru", "session", "create", "--workspace", "/tmp"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Create { workspace, .. })) = cli.command {
            assert_eq!(workspace, Some(std::path::PathBuf::from("/tmp")));
        } else {
            panic!("Expected Session Create command");
        }
    }

    #[test]
    fn test_session_pause_parses() {
        let cli = Cli::try_parse_from(["cru", "session", "pause", "session-123"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Pause { session_id, format })) = cli.command
        {
            assert_eq!(session_id, Some("session-123".to_string()));
            assert_eq!(format, "text");
        } else {
            panic!("Expected Session Pause command");
        }
    }

    #[test]
    fn test_session_unpause_parses() {
        let cli = Cli::try_parse_from(["cru", "session", "unpause", "session-123"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Unpause { session_id })) = cli.command {
            assert_eq!(session_id, Some("session-123".to_string()));
        } else {
            panic!("Expected Session Unpause command");
        }
    }

    #[test]
    fn test_session_end_parses() {
        let cli = Cli::try_parse_from(["cru", "session", "end", "session-123"]).unwrap();
        if let Some(Commands::Session(SessionCommands::End { session_id, format })) = cli.command {
            assert_eq!(session_id, Some("session-123".to_string()));
            assert_eq!(format, "text");
        } else {
            panic!("Expected Session End command");
        }
    }

    #[test]
    fn test_session_pause_with_format_json() {
        let cli =
            Cli::try_parse_from(["cru", "session", "pause", "session-123", "-f", "json"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Pause { session_id, format })) = cli.command
        {
            assert_eq!(session_id, Some("session-123".to_string()));
            assert_eq!(format, "json");
        } else {
            panic!("Expected Session Pause command");
        }
    }

    #[test]
    fn test_session_resume_with_format_json() {
        let cli = Cli::try_parse_from([
            "cru",
            "session",
            "resume",
            "chat-20260104-1530-a1b2",
            "-f",
            "json",
        ])
        .unwrap();
        if let Some(Commands::Session(SessionCommands::Resume { session_id, format })) = cli.command
        {
            assert_eq!(session_id, Some("chat-20260104-1530-a1b2".to_string()));
            assert_eq!(format, "json");
        } else {
            panic!("Expected Session Resume command");
        }
    }

    #[test]
    fn test_session_end_with_format_json() {
        let cli =
            Cli::try_parse_from(["cru", "session", "end", "session-123", "-f", "json"]).unwrap();
        if let Some(Commands::Session(SessionCommands::End { session_id, format })) = cli.command {
            assert_eq!(session_id, Some("session-123".to_string()));
            assert_eq!(format, "json");
        } else {
            panic!("Expected Session End command");
        }
    }

    #[test]
    fn test_session_send_positional_id_and_message() {
        let cli = Cli::try_parse_from(["cru", "session", "send", "chat-123", "hello"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Send {
            session_id_pos,
            message,
            session_id_flag,
            raw,
        })) = cli.command
        {
            assert_eq!(session_id_pos, Some("chat-123".to_string()));
            assert_eq!(message, Some("hello".to_string()));
            assert_eq!(session_id_flag, None);
            assert!(!raw);
        } else {
            panic!("Expected Session Send command");
        }
    }

    #[test]
    fn test_session_send_deprecated_session_flag() {
        let cli = Cli::try_parse_from(["cru", "session", "send", "--session", "chat-123", "hello"])
            .unwrap();
        if let Some(Commands::Session(SessionCommands::Send {
            session_id_pos,
            message,
            session_id_flag,
            raw,
        })) = cli.command
        {
            assert_eq!(session_id_pos, Some("hello".to_string()));
            assert_eq!(message, None);
            assert_eq!(session_id_flag, Some("chat-123".to_string()));
            assert!(!raw);
        } else {
            panic!("Expected Session Send command");
        }
    }

    #[test]
    fn test_session_send_single_positional_message_only() {
        let cli = Cli::try_parse_from(["cru", "session", "send", "hello"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Send {
            session_id_pos,
            message,
            session_id_flag,
            raw,
        })) = cli.command
        {
            assert_eq!(session_id_pos, Some("hello".to_string()));
            assert_eq!(message, None);
            assert_eq!(session_id_flag, None);
            assert!(!raw);
        } else {
            panic!("Expected Session Send command");
        }
    }
}
