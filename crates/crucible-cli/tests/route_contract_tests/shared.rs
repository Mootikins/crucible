//! Mock daemon infrastructure shared across route contract tests.

use axum::Router;
use crucible_cli::web::routes::{
    chat_routes, health_routes, plugin_routes, project_routes, search_routes, session_routes,
    skills_routes,
};
use crucible_cli::web::services::daemon::{AppState, EventBroker, ReconnectingDaemon};
use crucible_core::config::CliAppConfig;
use crucible_daemon::DaemonClient;
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

// =========================================================================
// Mock Daemon Infrastructure
// =========================================================================

/// A mock daemon that listens on a Unix socket and responds to JSON-RPC calls
/// with canned responses. This allows testing HTTP routes without a real daemon.
pub(super) struct MockDaemon {
    _tmp: TempDir,
}

/// Start a mock daemon on a temporary Unix socket. Returns the mock daemon
/// handle (holds TempDir alive) and a connected DaemonClient.
pub(super) async fn start_mock_daemon() -> (MockDaemon, DaemonClient) {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    let socket_path = tmp.path().join("mock-daemon.sock");

    let listener = UnixListener::bind(&socket_path).expect("Failed to bind mock socket");

    // Spawn mock daemon server
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let (read, mut write) = stream.into_split();
                let mut reader = BufReader::new(read);
                let mut line = String::new();

                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            let msg: Value = match serde_json::from_str(&line) {
                                Ok(m) => m,
                                Err(_) => continue,
                            };

                            let id = msg.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                            let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");

                            let result = mock_rpc_response(method, &msg);

                            let response = json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": result
                            });

                            let mut resp_str = serde_json::to_string(&response).unwrap();
                            resp_str.push('\n');

                            if write.write_all(resp_str.as_bytes()).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }
    });

    // Give the listener a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    let client = DaemonClient::connect_to(&socket_path)
        .await
        .expect("Failed to connect to mock daemon");

    (MockDaemon { _tmp: tmp }, client)
}

/// Generate mock RPC responses based on method name.
pub(super) fn mock_rpc_response(method: &str, _msg: &Value) -> Value {
    match method {
        "kiln.list" => json!([]),
        "list_notes" => json!([]),
        "get_note_by_name" => Value::Null,
        "note.upsert" => json!({}),
        "search_vectors" => json!([]),
        "session.create" => json!({"session_id": "test-session-001"}),
        "session.list" => json!([]),
        "session.get" => json!({
            "session_id": "test-session-001",
            "state": "active",
            "session_type": "chat",
            "kiln": "/tmp/test-kiln"
        }),
        "session.pause" => json!({"ok": true}),
        "session.resume" => json!({"ok": true}),
        "session.end" => json!({"ok": true}),
        "session.cancel" => json!({"cancelled": true}),
        "session.delete" => json!({"deleted": true}),
        "session.archive" => json!({"archived": true}),
        "session.unarchive" => json!({"archived": false}),
        "session.subscribe" => json!(null),
        "session.configure_agent" => json!(null),
        "session.send_message" => json!({"message_id": "msg-001"}),
        "session.interaction_respond" => json!(null),
        "session.list_models" => json!({"models": ["llama3.2", "mistral"]}),
        "session.switch_model" => json!(null),
        "session.set_title" => json!(null),
        "session.search" => json!([{"session_id": "s1", "title": "Test Session"}]),
        // Real daemon shape: `history` array of session events (not `messages`)
        "session.resume_from_storage" => {
            json!({"session_id": "test-session-001", "history": [], "total_events": 0})
        }
        "project.list" => json!([]),
        "project.register" => json!({
            "path": "/tmp/test-project",
            "name": "test-project",
            "kilns": [],
            "last_accessed": "2025-01-01T00:00:00Z"
        }),
        "project.unregister" => json!(null),
        "project.get" => Value::Null,
        "session.set_thinking_budget" => json!(null),
        "session.get_thinking_budget" => json!({"thinking_budget": 1024}),
        "session.set_temperature" => json!(null),
        "session.get_temperature" => json!({"temperature": 0.7}),
        "session.set_max_tokens" => json!(null),
        "session.get_max_tokens" => json!({"max_tokens": 4096}),
        "session.set_precognition" => json!(null),
        "session.get_precognition" => json!({"precognition_enabled": true}),
        "session.set_precognition_results" => json!(null),
        "session.get_precognition_results" => json!({"precognition_results": 5}),
        "session.render_markdown" => json!({"markdown": "# Test Session\n\nExported content"}),
        "providers.list" => json!({"providers": []}),
        "plugin.list" => json!({
            "plugins": ["mock-plugin"],
            "plugin_info": [{
                "name": "mock-plugin",
                "version": "0.1.0",
                "source": "User",
                "state": "Active",
                "dir": "/tmp/mock-plugin",
                "tools": 3,
                "commands": 1,
                "handlers": 2,
                "services": 0,
            }],
        }),
        "plugin.reload" => json!({
            "name": "mock-plugin",
            "reloaded": true,
            "tools": 3,
            "commands": 1,
            "handlers": 2,
            "services": 0,
        }),
        "plugin.install" => json!({
            "name": "installed-plugin",
            "outcome": { "kind": "cloned", "dest": "/tmp/installed-plugin" },
            "plugins_toml": "/tmp/plugins.toml",
        }),
        "plugin.remove" => json!({
            "name": "removed-plugin",
            "plugins_toml": "/tmp/plugins.toml",
            "purged_dir": Value::Null,
        }),
        "skills.list" => json!({
            "skills": [
                {
                    "name": "test-skill",
                    "scope": "user",
                    "description": "A test skill",
                    "shadowed_count": 0,
                }
            ]
        }),
        "skills.get" => json!({
            "name": "test-skill",
            "scope": "user",
            "description": "A test skill",
            "source_path": "/tmp/skill.md",
            "agent": Value::Null,
            "license": Value::Null,
            "body": "# Test Skill\n\nContent.",
        }),
        "skills.search" => json!({
            "skills": [
                {
                    "name": "matched-skill",
                    "scope": "user",
                    "description": "Matched",
                    "shadowed_count": 0,
                }
            ]
        }),
        _ => json!(null),
    }
}

/// Build an AppState using a mock daemon client.
pub(super) fn build_mock_state(client: DaemonClient) -> AppState {
    AppState {
        daemon: Arc::new(ReconnectingDaemon::new(client)),
        events: Arc::new(EventBroker::new()),
        config: Arc::new(CliAppConfig::default()),
        http_client: reqwest::Client::new(),
        layout_path: Arc::new(std::env::temp_dir().join(format!(
            "crucible-contract-layout-{}.json",
            std::process::id()
        ))),
    }
}

/// Build the full app router with mock state.
pub(super) fn build_test_app(state: AppState) -> Router {
    Router::new()
        .merge(chat_routes())
        .merge(session_routes())
        .merge(project_routes())
        .merge(search_routes())
        .merge(skills_routes())
        .merge(plugin_routes())
        .with_state(state)
        .merge(health_routes())
}
