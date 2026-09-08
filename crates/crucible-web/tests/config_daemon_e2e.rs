//! `/api/config`, over a real daemon that really evaluated an `init.lua`.
//!
//! The mock-backed route tests prove the forwarding. They cannot prove the
//! gate, because the gate is a property of the daemon's store: which layer
//! holds a leaf decides whether a save is refused, and the layer is decided by
//! the chunk name of the file that wrote it. Only a boot evaluation from a
//! real config root produces that, so this test performs one and binds a
//! daemon on it — the same two calls `cru daemon serve` makes.
//!
//! One test, one daemon, both halves. The app-config store is process-global
//! in `crucible-lua`, so a second boot evaluation in this process would
//! replace the store this one planted.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use crucible_daemon::{BindWithPluginConfigParams, DaemonClient, Server};
use crucible_web::test_support::{build_mock_state_with_config, build_test_app};
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

/// The human's own line, and a comment above it, so the reported line number
/// is the real one rather than a constant that happens to be 1.
const INIT_LUA: &str =
    "-- the human's own preference\ncru.config.set { chat = { show_thinking = true } }\n";

/// The leaf `init.lua` holds. A save of it must be refused.
const PINNED_KEY: &str = "chat.show_thinking";

/// A leaf nothing pins. A save of it must survive to the next read.
const FREE_MODEL: &str = "saved-from-the-browser";

#[tokio::test]
async fn a_pinned_key_is_refused_by_its_line_and_a_free_key_round_trips() {
    let home = tempfile::tempdir().expect("a temp home");
    let config_root = home.path().join("config");
    std::fs::create_dir_all(&config_root).expect("the config root");
    // The boot refuses a named config file that does not exist, so the
    // deprecated seed file is created empty; `init.lua` beside it is what this
    // test is about.
    let config_source = config_root.join("config.toml");
    std::fs::write(&config_source, "").expect("the seed file");
    std::fs::write(config_root.join("init.lua"), INIT_LUA).expect("the user's file");

    let app = serve_daemon_over_web(home.path(), &config_source).await;

    // The user's line holds the leaf, and the browser is told so.
    let before = get_config(&app).await;
    assert_eq!(
        before["config"]["chat"]["show_thinking"],
        serde_json::json!(true),
        "init.lua must own the leaf, or this test asserts nothing: {before}"
    );
    let pin = origin_row(&before, PINNED_KEY);
    assert_eq!(pin["source"], serde_json::json!("lua"), "{before}");
    assert_eq!(pin["line"], serde_json::json!(2), "{before}");
    assert!(
        pin["file"]
            .as_str()
            .is_some_and(|file| file.ends_with("init.lua")),
        "{before}"
    );
    assert_eq!(
        pin["pinned"],
        serde_json::json!(true),
        "the browser locks a control from this flag, not from the source word: {before}"
    );
    // And the control it locks is declared, so the lock has something to sit on.
    assert!(
        control_exists(&before["controls"]["options"], PINNED_KEY),
        "the declared tree must carry the key the browser locks: {before}"
    );

    // A save of that leaf is refused, and the refusal names the line to edit.
    let refusal = save_config(
        &app,
        serde_json::json!({ "chat": { "show_thinking": false } }),
    )
    .await;
    assert_eq!(
        refusal["ok"],
        serde_json::json!(false),
        "a leaf init.lua holds cannot be saved: {refusal}"
    );
    let refused = &refusal["refused"][0];
    assert_eq!(refused["key"], serde_json::json!(PINNED_KEY), "{refusal}");
    assert_eq!(refused["source"], serde_json::json!("lua"), "{refusal}");
    assert_eq!(refused["line"], serde_json::json!(2), "{refusal}");
    assert!(
        refused["file"]
            .as_str()
            .is_some_and(|file| file.ends_with("init.lua")),
        "the browser offers a jump to the pin, so the file must arrive: {refusal}"
    );
    assert_eq!(
        get_config(&app).await["config"]["chat"]["show_thinking"],
        serde_json::json!(true),
        "and the refused value must not have landed anyway"
    );

    // A leaf nothing pins saves, and the next read serves it back.
    let saved = save_config(&app, serde_json::json!({ "chat": { "model": FREE_MODEL } })).await;
    assert_eq!(
        saved["ok"],
        serde_json::json!(true),
        "nothing holds chat.model: {saved}"
    );
    assert_eq!(saved["refused"], serde_json::json!([]), "{saved}");

    let after = get_config(&app).await;
    assert_eq!(
        after["config"]["chat"]["model"],
        serde_json::json!(FREE_MODEL),
        "a saved value must reach the next GET: {after}"
    );
    assert_eq!(
        origin_row(&after, "chat.model")["source"],
        serde_json::json!("settings"),
        "and it must land in the layer settings.json holds: {after}"
    );
    assert_eq!(
        origin_row(&after, "chat.model")["pinned"],
        serde_json::json!(false),
        "the layer a save writes cannot pin against itself: {after}"
    );
    assert!(
        config_root.join("settings.json").exists(),
        "the save outlives the process, so it wrote the file"
    );
}

/// Whether the declared control tree holds a leaf at this dot-joined path.
fn control_exists(node: &Value, path: &str) -> bool {
    if node["path"] == serde_json::json!(path) {
        return true;
    }
    node["args"]
        .as_array()
        .is_some_and(|args| args.iter().any(|child| control_exists(child, path)))
}

/// Boot a daemon the way `cru daemon serve` does — evaluate the config root,
/// bind the socket with the VM that evaluated it — and answer with a web app
/// whose client talks to it.
async fn serve_daemon_over_web(home: &std::path::Path, config_source: &std::path::Path) -> Router {
    // No plugin search path: the injected resolver is what keeps this
    // evaluation away from the developer's own plugin directories.
    let boot = crucible_daemon::daemon_plugins::evaluate_boot_config_with_paths(
        Some(config_source.to_path_buf()),
        None,
        None,
        Arc::new(|_| Vec::new()),
    )
    .await
    .expect("the boot evaluation");
    assert_eq!(boot.eval_error, None, "the fixture init.lua must evaluate");

    let socket = home.join("daemon.sock");
    let server = Server::bind_with_plugin_config(
        BindWithPluginConfigParams {
            path: socket.clone(),
            app_config: serde_json::to_value(&boot.config).ok(),
            config_path: Some(boot.config_source.clone()),
            boot_hash: Some(boot.boot_hash.clone()),
            data_home: Some(home.join("data")),
            config_home: Some(home.join("config-home")),
            ..Default::default()
        }
        .with_loader(boot.loader),
    )
    .await
    .expect("the daemon binds");
    tokio::spawn(async move {
        let _ = server.run().await;
    });

    let client = connect(&socket).await;
    // The helper only wraps an `AppState` around a client; the client here is
    // a real daemon's rather than the mock's.
    build_test_app(build_mock_state_with_config(
        client,
        crucible_core::config::CliAppConfig::default(),
    ))
}

/// Connect once the daemon accepts, rather than after a fixed sleep.
async fn connect(socket: &std::path::Path) -> DaemonClient {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match DaemonClient::connect_to(socket).await {
            Ok(client) => return client,
            Err(e) if tokio::time::Instant::now() >= deadline => {
                panic!("the daemon never accepted a connection: {e}")
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
        }
    }
}

async fn get_config(app: &Router) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

async fn save_config(app: &Router, values: Value) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/config")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "values": values }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    json_body(response).await
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

/// The `origins` row for one leaf, or a failure naming the whole answer.
fn origin_row<'a>(config: &'a Value, key: &str) -> &'a Value {
    config["origins"]
        .as_array()
        .unwrap_or_else(|| panic!("origins is a row per leaf: {config}"))
        .iter()
        .find(|row| row["key"] == serde_json::json!(key))
        .unwrap_or_else(|| panic!("no origin row for {key}: {config}"))
}
