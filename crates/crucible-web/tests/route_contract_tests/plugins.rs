//! Plugin route contract tests (with mock daemon).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use super::shared::{build_mock_state, build_test_app, start_mock_daemon};

async fn response_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap_or(Value::Null)
}

#[tokio::test]
async fn list_plugins_returns_rich_plugin_info() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/plugins")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["plugins"].is_array());

    let plugin = &json["plugins"][0];
    assert_eq!(plugin["name"], "mock-plugin");
    assert_eq!(plugin["version"], "0.1.0");
    assert_eq!(plugin["source"], "User");
    assert_eq!(plugin["state"], "Active");
    assert_eq!(plugin["tools"], 3);
    assert_eq!(plugin["commands"], 1);
    assert_eq!(plugin["handlers"], 2);
    assert_eq!(plugin["services"], 0);
}

#[tokio::test]
async fn reload_plugin_returns_counts() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/plugins/mock-plugin/reload")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "mock-plugin");
    assert_eq!(json["reloaded"], true);
    assert_eq!(json["tools"], 3);
}

#[tokio::test]
async fn install_plugin_returns_200_with_outcome() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/plugins")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "url": "user/repo" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "installed-plugin");
    assert_eq!(json["outcome"]["kind"], "cloned");
}

#[tokio::test]
async fn install_plugin_rejects_empty_url() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/plugins")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::json!({ "url": "" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(response.status().is_client_error());
}

#[tokio::test]
async fn remove_plugin_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/plugins/some-plugin")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "removed-plugin");
}

#[tokio::test]
async fn remove_plugin_with_purge_query_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/plugins/some-plugin?purge=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// The settings tree reaches the browser unchanged.
///
/// The whole point of `cru.plugin.options` is that a plugin declares its
/// settings once and every frontend renders them. If this layer reshapes a
/// tree — flattening it, dropping a field it does not recognise — the web pane
/// stops being a projection of the plugin's declaration and becomes a second
/// schema to keep in step.
#[tokio::test]
async fn plugin_options_reach_the_client_verbatim() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/plugins/options")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let tree = &json["options"]["mock-plugin"];
    assert_eq!(tree["type"], "group");
    let args = tree["args"].as_array().expect("args survive as an array");
    assert_eq!(args.len(), 2);
    assert_eq!(args[0]["key"], "image");
    assert_eq!(args[0]["writable"], true);
    // The button, with the ordering hint the daemon resolved.
    assert_eq!(args[1]["type"], "execute");
    assert_eq!(args[1]["order"], -1);
}

#[tokio::test]
async fn an_option_read_write_and_press_each_reach_the_daemon() {
    for (action, expected) in [
        (
            serde_json::json!({"action": "get", "path": ["image"]}),
            "value",
        ),
        (
            serde_json::json!({"action": "set", "path": ["image"], "value": "debian"}),
            "ok",
        ),
        (
            serde_json::json!({"action": "execute", "path": ["cleanup"]}),
            "ok",
        ),
    ] {
        let (_mock, client) = start_mock_daemon().await;
        let app = build_test_app(build_mock_state(client));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/plugins/mock-plugin/option")
                    .header("content-type", "application/json")
                    .body(Body::from(action.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK, "action {action}");
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json.get(expected).is_some(),
            "action {action} should answer with `{expected}`, got {json}"
        );
    }
}

/// An empty path would address the tree's ROOT, where `get`/`set` are the
/// inherited accessors every leaf shares — writing there means nothing and the
/// plugin's setter would be called with no option to route on.
#[tokio::test]
async fn an_option_call_naming_no_path_is_rejected() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/plugins/mock-plugin/option")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"action":"get","path":[]}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // 422, the shape every other `WebError::Validation` takes here.
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

/// `?key=` narrows DAEMON-side, not in the client.
///
/// The daemon has always accepted a `key`; the route did not pass one, so
/// every caller received every plugin's published data and filtered it in the
/// browser. That is more than a block drawing one key needs, and once
/// third-party block code can run on this origin it is more than it should
/// receive. Drop the query wiring and this test sees the unnarrowed answer.
#[tokio::test]
async fn a_publications_key_reaches_the_daemon() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/plugins/publications?key=kanban:board")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    let publications = &json["publications"];
    assert!(
        publications.get("kanban:board").is_some(),
        "the asked-for key should be the answer, got {json}"
    );
    assert!(
        publications.get("and-more").is_none(),
        "another plugin's key must not come back, got {json}"
    );
}

/// Without a key the answer is everything, which is what the plugins panel
/// wants and what a single block must not ask for.
#[tokio::test]
async fn publications_without_a_key_still_answers_everything() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/plugins/publications")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let json = response_json(response).await;
    assert!(json["publications"].get("and-more").is_some(), "got {json}");
}

/// The enumeration a button needs, with the declared parameters a dialog would
/// be generated from.
///
/// `commands_json` has always emitted this — `plugin.commands` is a daemon RPC
/// method — but no HTTP route carried it, so a browser could invoke a command
/// it had no way to discover. `GET /api/commands` is a different thing: it
/// returns the hardcoded `SLASH_COMMANDS` const, not plugin commands.
#[tokio::test]
async fn plugin_commands_are_enumerable_with_their_parameters() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/plugins/commands")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    let commands = json["commands"].as_array().expect("an array of commands");
    assert_eq!(commands.len(), 1);

    let command = &commands[0];
    assert_eq!(command["name"], "mock_command");
    assert_eq!(command["plugin"], "mock-plugin");

    // The parameters are the point: without them a caller can invoke a command
    // but cannot ask a user for its arguments.
    let params = command["parameters"]
        .as_array()
        .expect("declared parameters, not an opaque blob");
    assert_eq!(params.len(), 2);
    assert_eq!(params[0]["name"], "target");
    assert_eq!(params[1]["optional"], true);
}
