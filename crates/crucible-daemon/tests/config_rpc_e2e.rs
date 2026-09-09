//! `config.set` must change what `config.effective` serves, in one daemon.
//!
//! The RPC seam is the point. `config.set` merges into the `crucible-lua`
//! process store; every daemon reader used to take a snapshot bound once at
//! startup. Nothing in-process could catch that, because both halves of the
//! bug live on the daemon side of the socket — a unit test on either half
//! passes.

mod common;

use common::{RpcConn, TestDaemon};

/// A value set over the RPC is visible to the next `config.effective`,
/// without a restart.
#[tokio::test]
async fn a_config_set_changes_what_config_effective_serves() {
    let daemon = TestDaemon::start().await.expect("daemon starts");
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the daemon");

    let before = conn
        .call_method("config.effective", serde_json::json!({}), 1)
        .await;
    assert_eq!(
        before["result"]["config"]["chat"]["show_thinking"],
        serde_json::json!(false),
        "the fixture daemon starts on the default: {before}"
    );

    let set = conn
        .call_method(
            "config.set",
            serde_json::json!({ "values": { "chat": { "show_thinking": true } } }),
            2,
        )
        .await;
    assert_eq!(set["result"]["ok"], serde_json::json!(true), "{set}");

    let after = conn
        .call_method("config.effective", serde_json::json!({}), 3)
        .await;
    assert_eq!(
        after["result"]["config"]["chat"]["show_thinking"],
        serde_json::json!(true),
        "config.set must reach the reader in the same process: {after}"
    );
}

/// The location keys keep coming from the bind snapshot, so the live store
/// does not empty them out.
///
/// `ConfigStore::end_boot_phase` drops the location keys from the stored
/// value on purpose. Serving the store alone would therefore answer
/// `config.effective` with no `kiln_path` at all.
#[tokio::test]
async fn a_config_set_does_not_strip_the_location_keys() {
    let daemon = TestDaemon::start().await.expect("daemon starts");
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the daemon");

    let before = conn
        .call_method("config.effective", serde_json::json!({}), 1)
        .await;
    let kiln_path = before["result"]["config"]["kiln_path"].clone();
    assert!(
        kiln_path.is_string(),
        "the fixture daemon has a kiln_path: {before}"
    );

    conn.call_method(
        "config.set",
        serde_json::json!({ "values": { "chat": { "show_thinking": true } } }),
        2,
    )
    .await;

    let after = conn
        .call_method("config.effective", serde_json::json!({}), 3)
        .await;
    assert_eq!(
        after["result"]["config"]["kiln_path"], kiln_path,
        "a config.set must not empty the location keys: {after}"
    );
}

/// A client that writes a key can read the daemon's own answer for it.
///
/// The TUI has no second copy of app config any more, so this round trip is
/// the whole read path for a `:set` of an app-config key. `config.set`
/// alone cannot serve it: the store may refuse or reshape what it was sent.
#[tokio::test]
async fn config_get_answers_with_what_config_set_wrote() {
    let daemon = TestDaemon::start().await.expect("daemon starts");
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the daemon");

    let before = conn
        .call_method("config.get", serde_json::json!({ "key": "myplugin" }), 1)
        .await;
    assert_eq!(
        before["result"]["value"],
        serde_json::Value::Null,
        "the fixture daemon knows no such key: {before}"
    );

    conn.call_method(
        "config.set",
        serde_json::json!({ "values": { "myplugin": { "retries": 3 } } }),
        2,
    )
    .await;

    let after = conn
        .call_method("config.get", serde_json::json!({ "key": "myplugin" }), 3)
        .await;
    assert_eq!(
        after["result"]["value"],
        serde_json::json!({ "retries": 3 }),
        "config.get must answer with the value config.set wrote: {after}"
    );
}

/// A key that names where the daemon acts is refused, and the store does not
/// hold it afterwards.
///
/// Both halves matter to a client: the refusal names the key, and the read
/// back reports nothing. A client that recorded its own value instead would
/// show a setting this daemon never took.
#[tokio::test]
async fn config_set_refuses_a_location_key_and_keeps_nothing() {
    let daemon = TestDaemon::start().await.expect("daemon starts");
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the daemon");

    let set = conn
        .call_method(
            "config.set",
            serde_json::json!({ "values": { "kiln_path": "/nowhere" } }),
            1,
        )
        .await;
    assert_eq!(
        set["result"]["rejected"],
        serde_json::json!(["kiln_path"]),
        "the store must name the key it refused: {set}"
    );

    let read_back = conn
        .call_method("config.get", serde_json::json!({ "key": "kiln_path" }), 2)
        .await;
    assert_eq!(
        read_back["result"]["value"],
        serde_json::Value::Null,
        "a refused key must not read back as set: {read_back}"
    );
}

/// `:set key&`. A key the runtime knob raised returns to what the compiled
/// defaults and the config files give it, without a restart.
///
/// The RPC seam is the point again. The store keeps the layers it merged and
/// re-merges what is left, so a reset is a property of the daemon's store —
/// a client-side undo would answer with a value the daemon does not hold.
#[tokio::test]
async fn config_reset_returns_a_key_to_what_the_files_give() {
    let daemon = TestDaemon::start().await.expect("daemon starts");
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the daemon");

    conn.call_method(
        "config.set",
        serde_json::json!({ "values": { "chat": { "show_thinking": true } } }),
        1,
    )
    .await;
    let raised = conn
        .call_method("config.effective", serde_json::json!({}), 2)
        .await;
    assert_eq!(
        raised["result"]["config"]["chat"]["show_thinking"],
        serde_json::json!(true),
        "the knob must be up before the reset can put it down: {raised}"
    );

    let reset = conn
        .call_method(
            "config.reset",
            serde_json::json!({ "key": "chat.show_thinking" }),
            3,
        )
        .await;
    assert_eq!(
        reset["result"]["outcome"],
        serde_json::json!("dropped"),
        "{reset}"
    );
    assert_eq!(
        reset["result"]["dropped"],
        serde_json::json!(["rpc"]),
        "a reset drops the ephemeral layer and nothing else: {reset}"
    );
    assert_eq!(
        reset["result"]["value"],
        serde_json::json!(false),
        "and answers with the value that shows once it is gone: {reset}"
    );

    let after = conn
        .call_method("config.effective", serde_json::json!({}), 4)
        .await;
    assert_eq!(
        after["result"]["config"]["chat"]["show_thinking"],
        serde_json::json!(false),
        "config.reset must reach the reader in the same process: {after}"
    );
}

/// `:set key^`. A key written in BOTH `settings.json` and `init.lua`, popped
/// once, answers with the `settings.json` value and names that layer.
///
/// This is what a merged value with one provenance row per leaf could not do.
/// The store retains the layers it merged, so dropping the top one and
/// re-merging is the merge rule itself — there is no second copy of the layer
/// order to drift.
#[tokio::test]
async fn config_pop_reveals_the_settings_value_under_the_lua_line() {
    let daemon = TestDaemon::start_with_home_setup(|home| {
        let config_dir = home.join(".config").join("crucible");
        std::fs::create_dir_all(&config_dir)?;
        std::fs::write(
            config_dir.join("init.lua"),
            "cru.config.set { chat = { model = \"from-lua\" } }\n",
        )?;
        std::fs::write(
            config_dir.join("settings.json"),
            "{\"chat\": {\"model\": \"from-settings\"}}\n",
        )?;
        Ok(())
    })
    .await
    .expect("daemon starts");
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the daemon");

    let before = conn
        .call_method(
            "config.origin",
            serde_json::json!({ "key": "chat.model" }),
            1,
        )
        .await;
    assert_eq!(before["result"]["value"], serde_json::json!("from-lua"));
    assert_eq!(
        before["result"]["source"],
        serde_json::json!("lua"),
        "{before}"
    );

    let popped = conn
        .call_method("config.pop", serde_json::json!({ "key": "chat.model" }), 2)
        .await;
    assert_eq!(
        popped["result"]["dropped"],
        serde_json::json!(["lua"]),
        "the pop drops the highest layer holding the leaf: {popped}"
    );
    assert_eq!(
        popped["result"]["value"],
        serde_json::json!("from-settings"),
        "and reveals the layer under it: {popped}"
    );
    assert_eq!(
        popped["result"]["source"],
        serde_json::json!("settings"),
        "which the store must now name: {popped}"
    );

    let after = conn
        .call_method("config.effective", serde_json::json!({}), 3)
        .await;
    assert_eq!(
        after["result"]["config"]["chat"]["model"],
        serde_json::json!("from-settings"),
        "config.pop must reach the reader in the same process: {after}"
    );
}

/// A drop is a write door, so it withholds the keys that name where the
/// daemon acts. A caller that could pop `runtimepath` would re-point the
/// trees the daemon reads code from without the floor ever seeing a path.
#[tokio::test]
async fn config_reset_and_pop_withhold_a_location_key() {
    let daemon = TestDaemon::start().await.expect("daemon starts");
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the daemon");

    let before = conn
        .call_method("config.effective", serde_json::json!({}), 1)
        .await;
    let kiln_path = before["result"]["config"]["kiln_path"].clone();
    assert!(kiln_path.is_string(), "{before}");

    for (id, method) in [(2, "config.reset"), (3, "config.pop")] {
        let answer = conn
            .call_method(method, serde_json::json!({ "key": "kiln_path" }), id)
            .await;
        assert_eq!(
            answer["result"]["outcome"],
            serde_json::json!("withheld"),
            "{method} must refuse a location key: {answer}"
        );
    }

    let after = conn
        .call_method("config.effective", serde_json::json!({}), 4)
        .await;
    assert_eq!(
        after["result"]["config"]["kiln_path"], kiln_path,
        "a withheld drop must change nothing: {after}"
    );
}
