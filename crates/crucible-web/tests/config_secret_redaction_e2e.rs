//! No credential leaves the web API through `/api/config`.
//!
//! `GET /api/config` serves the daemon's whole effective config, plus one
//! origin row per recorded leaf. Both carry `llm.providers.*.api_key` and
//! `web.api_key`, and neither type redacts on `Serialize` — only on `Debug`.
//! `bearer_auth` waves loopback callers through, so without a redaction pass
//! one unprivileged GET hands the browser every provider key and the web
//! server's own long-lived key.
//!
//! A real daemon is the only place this can be proven: the values must come
//! from a boot evaluation of a real `init.lua`, through the store, back out of
//! `config.effective` and `config.origin`, exactly as they reach a browser.
//!
//! One test, one daemon. The app-config store is process-global in
//! `crucible-lua`, so a second boot evaluation in this process would replace
//! the store this one planted — which is also why this file is a separate
//! binary from `config_daemon_e2e.rs`.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use crucible_daemon::{BindWithPluginConfigParams, DaemonClient, Server};
use crucible_web::test_support::{build_mock_state_with_config, build_test_app};
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

/// The provider credential. Distinctive enough that a substring search over
/// the whole body cannot match it by accident.
const PROVIDER_KEY: &str = "sk-provider-must-never-reach-the-browser";

/// The web server's own long-lived key — the one that authenticates every
/// LAN client, so serving it to a caller that already holds it is not the
/// point: serving it to a LOOPBACK caller that holds nothing is.
const WEB_KEY: &str = "web-api-key-must-never-reach-the-browser";

fn init_lua() -> String {
    format!(
        "cru.config.set {{\n  \
           llm = {{ providers = {{ openai = {{ type = \"openai\", api_key = \"{PROVIDER_KEY}\" }} }} }},\n  \
           web = {{ api_key = \"{WEB_KEY}\" }},\n\
         }}\n"
    )
}

#[tokio::test]
async fn no_api_key_reaches_the_browser_through_the_config_route() {
    let home = tempfile::tempdir().expect("a temp home");
    let config_root = home.path().join("config");
    std::fs::create_dir_all(&config_root).expect("the config root");
    // The boot refuses a named config file that does not exist, so the
    // deprecated seed file is created empty; `init.lua` beside it is what this
    // test is about.
    let config_source = config_root.join("config.toml");
    std::fs::write(&config_source, "").expect("the seed file");
    std::fs::write(config_root.join("init.lua"), init_lua()).expect("the user's file");

    let app = serve_daemon_over_web(home.path(), &config_source).await;
    let body = get_config(&app).await;

    // The leaves must actually be there, or the search below proves nothing.
    assert_eq!(
        body["config"]["llm"]["providers"]["openai"]["type"],
        serde_json::json!("openai"),
        "init.lua must reach the answer, or this test asserts nothing: {body}"
    );
    assert!(
        !body["config"]["llm"]["providers"]["openai"]["api_key"].is_null(),
        "the provider's api_key leaf must survive, redacted rather than dropped: {body}"
    );
    assert!(
        !body["config"]["web"]["api_key"].is_null(),
        "the web api_key leaf must survive, redacted rather than dropped: {body}"
    );

    // The whole body, not two named paths: a credential also rides in the
    // `origins` rows, whose field is called `value` and would slip past any
    // check that only looks at keys named `api_key`.
    let text = body.to_string();
    assert!(
        !text.contains(PROVIDER_KEY),
        "a provider api_key reached the browser: {text}"
    );
    assert!(
        !text.contains(WEB_KEY),
        "the web server's own api_key reached the browser: {text}"
    );
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
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
