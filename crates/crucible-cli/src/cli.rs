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

        /// Use internal LLM agent instead of external ACP agent
        /// Connects directly to configured LLM provider (Ollama, OpenAI, etc.)
        #[arg(long)]
        internal: bool,

        /// Force local agent execution (skip daemon).
        /// By default, chat uses daemon for agent execution.
        #[arg(long)]
        local: bool,

        /// LLM provider to use for internal agent (from config [llm.providers])
        /// Requires --internal flag
        #[arg(long, requires = "internal")]
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
    },

    /// Start MCP server exposing Crucible tools
    ///
    /// By default, starts an SSE (Server-Sent Events) server on port 3847.
    /// Use --stdio for traditional stdin/stdout transport.
    #[command(name = "mcp")]
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

        /// Number of parallel workers for processing (default: num_cpus / 2)
        #[arg(short = 'j', long = "parallel")]
        parallel: Option<usize>,
    },

    /// Display kiln statistics
    Stats,

    /// List available models from configured provider
    Models,

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

    /// Agent card management (defaults to 'list' if no subcommand given)
    Agents {
        #[command(subcommand)]
        command: Option<AgentsCommands>,
    },

    /// Task harness management
    Tasks {
        /// Path to tasks file (default: TASKS.md in cwd)
        #[arg(long, default_value = "TASKS.md")]
        file: PathBuf,

        #[command(subcommand)]
        command: crate::commands::tasks::TasksSubcommand,
    },

    /// Daemon management (start, stop, status)
    #[command(subcommand)]
    Daemon(crate::commands::daemon::DaemonCommands),

    /// Agent skills management
    #[command(subcommand)]
    Skills(SkillsCommands),

    /// Initialize a new kiln (crucible workspace)
    ///
    /// Creates a .crucible directory with configuration, sessions, and plugins directories.
    /// Use --path to specify a different location (defaults to current directory).
    /// Use --force to overwrite an existing kiln.
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
    },

    /// Session management (list, show, resume, export)
    #[command(subcommand)]
    Session(SessionCommands),

    /// Manage LLM provider credentials (defaults to 'list' if no subcommand given)
    Auth {
        #[command(subcommand)]
        command: Option<AuthCommands>,
    },
}

/// Session management subcommands
#[derive(Subcommand)]
pub enum SessionCommands {
    /// List recent sessions
    List {
        /// Maximum number of sessions to show (default: 20)
        #[arg(short = 'n', long, default_value = "20")]
        limit: u32,

        /// Filter by session type (chat, workflow, mcp)
        #[arg(short = 't', long)]
        session_type: Option<String>,

        /// Output format (table, json)
        #[arg(short = 'f', long, default_value = "table")]
        format: String,
    },

    /// Search sessions by title
    Search {
        /// Search query
        query: String,

        /// Maximum number of results
        #[arg(short = 'n', long, default_value = "20")]
        limit: u32,
    },

    /// Show session details
    Show {
        /// Session ID
        id: String,

        /// Output format (text, json, markdown)
        #[arg(short = 'f', long, default_value = "text")]
        format: String,
    },

    /// Resume a previous session
    Resume {
        /// Session ID to resume
        id: String,
    },

    /// Export session to markdown file
    Export {
        /// Session ID
        id: String,

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

    /// Manage daemon sessions (live sessions)
    #[command(subcommand)]
    Daemon(DaemonSessionCommands),
}

/// Daemon session management subcommands
#[derive(Subcommand, Debug)]
pub enum DaemonSessionCommands {
    /// List active daemon sessions
    List {
        /// Filter by state (active, paused, ended)
        #[arg(long)]
        state: Option<String>,
    },

    /// Create a new daemon session
    Create {
        /// Session type (chat, agent, workflow)
        #[arg(short = 't', long, default_value = "chat")]
        session_type: String,
    },

    /// Get details of a daemon session
    Get {
        /// Session ID
        session_id: String,
    },

    /// Pause a daemon session
    Pause {
        /// Session ID
        session_id: String,
    },

    /// Resume a paused daemon session
    Resume {
        /// Session ID
        session_id: String,
    },

    /// End a daemon session
    End {
        /// Session ID
        session_id: String,
    },

    /// Send a message to a session and stream response
    Send {
        /// Session ID
        session_id: String,

        /// Message to send
        message: String,

        /// Show raw events instead of formatted output
        #[arg(long)]
        raw: bool,
    },

    /// Configure agent for a session
    Configure {
        /// Session ID
        session_id: String,

        /// Provider (ollama, openai, anthropic)
        #[arg(short, long)]
        provider: String,

        /// Model name
        #[arg(short, long)]
        model: String,

        /// Custom endpoint URL
        #[arg(short, long)]
        endpoint: Option<String>,
    },

    /// Subscribe to session events (for debugging)
    Subscribe {
        /// Session IDs to subscribe to
        session_ids: Vec<String>,
    },

    /// Load a persisted session from storage into daemon memory
    Load {
        /// Session ID to load
        session_id: String,
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
    /// Migrate between storage modes (lightweight <-> full)
    Migrate {
        /// Target mode: "lightweight" or "full"
        #[arg(long)]
        to: String,
    },

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
    fn test_storage_migrate_parses() {
        let cli =
            Cli::try_parse_from(["cru", "storage", "migrate", "--to", "lightweight"]).unwrap();
        if let Some(Commands::Storage(StorageCommands::Migrate { to })) = cli.command {
            assert_eq!(to, "lightweight");
        } else {
            panic!("Expected Storage Migrate command");
        }
    }

    #[test]
    fn test_storage_migrate_to_full() {
        let cli = Cli::try_parse_from(["cru", "storage", "migrate", "--to", "full"]).unwrap();
        if let Some(Commands::Storage(StorageCommands::Migrate { to })) = cli.command {
            assert_eq!(to, "full");
        } else {
            panic!("Expected Storage Migrate command");
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
                interactive: false
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
        }) = cli.command
        {
            assert_eq!(path, Some(std::path::PathBuf::from("/tmp/test")));
            assert!(!force);
            assert!(!interactive);
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
        }) = cli.command
        {
            assert_eq!(path, None);
            assert!(force);
            assert!(!interactive);
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
        }) = cli.command
        {
            assert_eq!(path, Some(std::path::PathBuf::from("/tmp/test")));
            assert!(force);
            assert!(!interactive);
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
        }) = cli.command
        {
            assert_eq!(path, None);
            assert!(!force);
            assert!(interactive);
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
        }) = cli.command
        {
            assert_eq!(path, None);
            assert!(!force);
            assert!(interactive);
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
            ..
        })) = cli.command
        {
            assert_eq!(limit, 10);
            assert_eq!(session_type, Some("chat".to_string()));
        } else {
            panic!("Expected Session List command");
        }
    }

    #[test]
    fn test_session_show_parses() {
        let cli =
            Cli::try_parse_from(["cru", "session", "show", "chat-20260104-1530-a1b2"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Show { id, .. })) = cli.command {
            assert_eq!(id, "chat-20260104-1530-a1b2");
        } else {
            panic!("Expected Session Show command");
        }
    }

    #[test]
    fn test_session_resume_parses() {
        let cli =
            Cli::try_parse_from(["cru", "session", "resume", "chat-20260104-1530-a1b2"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Resume { id })) = cli.command {
            assert_eq!(id, "chat-20260104-1530-a1b2");
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
            assert_eq!(id, "chat-20260104-1530-a1b2");
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
    fn test_session_daemon_list_parses() {
        let cli = Cli::try_parse_from(["cru", "session", "daemon", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Session(SessionCommands::Daemon(
                DaemonSessionCommands::List { state: None }
            )))
        ));
    }

    #[test]
    fn test_session_daemon_list_with_state_parses() {
        let cli =
            Cli::try_parse_from(["cru", "session", "daemon", "list", "--state", "active"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Daemon(DaemonSessionCommands::List {
            state,
        }))) = cli.command
        {
            assert_eq!(state, Some("active".to_string()));
        } else {
            panic!("Expected Session Daemon List command");
        }
    }

    #[test]
    fn test_session_daemon_create_parses() {
        let cli = Cli::try_parse_from(["cru", "session", "daemon", "create"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Daemon(DaemonSessionCommands::Create {
            session_type,
        }))) = cli.command
        {
            assert_eq!(session_type, "chat"); // Default value
        } else {
            panic!("Expected Session Daemon Create command");
        }
    }

    #[test]
    fn test_session_daemon_create_with_type_parses() {
        let cli =
            Cli::try_parse_from(["cru", "session", "daemon", "create", "-t", "workflow"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Daemon(DaemonSessionCommands::Create {
            session_type,
        }))) = cli.command
        {
            assert_eq!(session_type, "workflow");
        } else {
            panic!("Expected Session Daemon Create command");
        }
    }

    #[test]
    fn test_session_daemon_get_parses() {
        let cli = Cli::try_parse_from(["cru", "session", "daemon", "get", "session-123"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Daemon(DaemonSessionCommands::Get {
            session_id,
        }))) = cli.command
        {
            assert_eq!(session_id, "session-123");
        } else {
            panic!("Expected Session Daemon Get command");
        }
    }

    #[test]
    fn test_session_daemon_pause_parses() {
        let cli =
            Cli::try_parse_from(["cru", "session", "daemon", "pause", "session-123"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Daemon(DaemonSessionCommands::Pause {
            session_id,
        }))) = cli.command
        {
            assert_eq!(session_id, "session-123");
        } else {
            panic!("Expected Session Daemon Pause command");
        }
    }

    #[test]
    fn test_session_daemon_resume_parses() {
        let cli =
            Cli::try_parse_from(["cru", "session", "daemon", "resume", "session-123"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Daemon(DaemonSessionCommands::Resume {
            session_id,
        }))) = cli.command
        {
            assert_eq!(session_id, "session-123");
        } else {
            panic!("Expected Session Daemon Resume command");
        }
    }

    #[test]
    fn test_session_daemon_end_parses() {
        let cli = Cli::try_parse_from(["cru", "session", "daemon", "end", "session-123"]).unwrap();
        if let Some(Commands::Session(SessionCommands::Daemon(DaemonSessionCommands::End {
            session_id,
        }))) = cli.command
        {
            assert_eq!(session_id, "session-123");
        } else {
            panic!("Expected Session Daemon End command");
        }
    }
}
