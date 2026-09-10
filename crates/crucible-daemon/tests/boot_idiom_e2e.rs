//! The one-VM boot, across the process seam: a real `cru daemon serve`
//! evaluates the user's `init.lua` BEFORE plugin activation, the
//! `require("<plugin>").setup{...}` idiom works verbatim at its top, and
//! activation reuses the same module instance — one module, one setup.

mod common;

use common::{RpcConn, TestDaemon};

/// Ask the daemon's plugin VM for one Lua expression's value.
async fn lua_eval(conn: &mut RpcConn, code: &str, id: i64) -> String {
    let resp = conn
        .call_method("lua.eval", serde_json::json!({ "code": code }), id)
        .await;
    resp["result"]["result"]
        .as_str()
        .unwrap_or_else(|| panic!("lua.eval failed: {resp}"))
        .to_string()
}

/// The idiom test, per the plan: the fixture plugin lives in the DEFAULT
/// user plugins dir (no `runtimepath` line at all), the user's init.lua
/// `require`s it and calls `setup` with a marker, and after the daemon has
/// fully booted: `setup` ran exactly ONCE and the marker survived
/// activation.
#[tokio::test]
async fn init_lua_setup_idiom_survives_a_real_daemon_boot() {
    let daemon = TestDaemon::start_with_home_setup(|home| {
        // The default user plugins dir under the hermetic XDG config home.
        let config_dir = home.join(".config").join("crucible");
        let plugin_dir = config_dir.join("plugins").join("prefs");
        std::fs::create_dir_all(&plugin_dir)?;
        std::fs::write(
            plugin_dir.join("init.lua"),
            r#"
_G.__prefs_setups = 0
return {
    name = "prefs",
    setup = function(cfg)
        _G.__prefs_setups = _G.__prefs_setups + 1
        _G.__prefs_config = cfg
    end,
}
"#,
        )?;
        // Requirable from line one, with no runtimepath line at all.
        std::fs::write(
            config_dir.join("init.lua"),
            r#"require("prefs").setup({ marker = "user-owned" })"#,
        )?;
        Ok(())
    })
    .await
    .expect("daemon must boot with the fixture home");

    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect");

    let count = lua_eval(&mut conn, "return tostring(_G.__prefs_setups)", 1).await;
    assert_eq!(
        count, "1",
        "setup must run exactly once — the user's own call"
    );
    let marker = lua_eval(&mut conn, "return tostring(_G.__prefs_config.marker)", 2).await;
    assert_eq!(
        marker, "user-owned",
        "the user's marker survives activation"
    );
}

/// `chat.system_prompt` in the user's `init.lua` reaches a real session.
///
/// The two halves this proves:
///
/// 1. `cru.config.set` on the daemon VM reaches the store every session
///    reads. The user's only other route would be a workspace file, which the
///    daemon no longer executes.
/// 2. The value outranks the shipped prompt. That prompt ships on the
///    `Default` layer, from `ChatConfig::default()`; a write from the user's
///    `init.lua` carries the `Lua` layer, which is higher, so it wins.
///
/// `system_prompt` proves both at once: an unreachable store would leave the
/// shipped prompt in place, and a layer that did not outrank `Default` would
/// too.
#[tokio::test]
async fn a_configured_system_prompt_reaches_a_new_session() {
    let daemon = TestDaemon::start_with_home_setup(|home| {
        let config_dir = home.join(".config").join("crucible");
        std::fs::create_dir_all(&config_dir)?;
        std::fs::write(
            config_dir.join("init.lua"),
            "cru.config.set { chat = { system_prompt = \"Only haiku.\" } }",
        )?;
        Ok(())
    })
    .await
    .expect("daemon must boot with the fixture home");

    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect");

    let created = conn
        .call_method(
            "session.create",
            serde_json::json!({ "session_type": "chat" }),
            1,
        )
        .await;
    let session_id = created["result"]["session_id"]
        .as_str()
        .unwrap_or_else(|| panic!("session.create failed: {created}"))
        .to_string();

    // An agent that names no prompt of its own, so the default has to fill it.
    let configured = conn
        .call_method(
            "session.configure_agent",
            serde_json::json!({
                "session_id": session_id,
                "agent": {
                    "agent_type": "internal",
                    "provider": "openai",
                    "model": "gpt-4o",
                    "system_prompt": "",
                }
            }),
            2,
        )
        .await;
    assert!(
        configured["error"].is_null(),
        "configure_agent failed: {configured}"
    );

    let session = conn
        .call_method(
            "session.get",
            serde_json::json!({ "session_id": session_id }),
            3,
        )
        .await;
    assert_eq!(
        session["result"]["agent"]["system_prompt"].as_str(),
        Some("Only haiku."),
        "the user's init.lua default must reach the session, and must win over \
         the shipped defaults file, which sets it too: {session}"
    );
}

/// `cru.modes.auto = nil` in the user's `init.lua` removes the shipped mode.
///
/// The point of an exec order rather than an override tier: removal needs no
/// mechanism of its own. The daemon VM runs the defaults file, which declares
/// `auto`, and then this file, which unsets it. Anything that re-applied the
/// first file's values on top would take this test red.
#[tokio::test]
async fn a_shipped_mode_can_be_removed_from_the_users_init_lua() {
    let daemon = TestDaemon::start_with_home_setup(|home| {
        let config_dir = home.join(".config").join("crucible");
        std::fs::create_dir_all(&config_dir)?;
        std::fs::write(config_dir.join("init.lua"), "cru.modes.auto = nil")?;
        Ok(())
    })
    .await
    .expect("daemon must boot with the fixture home");

    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect");

    let created = conn
        .call_method(
            "session.create",
            serde_json::json!({ "session_type": "chat" }),
            1,
        )
        .await;
    let session_id = created["result"]["session_id"]
        .as_str()
        .unwrap_or_else(|| panic!("session.create failed: {created}"))
        .to_string();

    let modes = conn
        .call_method(
            "session.list_modes",
            serde_json::json!({ "session_id": session_id }),
            2,
        )
        .await;
    let ids: Vec<&str> = modes["result"]["modes"]
        .as_array()
        .unwrap_or_else(|| panic!("session.list_modes failed: {modes}"))
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["ask", "plan"],
        "the user's file runs after the defaults file, so its removal stands: {modes}"
    );
}
