//! CLI application configuration types and loading.

/// Registry of all tracked configuration fields.
/// Each field is tracked for source provenance (file, CLI, env, default).
const TRACKED_FIELDS: &[(&str, &str)] = &[
    ("kiln_path", "Top-level"),
    ("agent_directories", "Top-level"),
    ("session_kiln", "Top-level"),
    ("llm.default", "LLM"),
    ("acp.default_agent", "ACP"),
    ("acp.enable_discovery", "ACP"),
    ("acp.session_timeout_minutes", "ACP"),
    ("acp.max_message_size_mb", "ACP"),
    ("chat.model", "Chat"),
    ("chat.enable_markdown", "Chat"),
    ("chat.endpoint", "Chat"),
    ("chat.temperature", "Chat"),
    ("chat.max_tokens", "Chat"),
    ("chat.timeout_secs", "Chat"),
    ("cli.show_progress", "CLI"),
    ("cli.confirm_destructive", "CLI"),
    ("cli.verbose", "CLI"),
    ("logging.level", "Logging"),
    ("processing.parallel_workers", "Processing"),
];

use crate::config::components::{
    AcpConfig, ChatConfig, CliConfig, ContextConfig, LlmConfig, McpConfig, PermissionConfig,
    StorageConfig,
};
use crate::config::EnrichmentConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, info};

#[cfg(feature = "toml")]
extern crate toml;

use super::errors::ConfigError;
use super::provider::EffectiveLlmConfig;
use super::server::{LoggingConfig, WebConfig};

/// Processing configuration for file processing operations.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessingConfig {
    /// Number of parallel workers for processing (default: num_cpus / 2)
    #[serde(default)]
    pub parallel_workers: Option<usize>,
}

/// CLI application composite configuration structure.
///
/// This provides the main configuration interface for the CLI application,
/// combining all necessary components with sensible defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliAppConfig {
    /// Path to the Obsidian kiln directory
    #[serde(default = "default_kiln_path")]
    pub kiln_path: std::path::PathBuf,

    /// Default kiln for storing chat sessions.
    ///
    /// When set, `cru chat` will save sessions to this kiln instead of `kiln_path`.
    /// This allows separating personal session storage from workspace kilns.
    #[serde(default)]
    pub session_kiln: Option<std::path::PathBuf>,

    /// Named kilns registry. Each entry maps a name to a path (+ options).
    /// If empty, falls back to `kiln_path` for backward compatibility.
    #[serde(default)]
    pub kilns: HashMap<String, crate::config::config::registry::KilnEntry>,

    /// Registered projects with kiln bindings.
    #[serde(default)]
    pub projects: HashMap<String, crate::config::config::registry::ProjectEntry>,

    /// Which named kiln is the default (session storage, tool scoping).
    /// If unset and `kilns` is non-empty, uses the first kiln alphabetically.
    #[serde(default)]
    pub default_kiln: Option<String>,

    /// Additional directories to search for agent cards
    ///
    /// Paths can be absolute or relative (to config file location).
    /// These are loaded after the default locations.
    #[serde(default)]
    pub agent_directories: Vec<std::path::PathBuf>,

    /// ACP (Agent Client Protocol) configuration
    #[serde(default)]
    pub acp: AcpConfig,

    /// Chat configuration
    #[serde(default)]
    pub chat: ChatConfig,

    /// LLM provider configuration with named instances
    #[serde(default)]
    pub llm: LlmConfig,

    /// Enrichment configuration (embedding provider, pipeline settings)
    #[serde(default)]
    pub enrichment: Option<EnrichmentConfig>,

    /// CLI-specific configuration
    #[serde(default)]
    pub cli: CliConfig,

    /// Logging configuration
    #[serde(default)]
    pub logging: Option<LoggingConfig>,

    /// Processing configuration
    #[serde(default)]
    pub processing: ProcessingConfig,

    /// Context configuration (rules files, etc.)
    #[serde(default)]
    pub context: Option<ContextConfig>,

    /// Storage configuration (embedded vs daemon mode)
    #[serde(default)]
    pub storage: Option<StorageConfig>,

    /// MCP server configuration (upstream servers, gateway settings)
    #[serde(default)]
    pub mcp: Option<McpConfig>,

    /// Permission configuration for tool access control.
    #[serde(default)]
    pub permissions: Option<PermissionConfig>,

    /// Declarative schedules that run Lua snippets at fixed intervals.
    #[serde(default)]
    pub schedules: Vec<super::types::ScheduleEntry>,

    /// Runtime search path for plugins, themes, and skills.
    ///
    /// Directories are searched in order (first match wins). Each directory
    /// can contain `plugins/`, `themes/`, and `skills/` subdirectories.
    ///
    /// Defaults to: `[~/.config/crucible, $CRUCIBLE_RUNTIME, <exe-relative>]`
    /// Set `CRUCIBLE_PLUGIN_PATH` to prepend additional paths.
    ///
    /// ```toml
    /// runtimepath = ["~/.config/crucible", "/opt/crucible/runtime"]
    /// ```
    #[serde(default)]
    pub runtimepath: Vec<std::path::PathBuf>,

    /// Per-plugin configuration sections (e.g. `[plugins.discord]`)
    #[serde(default)]
    pub plugins: HashMap<String, serde_json::Value>,

    /// Web UI server configuration
    #[serde(default)]
    pub web: Option<WebConfig>,

    /// Server configuration (daemon settings, auto-archive, etc.)
    #[serde(default)]
    pub server: Option<super::server::ServerConfig>,

    /// Value source tracking for configuration provenance
    ///
    /// Tracks where each configuration value came from (file, environment, CLI, default).
    /// Populated during `load()` or `load_with_tracking()`.
    #[serde(skip)]
    pub source_map: Option<crate::config::value_source::ValueSourceMap>,
}

fn default_kiln_path() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}

impl Default for CliAppConfig {
    fn default() -> Self {
        Self {
            kiln_path: default_kiln_path(),
            session_kiln: None,
            kilns: HashMap::new(),
            projects: HashMap::new(),
            default_kiln: None,
            agent_directories: Vec::new(),
            acp: AcpConfig::default(),
            chat: ChatConfig::default(),
            llm: LlmConfig::default(),
            enrichment: None,
            cli: CliConfig::default(),
            logging: None,
            processing: ProcessingConfig::default(),
            context: None,
            storage: None,
            mcp: None,
            permissions: None,
            schedules: Vec::new(),
            runtimepath: Vec::new(),
            plugins: HashMap::new(),
            web: None,
            server: None,
            source_map: None,
        }
    }
}

impl CliAppConfig {
    /// Load CLI configuration from file with env var and CLI flag overrides
    ///
    /// Priority (highest to lowest):
    /// 1. CLI flags (--kiln-path, --embedding-url, --embedding-model)
    /// 2. Config file (~/.config/crucible/config.toml)
    /// 3. Default values
    ///
    /// Note: API keys are read from environment variables specified in config
    /// (e.g., `api_key = "OPENAI_API_KEY"`)
    ///
    /// This version also populates the `source_map` field to track where each
    /// configuration value came from. Use `--trace` with `cru config show` to
    /// display this information.
    pub fn load(
        config_file: Option<std::path::PathBuf>,
        embedding_url: Option<String>,
        embedding_model: Option<String>,
    ) -> Result<Self, ConfigError> {
        Self::load_inner(config_file, embedding_url, embedding_model).map_err(ConfigError::from)
    }

    /// Internal implementation using anyhow for ergonomic error handling.
    fn load_inner(
        config_file: Option<std::path::PathBuf>,
        embedding_url: Option<String>,
        embedding_model: Option<String>,
    ) -> anyhow::Result<Self> {
        use crate::config::value_source::{ValueSource, ValueSourceMap};

        // Determine config file path
        let config_path = config_file.unwrap_or_else(Self::default_config_path);

        debug!("Attempting to load config from: {}", config_path.display());

        let mut source_map = ValueSourceMap::new();
        let config_path_str = config_path.to_string_lossy().to_string();

        // Try to load config file or use defaults
        let (mut config, file_fields) = if config_path.exists() {
            info!("Found config file at: {}", config_path.display());

            let contents = std::fs::read_to_string(&config_path)
                .map_err(|e| anyhow::anyhow!("Failed to read config file: {}", e))?;

            #[cfg(feature = "toml")]
            {
                // First parse as a raw TOML table to detect which fields are present
                let raw_table: toml::Table = toml::from_str(&contents).map_err(|e| {
                    error!(
                        "Failed to parse config file {}: {}",
                        config_path.display(),
                        e
                    );
                    anyhow::anyhow!(
                        "Failed to parse config file {}: {}",
                        config_path.display(),
                        e
                    )
                })?;

                if raw_table.contains_key("embedding") {
                    return Err(anyhow::anyhow!(
                        "Failed to parse config file {}: legacy [embedding] is no longer supported. Use [llm.providers.<name>] with [llm].default",
                        config_path.display()
                    ));
                }
                if raw_table.contains_key("providers") {
                    return Err(anyhow::anyhow!(
                        "Failed to parse config file {}: legacy [providers] is no longer supported. Use [llm.providers.<name>] with [llm].default",
                        config_path.display()
                    ));
                }
                if let Some(toml::Value::Table(chat)) = raw_table.get("chat") {
                    if chat.contains_key("provider") {
                        return Err(anyhow::anyhow!(
                            "Failed to parse config file {}: chat.provider is no longer supported. Use [llm.providers.<name>] with [llm].default",
                            config_path.display()
                        ));
                    }
                }

                let file_fields = Self::detect_present_fields(&raw_table);
                let mut value = toml::Value::Table(raw_table);
                let base_dir = config_path.parent().unwrap_or(std::path::Path::new("."));
                if let Err(errors) = crate::config::includes::process_file_references(
                    &mut value,
                    base_dir,
                    crate::config::includes::ResolveMode::BestEffort,
                ) {
                    for error in errors {
                        tracing::warn!("Config reference error: {}", error);
                    }
                }

                match value.try_into::<CliAppConfig>() {
                    Ok(cfg) => {
                        info!("Successfully loaded config file: {}", config_path.display());
                        (cfg, file_fields)
                    }
                    Err(e) => {
                        error!(
                            "Failed to parse config file {}: {}",
                            config_path.display(),
                            e
                        );
                        return Err(anyhow::anyhow!(
                            "Failed to parse config file {}: {}",
                            config_path.display(),
                            e
                        ));
                    }
                }
            }

            #[cfg(not(feature = "toml"))]
            {
                return Err(anyhow::anyhow!(
                    "Failed to parse config file: TOML feature not enabled"
                ));
            }
        } else {
            debug!(
                "No config file found at {}, using defaults",
                config_path.display()
            );
            (Self::default(), Vec::new())
        };

        // Track sources for all known fields
        let all_tracked_fields = TRACKED_FIELDS
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>();

        for field in &all_tracked_fields {
            if file_fields.contains(&(*field).to_string()) {
                source_map.set(
                    field,
                    ValueSource::File {
                        path: Some(config_path_str.clone()),
                    },
                );
            } else {
                source_map.set(field, ValueSource::Default);
            }
        }

        // Apply CLI flag overrides (priority 1 - highest)
        if let Some(url) = embedding_url {
            if let Some(default_key) = config.llm.default.clone() {
                if let Some(provider) = config.llm.providers.get_mut(&default_key) {
                    debug!(
                        "Overriding llm.providers.{}.endpoint from CLI flag: {}",
                        default_key, url
                    );
                    provider.endpoint = Some(url);
                    source_map.set("llm.default.endpoint", ValueSource::Cli);
                }
            }
        }
        if let Some(model) = embedding_model {
            if let Some(default_key) = config.llm.default.clone() {
                if let Some(provider) = config.llm.providers.get_mut(&default_key) {
                    debug!(
                        "Overriding llm.providers.{}.default_model from CLI flag: {}",
                        default_key, model
                    );
                    provider.default_model = Some(model);
                    source_map.set("llm.default.model", ValueSource::Cli);
                }
            }
        }

        config.source_map = Some(source_map);
        Ok(config)
    }

    /// Detect which [`TRACKED_FIELDS`] are present in a TOML table.
    ///
    /// Each tracked path is either top-level (`"kiln_path"`) or one level
    /// deep (`"acp.default_agent"`); the walk mirrors the const's order.
    #[cfg(feature = "toml")]
    fn detect_present_fields(table: &toml::Table) -> Vec<String> {
        TRACKED_FIELDS
            .iter()
            .filter(|(path, _)| match path.split_once('.') {
                None => table.contains_key(*path),
                Some((section, key)) => matches!(
                    table.get(section),
                    Some(toml::Value::Table(t)) if t.contains_key(key)
                ),
            })
            .map(|(path, _)| path.to_string())
            .collect()
    }

    /// Log the effective configuration for debugging
    pub fn log_config(&self) {
        info!("Effective configuration:");
        info!("  kiln_path: {}", self.kiln_path.display());
        info!("  session_kiln: {:?}", self.session_kiln);
        info!("  llm.default: {:?}", self.llm.default);
        info!("  acp.default_agent: {:?}", self.acp.default_agent);
        info!("  acp.enable_discovery: {}", self.acp.enable_discovery);
        info!(
            "  acp.session_timeout_minutes: {}",
            self.acp.session_timeout_minutes
        );
        info!("  cli.show_progress: {}", self.cli.show_progress);
        info!(
            "  cli.confirm_destructive: {}",
            self.cli.confirm_destructive
        );
        info!("  cli.verbose: {}", self.cli.verbose);
    }

    /// Get database path (always derived from kiln path)
    ///
    /// Returns the path to the SQLite database file within the
    /// `.crucible` directory under the kiln path.
    pub fn database_path(&self) -> std::path::PathBuf {
        // Only use PID suffix in test mode to prevent database lock collisions
        let db_name = if std::env::var("CRUCIBLE_TEST_MODE").is_ok() {
            let pid = std::process::id();
            format!("crucible-{}.db", pid)
        } else {
            "crucible.db".to_string()
        };
        self.kiln_path.join(".crucible").join(db_name)
    }

    /// Get tools directory path (always derived from kiln path)
    pub fn tools_path(&self) -> std::path::PathBuf {
        self.kiln_path.join("tools")
    }

    /// Get database path as a string
    pub fn database_path_str(&self) -> Result<String, ConfigError> {
        self.database_path()
            .to_str()
            .map(|s| s.to_string())
            .ok_or_else(|| ConfigError::InvalidValue {
                field: "database_path".into(),
                value: self.database_path().display().to_string(),
            })
    }

    /// Get kiln path as a string
    pub fn kiln_path_str(&self) -> Result<String, ConfigError> {
        self.kiln_path
            .to_str()
            .map(|s| s.to_string())
            .ok_or_else(|| ConfigError::InvalidValue {
                field: "kiln_path".into(),
                value: self.kiln_path.display().to_string(),
            })
    }

    /// Display the current configuration as TOML
    #[cfg(feature = "toml")]
    pub fn display_as_toml(&self) -> Result<String, ConfigError> {
        toml::to_string_pretty(self).map_err(|e| ConfigError::TomlSer(e.to_string()))
    }

    /// Display the current configuration as TOML (placeholder when toml feature is disabled)
    #[cfg(not(feature = "toml"))]
    pub fn display_as_toml(&self) -> Result<String, ConfigError> {
        Err(ConfigError::Other("TOML feature not enabled".into()))
    }

    /// Display the current configuration as JSON
    pub fn display_as_json(&self) -> Result<String, ConfigError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Get the source map, preferring the stored one if available
    fn get_source_map(&self) -> crate::config::value_source::ValueSourceMap {
        if let Some(ref map) = self.source_map {
            return map.clone();
        }
        // Fallback to heuristic for configs created without load()
        Self::build_fallback_source_map()
    }

    /// Build a fallback source map when no tracking data is available.
    /// Used when config is created with default() instead of load().
    fn build_fallback_source_map() -> crate::config::value_source::ValueSourceMap {
        use crate::config::value_source::{ValueSource, ValueSourceMap};

        let mut map = ValueSourceMap::new();
        for (field_name, _) in TRACKED_FIELDS {
            map.set(field_name, ValueSource::Default);
        }

        map
    }

    /// Current value of a [`TRACKED_FIELDS`] path as JSON.
    ///
    /// Returns `None` when the field is `Option`-typed and unset — callers
    /// render those as explicit "unset" entries rather than omitting them.
    /// The `tracked_value_covers_every_tracked_field` test keeps this match
    /// in sync with the const.
    fn tracked_value(&self, path: &str) -> Option<serde_json::Value> {
        use serde_json::json;
        match path {
            "kiln_path" => Some(json!(self.kiln_path.to_string_lossy())),
            "agent_directories" => Some(json!(self
                .agent_directories
                .iter()
                .map(|p| p.to_string_lossy())
                .collect::<Vec<_>>())),
            "session_kiln" => self
                .session_kiln
                .as_ref()
                .map(|p| json!(p.to_string_lossy())),
            "llm.default" => self.llm.default.as_ref().map(|v| json!(v)),
            "acp.default_agent" => self.acp.default_agent.as_ref().map(|v| json!(v)),
            "acp.enable_discovery" => Some(json!(self.acp.enable_discovery)),
            "acp.session_timeout_minutes" => Some(json!(self.acp.session_timeout_minutes)),
            "acp.max_message_size_mb" => Some(json!(self.acp.max_message_size_mb)),
            "chat.model" => self.chat.model.as_ref().map(|v| json!(v)),
            "chat.enable_markdown" => Some(json!(self.chat.enable_markdown)),
            "chat.endpoint" => self.chat.endpoint.as_ref().map(|v| json!(v)),
            "chat.temperature" => self.chat.temperature.map(|v| json!(v)),
            "chat.max_tokens" => self.chat.max_tokens.map(|v| json!(v)),
            "chat.timeout_secs" => self.chat.timeout_secs.map(|v| json!(v)),
            "cli.show_progress" => Some(json!(self.cli.show_progress)),
            "cli.confirm_destructive" => Some(json!(self.cli.confirm_destructive)),
            "cli.verbose" => Some(json!(self.cli.verbose)),
            "logging.level" => self.logging.as_ref().map(|l| json!(l.level)),
            "processing.parallel_workers" => self.processing.parallel_workers.map(|v| json!(v)),
            _ => None,
        }
    }

    /// Display the current configuration as JSON with source tracking.
    ///
    /// Every [`TRACKED_FIELDS`] entry appears in the output (unset fields get
    /// a `null` value) so the trace can't silently drop fields.
    pub fn display_as_json_with_sources(&self) -> Result<String, ConfigError> {
        use crate::config::value_source::ValueSource;

        let source_map = self.get_source_map();
        let mut output = serde_json::Map::new();

        for (path, _) in TRACKED_FIELDS {
            let source = source_map.get(path).unwrap_or(&ValueSource::Default);
            let mut item = serde_json::Map::new();
            item.insert(
                "value".to_string(),
                self.tracked_value(path).unwrap_or(serde_json::Value::Null),
            );
            item.insert(
                "source".to_string(),
                serde_json::Value::String(source.detail()),
            );
            item.insert(
                "source_short".to_string(),
                serde_json::Value::String(source.short().to_string()),
            );
            let item = serde_json::Value::Object(item);

            match path.split_once('.') {
                None => {
                    output.insert(path.to_string(), item);
                }
                Some((section, key)) => {
                    output
                        .entry(section.to_string())
                        .or_insert_with(|| serde_json::Value::Object(Default::default()))
                        .as_object_mut()
                        .expect("section entries are always objects")
                        .insert(key.to_string(), item);
                }
            }
        }

        Ok(serde_json::to_string_pretty(&output)?)
    }

    /// Display the current configuration as TOML with source tracking.
    ///
    /// Every [`TRACKED_FIELDS`] entry appears in the output; unset fields
    /// render as `# key = <unset>` comments so the document stays valid TOML.
    pub fn display_as_toml_with_sources(&self) -> Result<String, ConfigError> {
        use crate::config::value_source::ValueSource;

        let source_map = self.get_source_map();
        let mut output = String::new();
        output.push_str("# Effective Configuration with Value Sources\n");
        output.push_str("# Sources: file (<path>), cli, env (<var>), default\n\n");

        // TRACKED_FIELDS keeps each section's fields contiguous, so emitting a
        // header on section change yields one [section] block per section.
        let mut current_section = "";
        for (path, _) in TRACKED_FIELDS {
            let (section, key) = path.split_once('.').unwrap_or(("", path));
            if section != current_section {
                output.push_str(&format!("\n[{section}]\n"));
                current_section = section;
            }
            let source = source_map.get(path).unwrap_or(&ValueSource::Default);
            match self.tracked_value(path) {
                // Compact JSON encoding of the tracked scalar/string-array
                // types is also valid TOML.
                Some(value) => {
                    output.push_str(&format!("{key} = {value}  # from: {}\n", source.detail()));
                }
                None => {
                    output.push_str(&format!("# {key} = <unset>  # from: {}\n", source.detail()));
                }
            }
        }

        Ok(output)
    }

    /// Create a new config file with example values
    pub fn create_example(path: &std::path::Path) -> Result<(), ConfigError> {
        let example = r#"# Crucible CLI Configuration
# Location: ~/.config/crucible/config.toml

# Path to your Obsidian kiln
# Default: current directory
kiln_path = "/home/user/Documents/my-kiln"

# Additional directories to search for agent cards (optional)
# Paths can be absolute or relative to this config file location
# agent_directories = ["/home/user/shared-agents", "./docs/agents"]

# Default kiln for storing chat sessions (optional)
# When set, sessions are saved here instead of kiln_path
# session_kiln = "/home/user/Documents/my-sessions"

# LLM provider configuration
[llm]
default = "local"

[llm.providers.local]
type = "ollama"
default_model = "llama3.2"
endpoint = "http://localhost:11434"

# ACP (Agent Client Protocol) configuration
[acp]
default_agent = null
enable_discovery = true
session_timeout_minutes = 30
max_message_size_mb = 25

# Chat configuration
[chat]
model = null
enable_markdown = true

# CLI configuration
[cli]
show_progress = true
confirm_destructive = true
verbose = false

# Logging configuration (optional)
# If not set, defaults to "off" unless --verbose or --log-level is specified
# [logging]
# level = "info"  # off, error, warn, info, debug, trace

# Processing configuration (optional)
# [processing]
# parallel_workers = 4  # Number of parallel workers (default: num_cpus / 2)
"#;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(path, example)?;
        Ok(())
    }

    // Legacy compatibility methods
    #[allow(missing_docs)]
    pub fn chat_model(&self) -> String {
        self.chat
            .model
            .clone()
            .unwrap_or_else(|| "llama3.2".to_string())
    }

    #[allow(missing_docs)]
    pub fn temperature(&self) -> f32 {
        crate::config::components::defaults::DEFAULT_TEMPERATURE
    }

    #[allow(missing_docs)]
    pub fn max_tokens(&self) -> u32 {
        crate::config::components::defaults::DEFAULT_CHAT_MAX_TOKENS
    }

    #[allow(missing_docs)]
    pub fn streaming(&self) -> bool {
        true // Default streaming
    }

    #[allow(missing_docs)]
    /// Minimal fallback — the real default is set in Lua init.lua
    pub fn system_prompt(&self) -> String {
        "Answer from the notes and context provided to you. If information isn't in your context, say so — do not fabricate. Be brief.".to_string()
    }

    #[allow(missing_docs)]
    pub fn ollama_endpoint(&self) -> String {
        "http://localhost:11434".to_string()
    }

    #[allow(missing_docs)]
    pub fn timeout(&self) -> u64 {
        30 // Default timeout
    }

    #[allow(missing_docs)]
    pub fn openai_api_key(&self) -> Option<String> {
        std::env::var("OPENAI_API_KEY").ok()
    }

    #[allow(missing_docs)]
    pub fn anthropic_api_key(&self) -> Option<String> {
        std::env::var("ANTHROPIC_API_KEY").ok()
    }

    /// Get the default config file path
    ///
    /// Uses platform-appropriate directories:
    /// - Linux: `~/.config/crucible/config.toml` (XDG Base Directory)
    /// - macOS: `~/Library/Application Support/crucible/config.toml`
    /// - Windows: `%APPDATA%\crucible\config.toml` (Roaming AppData)
    pub fn default_config_path() -> std::path::PathBuf {
        // Allow overriding config directory via environment variable
        // This is crucial for test isolation and custom setups
        if let Ok(config_dir) = std::env::var("CRUCIBLE_CONFIG_DIR") {
            return std::path::PathBuf::from(config_dir).join("config.toml");
        }

        // Use platform-appropriate config directory
        // dirs::config_dir() returns:
        // - Windows: %APPDATA% (Roaming AppData)
        // - Linux: ~/.config (XDG Base Directory)
        // - macOS: ~/Library/Application Support
        if let Some(config_dir) = dirs::config_dir() {
            return config_dir.join("crucible").join("config.toml");
        }

        // Fallback: Use home directory with .config subdirectory
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        home.join(".config").join("crucible").join("config.toml")
    }

    /// Get the logging level from config, if set
    ///
    /// Returns the log level string (e.g., "off", "error", "warn", "info", "debug", "trace")
    /// from the logging configuration section, or None if not configured.
    pub fn logging_level(&self) -> Option<String> {
        self.logging.as_ref().map(|l| l.level.clone())
    }

    /// Get the parallel workers setting from config, if set
    ///
    /// Returns the number of parallel workers for processing, or None if not configured.
    /// When None, the CLI should use a default (e.g., num_cpus / 2).
    pub fn parallel_workers(&self) -> Option<usize> {
        self.processing.parallel_workers
    }

    /// Get the effective LLM provider for chat.
    pub fn effective_llm_provider(&self) -> Result<EffectiveLlmConfig, ConfigError> {
        if let Some((key, provider)) = self.llm.default_provider() {
            return Ok(EffectiveLlmConfig {
                key: key.clone(),
                provider_type: provider.provider_type,
                endpoint: provider.endpoint(),
                model: provider.model(),
                temperature: provider.temperature(),
                max_tokens: provider.max_tokens(),
                timeout_secs: provider.timeout_secs(),
                api_key: provider.api_key(),
            });
        }

        Err(ConfigError::MissingValue {
            field: "llm.default".to_string(),
        })
    }

    /// Returns the effective kilns map, synthesizing from `kiln_path` if `kilns` is empty.
    pub fn resolved_kilns(&self) -> HashMap<String, crate::config::config::registry::KilnEntry> {
        if !self.kilns.is_empty() {
            return self.kilns.clone();
        }
        let mut map = HashMap::new();
        map.insert(
            "default".to_string(),
            crate::config::config::registry::KilnEntry::Path(self.kiln_path.clone()),
        );
        map
    }

    /// Returns the effective default kiln name.
    pub fn resolved_default_kiln(&self) -> String {
        if let Some(ref name) = self.default_kiln {
            return name.clone();
        }
        if !self.kilns.is_empty() {
            // First alphabetically
            return self.kilns.keys().min().cloned().unwrap_or_default();
        }
        "default".to_string()
    }

    /// Returns the kiln path where sessions should be stored.
    ///
    /// Uses `session_kiln` if explicitly set, otherwise `crucible_home()` (~/.crucible).
    /// Sessions are user-scoped — they shouldn't be scattered across kiln directories.
    pub fn session_storage_path(&self) -> std::path::PathBuf {
        self.session_kiln
            .clone()
            .unwrap_or_else(super::crucible_home)
    }
}

/// Register a project in the global config file.
///
/// Reads the existing config (or creates a default), inserts a `ProjectEntry`
/// under `[projects.<name>]`, and writes the file back with `toml::to_string_pretty`.
#[cfg(feature = "toml")]
pub fn register_project_in_config(
    config_path: &std::path::Path,
    name: &str,
    project_path: &std::path::Path,
    kilns: &[&str],
    default_kiln: Option<&str>,
) -> anyhow::Result<()> {
    let mut config: CliAppConfig = if config_path.exists() {
        let contents = std::fs::read_to_string(config_path)?;
        toml::from_str(&contents)?
    } else {
        CliAppConfig::default()
    };

    config.projects.insert(
        name.to_string(),
        crate::config::config::registry::ProjectEntry {
            path: project_path.to_path_buf(),
            kilns: kilns.iter().map(|s| s.to_string()).collect(),
            default_kiln: default_kiln.map(|s| s.to_string()),
        },
    );

    let contents = toml::to_string_pretty(&config)?;
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(config_path, contents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::EnvVarGuard;

    use tempfile::NamedTempFile;

    /// A config with every Option-typed tracked field populated, so coverage
    /// tests exercise the set-value path for all of TRACKED_FIELDS.
    fn fully_populated_config() -> CliAppConfig {
        let mut config = CliAppConfig::default();
        config.session_kiln = Some("/tmp/sessions".into());
        config.agent_directories = vec!["/tmp/agents".into()];
        config.llm.default = Some("test-provider".into());
        config.acp.default_agent = Some("claude".into());
        config.chat.model = Some("test-model".into());
        config.chat.endpoint = Some("http://localhost:11434".into());
        config.chat.temperature = Some(0.7);
        config.chat.max_tokens = Some(2048);
        config.chat.timeout_secs = Some(30);
        config.logging = Some(LoggingConfig::default());
        config.processing.parallel_workers = Some(4);
        config
    }

    #[test]
    fn tracked_value_covers_every_tracked_field() {
        let config = fully_populated_config();
        for (path, _) in TRACKED_FIELDS {
            assert!(
                config.tracked_value(path).is_some(),
                "tracked_value has no accessor for TRACKED_FIELDS entry {path:?}"
            );
        }
    }

    #[test]
    fn json_with_sources_covers_every_tracked_field() {
        let config = fully_populated_config();
        let json = config.display_as_json_with_sources().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        for (path, _) in TRACKED_FIELDS {
            let item = match path.split_once('.') {
                None => parsed.get(path),
                Some((section, key)) => parsed.get(section).and_then(|s| s.get(key)),
            };
            let item = item.unwrap_or_else(|| panic!("{path} missing from JSON trace output"));
            assert!(
                item.get("source").is_some(),
                "{path} has no source annotation"
            );
        }
    }

    #[test]
    fn toml_with_sources_covers_every_tracked_field() {
        let config = fully_populated_config();
        let toml = config.display_as_toml_with_sources().unwrap();
        for (path, _) in TRACKED_FIELDS {
            let key = match path.split_once('.') {
                None => path,
                Some((_, key)) => key,
            };
            assert!(
                toml.contains(&format!("{key} = ")),
                "{path} missing from TOML trace output:\n{toml}"
            );
        }
    }

    #[test]
    fn with_sources_render_unset_fields_instead_of_omitting() {
        let config = CliAppConfig::default();

        let json = config.display_as_json_with_sources().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let model = parsed
            .get("chat")
            .and_then(|c| c.get("model"))
            .expect("unset chat.model should still appear in JSON trace");
        assert!(model.get("value").unwrap().is_null());

        let toml = config.display_as_toml_with_sources().unwrap();
        assert!(
            toml.contains("# model = <unset>"),
            "unset chat.model should render as a comment in TOML trace:\n{toml}"
        );
    }

    #[test]
    fn test_load_resolves_env_var_in_api_key() {
        // Create a temporary config file with {env:VAR} in api_key
        let mut temp_file = NamedTempFile::new().unwrap();
        let config_content = r#"
[llm]
default = "test-provider"

[llm.providers.test-provider]
type = "openai"
api_key = "{env:CRUCIBLE_TEST_KEY_12345}"
"#;
        use std::io::Write;
        temp_file.write_all(config_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        // Set the environment variable
        let _guard = EnvVarGuard::set("CRUCIBLE_TEST_KEY_12345", "test-resolved-value".to_string());

        // Load the config
        let config = CliAppConfig::load(Some(temp_file.path().to_path_buf()), None, None).unwrap();

        // Assert that the api_key was resolved
        let provider = config.llm.providers.get("test-provider").unwrap();
        assert_eq!(provider.api_key.as_deref(), Some("test-resolved-value"));
    }

    #[test]
    fn test_load_missing_env_var_warns_not_crashes() {
        // Create a temporary config file with a non-existent env var
        let mut temp_file = NamedTempFile::new().unwrap();
        let config_content = r#"
[llm]
default = "test-provider"

[llm.providers.test-provider]
type = "openai"
api_key = "{env:CRUCIBLE_NONEXISTENT_VAR_XYZ_12345}"
"#;
        use std::io::Write;
        temp_file.write_all(config_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        // Ensure the env var is NOT set
        std::env::remove_var("CRUCIBLE_NONEXISTENT_VAR_XYZ_12345");

        // Load the config — should succeed (not crash)
        let result = CliAppConfig::load(Some(temp_file.path().to_path_buf()), None, None);
        assert!(
            result.is_ok(),
            "Config load should succeed even with missing env var"
        );

        let config = result.unwrap();
        let provider = config.llm.providers.get("test-provider").unwrap();
        // The api_key should either be unresolved or None
        // (depends on how process_file_references handles missing vars)
        // The important thing is that load succeeded
        assert!(
            provider.api_key.is_none()
                || provider.api_key.as_deref() == Some("{env:CRUCIBLE_NONEXISTENT_VAR_XYZ_12345}")
        );
    }

    #[test]
    fn test_detect_present_fields_unaffected_by_env_resolution() {
        // Create a temporary config file with both a field and an env var
        let mut temp_file = NamedTempFile::new().unwrap();
        let config_content = r#"
[llm]
default = "my-provider"

[llm.providers.my-provider]
type = "openai"
api_key = "{env:SOME_VAR}"
"#;
        use std::io::Write;
        temp_file.write_all(config_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        // Set the env var so load succeeds
        let _guard = EnvVarGuard::set("SOME_VAR", "test-value".to_string());

        // Load the config
        let config = CliAppConfig::load(Some(temp_file.path().to_path_buf()), None, None).unwrap();

        // Assert that the llm.default field is present and correct
        // This verifies that detect_present_fields ran correctly before env resolution
        assert_eq!(config.llm.default, Some("my-provider".to_string()));

        // Also verify the provider exists and api_key was resolved
        let provider = config.llm.providers.get("my-provider").unwrap();
        assert_eq!(provider.api_key.as_deref(), Some("test-value"));
    }

    #[test]
    fn cli_app_config_deserializes_kilns_and_projects() {
        let toml_str = r#"
kiln_path = "~/vault"
default_kiln = "vault"

[kilns]
vault = "~/vault"
docs = "~/crucible/docs"

[kilns.work]
path = "~/work/notes"
lazy = true

[projects.crucible]
path = "~/crucible"
kilns = ["docs", "vault"]
default_kiln = "vault"
"#;
        let config: CliAppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.kilns.len(), 3);
        assert_eq!(config.projects.len(), 1);
        assert_eq!(config.default_kiln.as_deref(), Some("vault"));
        assert!(config.kilns["work"].lazy());
    }

    #[test]
    fn cli_app_config_empty_kilns_defaults() {
        let toml_str = r#"kiln_path = "~/vault""#;
        let config: CliAppConfig = toml::from_str(toml_str).unwrap();
        assert!(config.kilns.is_empty());
        assert!(config.projects.is_empty());
    }

    #[test]
    fn resolved_kilns_falls_back_to_kiln_path() {
        use std::path::PathBuf;
        let toml_str = r#"kiln_path = "~/vault""#;
        let config: CliAppConfig = toml::from_str(toml_str).unwrap();
        let resolved = config.resolved_kilns();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved["default"].path(), PathBuf::from("~/vault"));
    }

    #[test]
    fn resolved_kilns_uses_explicit_kilns_when_present() {
        let toml_str = r#"
kiln_path = "~/old"
[kilns]
vault = "~/vault"
docs = "~/docs"
"#;
        let config: CliAppConfig = toml::from_str(toml_str).unwrap();
        let resolved = config.resolved_kilns();
        assert_eq!(resolved.len(), 2);
        assert!(resolved.contains_key("vault"));
        assert!(resolved.contains_key("docs"));
    }

    #[test]
    fn resolved_default_kiln_name() {
        let toml_str = r#"
default_kiln = "vault"
[kilns]
vault = "~/vault"
docs = "~/docs"
"#;
        let config: CliAppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.resolved_default_kiln(), "vault");
    }

    #[test]
    fn resolved_default_kiln_falls_back() {
        let toml_str = r#"kiln_path = "~/vault""#;
        let config: CliAppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.resolved_default_kiln(), "default");
    }

    #[test]
    fn register_project_writes_to_config_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(
            &config_path,
            "kiln_path = \"~/vault\"\n\n[kilns]\nvault = \"~/vault\"\n",
        )
        .unwrap();

        register_project_in_config(
            &config_path,
            "myproject",
            tmp.path(),
            &["vault"],
            Some("vault"),
        )
        .unwrap();

        let contents = std::fs::read_to_string(&config_path).unwrap();
        let config: CliAppConfig = toml::from_str(&contents).unwrap();
        assert_eq!(config.projects.len(), 1);
        assert_eq!(config.projects["myproject"].kilns, vec!["vault"]);
        assert_eq!(
            config.projects["myproject"].default_kiln.as_deref(),
            Some("vault")
        );
        assert_eq!(config.projects["myproject"].path, tmp.path().to_path_buf());
    }

    #[test]
    fn register_project_creates_config_if_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("subdir").join("config.toml");

        register_project_in_config(&config_path, "newproj", tmp.path(), &[], None).unwrap();

        let contents = std::fs::read_to_string(&config_path).unwrap();
        let config: CliAppConfig = toml::from_str(&contents).unwrap();
        assert_eq!(config.projects.len(), 1);
        assert!(config.projects.contains_key("newproj"));
        assert!(config.projects["newproj"].kilns.is_empty());
        assert!(config.projects["newproj"].default_kiln.is_none());
    }

    #[test]
    fn register_project_preserves_existing_projects() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, "kiln_path = \"~/vault\"\n").unwrap();

        register_project_in_config(&config_path, "proj1", tmp.path(), &["vault"], None).unwrap();
        register_project_in_config(
            &config_path,
            "proj2",
            &tmp.path().join("other"),
            &["docs"],
            Some("docs"),
        )
        .unwrap();

        let contents = std::fs::read_to_string(&config_path).unwrap();
        let config: CliAppConfig = toml::from_str(&contents).unwrap();
        assert_eq!(config.projects.len(), 2);
        assert!(config.projects.contains_key("proj1"));
        assert!(config.projects.contains_key("proj2"));
    }
}
