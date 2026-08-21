//! Server, web, SCM, and logging configuration types.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Server configuration.
///
/// `deny_unknown_fields` because this struct shares a file with [`WebConfig`],
/// and every security-relevant key (`api_key`, `remote_shell`, `allowed_hosts`,
/// `registration_roots`) lives on that one under `[web]`. Six separate places in
/// the tree once told operators to configure those under `[server]`; serde's
/// default is to ignore an unknown key, so each of them produced a config that
/// parsed cleanly and did nothing. Failing loudly is the only version of this a
/// reader can debug.
/// The `[server]` section.
///
/// It once carried `host`, `port`, `https`, `cert_file`, `key_file`,
/// `max_body_size` and `timeout_seconds`. None of them were read by anything:
/// the daemon binds a Unix socket rather than a TCP address, and the web
/// server takes its address from `[web]`. `deny_unknown_fields` is kept
/// deliberately, so a config that still sets one fails to load and names the
/// key, rather than accepting it and doing nothing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    /// Auto-archive threshold in hours for inactive sessions.
    #[serde(default)]
    pub auto_archive_hours: Option<u64>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            auto_archive_hours: Some(72),
        }
    }
}

/// Web UI server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
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
    ///
    /// The only field here that does anything. `format`, `console`, `file`,
    /// `file_path`, `rotation`, `max_file_size`, `max_files`,
    /// `component_levels`, `timestamps`, `target` and `ansi` were all
    /// deserialized, validated, and read by nothing —
    /// `CliAppConfig::logging_level` takes `level` and no other accessor exists.
    #[serde(default = "default_level")]
    pub level: String,
}

fn default_level() -> String {
    "info".to_string()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_level(),
        }
    }
}
