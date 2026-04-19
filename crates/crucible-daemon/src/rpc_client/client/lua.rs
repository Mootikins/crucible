//! Lua plugin RPC methods
//!
//! Methods for managing Lua plugins, hooks, and plugin lifecycle.

use anyhow::Result;

use super::DaemonClient;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaInitSessionRequest {
    pub session_id: String,
    pub kiln_path: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaInitSessionResponse {
    pub session_id: String,
    #[serde(default)]
    pub commands: Vec<serde_json::Value>,
    #[serde(default)]
    pub views: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRegisterHooksRequest {
    pub session_id: String,
    pub hooks: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRegisterHooksResponse {
    pub status: String,
    pub registered: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaExecuteHookRequest {
    pub session_id: String,
    pub hook_name: String,
    #[serde(default)]
    pub context: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaExecuteHookResponse {
    pub executed: usize,
    #[serde(default)]
    pub results: Vec<serde_json::Value>,
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRunPluginTestsResponse {
    pub passed: usize,
    pub failed: usize,
    pub load_failures: usize,
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

    pub async fn lua_register_hooks(
        &self,
        params: LuaRegisterHooksRequest,
    ) -> Result<LuaRegisterHooksResponse> {
        self.typed_call("lua.register_hooks", params).await
    }

    pub async fn lua_execute_hook(
        &self,
        params: LuaExecuteHookRequest,
    ) -> Result<LuaExecuteHookResponse> {
        self.typed_call("lua.execute_hook", params).await
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
