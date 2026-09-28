//! Lua plugin RPC methods
//!
//! Methods for managing Lua plugins, hooks, and plugin lifecycle.

use anyhow::Result;
use crucible_core::protocol::requests::*;
use crucible_core::protocol::RpcMethod;

use super::DaemonClient;

impl DaemonClient {
    pub async fn lua_init_session(
        &self,
        params: LuaInitSessionRequest,
    ) -> Result<LuaInitSessionResponse> {
        self.typed_call(RpcMethod::LuaInitSession, params).await
    }

    pub async fn lua_shutdown_session(
        &self,
        params: LuaShutdownSessionRequest,
    ) -> Result<LuaShutdownSessionResponse> {
        self.typed_call(RpcMethod::LuaShutdownSession, params).await
    }

    // =========================================================================
    // Lua Plugin Management RPC Methods
    // =========================================================================

    /// Discover plugins from a kiln path.
    pub async fn lua_discover_plugins(
        &self,
        params: LuaDiscoverPluginsRequest,
    ) -> Result<LuaDiscoverPluginsResponse> {
        self.typed_call(RpcMethod::LuaDiscoverPlugins, params).await
    }

    /// Run health checks for a plugin.
    pub async fn lua_plugin_health(
        &self,
        params: LuaPluginHealthRequest,
    ) -> Result<LuaPluginHealthResponse> {
        self.typed_call(RpcMethod::LuaPluginHealth, params).await
    }

    /// Generate or verify Lua type stubs.
    pub async fn lua_generate_stubs(
        &self,
        params: LuaGenerateStubsRequest,
    ) -> Result<LuaGenerateStubsResponse> {
        self.typed_call(RpcMethod::LuaGenerateStubs, params).await
    }

    /// Run plugin test files.
    pub async fn lua_run_plugin_tests(
        &self,
        params: LuaRunPluginTestsRequest,
    ) -> Result<LuaRunPluginTestsResponse> {
        self.typed_call(RpcMethod::LuaRunPluginTests, params).await
    }
}
