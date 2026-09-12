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

/// A `config.set` records WHICH client wrote the leaf.
///
/// `ConfigSource::Rpc` carries `chan`, the dispatcher's `ClientId`, after
/// `sctx_T`'s `sc_chan` — so `cru config show --sources` and the settings pane
/// can tell one client's runtime write from another's rather than seeing one
/// flat `rpc` row.
///
/// This has to cross the socket. The channel id is assigned per CONNECTION, so
/// an in-process test on either half cannot observe it: the dispatcher's
/// `client_id` argument is the only place it exists, and a unit test would have
/// to invent one.
///
/// Two connections, so the test proves the id DISTINGUISHES clients rather
/// than merely being present. A hardcoded expected number would be wrong — the
/// counter is process-wide and other connections advance it — so the assertion
/// is that the two rows disagree.
#[tokio::test]
async fn a_config_set_records_which_client_wrote_the_leaf() {
    let daemon = TestDaemon::start().await.expect("daemon starts");

    let channel_for = |key: &'static str, value: bool| {
        let socket = daemon.socket_path.clone();
        async move {
            let mut conn = RpcConn::connect(&socket).await.expect("connect");
            let set = conn
                .call_method(
                    "config.set",
                    serde_json::json!({ "values": { "chat": { key: value } } }),
                    1,
                )
                .await;
            assert_eq!(set["result"]["ok"], serde_json::json!(true), "{set}");

            let effective = conn
                .call_method("config.effective", serde_json::json!({}), 2)
                .await;
            let row = effective["result"]["provenance"][format!("chat.{key}")].clone();
            assert!(
                row["rpc"].is_object(),
                "a config.set must record the `rpc` layer for chat.{key}: {effective}"
            );
            row["rpc"]["chan"]
                .as_u64()
                .unwrap_or_else(|| panic!("the rpc row must name a channel: {row}"))
        }
    };

    let first = channel_for("show_thinking", true).await;
    let second = channel_for("stream", false).await;
    assert_ne!(
        first, second,
        "two connections must record two channel ids, or the field \
         distinguishes nothing"
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

/// `config.unset`. A stale provider goes, and its siblings — plus the one the
/// human's own `init.lua` declares — stand.
///
/// The verb a flat store needs, across the process seam. `config.set` writes
/// one leaf per value, so it can add `llm.providers.stale.endpoint` and change
/// it, but it can never say the provider is gone. Only a real daemon shows
/// that the removal reaches the reader: the store, the effective config and
/// the origin row are three doors onto one answer.
#[tokio::test]
async fn config_unset_removes_one_provider_and_leaves_the_others() {
    let daemon = TestDaemon::start_with_home_setup(|home| {
        let config_dir = home.join(".config").join("crucible");
        std::fs::create_dir_all(&config_dir)?;
        std::fs::write(
            config_dir.join("init.lua"),
            "cru.config.set { llm = { providers = { keeper = \
             { type = \"ollama\", endpoint = \"http://keeper\" } } } }\n",
        )?;
        Ok(())
    })
    .await
    .expect("daemon starts");
    let mut conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to the daemon");

    conn.call_method(
        "config.set",
        serde_json::json!({ "values": { "llm": { "providers": {
            "stale": { "type": "ollama", "endpoint": "http://stale", "default_model": "m" },
            "fresh": { "type": "ollama", "endpoint": "http://fresh" }
        } } } }),
        1,
    )
    .await;
    let before = conn
        .call_method("config.get", serde_json::json!({}), 2)
        .await;
    assert_eq!(
        before["result"]["config"]["llm"]["providers"]["stale"]["endpoint"],
        serde_json::json!("http://stale"),
        "the stale provider must be there before the unset removes it: {before}"
    );

    let unset = conn
        .call_method(
            "config.unset",
            serde_json::json!({ "key": "llm.providers.stale" }),
            3,
        )
        .await;
    assert_eq!(
        unset["result"]["outcome"],
        serde_json::json!("dropped"),
        "{unset}"
    );
    assert_eq!(
        unset["result"]["dropped"],
        serde_json::json!(["rpc"]),
        "an unset reaches the layers a reset reaches, and no file: {unset}"
    );

    let after = conn
        .call_method("config.get", serde_json::json!({}), 4)
        .await;
    let providers = &after["result"]["config"]["llm"]["providers"];
    assert!(
        providers.get("stale").is_none(),
        "the whole map entry must go, not just the leaf named: {after}"
    );
    assert_eq!(
        providers["fresh"]["endpoint"],
        serde_json::json!("http://fresh"),
        "a sibling the caller did not name stands: {after}"
    );
    assert_eq!(
        providers["keeper"]["endpoint"],
        serde_json::json!("http://keeper"),
        "and so does the provider the human's own init.lua declares: {after}"
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
