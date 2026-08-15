//! Server, web, SCM, and logging configuration types.

use crate::config::serde_helpers::default_true;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Server configuration.
///
/// `deny_unknown_fields` because this struct shares a file with [`WebConfig`],
/// and every security-relevant key (`api_key`, `remote_shell`, `allowed_hosts`,
/// `registration_roots`) lives on that one under `[web]`. Six separate places in
/// the tree once told operators to configure those under `[server]`; serde's
/// default is to ignore an unknown key, so each of them produced a config that
/// parsed cleanly and did nothing. Failing loudly is the only version of this a
/// reader can debug.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    /// Server host address.
    #[serde(default = "default_host")]
    pub host: String,

    /// Server port.
    #[serde(default = "default_port")]
    pub port: u16,

    /// Enable HTTPS. Reserved for future use — not yet wired to server behavior.
    #[serde(default)]
    pub https: bool,

    /// Path to TLS certificate file. Reserved for future use — not yet wired to server behavior.
    pub cert_file: Option<String>,

    /// Path to TLS private key file. Reserved for future use — not yet wired to server behavior.
    pub key_file: Option<String>,

    /// Maximum request body size in bytes. Reserved for future use — not yet wired to server behavior.
    pub max_body_size: Option<usize>,

    /// Request timeout in seconds. Reserved for future use — not yet wired to server behavior.
    pub timeout_seconds: Option<u64>,

    /// Auto-archive threshold in hours for inactive sessions.
    #[serde(default)]
    pub auto_archive_hours: Option<u64>,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8080
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            https: false,
            cert_file: None,
            key_file: None,
            max_body_size: Some(10 * 1024 * 1024), // 10MB
            timeout_seconds: Some(30),
            auto_archive_hours: Some(72),
        }
    }
}

/// Web UI server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    /// Enable the web UI server.
    #[serde(default)]
    pub enabled: bool,

    /// Web server port.
    #[serde(default = "default_web_port")]
    pub port: u16,

    /// Web server host address.
    #[serde(default = "default_web_host")]
    pub host: String,

    /// Path to static web assets directory (optional, uses embedded assets if not set).
    #[serde(default)]
    pub static_dir: Option<String>,

    /// API key for Bearer token authentication on API routes.
    /// If not set, one is generated and stored in `~/.config/crucible/api_key`.
    /// Set to empty string `""` to disable auth entirely.
    #[serde(default)]
    pub api_key: Option<String>,

    /// Allow AUTHENTICATED non-localhost clients to use the terminal/shell
    /// routes (a PTY is full shell access, so this is loopback-only by
    /// default). Fail-closed: ignored unless an API key is configured.
    #[serde(default)]
    pub remote_shell: bool,

    /// Directories under which `POST /api/project/register` may create a NEW
    /// project root. A leading `~/` expands to the home directory.
    ///
    /// A registered project root is also a read scope for `/api/file/raw`, so
    /// registering a directory grants the web API read access to everything
    /// beneath it. Registration is therefore contained the same way
    /// `scm.clone` contains its destination.
    ///
    /// Default: EMPTY, which leaves the hardcoded floor (filesystem root,
    /// home directory, credential stores, config trees are always refused) as
    /// the only gate — any other ordinary directory registers, exactly as
    /// running `cru` inside it would. Setting a non-empty list additionally
    /// confines registration to paths contained in one of the entries; a
    /// non-empty list whose entries are all invalid refuses everything
    /// (fails closed).
    #[serde(default)]
    pub registration_roots: Vec<String>,

    /// Host authorities (`host` or `host:port`) this server will answer to,
    /// compared against the request's `Host` header. Guards against DNS
    /// rebinding, where an attacker-controlled name resolving to 127.0.0.1
    /// makes a victim's browser talk to a loopback-bound `cru web`.
    ///
    /// An entry beginning with a dot (`.example.com`) is a suffix: it matches
    /// the apex and exactly one label under it (`app.example.com`), never
    /// deeper. A malformed entry — a glob, a bare `.`, a public suffix like
    /// `.com` or `.local` — makes the server refuse to start rather than be
    /// dropped with a warning nobody reads.
    ///
    /// Default: EMPTY, which means "derive the expected authority from the
    /// bind address". Set it when the server sits behind a reverse proxy or a
    /// tunnel, where the public authority is not the one it binds.
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
}

fn default_web_port() -> u16 {
    3000
}

fn default_web_host() -> String {
    "127.0.0.1".to_string()
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_web_port(),
            host: default_web_host(),
            static_dir: None,
            api_key: None,
            remote_shell: false,
            registration_roots: Vec::new(),
            allowed_hosts: Vec::new(),
        }
    }
}

/// SCM (git) integration configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScmConfig {
    /// Enable SCM detection for projects.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Detect and group git worktrees under their main repository.
    #[serde(default)]
    pub detect_worktrees: bool,
    /// Where `scm.clone` puts cloned repositories. A leading `~/` expands to
    /// the home directory. Default: "~/Projects".
    #[serde(default)]
    pub projects_dir: Option<String>,
    /// Where session-unique scratch workspaces are created for sessions
    /// started WITHOUT a project/workspace. Each such session gets its own
    /// `<session_workspace_dir>/<session_id>` directory as its workspace
    /// (filesystem containment boundary) instead of falling back to the kiln
    /// path. A leading `~/` expands to the home directory. When unset, defaults
    /// to `<data root>/workspaces` — i.e. "~/.crucible/workspaces".
    #[serde(default)]
    pub session_workspace_dir: Option<String>,
}

/// Logging configuration.
///
/// Consolidated from all crates to provide comprehensive logging control.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LoggingConfig {
    /// Global log level (trace, debug, info, warn, error).
    #[serde(default = "default_level")]
    pub level: String,

    /// Log format (json, text, compact).
    #[serde(default = "default_format")]
    pub format: String,

    /// Enable console/stdout logging.
    #[serde(default = "default_true")]
    pub console: bool,

    /// Enable file logging.
    #[serde(default)]
    pub file: bool,

    /// Log file path.
    pub file_path: Option<String>,

    /// Enable log rotation.
    #[serde(default = "default_true")]
    pub rotation: bool,

    /// Maximum log file size in bytes.
    pub max_file_size: Option<u64>,

    /// Number of log files to retain.
    pub max_files: Option<u32>,

    /// Component/module-specific log levels (e.g., "crucible_core" => "debug").
    #[serde(default)]
    pub component_levels: HashMap<String, String>,

    /// Include timestamps in log output.
    #[serde(default = "default_true")]
    pub timestamps: bool,

    /// Include module/target path in log output.
    #[serde(default = "default_true")]
    pub target: bool,

    /// Use ANSI colors in console output.
    #[serde(default = "default_true")]
    pub ansi: bool,
}

fn default_level() -> String {
    "info".to_string()
}

fn default_format() -> String {
    "text".to_string()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_level(),
            format: default_format(),
            console: true,
            file: false,
            file_path: None,
            rotation: true,
            max_file_size: Some(10 * 1024 * 1024), // 10MB
            max_files: Some(5),
            component_levels: HashMap::new(),
            timestamps: true,
            target: true,
            ansi: true,
        }
    }
}
