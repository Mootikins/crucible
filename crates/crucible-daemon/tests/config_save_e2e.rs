//! Two config verbs, across the process seam: the runtime knob and the
//! durable preference.
//!
//! `config.set` is what `:set` drives. It must keep working on a key the
//! user's own `init.lua` holds, because a user raises a pinned budget for one
//! turn without editing a file, and the write dies with the process.
//! `config.save` is what a settings UI drives. It must refuse that same key
//! and name the line, because `settings.json` loads below `init.lua`: a saved
//! value there would be shadowed at the next boot and the click would act
//! nowhere.
//!
//! Only a real daemon proves it. The pin is decided by the chunk name of the
//! file that wrote the leaf, against the author roots the boot installs — and
//! nothing in-process evaluates the user's `init.lua` from the config root it
//! really lives in.

mod common;

use common::{RpcConn, TestDaemon};

/// The user's own line, and the comment above it, so the reported line number
/// is the real one rather than a constant that happens to be 1.
const INIT_LUA: &str =
    "-- the human's own preference\ncru.config.set { chat = { show_thinking = true } }\n";

/// The A1 gate, whole: a `:set` of a pinned key succeeds and writes no file,
/// and a `config.save` of that same key is refused with the file and line.
#[tokio::test]
async fn a_key_pinned_in_init_lua_takes_a_set_and_refuses_a_save() {
    let daemon = TestDaemon::start_with_home_setup(|home| {
        let config_dir = home.join(".config").join("crucible");
        std::fs::create_dir_all(&config_dir)?;
        std::fs::write(config_dir.join("init.lua"), INIT_LUA)?;
        Ok(())
    })
    .await
    .expect("the daemon must boot with the fixture home");
    let config_dir = daemon.home().join(".config").join("crucible");
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the daemon");

    // The user's line holds the key, and `config.origin` says where from.
    let pinned = conn
        .call_method(
            "config.origin",
            serde_json::json!({ "key": "chat.show_thinking" }),
            1,
        )
        .await;
    assert_eq!(
        pinned["result"]["source"],
        serde_json::json!("lua"),
        "init.lua must own the leaf, or this test asserts nothing: {pinned}"
    );
    assert_eq!(pinned["result"]["value"], serde_json::json!(true));
    assert_eq!(
        pinned["result"]["line"],
        serde_json::json!(2),
        "the refusal has to name the line the user wrote: {pinned}"
    );
    assert!(
        pinned["result"]["file"]
            .as_str()
            .is_some_and(|file| file.ends_with("init.lua")),
        "and the file: {pinned}"
    );

    // `:set` on that key still works.
    let set = conn
        .call_method(
            "config.set",
            serde_json::json!({ "values": { "chat": { "show_thinking": false } } }),
            2,
        )
        .await;
    assert_eq!(
        set["result"]["ok"],
        serde_json::json!(true),
        "a runtime set must never refuse a pinned key: {set}"
    );
    let after_set = conn
        .call_method("config.get", serde_json::json!({ "key": "chat" }), 3)
        .await;
    assert_eq!(
        after_set["result"]["value"]["show_thinking"],
        serde_json::json!(false),
        "and it must take effect for this run: {after_set}"
    );
    assert!(
        !config_dir.join("settings.json").exists(),
        "a runtime set writes no file"
    );

    // `config.save` of the same key is refused, and says which line to edit.
    let save = conn
        .call_method(
            "config.save",
            serde_json::json!({ "values": { "chat": { "show_thinking": false } } }),
            4,
        )
        .await;
    assert_eq!(
        save["result"]["ok"],
        serde_json::json!(false),
        "the pinned leaf must be refused: {save}"
    );
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
    assert!(
        !config_dir.join("settings.json").exists(),
        "a refused save writes no file"
    );
}

/// A key the user's file does not hold saves without a refusal, and the store
/// records it as the persisted layer. The refusal is a rule about authorship,
/// not a rule that blocks the settings UI.
#[tokio::test]
async fn a_key_no_file_holds_saves_without_a_refusal() {
    let daemon = TestDaemon::start_with_home_setup(|home| {
        let config_dir = home.join(".config").join("crucible");
        std::fs::create_dir_all(&config_dir)?;
        std::fs::write(config_dir.join("init.lua"), INIT_LUA)?;
        Ok(())
    })
    .await
    .expect("the daemon must boot with the fixture home");
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the daemon");

    let save = conn
        .call_method(
            "config.save",
            serde_json::json!({ "values": { "chat": { "model": "sonnet" } } }),
            1,
        )
        .await;
    assert_eq!(save["result"]["ok"], serde_json::json!(true), "{save}");
    assert_eq!(save["result"]["refused"], serde_json::json!([]), "{save}");

    let origin = conn
        .call_method(
            "config.origin",
            serde_json::json!({ "key": "chat.model" }),
            2,
        )
        .await;
    assert_eq!(origin["result"]["value"], serde_json::json!("sonnet"));
    assert_eq!(
        origin["result"]["source"],
        serde_json::json!("settings"),
        "a save lands in the layer settings.json holds: {origin}"
    );
}

/// The web control renders its lock from `config.origin`, and the save it
/// invites is answered by `config.save`. The two must answer the same
/// question about the same leaf, at every point in a session.
///
/// The routine `:set` is what breaks the agreement: it writes the ephemeral
/// layer and becomes the last writer, while the line in `init.lua` still
/// re-applies at the next boot and still refuses the save. A control that read
/// the last writer would unlock after one `:set`, invite the save, and get a
/// refusal.
///
/// Only a real daemon proves it, for the reason at the top of this file: the
/// pin follows the chunk name of the file that wrote the leaf.
#[tokio::test]
async fn config_origin_and_config_save_agree_after_a_runtime_set() {
    let daemon = TestDaemon::start_with_home_setup(|home| {
        let config_dir = home.join(".config").join("crucible");
        std::fs::create_dir_all(&config_dir)?;
        std::fs::write(config_dir.join("init.lua"), INIT_LUA)?;
        Ok(())
    })
    .await
    .expect("the daemon must boot with the fixture home");
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the daemon");

    // The same leaf the human's line holds, asked of both verbs.
    let key = "chat.show_thinking";
    let values = serde_json::json!({ "values": { "chat": { "show_thinking": false } } });
    let mut id = 0;
    let mut next_id = move || {
        id += 1;
        id
    };

    for step in ["at boot", "after a runtime set", "after a refused save"] {
        let origin = conn
            .call_method(
                "config.origin",
                serde_json::json!({ "key": key }),
                next_id(),
            )
            .await;
        let origin = &origin["result"];
        let save = conn
            .call_method("config.save", values.clone(), next_id())
            .await;
        let save = &save["result"];
        let refused = save["refused"]
            .as_array()
            .and_then(|rows| rows.first())
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        assert_eq!(
            origin["pinned"],
            serde_json::json!(!refused.is_null()),
            "{step}: the lock the control renders must be the refusal the save gives: \
             origin={origin} save={save}"
        );
        assert_eq!(
            origin["source"], refused["source"],
            "{step}: origin and refusal must name one source: origin={origin} save={save}"
        );
        assert_eq!(
            origin["file"], refused["file"],
            "{step}: the jump-to-pin must open the file the refusal names: \
             origin={origin} save={save}"
        );
        assert_eq!(
            origin["line"], refused["line"],
            "{step}: and the line: origin={origin} save={save}"
        );

        if step == "at boot" {
            // The steps are only distinct if the runtime set really lands.
            let set = conn
                .call_method("config.set", values.clone(), next_id())
                .await;
            assert_eq!(
                set["result"]["ok"],
                serde_json::json!(true),
                "a runtime set must never refuse a pinned key: {set}"
            );
        }
    }
}
