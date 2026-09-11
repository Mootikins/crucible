//! A plugin installed at RUNTIME writes the plugin layer, not the human's.
//!
//! The author roots are resolved once, while the daemon boots. On a fresh
//! machine `~/.config/crucible/plugins` does not exist yet, so the boot's
//! path resolution drops it and only the config root itself is registered.
//! A plugin the user then installs from the web UI lands UNDER that config
//! root and matches no plugin root, so every `cru.config.set` its `setup()`
//! makes is classified as the human's own line: the key pins, `config.save`
//! refuses it forever, and the settings UI names a plugin file as if the
//! user had written it.
//!
//! Only a real daemon proves this. The classification reads the chunk name
//! of the file that wrote the leaf against the roots the daemon installed,
//! and nothing in-process installs a plugin into a config root it did not
//! have at boot.

mod common;

use common::{RpcConn, TestDaemon};

/// The fixture plugin: one `setup()` that writes one app-config leaf.
const PLUGIN_INIT: &str = r#"
return {
    name = "fixture",
    version = "0.1.0",
    setup = function()
        cru.config.set { chat = { show_thinking = true } }
    end,
}
"#;

/// A plugin the daemon ships, so its presence in `plugin.list` is a fact
/// about the running system rather than about this test's fixtures.
const BUNDLED_PLUGIN: &str = "reflection";

/// Block until the boot's own plugin pass is over.
///
/// The socket accepts connections before that pass runs, and the pass
/// discovers every plugin directory that exists at the moment it sweeps. A
/// fixture written before it finishes is loaded by the BOOT — which is the
/// path this test is NOT about, and which fails it for the wrong reason.
/// `plugin.list` answers out of the loader the pass populates, so a bundled
/// plugin in the answer means the sweep already happened.
async fn wait_for_boot_plugin_pass(conn: &mut RpcConn, first_id: i64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let mut id = first_id;
    loop {
        let listed = conn
            .call_method("plugin.list", serde_json::json!({}), id)
            .await;
        id += 1;
        if listed["result"]["plugins"]
            .as_array()
            .is_some_and(|names| names.iter().any(|n| n == BUNDLED_PLUGIN))
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the daemon never finished its boot plugin pass: {listed}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn a_plugin_installed_at_runtime_writes_the_plugin_layer() {
    let daemon = TestDaemon::start()
        .await
        .expect("the daemon must boot with the fixture home");
    let config_dir = daemon.home().join(".config").join("crucible");
    let plugins_dir = config_dir.join("plugins");

    // The premise: the plugins directory does NOT exist while the daemon
    // resolves its roots. Without this the boot registers the root anyway
    // and the test asserts nothing.
    assert!(
        !plugins_dir.exists(),
        "the fixture home must have no plugins directory at boot: {}",
        plugins_dir.display()
    );

    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the daemon");
    wait_for_boot_plugin_pass(&mut conn, 1).await;

    // Nothing has written the leaf yet, so whatever writes it next is the
    // install.
    let before = conn
        .call_method(
            "config.origin",
            serde_json::json!({ "key": "chat.show_thinking" }),
            100,
        )
        .await;
    assert_eq!(
        before["result"]["source"],
        serde_json::json!("default"),
        "the leaf must still be the compiled-in default: {before}"
    );

    // The install: the clone destination appears, then `plugin.install`
    // records and activates it. A local directory is not a clonable URL, so
    // the fixture takes the `AlreadyPresent` path — the same runtime load
    // pass a cloned plugin takes, without a network.
    let plugin_dir = plugins_dir.join("fixture");
    std::fs::create_dir_all(&plugin_dir).expect("create the fixture plugin directory");
    std::fs::write(plugin_dir.join("init.lua"), PLUGIN_INIT).expect("write the fixture plugin");

    let install = conn
        .call_method(
            "plugin.install",
            serde_json::json!({ "url": "crucible-fixtures/fixture" }),
            101,
        )
        .await;
    assert_eq!(
        install["result"]["loaded"],
        serde_json::json!(true),
        "the fixture must load, or its setup() never wrote anything: {install}"
    );

    // The leaf belongs to the plugin, and it names the plugin's own file.
    let origin = conn
        .call_method(
            "config.origin",
            serde_json::json!({ "key": "chat.show_thinking" }),
            102,
        )
        .await;
    assert_eq!(
        origin["result"]["value"],
        serde_json::json!(true),
        "the plugin's setup() must reach the store: {origin}"
    );
    assert_eq!(
        origin["result"]["source"],
        serde_json::json!("plugin"),
        "a runtime-installed plugin writes the plugin layer, not the human's: {origin}"
    );
    assert!(
        origin["result"]["file"]
            .as_str()
            .is_some_and(|file| file.ends_with("fixture/init.lua")),
        "and the file it names is the plugin's own: {origin}"
    );

    // And the settings UI can still save that key: a plugin default is a
    // default, so nothing is pinned and nothing is refused.
    let save = conn
        .call_method(
            "config.save",
            serde_json::json!({ "values": { "chat": { "show_thinking": false } } }),
            103,
        )
        .await;
    assert_eq!(
        save["result"]["refused"],
        serde_json::json!([]),
        "a plugin default must never refuse a save: {save}"
    );
    assert_eq!(save["result"]["ok"], serde_json::json!(true), "{save}");
    assert!(
        config_dir.join("settings.json").exists(),
        "an accepted save persists the value"
    );
}
