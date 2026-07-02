use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use tracing_subscriber::filter::LevelFilter;

mod agents;
mod auth;
mod config;
mod proposals;
mod session;
mod skills;
mod storage;
mod tools;

#[cfg(test)]
mod tests;

pub use agents::AgentsCommands;
pub use auth::AuthCommands;
pub use config::ConfigCommands;
pub use proposals::ProposalsCommands;
pub use session::SessionCommands;
pub use skills::SkillsCommands;
pub use storage::StorageCommands;
pub use tools::ToolsCommands;

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
#[command(infer_subcommands = true)]
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
        /// Same syntax as TUI :set — examples: --set model=llama3 --set thinkingbudget=high
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
        #[arg(long)]
        record: Option<PathBuf>,

        /// Replay a previously recorded JSONL session
        #[arg(long)]
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

        /// Emit a single JSON result summary instead of human-readable text
        #[arg(long, conflicts_with = "watch")]
        json: bool,
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

    /// Run installation diagnostics for Crucible
    #[command(long_about = "Run installation diagnostics for Crucible.

Checks daemon reachability, config validity, provider connectivity, kiln accessibility, embedding backend availability, plugin health, and config validation.

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

    /// Inspect workflow notes in the active kiln (list, show)
    ///
    /// Workflows are markdown notes with `type: workflow` in frontmatter.
    /// Phase 1 ships parsing and read-only views; execution is planned.
    #[command(
        long_about = "Inspect workflow notes in the active kiln.\n\nWorkflows are markdown notes whose frontmatter declares `type: workflow`. This command parses them into a typed AST and renders goals, validation criteria, gates, and the step tree. Execution is not yet implemented.\n\nExamples:\n  # List all workflows in the kiln\n  cru workflow list\n\n  # Show a workflow by filename stem, title, or path\n  cru workflow show deploy\n  cru workflow show \"Deploy Feature\"\n  cru workflow show workflows/deploy.md\n\n  # JSON output for scripting\n  cru workflow list -f json\n  cru workflow show deploy -f json"
    )]
    Workflow {
        #[command(subcommand)]
        command: crate::commands::workflow::WorkflowSubcommand,
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

    /// Review reflection-pass proposals (list, show, accept, reject)
    #[command(
        subcommand,
        long_about = "Review notes proposed by the reflection pass.\n\nAfter a session ends, the reflection plugin stages proposed notes in the kiln's .crucible/proposals/ directory (outside the index). These commands let you review and dispose of them.\n\nExamples:\n  # List pending proposals\n  cru proposals list\n\n  # Show a proposal's content\n  cru proposals show insight-20260702-1a2b\n\n  # Accept (promote into the kiln, then indexed)\n  cru proposals accept insight-20260702-1a2b\n\n  # Reject (delete)\n  cru proposals reject insight-20260702-1a2b"
    )]
    Proposals(ProposalsCommands),

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

    /// Install a plugin from a git URL (alias for `cru plugin add`)
    #[command(
        long_about = "Install a plugin from a git URL. Shorthand for `cru plugin add`.\n\nExamples:\n  cru install user/repo\n  cru install https://github.com/user/repo.git --branch main\n  cru install user/repo --pin v1.2.0"
    )]
    Install(crate::commands::plugin::AddArgs),

    /// Evaluate Lua code in the daemon's plugin runtime
    #[command(
        long_about = "Evaluate Lua code in the daemon's plugin runtime.\n\nRuns code in the same Lua VM that plugins use. Use '=' prefix for expressions.\n\nExamples:\n  # Evaluate an expression\n  cru lua '=1+1'\n\n  # Call a function\n  cru lua 'print(\"hello\")'\n\n  # Inspect the cru namespace\n  cru lua '=cru'\n\n  # Run a script file\n  cru lua --file plugin_test.lua\n\n  # Pipe from stdin\n  echo 'print(42)' | cru lua -"
    )]
    Lua {
        /// Lua code to evaluate. Use '=' prefix for expressions (e.g., '=1+1'),
        /// or '-' to read from stdin. Mutually exclusive with --file.
        #[arg(conflicts_with = "file")]
        code: Option<String>,
        /// Read Lua code from a file instead of as an argument.
        #[arg(long, value_name = "PATH")]
        file: Option<PathBuf>,
    },

    /// Initialize a new kiln or project
    #[command(
        long_about = "Initialize a directory as a Crucible kiln (knowledge store) or project.\n\nAuto-detects whether the directory is already a kiln or project. For new directories,\nan interactive prompt asks which type to create.\n\nExamples:\n  # Initialize in current directory (interactive)\n  cru init\n\n  # Initialize in specific directory\n  cru init --path ~/my-notes\n\n  # Skip prompts, use defaults (kiln)\n  cru init -y\n\n  # Force overwrite existing config\n  cru init --force",
        visible_alias = "i"
    )]
    Init {
        /// Path to initialize (defaults to current directory)
        #[arg(short, long)]
        path: Option<PathBuf>,

        /// Overwrite existing configuration
        #[arg(short = 'F', long)]
        force: bool,

        /// Skip interactive prompts, use defaults
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Manage chat sessions (create, configure, send, pause, resume, end)
    #[command(
        subcommand,
        long_about = "Manage chat sessions through their full lifecycle.\n\nLifecycle: create -> configure -> send -> pause -> resume -> end\n\nExamples:\n  cru session create --title \"My task\"           # Create and name a session\n  cru session configure <id> -p openai -m gpt-4  # Set backend\n  cru session send <id> \"message\"                 # Send and stream response\n  cru session pause <id>                          # Pause\n  cru session resume <id>                         # Resume from paused\n  cru session end <id>                            # End when done\n\n  cru session list                                # List recent sessions\n  cru session show <id>                           # Show session details\n  cru session open <id>                           # Open in TUI\n  cru session export <id> -o session.md           # Export to markdown\n  cru session search \"rust\"                       # Search by title\n\nScripting (non-interactive):\n  ID=$(cru session create -q)                     # Capture session ID\n  CRU_SESSION=$ID cru session send \"hello\"        # Use env var",
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
    ///   CRU_SESSION=chat-20260217-1030 cru set thinkingbudget=high
    #[command(
        name = "set",
        long_about = "Configure a running session's settings remotely (same syntax as TUI :set).\n\nRequires session targeting via positional SESSION_ID or CRU_SESSION env var.\nOnly daemon-synced settings (model, thinkingbudget, maxiterations) are supported.\nTUI-local settings (verbose, thinking, theme, etc.) must be set via `cru chat --set`.\n\nExamples:\n  # Switch model on a running session\n  cru set chat-20260217-1030 model=llama3\n\n  # Set thinking budget using env var for session\n  CRU_SESSION=chat-20260217-1030 cru set thinkingbudget=high\n\n  # Set multiple settings at once\n  cru set chat-20260217-1030 model=llama3 thinkingbudget=high"
    )]
    Set {
        /// Session ID and/or settings (positional args, or use CRU_SESSION env var)
        #[arg(value_name = "SESSION_ID|SETTING", num_args = 1..)]
        args: Vec<String>,

        /// [DEPRECATED] Use positional SESSION_ID instead
        #[arg(long = "session", value_name = "SESSION_ID", hide = true)]
        session_id_flag: Option<String>,
    },

    /// Bootstrap the Crucible runtime (plugins, themes, default init.lua)
    #[command(
        long_about = "Bootstrap the Crucible runtime directory with bundled plugins, themes, and a template init.lua.\n\nRun this after installing Crucible to set up the runtime files that plugins and themes need.\n\nExamples:\n  # Bootstrap runtime to default location\n  cru setup\n\n  # Bootstrap to custom location\n  cru setup --runtime-dir ~/.config/crucible/runtime\n\n  # Force re-bootstrap (overwrites existing)\n  cru setup --force"
    )]
    Setup {
        /// Custom runtime directory (default: ~/.config/crucible/runtime)
        #[arg(long)]
        runtime_dir: Option<PathBuf>,

        /// Overwrite existing runtime files
        #[arg(long)]
        force: bool,
    },

    /// Generate shell completion scripts for bash, zsh, and fish
    #[command(
        long_about = "Generate shell completion scripts for bash, zsh, and fish.\n\nOutput completion script to stdout for installation in your shell configuration.\n\nExamples:\n  # Generate bash completions\n  cru completions bash\n\n  # Generate zsh completions\n  cru completions zsh\n\n  # Generate fish completions\n  cru completions fish\n\n  # Install bash completions\n  cru completions bash | sudo tee /etc/bash_completion.d/cru\n\n  # Install zsh completions\n  cru completions zsh | sudo tee /usr/share/zsh/site-functions/_cru\n\n  # Install fish completions\n  cru completions fish > ~/.config/fish/completions/cru.fish"
    )]
    Completions {
        /// Shell type (bash, zsh, or fish)
        #[arg(value_name = "SHELL")]
        shell: String,
    },

    /// Start the web UI server for browser-based chat
    #[command(
        long_about = "Start the web UI server for browser-based chat interface.\n\nConnects to the Crucible daemon for session management and agent execution.\n\nExamples:\n  # Start web server with defaults from config\n  cru web\n\n  # Start on custom port\n  cru web --port 8080\n\n  # Bind to all interfaces\n  cru web --host 0.0.0.0"
    )]
    Web(crate::commands::web::WebCommand),
}
