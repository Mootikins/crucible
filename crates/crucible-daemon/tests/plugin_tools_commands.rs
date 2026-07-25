//! Plugin-declared tools and commands must be reachable, not just counted.
//!
//! `plugin.list` has always advertised `"tools": N` for tools that no agent
//! could invoke and `"commands": N` for commands no client could list. These
//! tests pin the two reachability paths: a spec tool reaches the agent's tool
//! dispatcher, and spec commands are enumerable/invocable over RPC.

use crucible_core::session::{Session, SessionType};
use crucible_daemon::background_manager::BackgroundJobManager;
use crucible_daemon::tools::workspace::WorkspaceTools;
use crucible_daemon::{
    AgentManager, AgentManagerParams, DaemonPluginLoader, FileSessionStorage, KilnManager,
    SessionManager,
};
use crucible_lua::PluginSource;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::broadcast;

/// Write a plugin declaring one tool and one command, both with real `fn`s.
fn write_fixture_plugin(root: &std::path::Path) -> std::path::PathBuf {
    let plugins_dir = root.join("plugins");
    let plugin_dir = plugins_dir.join("shout");
    std::fs::create_dir_all(&plugin_dir).expect("create plugin dir");

    std::fs::write(
        plugin_dir.join("plugin.yaml"),
        r#"name: shout
version: "0.1.0"
description: Test fixture plugin declaring a tool and a command
main: init.lua
capabilities: []
"#,
    )
    .expect("write plugin.yaml");

    std::fs::write(
        plugin_dir.join("init.lua"),
        r#"
local M = {}

function M.shout(args)
    return { shouted = string.upper(args.text) }
end

function M.greet(args)
    return "hello " .. (args and args.who or "world")
end

return {
    name = "shout",
    version = "0.1.0",
    description = "Test fixture plugin",

    tools = {
        shout = {
            desc = "Uppercase the given text",
            params = {
                { name = "text", type = "string", desc = "Text to shout" },
            },
            fn = M.shout,
        },
    },

    commands = {
        greet = {
            desc = "Greet someone",
            hint = "[name]",
            params = {
                { name = "who", type = "string", desc = "Who to greet", optional = true },
            },
            fn = M.greet,
        },
    },
}
"#,
    )
    .expect("write init.lua");

    plugins_dir
}

async fn loader_with_fixture(root: &std::path::Path) -> DaemonPluginLoader {
    let plugins_dir = write_fixture_plugin(root);
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(plugins_dir, PluginSource::User)])
        .await
        .expect("load plugins");
    loader
}

#[tokio::test]
async fn plugin_declared_tool_is_dispatchable_by_the_agent() {
    let tmp = TempDir::new().expect("tempdir");
    let loader = loader_with_fixture(tmp.path()).await;

    let session_manager = Arc::new(SessionManager::with_storage(Arc::new(
        FileSessionStorage::new(),
    )));
    let (event_tx, _rx) = broadcast::channel(16);
    let manager = AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager,
        background_manager: Arc::new(BackgroundJobManager::new(event_tx)),
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        permission_config: None,
        plugin_loader: Some(Arc::new(tokio::sync::Mutex::new(Some(loader)))),
        workspace_tools: Arc::new(WorkspaceTools::new(tmp.path().to_path_buf())),
    });

    let session = Session::new(SessionType::Chat, tmp.path().to_path_buf());
    let dispatcher = manager.get_or_create_session_dispatcher(&session).await;

    assert!(
        dispatcher.has_tool("shout"),
        "plugin-declared tool should be visible to the agent's dispatcher"
    );

    let result = dispatcher
        .dispatch_tool(
            "shout",
            serde_json::json!({ "text": "hello" }),
            Default::default(),
        )
        .await
        .expect("plugin tool should dispatch");

    assert_eq!(
        result,
        serde_json::json!({ "shouted": "HELLO" }),
        "plugin tool should return its Lua result"
    );
}

#[tokio::test]
async fn plugin_declared_command_is_listed_and_invocable() {
    let tmp = TempDir::new().expect("tempdir");
    let loader = loader_with_fixture(tmp.path()).await;
    let registry = loader.plugin_registry();

    let commands = registry.commands_json();
    assert_eq!(commands.len(), 1, "expected one command, got {commands:?}");
    assert_eq!(commands[0]["name"], "greet");
    assert_eq!(commands[0]["plugin"], "shout");
    assert_eq!(commands[0]["hint"], "[name]");
    assert_eq!(commands[0]["description"], "Greet someone");

    let result = registry
        .run_command("greet", serde_json::json!({ "who": "crucible" }))
        .await
        .expect("command should run")
        .expect("command should be found");
    assert_eq!(result, serde_json::json!("hello crucible"));

    assert!(
        registry
            .run_command("nope", serde_json::json!({}))
            .await
            .expect("lookup should not error")
            .is_none(),
        "unknown commands report absence, not failure"
    );
}

#[test]
fn plugin_commands_are_reachable_over_rpc() {
    assert!(
        crucible_daemon::rpc::METHODS.contains(&"plugin.commands"),
        "clients (TUI and web) need an RPC to list plugin-declared commands"
    );
    assert!(
        crucible_daemon::rpc::METHODS.contains(&"plugin.run_command"),
        "listing commands is useless without a way to invoke them"
    );
}
