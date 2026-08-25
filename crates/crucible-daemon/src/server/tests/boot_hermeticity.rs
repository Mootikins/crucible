//! An in-process (value-injection) bind evaluates NO user file.
//!
//! The plugin boot used to read `dirs::config_dir()/crucible/init.lua`
//! directly, so every in-process daemon test executed whatever Lua the
//! DEVELOPER's real config held — arbitrary user code deciding a test
//! suite's behaviour. The bind path must never read the environment for a
//! user file again; only the boot evaluation (`evaluate_boot_config`),
//! which takes its root as a value, may.

use super::*;

/// Point the config-dir environment at a directory whose `init.lua` leaves
/// a marker, bind an in-process daemon, run the plugin boot — and the
/// marker must NOT appear in the plugin VM.
#[tokio::test(flavor = "multi_thread")]
async fn an_in_process_bind_evaluates_no_user_init_lua() {
    let tmp = tempfile::TempDir::new().unwrap();

    // The bait: a config home whose init.lua would announce itself. This is
    // a genuine env-read regression test, so EnvVarGuard is the right tool.
    let config_home = tmp.path().join("xdg");
    let crucible_dir = config_home.join("crucible");
    std::fs::create_dir_all(&crucible_dir).unwrap();
    std::fs::write(
        crucible_dir.join("init.lua"),
        "_G.__leaked_user_init = true\n",
    )
    .unwrap();
    let _guard = crucible_core::test_support::EnvVarGuard::set(
        "XDG_CONFIG_HOME",
        config_home.to_string_lossy().to_string(),
    );

    let sock = tmp.path().join("d.sock");
    let server = Server::bind_with_data_home(&sock, tmp.path().join("data"))
        .await
        .expect("bind");
    server.boot_plugins().await;

    let loader = server.plugin_loader.lock().await;
    let loader = loader.as_ref().expect("loader present");
    let leaked = loader
        .eval("return tostring(_G.__leaked_user_init)")
        .await
        .expect("eval");
    assert_eq!(
        leaked, "nil",
        "a value-injection bind must not evaluate a user init.lua from the environment"
    );
}
