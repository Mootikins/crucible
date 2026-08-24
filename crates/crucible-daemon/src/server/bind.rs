//! How a [`Server`] is asked to bind: the parameter struct and the two
//! convenience constructors that fill it in.
//!
//! Split out of `server/mod.rs` because the struct and its two literal-by-
//! literal call sites were a third of that file's declarations and none of its
//! behaviour. `bind_with_plugin_config`, which consumes these, stays with the
//! wiring it performs.

use super::Server;
use anyhow::Result;
use std::path::Path;

/// Parameters for binding the server to a Unix socket with plugin configuration.
pub struct BindWithPluginConfigParams {
    pub path: std::path::PathBuf,
    pub mcp_config: Option<crucible_core::config::McpConfig>,
    pub plugin_config: std::collections::HashMap<String, serde_json::Value>,
    pub runtimepath: Vec<std::path::PathBuf>,
    pub plugin_watch: bool,
    pub auto_archive_hours: Option<u64>,
    pub llm_config: Option<crucible_core::config::LlmConfig>,
    pub enrichment_config: Option<crucible_core::config::EmbeddingProviderConfig>,
    pub max_precognition_chars: usize,
    pub acp_config: Option<crucible_core::config::components::acp::AcpConfig>,
    pub context_config: Option<crucible_core::config::ContextConfig>,
    pub permission_config: Option<crucible_core::config::components::permissions::PermissionConfig>,
    pub schedules: Vec<crucible_core::config::ScheduleEntry>,
    /// Full loaded app config as JSON — seeds the Lua `cru.config` store
    /// before init.lua runs (TOML seeds, Lua overrides, RPC merges).
    pub app_config: Option<serde_json::Value>,
    /// Daemon data root — registry (`projects.json`), default session storage,
    /// the home kiln, logs. `None` resolves to `crucible_home()` (the
    /// `$CRUCIBLE_HOME`/`~/.crucible` default). Injected as a TempDir in tests so
    /// the in-process daemon never reads the developer's real `~/.crucible`.
    pub data_home: Option<std::path::PathBuf>,
    /// Root the global agent-card directory (`<config_home>/crucible/agents`)
    /// hangs off. `None` resolves to `dirs::config_dir()`. Injected in tests so
    /// an in-process daemon never resolves the developer's personal cards —
    /// they are first in discovery precedence, so they would shadow a
    /// fixture's.
    pub config_home: Option<std::path::PathBuf>,
}

impl BindWithPluginConfigParams {
    /// Every daemon knob the loaded app config decides, in one place.
    ///
    /// Two call sites build these params: `cru daemon serve` and the
    /// standalone daemon in `main.rs`. They were literal-by-literal copies and
    /// they drifted: the standalone one passed `None` for `permission_config`,
    /// so `cru --standalone` ran with none of the user's permission rules, and
    /// BOTH passed `None` for `mcp_config`, so the MCP gateway never started
    /// outside a test. A caller now names only what the config cannot decide:
    /// the socket, the split plugin sections and the watch flag.
    pub fn from_app_config(
        path: std::path::PathBuf,
        config: &crucible_core::config::CliAppConfig,
        plugin_config: std::collections::HashMap<String, serde_json::Value>,
        plugin_watch: bool,
    ) -> Self {
        Self {
            path,
            mcp_config: config.mcp.clone(),
            plugin_config,
            runtimepath: config.runtimepath.clone(),
            plugin_watch,
            auto_archive_hours: config.server.as_ref().and_then(|s| s.auto_archive_hours),
            llm_config: Some(config.llm.clone()),
            enrichment_config: config.enrichment.as_ref().map(|e| e.provider.clone()),
            max_precognition_chars: config
                .enrichment
                .as_ref()
                .map(|e| e.pipeline.max_precognition_chars)
                .unwrap_or_else(crucible_core::config::default_max_precognition_chars),
            acp_config: Some(config.acp.clone()),
            context_config: config.context.clone(),
            permission_config: config.permissions.clone(),
            schedules: config.schedules.clone(),
            app_config: serde_json::to_value(config).ok(),
            data_home: config.data_home.clone(),
            config_home: None,
        }
    }
}

impl Default for BindWithPluginConfigParams {
    /// Every field off/absent, so a constructor spells out only what it
    /// changes. Not derived: `max_precognition_chars` defaults to the config
    /// crate's value, not zero.
    fn default() -> Self {
        Self {
            path: std::path::PathBuf::new(),
            mcp_config: None,
            plugin_config: std::collections::HashMap::new(),
            runtimepath: Vec::new(),
            plugin_watch: false,
            auto_archive_hours: None,
            llm_config: None,
            enrichment_config: None,
            max_precognition_chars: crucible_core::config::default_max_precognition_chars(),
            acp_config: None,
            context_config: None,
            permission_config: None,
            schedules: Vec::new(),
            app_config: None,
            data_home: None,
            config_home: None,
        }
    }
}

impl Server {
    /// Test constructor: bind with an isolated data root injected as a value
    /// (no `CRUCIBLE_HOME` env mutation). The daemon reads registry, sessions,
    /// and the home kiln from `data_home` instead of the developer's real
    /// `~/.crucible`.
    ///
    /// Session storage honors this too: `FileSessionStorage` is rooted at
    /// `{data_home}/sessions` with no process-global read on the path, so a
    /// test's sessions land under the injected root in exactly the layout
    /// production uses.
    #[allow(dead_code)] // used by in-process integration-test fixtures
    pub async fn bind_with_data_home(path: &Path, data_home: std::path::PathBuf) -> Result<Self> {
        Self::bind_with_data_home_and_kilns(path, data_home, &[]).await
    }

    /// [`Self::bind_with_data_home`], with `[kilns]` entries in the app config
    /// the daemon is handed.
    ///
    /// Kilns are addressed by NAME across the RPC surface, and the registry is
    /// built from `params.app_config` — so a fixture that binds without one has
    /// no kilns at all and every scoped request it makes is refused. This is
    /// how a test says "the daemon knows about this directory", through exactly
    /// the config path production uses.
    #[allow(dead_code)] // used by in-process integration-test fixtures
    pub async fn bind_with_data_home_and_kilns(
        path: &Path,
        data_home: std::path::PathBuf,
        kilns: &[(&str, &Path)],
    ) -> Result<Self> {
        let entries: serde_json::Map<String, serde_json::Value> = kilns
            .iter()
            .map(|(name, dir)| {
                (
                    (*name).to_string(),
                    serde_json::Value::String(dir.to_string_lossy().into_owned()),
                )
            })
            .collect();
        Self::bind_with_plugin_config(BindWithPluginConfigParams {
            path: path.to_path_buf(),
            // Under the isolated root too: agent-card discovery must not reach
            // the developer's `~/.config/crucible/agents/`, which outranks
            // every fixture card. The directory usually does not exist, which
            // is exactly the intended "no global cards".
            config_home: Some(data_home.join("config")),
            data_home: Some(data_home),
            app_config: Some(serde_json::Value::Object(
                [("kilns".to_string(), serde_json::Value::Object(entries))]
                    .into_iter()
                    .collect(),
            )),
            ..Default::default()
        })
        .await
    }
}
