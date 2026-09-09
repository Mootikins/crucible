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
                .header("x-crucible-plugin", "app")
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
                .header("x-crucible-plugin", "app")
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
                .header("x-crucible-plugin", "app")
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
                .header("x-crucible-plugin", "app")
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
                .header("x-crucible-plugin", "app")
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
                    .header("x-crucible-plugin", "app")
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
                .header("x-crucible-plugin", "app")
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
                .header("x-crucible-plugin", "app")
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
                .header("x-crucible-plugin", "app")
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

// ── the caller-identity seam ────────────────────────────────────────────────
//
// `routes/plugin_caller.rs` says at length what this is not: a block is
// same-origin script and can send any header, so none of these tests prove a
// hostile block is stopped. They prove the three-valued identity behaves —
// app, a named plugin, absent — and that absent is a refusal.
//
// The last part is the one that matters. Answer "allowed" for a request with
// no identity and every other test below still passes, because a block would
// then bypass the check by omitting the header rather than by forging one.

/// Every gated route, with a request body where the route needs one.
///
/// One table, driven by every test in this section, so a route that grows a
/// check is exercised for omission, for the app, and for a plugin without
/// three separate lists drifting apart.
fn gated_routes() -> Vec<(&'static str, &'static str, Option<&'static str>)> {
    vec![
        (
            "POST",
            "/api/plugins/command",
            Some(r#"{"name":"mock_command","args":{}}"#),
        ),
        (
            "POST",
            "/api/plugins/mock-plugin/option",
            Some(r#"{"action":"get","path":["image"]}"#),
        ),
        ("POST", "/api/plugins", Some(r#"{"url":"user/repo"}"#)),
        ("DELETE", "/api/plugins/mock-plugin", None),
        ("POST", "/api/plugins/mock-plugin/reload", None),
        ("GET", "/api/plugins/publications", None),
    ]
}

fn request_as(
    caller: Option<&str>,
    method: &str,
    uri: &str,
    body: Option<&'static str>,
) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(caller) = caller {
        builder = builder.header("x-crucible-plugin", caller);
    }
    match body {
        Some(json) => builder
            .header("content-type", "application/json")
            .body(Body::from(json))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

/// **The load-bearing one.** A caller that names nobody is refused.
///
/// Two live callers had no plugin identity when this landed, so the obvious
/// reading — "no identity means the app" — was available and would have made
/// omission the way past every other check here.
#[tokio::test]
async fn a_request_naming_no_caller_is_refused_on_every_gated_route() {
    for (method, uri, body) in gated_routes() {
        let (_mock, client) = start_mock_daemon().await;
        let app = build_test_app(build_mock_state(client));

        let response = app
            .oneshot(request_as(None, method, uri, body))
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {uri} answered a caller that named nobody"
        );
    }
}

/// An empty header value names nobody either — a client that built the header
/// from an unset variable must not read as the app.
#[tokio::test]
async fn an_empty_caller_header_names_nobody() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(request_as(
            Some("   "),
            "GET",
            "/api/plugins/publications",
            None,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// The app reaches every gated route. `api.ts` and the plugins panel declare
/// `app`, and this is the test that fails if the seam narrows them.
#[tokio::test]
async fn the_app_reaches_every_gated_route() {
    for (method, uri, body) in gated_routes() {
        let (_mock, client) = start_mock_daemon().await;
        let app = build_test_app(build_mock_state(client));

        let response = app
            .oneshot(request_as(Some("app"), method, uri, body))
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "{method} {uri} refused the app"
        );
    }
}

/// A plugin invokes its own command. `plugin.commands` records the owner, so
/// the check is a comparison against what the daemon already knows.
#[tokio::test]
async fn a_plugin_may_invoke_its_own_command() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(request_as(
            Some("mock-plugin"),
            "POST",
            "/api/plugins/command",
            Some(r#"{"name":"mock_command","args":{}}"#),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_plugin_may_not_invoke_another_plugins_command() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(request_as(
            Some("other-plugin"),
            "POST",
            "/api/plugins/command",
            Some(r#"{"name":"mock_command","args":{}}"#),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// A command no loaded plugin owns cannot be attributed to the caller either.
#[tokio::test]
async fn a_plugin_may_not_invoke_a_command_nobody_owns() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(request_as(
            Some("mock-plugin"),
            "POST",
            "/api/plugins/command",
            Some(r#"{"name":"no_such_command","args":{}}"#),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// `{name}` on the option route is caller-supplied, which is the whole hole:
/// without the comparison a block reads and writes any plugin's settings by
/// naming it.
#[tokio::test]
async fn a_plugin_reaches_its_own_settings_and_no_others() {
    for (caller, expected) in [
        ("mock-plugin", StatusCode::OK),
        ("other-plugin", StatusCode::FORBIDDEN),
    ] {
        let (_mock, client) = start_mock_daemon().await;
        let app = build_test_app(build_mock_state(client));

        let response = app
            .oneshot(request_as(
                Some(caller),
                "POST",
                "/api/plugins/mock-plugin/option",
                Some(r#"{"action":"set","path":["image"],"value":"debian"}"#),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), expected, "caller {caller}");
    }
}

/// Install, remove and reload are the app's. Installing clones code from a URL
/// the caller chose and loads it, which is a larger hole than calling another
/// plugin's command and sits behind the same cookie.
#[tokio::test]
async fn the_lifecycle_routes_refuse_a_plugin() {
    for (method, uri, body) in gated_routes() {
        // The two per-plugin routes are covered above; these three are the
        // app-only ones.
        let app_only = matches!(
            (method, uri),
            ("POST", "/api/plugins")
                | ("DELETE", "/api/plugins/mock-plugin")
                | ("POST", "/api/plugins/mock-plugin/reload")
        );
        if !app_only {
            continue;
        }

        let (_mock, client) = start_mock_daemon().await;
        let app = build_test_app(build_mock_state(client));

        // `mock-plugin` is the plugin the route names, so this is not refused
        // for naming someone else — a plugin has no business here at all.
        let response = app
            .oneshot(request_as(Some("mock-plugin"), method, uri, body))
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {uri} let a plugin in"
        );
    }
}

/// A plugin reading publications sees its own rows and nothing else, with or
/// without `?key=`. The app keeps the unnarrowed answer, which is what
/// `PluginBlockPanel` enumerates every published block from.
#[tokio::test]
async fn a_plugin_reads_only_its_own_publications() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_mock_state(client));

    let response = app
        .oneshot(request_as(
            Some("mock-plugin"),
            "GET",
            "/api/plugins/publications",
            None,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = response_json(response).await;
    let publications = &json["publications"];
    assert!(
        publications.get("everything").is_some(),
        "its own row should survive, got {json}"
    );
    assert!(
        publications.get("and-more").is_none(),
        "another plugin's row must not come back, got {json}"
    );
}
