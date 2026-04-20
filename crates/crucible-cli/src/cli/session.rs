use clap::Subcommand;
use std::path::PathBuf;

/// Session management subcommands
#[derive(Subcommand)]
pub enum SessionCommands {
    /// List recent sessions
    List {
        /// Maximum number of sessions to show (default: 20)
        #[arg(short = 'n', long, default_value = "20")]
        limit: u32,

        /// Filter by session type (chat, agent, workflow; legacy: mcp→chat)
        #[arg(
            short = 't',
            long,
            value_parser = crate::commands::session::helpers::parse_session_type_arg,
        )]
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
        /// Session type (chat, agent, workflow; legacy: mcp→chat)
        #[arg(
            short = 't',
            long,
            default_value = "chat",
            value_parser = crate::commands::session::helpers::parse_session_type_arg,
        )]
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

        /// Override permission mode (allow, deny, or ask). Overrides CRUCIBLE_PERMISSIONS env var.
        #[arg(long, value_name = "MODE", value_parser = ["allow", "deny", "ask"])]
        permissions: Option<String>,
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

        /// Override permission mode (allow, deny, or ask). Overrides CRUCIBLE_PERMISSIONS env var.
        /// Use `--permissions allow` for automation / fixture recording to bypass prompts.
        #[arg(long, value_name = "MODE", value_parser = ["allow", "deny", "ask"])]
        permissions: Option<String>,
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
