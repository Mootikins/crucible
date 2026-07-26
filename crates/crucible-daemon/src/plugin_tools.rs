//! Plugin-declared tools and commands, made reachable.
//!
//! A Lua plugin's returned spec table can declare `tools` and `commands`, each
//! with a `fn`. The spec sandbox ([`crucible_lua::load_plugin_spec`]) reads the
//! *metadata* (name, description, params) from a throwaway VM; the callable
//! `fn` only exists in the daemon's plugin VM. [`PluginRegistry`] pairs the two
//! and hands them to the two consumers that matter:
//!
//! - [`PluginToolExecutor`] — a `ToolExecutor` provider so the agent's tool
//!   dispatcher can actually invoke a plugin tool.
//! - the `plugin.commands` / `plugin.run_command` RPCs — so TUI and web can
//!   list and run plugin commands.

use async_trait::async_trait;
use crucible_core::traits::tools::{
    ExecutionContext, ToolDefinition, ToolError, ToolExecutor, ToolResult,
};
use crucible_lua::{
    discovered_params_to_json_schema, json_to_lua, lua_to_json, DiscoveredCommand, DiscoveredTool,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::warn;

/// A callable declared by a plugin: the metadata the spec sandbox extracted
/// plus the live `mlua::Function` from the daemon's plugin VM.
struct PluginCallable {
    plugin: String,
    definition: ToolDefinition,
    input_hint: Option<String>,
    /// The VM the function belongs to. Held alongside the function because
    /// JSON↔Lua conversion must happen in the *same* state, and a `Function`
    /// alone doesn't hand its `Lua` back.
    lua: mlua::Lua,
    func: mlua::Function,
}

/// Tools and commands contributed by loaded plugins.
///
/// Shared by `Arc` between the plugin loader (which populates it on load and
/// reload), the agent's tool dispatcher, and the plugin RPC handlers.
#[derive(Default)]
pub struct PluginRegistry {
    tools: RwLock<HashMap<String, PluginCallable>>,
    commands: RwLock<HashMap<String, PluginCallable>>,
}

/// Names a plugin may not claim.
///
/// **Collision policy: built-ins always win, and a colliding plugin name is
/// rejected outright rather than silently shadowed or silently ignored.**
///
/// Rejecting is the only honest option. Letting a plugin shadow `bash` or
/// `read_file` would silently redirect every agent's core toolchain through
/// third-party Lua — a privilege-escalation seam. Registering the plugin tool
/// anyway and losing every dispatch to the built-in (providers are ordered,
/// built-ins first) would advertise a tool to the model that can never run.
/// So: refuse, and log loudly enough that the plugin author renames.
///
/// Plugin-vs-plugin collisions resolve first-load-wins, likewise with a warning
/// — load order is the only tiebreaker available and neither plugin has a claim
/// on the name.
fn builtin_tool_names() -> std::collections::HashSet<String> {
    // A throwaway server just to enumerate the kiln MCP tool names; the empty
    // providers are never called because we only read the catalog.
    let mcp = crate::tools::mcp_server::CrucibleMcpServer::new(
        String::new(),
        Arc::new(crate::empty_providers::EmptyKnowledgeRepository),
        Arc::new(crate::empty_providers::EmptyEmbeddingProvider),
    );
    mcp.list_tools()
        .into_iter()
        .map(|t| t.name.to_string())
        .chain(
            crate::tools::workspace::WorkspaceTools::tool_definitions()
                .into_iter()
                .map(|t| t.name.to_string()),
        )
        .chain(
            [
                // `list_tools` hides `delegate_session` when delegation is off,
                // but the name is still ours in every session that enables it.
                "delegate_session",
                // The progressive-disclosure bridge is handled inside the
                // dispatcher itself and never reaches a provider, so a plugin
                // claiming these names would be unreachable by construction.
                "discover_tools",
                "get_tool_schema",
                "invoke_tool",
            ]
            .into_iter()
            .map(String::from),
        )
        .collect()
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace everything registered by `plugin`, then register the given
    /// tools/commands. Idempotent so plugin reload doesn't accumulate stale
    /// `mlua::Function` handles pointing at the previous module instance.
    ///
    /// `funcs` maps declared name → the function extracted from the daemon VM.
    /// A declaration with no matching function is skipped: the plugin declared
    /// a tool it did not export, and advertising it would produce a tool call
    /// that always fails.
    /// Remove every tool and command a plugin registered.
    ///
    /// The failed-reload path: `register_plugin` only replaces entries on a
    /// successful load, so a plugin that broke on reload otherwise kept its
    /// previous version's tools registered while `plugin.list` said Error.
    pub fn remove_plugin(&self, plugin: &str) {
        self.tools
            .write()
            .expect("plugin tools lock poisoned")
            .retain(|_, entry| entry.plugin != plugin);
        self.commands
            .write()
            .expect("plugin commands lock poisoned")
            .retain(|_, entry| entry.plugin != plugin);
    }

    pub fn register_plugin(
        &self,
        plugin: &str,
        lua: &mlua::Lua,
        tools: &[DiscoveredTool],
        commands: &[DiscoveredCommand],
        tool_funcs: HashMap<String, mlua::Function>,
        command_funcs: HashMap<String, mlua::Function>,
    ) {
        let builtins = builtin_tool_names();
        let mut tool_map = self.tools.write().expect("plugin tools lock poisoned");
        tool_map.retain(|_, entry| entry.plugin != plugin);

        for tool in tools {
            if builtins.contains(&tool.name) {
                warn!(
                    plugin,
                    tool = %tool.name,
                    "Plugin tool rejected: name collides with a built-in tool. Rename it."
                );
                continue;
            }
            let Some(func) = tool_funcs.get(&tool.name) else {
                warn!(
                    plugin,
                    tool = %tool.name,
                    "Plugin declared a tool with no `fn`; not registered"
                );
                continue;
            };
            if let Some(existing) = tool_map.get(&tool.name) {
                warn!(
                    plugin,
                    tool = %tool.name,
                    owner = %existing.plugin,
                    "Plugin tool rejected: name already claimed by another plugin"
                );
                continue;
            }
            tool_map.insert(
                tool.name.clone(),
                PluginCallable {
                    plugin: plugin.to_string(),
                    definition: ToolDefinition {
                        name: tool.name.clone(),
                        description: tool.description.clone(),
                        category: Some("plugin".to_string()),
                        parameters: Some(discovered_params_to_json_schema(&tool.params)),
                        returns: None,
                        examples: vec![],
                        required_permissions: vec![],
                    },
                    input_hint: None,
                    lua: lua.clone(),
                    func: func.clone(),
                },
            );
        }
        drop(tool_map);

        // Commands are namespaced separately from tools: `/greet` and a `greet`
        // tool are different surfaces, and only tools share the model's name
        // space with built-ins.
        let mut command_map = self
            .commands
            .write()
            .expect("plugin commands lock poisoned");
        command_map.retain(|_, entry| entry.plugin != plugin);
        for command in commands {
            let Some(func) = command_funcs.get(&command.name) else {
                warn!(
                    plugin,
                    command = %command.name,
                    "Plugin declared a command with no `fn`; not registered"
                );
                continue;
            };
            if let Some(existing) = command_map.get(&command.name) {
                warn!(
                    plugin,
                    command = %command.name,
                    owner = %existing.plugin,
                    "Plugin command rejected: name already claimed by another plugin"
                );
                continue;
            }
            command_map.insert(
                command.name.clone(),
                PluginCallable {
                    plugin: plugin.to_string(),
                    definition: ToolDefinition {
                        name: command.name.clone(),
                        description: command.description.clone(),
                        category: Some("plugin".to_string()),
                        parameters: Some(discovered_params_to_json_schema(&command.params)),
                        returns: None,
                        examples: vec![],
                        required_permissions: vec![],
                    },
                    input_hint: command.input_hint.clone(),
                    lua: lua.clone(),
                    func: func.clone(),
                },
            );
        }
    }

    /// Names of every registered plugin tool. Feeds the plan-mode filter and
    /// dispatch guard, which exclude plugin tools categorically in plan.
    pub fn tool_names(&self) -> std::collections::HashSet<String> {
        let tools = self.tools.read().expect("plugin tools lock poisoned");
        tools.keys().cloned().collect()
    }

    /// Tool definitions for every registered plugin tool, sorted by name so the
    /// model-facing tool list is stable across daemon restarts.
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        let tools = self.tools.read().expect("plugin tools lock poisoned");
        let mut defs: Vec<ToolDefinition> = tools.values().map(|e| e.definition.clone()).collect();
        defs.sort_by(|a, b| a.name.cmp(&b.name));
        defs
    }

    /// Commands as JSON for RPC clients, sorted by name.
    pub fn commands_json(&self) -> Vec<serde_json::Value> {
        let commands = self.commands.read().expect("plugin commands lock poisoned");
        let mut out: Vec<serde_json::Value> = commands
            .values()
            .map(|entry| {
                serde_json::json!({
                    "plugin": entry.plugin,
                    "name": entry.definition.name,
                    "description": entry.definition.description,
                    "hint": entry.input_hint,
                    "parameters": entry.definition.parameters,
                })
            })
            .collect();
        out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        out
    }

    fn tool_func(&self, name: &str) -> Option<(mlua::Lua, mlua::Function)> {
        self.tools
            .read()
            .expect("plugin tools lock poisoned")
            .get(name)
            .map(|e| (e.lua.clone(), e.func.clone()))
    }

    fn command_func(&self, name: &str) -> Option<(mlua::Lua, mlua::Function)> {
        self.commands
            .read()
            .expect("plugin commands lock poisoned")
            .get(name)
            .map(|e| (e.lua.clone(), e.func.clone()))
    }

    /// Invoke a plugin command by name. `Ok(None)` means no such command.
    pub async fn run_command(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        let Some((lua, func)) = self.command_func(name) else {
            return Ok(None);
        };
        call_plugin_fn(&lua, &func, args)
            .await
            .map(Some)
            .map_err(|e| anyhow::anyhow!("plugin command '{name}': {e}"))
    }
}

/// Call a plugin `fn` with JSON args and convert its return value back to JSON.
async fn call_plugin_fn(
    lua: &mlua::Lua,
    func: &mlua::Function,
    args: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let lua_args =
        json_to_lua(lua, args).map_err(|e| anyhow::anyhow!("argument conversion: {e}"))?;
    let ret: mlua::Value = func
        .call_async(lua_args)
        .await
        .map_err(|e| anyhow::anyhow!("{}", crucible_lua::format_lua_error(None, &e)))?;
    lua_to_json(lua, ret).map_err(|e| anyhow::anyhow!("result conversion: {e}"))
}

/// Dispatcher provider for plugin-declared tools.
///
/// Registered *after* the built-in providers so a built-in always wins a
/// dispatch race; [`builtin_tool_names`] additionally refuses the collision at
/// registration time, so ordering here is belt-and-braces, not the policy.
pub struct PluginToolExecutor {
    registry: Arc<PluginRegistry>,
}

impl PluginToolExecutor {
    pub fn new(registry: Arc<PluginRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ToolExecutor for PluginToolExecutor {
    async fn execute_tool(
        &self,
        name: &str,
        params: serde_json::Value,
        _context: &ExecutionContext,
    ) -> ToolResult<serde_json::Value> {
        let Some((lua, func)) = self.registry.tool_func(name) else {
            return Err(ToolError::NotFound(name.to_string()));
        };
        call_plugin_fn(&lua, &func, params)
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }

    async fn list_tools(&self) -> ToolResult<Vec<ToolDefinition>> {
        Ok(self.registry.tool_definitions())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_lua::DiscoveredParam;

    fn tool(name: &str) -> DiscoveredTool {
        DiscoveredTool {
            name: name.to_string(),
            description: format!("{name} description"),
            params: vec![DiscoveredParam {
                name: "text".to_string(),
                param_type: "string".to_string(),
                description: "input".to_string(),
                optional: false,
            }],
            return_type: None,
            source_path: "test".to_string(),
            is_fennel: false,
        }
    }

    fn command(name: &str) -> DiscoveredCommand {
        DiscoveredCommand {
            name: name.to_string(),
            description: format!("{name} description"),
            params: vec![],
            input_hint: Some("[args]".to_string()),
            source_path: "test".to_string(),
            handler_fn: name.to_string(),
            is_fennel: false,
        }
    }

    fn lua_with_fn() -> (mlua::Lua, mlua::Function) {
        let lua = mlua::Lua::new();
        let func: mlua::Function = lua
            .load("return function(args) return { echoed = args.text } end")
            .eval()
            .expect("eval fn");
        (lua, func)
    }

    #[test]
    fn plugin_tool_colliding_with_builtin_is_rejected() {
        let (lua, func) = lua_with_fn();
        let registry = PluginRegistry::new();
        registry.register_plugin(
            "rogue",
            &lua,
            &[tool("bash"), tool("safe_name")],
            &[],
            HashMap::from([
                ("bash".to_string(), func.clone()),
                ("safe_name".to_string(), func),
            ]),
            HashMap::new(),
        );

        let names: Vec<String> = registry
            .tool_definitions()
            .into_iter()
            .map(|d| d.name)
            .collect();
        assert_eq!(
            names,
            vec!["safe_name".to_string()],
            "a plugin must not be able to shadow the built-in `bash`"
        );
    }

    #[test]
    fn first_plugin_to_claim_a_tool_name_keeps_it() {
        let (lua, func) = lua_with_fn();
        let registry = PluginRegistry::new();
        registry.register_plugin(
            "first",
            &lua,
            &[tool("shared")],
            &[],
            HashMap::from([("shared".to_string(), func.clone())]),
            HashMap::new(),
        );
        registry.register_plugin(
            "second",
            &lua,
            &[tool("shared")],
            &[],
            HashMap::from([("shared".to_string(), func)]),
            HashMap::new(),
        );

        assert_eq!(registry.tool_definitions().len(), 1);
    }

    #[test]
    fn declared_tool_without_a_fn_is_not_advertised() {
        let registry = PluginRegistry::new();
        let lua = mlua::Lua::new();
        registry.register_plugin(
            "empty",
            &lua,
            &[tool("ghost")],
            &[],
            HashMap::new(),
            HashMap::new(),
        );

        assert!(
            registry.tool_definitions().is_empty(),
            "a tool with no callable would always fail when the model calls it"
        );
    }

    #[test]
    fn reregistering_a_plugin_replaces_its_previous_entries() {
        let (lua, func) = lua_with_fn();
        let registry = PluginRegistry::new();
        registry.register_plugin(
            "p",
            &lua,
            &[tool("old")],
            &[],
            HashMap::from([("old".to_string(), func.clone())]),
            HashMap::new(),
        );
        registry.register_plugin(
            "p",
            &lua,
            &[tool("new")],
            &[],
            HashMap::from([("new".to_string(), func)]),
            HashMap::new(),
        );

        let names: Vec<String> = registry
            .tool_definitions()
            .into_iter()
            .map(|d| d.name)
            .collect();
        assert_eq!(names, vec!["new".to_string()]);
    }

    #[test]
    fn commands_are_listed_with_hint_and_schema() {
        let (lua, func) = lua_with_fn();
        let registry = PluginRegistry::new();
        registry.register_plugin(
            "p",
            &lua,
            &[],
            &[command("greet")],
            HashMap::new(),
            HashMap::from([("greet".to_string(), func)]),
        );

        let commands = registry.commands_json();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0]["name"], "greet");
        assert_eq!(commands[0]["plugin"], "p");
        assert_eq!(commands[0]["hint"], "[args]");
    }

    #[test]
    fn a_command_may_share_a_name_with_a_builtin_tool() {
        let (lua, func) = lua_with_fn();
        let registry = PluginRegistry::new();
        registry.register_plugin(
            "p",
            &lua,
            &[],
            &[command("bash")],
            HashMap::new(),
            HashMap::from([("bash".to_string(), func)]),
        );

        assert_eq!(
            registry.commands_json().len(),
            1,
            "commands are a separate surface from the model's tool namespace"
        );
    }

    #[tokio::test]
    async fn executor_runs_a_registered_tool_and_reports_unknown_ones() {
        let (lua, func) = lua_with_fn();
        let registry = Arc::new(PluginRegistry::new());
        registry.register_plugin(
            "p",
            &lua,
            &[tool("echo")],
            &[],
            HashMap::from([("echo".to_string(), func)]),
            HashMap::new(),
        );
        let executor = PluginToolExecutor::new(registry);
        let ctx = ExecutionContext::default();

        let value = executor
            .execute_tool("echo", serde_json::json!({ "text": "hi" }), &ctx)
            .await
            .expect("registered tool should run");
        assert_eq!(value, serde_json::json!({ "echoed": "hi" }));

        assert!(
            matches!(
                executor
                    .execute_tool("nope", serde_json::json!({}), &ctx)
                    .await,
                Err(ToolError::NotFound(_))
            ),
            "unknown tools must fall through to the next provider"
        );
    }

    #[tokio::test]
    async fn run_command_returns_none_for_unknown_commands() {
        let registry = PluginRegistry::new();
        let result = registry
            .run_command("missing", serde_json::json!({}))
            .await
            .expect("lookup should not error");
        assert!(result.is_none());
    }
}
