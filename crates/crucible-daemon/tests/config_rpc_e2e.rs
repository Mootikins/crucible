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
