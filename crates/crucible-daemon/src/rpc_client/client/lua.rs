//! Lua plugin RPC methods
//!
//! Methods for managing Lua plugins, hooks, and plugin lifecycle.

use anyhow::Result;

use super::DaemonClient;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaInitSessionRequest {
    pub session_id: String,
    /// Optional because the handler treats it as optional: an absent path falls
    /// back to the daemon's data root. It was a required `String` while the
    /// server read it with `optional_param!`, so the type claimed a guarantee
    /// the wire never had.
    ///
    /// `kiln` is accepted as an alias. No in-tree caller sends it, but
    /// `lua.init_session` is a public RPC method and the server has always
    /// honoured both spellings — the alias records that here instead of leaving
    /// it as an undocumented second name in the handler.
    #[serde(default, alias = "kiln", skip_serializing_if = "Option::is_none")]
    pub kiln_path: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaInitSessionResponse {
    pub session_id: String,
    #[serde(default)]
    pub commands: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaShutdownSessionRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaShutdownSessionResponse {
    pub shutdown: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaDiscoverPluginsRequest {
    pub kiln_path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaDiscoverPluginsResponse {
    #[serde(default)]
    pub plugins: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaPluginHealthRequest {
    pub plugin_path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaPluginHealthResponse {
    pub name: String,
    pub healthy: bool,
    #[serde(default)]
    pub checks: Vec<serde_json::Value>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaGenerateStubsRequest {
    pub output_dir: String,
    #[serde(default)]
    pub verify: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaGenerateStubsResponse {
    pub status: String,
    pub path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRunPluginTestsRequest {
    pub test_path: String,
    #[serde(default)]
    pub filter: Option<String>,
}

/// A single failed Lua test, as the plugin test runner saw it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginTestFailure {
    pub name: String,
    /// Full `describe` path. The bare test name is ambiguous across suites.
    #[serde(default)]
    pub suite: Option<String>,
    pub error: String,
    /// Test file and line the assertion fired on, taken from the location Lua
    /// prefixes onto a level-2 `error()` — not from `debug.traceback()`, whose
    /// innermost frame is always inside the runner.
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub line: Option<String>,
}

/// A test file that could not be read or parsed at all.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginTestLoadFailure {
    pub file: String,
    pub error: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRunPluginTestsResponse {
    pub passed: usize,
    pub failed: usize,
    pub load_failures: usize,
    /// Per-test diagnostics. Without these the caller sees only counts — the
    /// runner's own `✗` output goes to the daemon's stdout, not the client's.
    #[serde(default)]
    pub failures: Vec<PluginTestFailure>,
    #[serde(default)]
    pub load_failure_details: Vec<PluginTestLoadFailure>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRegisterCommandsRequest {
    pub session_id: String,
    pub commands: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRegisterCommandsResponse {
    pub registered: usize,
}

impl DaemonClient {
    pub async fn lua_init_session(
        &self,
        params: LuaInitSessionRequest,
    ) -> Result<LuaInitSessionResponse> {
        self.typed_call("lua.init_session", params).await
    }

    pub async fn lua_shutdown_session(
        &self,
        params: LuaShutdownSessionRequest,
    ) -> Result<LuaShutdownSessionResponse> {
        self.typed_call("lua.shutdown_session", params).await
    }

    // =========================================================================
    // Lua Plugin Management RPC Methods
    // =========================================================================

    /// Discover plugins from a kiln path.
    pub async fn lua_discover_plugins(
        &self,
        params: LuaDiscoverPluginsRequest,
    ) -> Result<LuaDiscoverPluginsResponse> {
        self.typed_call("lua.discover_plugins", params).await
    }

    /// Run health checks for a plugin.
    pub async fn lua_plugin_health(
        &self,
        params: LuaPluginHealthRequest,
    ) -> Result<LuaPluginHealthResponse> {
        self.typed_call("lua.plugin_health", params).await
    }

    /// Generate or verify Lua type stubs.
    pub async fn lua_generate_stubs(
        &self,
        params: LuaGenerateStubsRequest,
    ) -> Result<LuaGenerateStubsResponse> {
        self.typed_call("lua.generate_stubs", params).await
    }

    /// Run plugin test files.
    pub async fn lua_run_plugin_tests(
        &self,
        params: LuaRunPluginTestsRequest,
    ) -> Result<LuaRunPluginTestsResponse> {
        self.typed_call("lua.run_plugin_tests", params).await
    }

    /// Register Lua commands in a session.
    pub async fn lua_register_commands(
        &self,
        params: LuaRegisterCommandsRequest,
    ) -> Result<LuaRegisterCommandsResponse> {
        self.typed_call("lua.register_commands", params).await
    }
}
