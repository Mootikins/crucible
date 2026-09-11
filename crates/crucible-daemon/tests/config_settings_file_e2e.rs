//! `settings.json`, across a restart: what a save persists, and what it
//! refuses to persist.
//!
//! Persistence is the whole point of the second config verb, and only a
//! second process can prove it. A value the running daemon still holds in
//! memory proves nothing: the store is live, so an unwritten save looks
//! exactly like a written one until the daemon is replaced.
//!
//! The refusal is the same story from the other side. A leaf `init.lua`
//! holds must never reach the file, because `settings.json` loads BELOW
//! `init.lua`: the next boot would restore the user's value over it, and the
//! click that saved it would have acted nowhere at all.

mod common;

use common::{RpcConn, TestDaemon};

/// The user's own line, with a comment above it so the reported line number
/// is the real one rather than a constant that happens to be 1.
const INIT_LUA: &str =
    "-- the human's own preference\ncru.config.set { chat = { show_thinking = true } }\n";

fn settings_file(daemon: &TestDaemon) -> std::path::PathBuf {
    daemon
        .home()
        .join(".config")
        .join("crucible")
        .join("settings.json")
}

fn read_settings(daemon: &TestDaemon) -> serde_json::Value {
    let path = settings_file(daemon);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} is not JSON: {e}", path.display()))
}

async fn daemon_with_init_lua() -> TestDaemon {
    TestDaemon::start_with_home_setup(|home| {
        let config_dir = home.join(".config").join("crucible");
        std::fs::create_dir_all(&config_dir)?;
        std::fs::write(config_dir.join("init.lua"), INIT_LUA)?;
        Ok(())
    })
    .await
    .expect("the daemon must boot with the fixture home")
}

/// One save, one restart, and the two halves of the rule: the accepted leaf
/// is in the file and holds after the restart, while the leaf `init.lua`
/// pins is refused, never written, and still answers with the user's value.
///
/// A saved leaf that the file did not hold would mean the settings UI loses
/// every preference at the next daemon start. A pinned leaf that the file
/// DID hold would be worse: it would load back as a `Settings` leaf, and the
/// refusal that protects the human's line would be gone for good.
#[tokio::test]
async fn a_save_persists_the_accepted_leaf_and_never_the_pinned_one() {
    let mut daemon = daemon_with_init_lua().await;
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the daemon");

    let save = conn
        .call_method(
            "config.save",
            serde_json::json!({ "values": {
                "chat": { "show_thinking": false, "model": "sonnet" },
            }}),
            1,
        )
        .await;

    // The refusal names the file and the line to edit instead.
    assert_eq!(save["result"]["ok"], serde_json::json!(false), "{save}");
    let refused = &save["result"]["refused"][0];
    assert_eq!(refused["key"], serde_json::json!("chat.show_thinking"));
    assert_eq!(refused["source"], serde_json::json!("lua"));
    assert_eq!(refused["line"], serde_json::json!(2), "{save}");
    assert!(
        refused["file"]
            .as_str()
            .is_some_and(|file| file.ends_with("init.lua")),
        "{save}"
    );

    // The file holds the accepted leaf, says what it is, and holds nothing
    // the user's own file holds.
    let written = read_settings(&daemon);
    assert_eq!(written["chat"]["model"], serde_json::json!("sonnet"));
    assert!(
        written["_"].is_string(),
        "the file must say who owns it: {written}"
    );
    assert!(
        written["chat"].get("show_thinking").is_none(),
        "a leaf init.lua pins must never reach settings.json, or the next \
         boot loads it back as a saved setting and the refusal is bypassed \
         for good: {written}"
    );

    // A new process, reading the files and nothing else.
    daemon.restart().await.expect("the daemon must restart");
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the restarted daemon");

    let saved = conn
        .call_method(
            "config.origin",
            serde_json::json!({ "key": "chat.model" }),
            2,
        )
        .await;
    assert_eq!(
        saved["result"]["value"],
        serde_json::json!("sonnet"),
        "the saved value must survive the restart: {saved}"
    );
    assert_eq!(
        saved["result"]["source"],
        serde_json::json!("settings"),
        "and it must come back as the layer that saved it: {saved}"
    );

    let pinned = conn
        .call_method(
            "config.origin",
            serde_json::json!({ "key": "chat.show_thinking" }),
            3,
        )
        .await;
    assert_eq!(
        pinned["result"]["value"],
        serde_json::json!(true),
        "the human's line still owns the leaf it pins: {pinned}"
    );
    assert_eq!(pinned["result"]["source"], serde_json::json!("lua"));
}

/// The runtime knob writes no file, and therefore changes nothing about the
/// next boot. `:set` is the verb a user reaches for to raise a budget for one
/// turn; if it persisted, one experiment would become a preference.
#[tokio::test]
async fn a_runtime_set_leaves_no_trace_across_a_restart() {
    let mut daemon = daemon_with_init_lua().await;
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the daemon");

    let set = conn
        .call_method(
            "config.set",
            serde_json::json!({ "values": { "chat": { "model": "ephemeral" } } }),
            1,
        )
        .await;
    assert_eq!(set["result"]["ok"], serde_json::json!(true), "{set}");
    assert!(
        !settings_file(&daemon).exists(),
        "a runtime set must write no file"
    );

    daemon.restart().await.expect("the daemon must restart");
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the restarted daemon");

    let origin = conn
        .call_method(
            "config.origin",
            serde_json::json!({ "key": "chat.model" }),
            2,
        )
        .await;
    assert_ne!(
        origin["result"]["value"],
        serde_json::json!("ephemeral"),
        "the runtime knob must die with the process: {origin}"
    );
}

/// A key that names WHERE the daemon acts reaches neither the running store
/// nor the file.
///
/// The store already withholds it: the socket has no authentication, so
/// `kiln_path` through this door would re-point the daemon's knowledge scope
/// without the kiln floor seeing it. The file matters just as much, and for a
/// reason the runtime rule cannot state on its own: `settings.json` loads in
/// the BOOT phase, where location keys are accepted. A persisted one would
/// come back at the next start as exactly the authority the save was refused.
#[tokio::test]
async fn a_location_key_reaches_neither_the_store_nor_the_file() {
    let mut daemon = daemon_with_init_lua().await;
    let elsewhere = daemon.home().join("elsewhere");
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the daemon");

    let save = conn
        .call_method(
            "config.save",
            serde_json::json!({ "values": {
                "kiln_path": elsewhere,
                "chat": { "model": "sonnet" },
            }}),
            1,
        )
        .await;
    assert_eq!(
        save["result"]["rejected"],
        serde_json::json!(["kiln_path"]),
        "the store must withhold the location key: {save}"
    );

    let written = read_settings(&daemon);
    assert_eq!(
        written["chat"]["model"],
        serde_json::json!("sonnet"),
        "precondition: the rest of the save was written: {written}"
    );
    assert!(
        written.get("kiln_path").is_none(),
        "a location key the store refused must not reach the file, or the \
         next boot accepts it: {written}"
    );

    daemon.restart().await.expect("the daemon must restart");
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the restarted daemon");
    let origin = conn
        .call_method(
            "config.origin",
            serde_json::json!({ "key": "kiln_path" }),
            2,
        )
        .await;
    assert_ne!(
        origin["result"]["value"],
        serde_json::json!(elsewhere),
        "the refused location key must not come back at the next boot: {origin}"
    );
}
