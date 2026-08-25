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
            plugin_dir.join("plugin.yaml"),
            "name: prefs\nversion: \"0.1.0\"\nmain: init.lua\n",
        )?;
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
