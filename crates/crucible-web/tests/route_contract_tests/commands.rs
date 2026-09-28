//! Built-in command contract tests (with mock daemon)

use crucible_core::protocol::rpc::RpcMethod;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use super::shared::{build_state, build_test_app, start_mock_daemon};

/// POST one command line to the command route; answer the reply body.
async fn run(app: axum::Router, line: &str) -> Value {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/command")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "command": line }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK, "{line}");
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// GET the session's command catalog.
async fn catalog(app: axum::Router) -> Vec<Value> {
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/session/test-session-001/commands")
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
    json["commands"].as_array().unwrap().clone()
}

#[tokio::test]
async fn help_lists_the_session_catalog_with_the_source_of_each_command() {
    let (_mock, client) = start_mock_daemon().await;
    let json = run(build_test_app(build_state(client)), "/help").await;
    assert_eq!(json["type"], "success");
    let result = json["result"].as_str().unwrap();
    for expected in [
        "/help",
        "/search <query>",
        "/model [name]",
        "/clear",
        "/export",
    ] {
        assert!(result.contains(expected), "missing {expected}: {result}");
    }
    assert!(
        result.contains("/reflect — Run a reflection pass (plugin)"),
        "{result}"
    );
}

#[tokio::test]
async fn the_commands_route_answers_the_daemon_catalog() {
    let (_mock, client) = start_mock_daemon().await;
    let commands = catalog(build_test_app(build_state(client))).await;
    let names: Vec<&str> = commands
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"help") && names.contains(&"reflect"),
        "{names:?}"
    );
    assert_eq!(commands.last().unwrap()["kind"], "plugin");
}

/// Every built-in command of the catalog runs on the command route.
#[tokio::test]
async fn every_built_in_command_of_the_catalog_runs() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    for command in catalog(build_test_app(state.clone())).await {
        if command["kind"] != "builtin" {
            continue;
        }
        let name = command["name"].as_str().unwrap();
        let json = run(build_test_app(state.clone()), &format!("/{name}")).await;
        assert!(
            !json["result"]
                .as_str()
                .unwrap()
                .contains("not a built-in command"),
            "/{name} is in the catalog but the route does not run it"
        );
    }
}

/// A command that is not built in is a chat message; the route says so.
#[tokio::test]
async fn a_command_that_is_not_built_in_is_refused_with_a_reason() {
    let (_mock, client) = start_mock_daemon().await;
    let json = run(build_test_app(build_state(client)), "/reflect").await;
    assert_eq!(json["type"], "error");
    assert!(
        json["result"]
            .as_str()
            .unwrap()
            .contains("/reflect is not a built-in command"),
        "{json}"
    );
}

#[tokio::test]
async fn command_search_no_args_returns_error() {
    let (_mock, client) = start_mock_daemon().await;
    let json = run(build_test_app(build_state(client)), "/search").await;
    assert_eq!(json["type"], "error");
    assert!(json["result"].as_str().unwrap().contains("Usage"));
}

#[tokio::test]
async fn command_search_with_query_returns_results() {
    let (_mock, client) = start_mock_daemon().await;
    let json = run(build_test_app(build_state(client)), "/search test query").await;
    assert_eq!(json["type"], "success");
    let result = json["result"].as_str().unwrap();
    assert!(result.contains("test query"), "{result}");
    assert!(result.contains("Test Session"), "{result}");
}

/// `/model` with no name lists the models, as `/models` did.
#[tokio::test]
async fn model_without_a_name_lists_the_models() {
    let (_mock, client) = start_mock_daemon().await;
    let json = run(build_test_app(build_state(client)), "/model").await;
    assert_eq!(json["type"], "success");
    let result = json["result"].as_str().unwrap();
    assert!(
        result.contains("llama3.2") && result.contains("mistral"),
        "{result}"
    );
}

#[tokio::test]
async fn command_model_with_name_switches_model() {
    let (mock, client) = start_mock_daemon().await;
    let json = run(build_test_app(build_state(client)), "/model mistral").await;
    assert_eq!(json["type"], "success");
    assert!(json["result"].as_str().unwrap().contains("mistral"));
    assert_eq!(
        mock.received_params(RpcMethod::SessionSwitchModel).unwrap()["model_id"],
        "mistral"
    );
}

/// `/mode` moves to the mode after the current one in the daemon's list.
#[tokio::test]
async fn mode_switches_to_the_next_mode() {
    let (mock, client) = start_mock_daemon().await;
    let json = run(build_test_app(build_state(client)), "/mode").await;
    assert_eq!(json["result"], "Mode: plan");
    assert_eq!(
        mock.received_params(RpcMethod::SessionSetMode).unwrap()["mode_id"],
        "plan"
    );
}

#[tokio::test]
async fn undo_asks_the_daemon_for_the_turn_count() {
    let (mock, client) = start_mock_daemon().await;
    let state = build_state(client);
    let json = run(build_test_app(state.clone()), "/undo 2").await;
    assert_eq!(json["type"], "success");
    assert_eq!(
        mock.received_params(RpcMethod::SessionUndo).unwrap()["count"],
        2
    );
    let json = run(build_test_app(state), "/undo two").await;
    assert_eq!(json["type"], "error");
}

/// `/resume <id>` tells the browser which session to open.
#[tokio::test]
async fn resume_with_an_id_opens_that_session() {
    let (_mock, client) = start_mock_daemon().await;
    let json = run(build_test_app(build_state(client)), "/resume s-7").await;
    assert_eq!(json["open_session"], "s-7");
    let (_mock, client) = start_mock_daemon().await;
    let json = run(build_test_app(build_state(client)), "/resume").await;
    assert!(json.get("open_session").is_none(), "{json}");
}

#[tokio::test]
async fn command_export_returns_hint() {
    let (_mock, client) = start_mock_daemon().await;
    let json = run(build_test_app(build_state(client)), "/export").await;
    assert_eq!(json["type"], "success");
    assert!(json["result"].as_str().unwrap().contains("export"));
}

#[tokio::test]
async fn command_without_slash_prefix_works() {
    let (_mock, client) = start_mock_daemon().await;
    let json = run(build_test_app(build_state(client)), "help").await;
    assert_eq!(json["type"], "success");
    assert!(json["result"].as_str().unwrap().contains("/help"));
}

#[tokio::test]
async fn command_with_whitespace_padding_works() {
    let (_mock, client) = start_mock_daemon().await;
    let json = run(build_test_app(build_state(client)), "  /help  ").await;
    assert_eq!(json["type"], "success");
    assert!(json["result"].as_str().unwrap().contains("/help"));
}

/// `/clear` clears the model context through the daemon, with the same
/// command and the same arguments as the TUI's `/clear`.
#[tokio::test]
async fn command_clear_runs_the_daemon_clear_of_the_session() {
    let (mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_state(client));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/session/test-session-001/command")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"command":"/clear"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let params = mock
        .received_params(RpcMethod::SessionClear)
        .expect("the clear reaches the daemon");
    assert_eq!(params["session_id"], "test-session-001");
}
