//! Plugin-declared tools and commands must be reachable, not just counted.
//!
//! `plugin.list` has always advertised `"tools": N` for tools that no agent
//! could invoke and `"commands": N` for commands no client could list. These
//! tests pin the two reachability paths: a spec tool reaches the agent's tool
//! dispatcher, and spec commands are enumerable/invocable over RPC.

use crucible_core::session::{Session, SessionType};
use crucible_daemon::background_manager::BackgroundJobManager;
use crucible_daemon::test_support::{kiln_name, temp_session_manager};
use crucible_daemon::{AgentManager, AgentManagerParams, DaemonPluginLoader, KilnManager};
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
    -- `_probe` is absent unless a test installs one; see
    -- `a_plugin_tool_runs_under_its_own_plugins_context`.
    if _probe then _probe() end
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
            effect = "read",
            params = {
                { name = "who", type = "string", desc = "Who to greet", optional = true },
            },
            fn = M.greet,
        },
        rename = {
            desc = "Rename someone",
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

    let session_manager = temp_session_manager();
    let (event_tx, _rx) = broadcast::channel(16);
    let manager = AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager,
        background_manager: Arc::new(BackgroundJobManager::new(event_tx)),
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        context_config: None,
        permission_config: None,
        plugin_loader: Some(Arc::new(tokio::sync::Mutex::new(Some(loader)))),
        card_roots: Default::default(),
    });

    let session = Session::new(SessionType::Chat, vec![kiln_name("kiln")]);
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

/// A plugin tool runs under its OWN plugin's context.
///
/// `PluginToolExecutor::execute_tool` used to call the Lua function under
/// whatever context was left behind, so a tool's `cru.storage` writes landed
/// in another plugin's namespace and `cru.plugin.publish` filed them under
/// another plugin's name.
///
/// The probe is a Rust closure, because the running plugin's name lives in the
/// VM's app data and Lua deliberately cannot read it.
#[tokio::test]
async fn a_plugin_tool_runs_under_its_own_plugins_context() {
    use std::sync::Mutex;

    let tmp = TempDir::new().expect("tempdir");
    let loader = loader_with_fixture(tmp.path()).await;
    let lua = loader.plugin_lua();

    let seen: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let recorder = Arc::clone(&seen);
    let probe = lua
        .create_function(move |lua, ()| {
            *recorder.lock().expect("probe lock") = crucible_lua::current_plugin_name(lua);
            Ok(())
        })
        .expect("probe");
    lua.globals().set("_probe", probe).expect("install probe");

    let session_manager = temp_session_manager();
    let (event_tx, _rx) = broadcast::channel(16);
    let manager = AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager,
        background_manager: Arc::new(BackgroundJobManager::new(event_tx)),
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        context_config: None,
        permission_config: None,
        plugin_loader: Some(Arc::new(tokio::sync::Mutex::new(Some(loader)))),
        card_roots: Default::default(),
    });

    let session = Session::new(SessionType::Chat, vec![kiln_name("kiln")]);
    let dispatcher = manager.get_or_create_session_dispatcher(&session).await;
    dispatcher
        .dispatch_tool(
            "shout",
            serde_json::json!({ "text": "hello" }),
            Default::default(),
        )
        .await
        .expect("plugin tool should dispatch");

    assert_eq!(
        seen.lock().expect("probe lock").as_deref(),
        Some("shout"),
        "the tool body must run under the plugin that declared it"
    );
    assert!(
        crucible_lua::current_plugin_context(&lua).is_none(),
        "the executor must restore the previous (absent) context"
    );
}

#[tokio::test]
async fn plugin_declared_command_is_listed_and_invocable() {
    let tmp = TempDir::new().expect("tempdir");
    let loader = loader_with_fixture(tmp.path()).await;
    let registry = loader.plugin_registry();

    let commands = registry.commands_json();
    assert_eq!(commands.len(), 2, "expected two commands, got {commands:?}");
    assert_eq!(commands[0]["name"], "greet");
    assert_eq!(commands[0]["plugin"], "shout");
    assert_eq!(commands[0]["hint"], "[name]");
    assert_eq!(commands[0]["description"], "Greet someone");

    // The two halves a caller needs before it can offer a command as a button:
    // the JSON Schema a dialog is generated from, and the marker that says
    // whether pressing the button is safe to do speculatively.
    assert_eq!(
        commands[0]["parameters"],
        serde_json::json!({
            "type": "object",
            "properties": {
                "who": { "type": "string", "description": "Who to greet" },
            },
            "required": [],
        }),
        "a declared parameter must cross as JSON Schema, not as opaque text"
    );
    assert_eq!(
        commands[0]["effect"], "read",
        "a declared read must reach a client as a read"
    );

    // `rename` declares no effect, and an undeclared command is unknown.
    // Unknown must cost a question, not a file.
    assert_eq!(commands[1]["name"], "rename");
    assert_eq!(
        commands[1]["effect"], "write",
        "an undeclared command must reach a client as a write"
    );

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
