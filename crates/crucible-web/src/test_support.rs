// The mock daemon + router helpers are shared with integration tests
// (tests/route_contract_tests/) via the `test-utils` feature — crucible-cli
// dev-depends on itself with that feature, so they compile in every test
// build without CI feature flags. Do NOT fork a second copy: the two copies
// this replaced drifted (different resume_from_storage shapes).
#[cfg(any(test, feature = "test-utils"))]
use crate::routes::{
    agents_routes, chat_routes, fs_routes, health_routes, project_routes, search_routes,
    session_routes,
};
#[cfg(any(test, feature = "test-utils"))]
use crate::services::daemon::{AppState, EventBroker, ReconnectingDaemon};
#[cfg(any(test, feature = "test-utils"))]
use axum::Router;
#[cfg(any(test, feature = "test-utils"))]
use crucible_core::config::CliAppConfig;
#[cfg(any(test, feature = "test-utils"))]
use crucible_daemon::DaemonClient;
#[cfg(any(test, feature = "test-utils"))]
use serde_json::{json, Value};
#[cfg(any(test, feature = "test-utils"))]
use std::collections::HashMap;
#[cfg(any(test, feature = "test-utils"))]
use std::sync::Arc;
#[cfg(any(test, feature = "test-utils"))]
use tempfile::TempDir;
#[cfg(any(test, feature = "test-utils"))]
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(any(test, feature = "test-utils"))]
use tokio::net::UnixListener;

use std::net::Ipv4Addr;

use proptest::prelude::*;

pub fn arb_url_scheme() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("http".to_string()),
        Just("https".to_string()),
        Just("ftp".to_string()),
        Just("file".to_string()),
        Just("javascript".to_string()),
        Just("data".to_string()),
        Just("gopher".to_string()),
    ]
}

pub fn arb_ipv4_private() -> impl Strategy<Value = String> {
    prop_oneof![
        (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(b, c, d)| format!("10.{b}.{c}.{d}")),
        (16u8..=31, any::<u8>(), any::<u8>()).prop_map(|(b, c, d)| format!("172.{b}.{c}.{d}")),
        (any::<u8>(), any::<u8>()).prop_map(|(c, d)| format!("192.168.{c}.{d}")),
        (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(b, c, d)| format!("127.{b}.{c}.{d}")),
    ]
}

pub fn arb_ipv4_public() -> impl Strategy<Value = String> {
    any::<[u8; 4]>()
        .prop_map(Ipv4Addr::from)
        .prop_filter("must be routable/public-ish", |ip| {
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_broadcast()
                && !ip.is_unspecified()
        })
        .prop_map(|ip| ip.to_string())
}

pub fn arb_ipv6_loopback() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("::1".to_string()),
        Just("0:0:0:0:0:0:0:1".to_string()),
        Just("[::1]".to_string()),
        Just("[0:0:0:0:0:0:0:1]".to_string()),
    ]
}

pub fn arb_hostname() -> impl Strategy<Value = String> {
    let valid = "[a-zA-Z0-9-]{1,20}(\\.[a-zA-Z0-9-]{1,20}){0,3}";
    prop_oneof![
        7 => valid.prop_map(|s| s.to_ascii_lowercase()),
        1 => Just("".to_string()),
        1 => "[ .]{1,8}".prop_map(|s| s.to_string()),
        1 => "[a-zA-Z0-9]{1,12}_+[a-zA-Z0-9]{1,12}".prop_map(|s| s.to_string()),
    ]
}

pub fn arb_endpoint_url() -> impl Strategy<Value = String> {
    (
        arb_url_scheme(),
        prop_oneof![
            arb_ipv4_private(),
            arb_ipv4_public(),
            arb_hostname(),
            arb_ipv6_loopback(),
        ],
        prop::option::of(1u16..=65535),
        "(/[a-zA-Z0-9._~!$&'()*+,;=:@%-]{0,24}){0,4}",
    )
        .prop_map(|(scheme, host, port, path)| {
            port.map_or_else(
                || format!("{scheme}://{host}{path}"),
                |port| format!("{scheme}://{host}:{port}{path}"),
            )
        })
}

pub fn arb_traversal_path() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z0-9/_-]{0,24}\\.\\.[a-zA-Z0-9/_-]{0,24}".prop_map(|s| s.to_string()),
        "[a-zA-Z0-9/_-]{0,24}\\x00[a-zA-Z0-9/_-]{0,24}".prop_map(|s| s.to_string()),
        Just("../etc/passwd".to_string()),
        Just("safe/..\0/evil".to_string()),
    ]
}

pub fn arb_safe_path() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_-][a-zA-Z0-9/_-]{0,63}"
        .prop_filter("no traversal, null byte, or absolute path", |s| {
            !s.contains("..") && !s.contains('\0') && !s.starts_with('/')
        })
}

#[cfg(any(test, feature = "test-utils"))]
/// A mock daemon that listens on a Unix socket and responds to JSON-RPC calls
/// with canned responses. This allows testing HTTP routes without a real daemon.
pub struct MockDaemon {
    _tmp: TempDir,
}

#[cfg(any(test, feature = "test-utils"))]
/// Per-method scripted error envelopes: method name → (code, message).
/// Methods present here answer `{"error": {...}}` instead of a result, so
/// tests can exercise the daemon-error → HTTP-status surface.
pub type MockErrors = HashMap<String, (i64, String)>;

#[cfg(any(test, feature = "test-utils"))]
/// Start a mock daemon on a temporary Unix socket. Returns the mock daemon
/// handle (holds TempDir alive) and a connected DaemonClient.
pub async fn start_mock_daemon() -> (MockDaemon, DaemonClient) {
    start_mock_daemon_with_errors(MockErrors::new()).await
}

#[cfg(any(test, feature = "test-utils"))]
/// Like [`start_mock_daemon`], but methods listed in `errors` respond with a
/// JSON-RPC error envelope instead of their canned result.
pub async fn start_mock_daemon_with_errors(errors: MockErrors) -> (MockDaemon, DaemonClient) {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    let socket_path = tmp.path().join("mock-daemon.sock");

    let listener = UnixListener::bind(&socket_path).expect("Failed to bind mock socket");

    // Spawn mock daemon server
    let errors = Arc::new(errors);
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let errors = errors.clone();
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

                            let response = if let Some((code, message)) = errors.get(method) {
                                json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "error": { "code": code, "message": message }
                                })
                            } else {
                                json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": mock_rpc_response(method, &msg)
                                })
                            };

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

#[cfg(any(test, feature = "test-utils"))]
/// Generate mock RPC responses based on method name.
pub fn mock_rpc_response(method: &str, msg: &Value) -> Value {
    match method {
        "kiln.list" => json!([]),
        "kiln.graph" => json!({
            "notes": [
                { "path": "Alpha.md", "title": "Alpha", "tags": ["rust"] },
                { "path": "Beta.md", "title": "Beta", "tags": [] }
            ],
            "links": [
                { "source": "Alpha.md", "target": "Beta.md", "resolved": true },
                { "source": "Alpha.md", "target": "ghost", "resolved": false }
            ]
        }),
        "list_notes" => json!([]),
        "get_note_by_name" => Value::Null,
        // Note name "missing" resolves to nothing (404 path); anything else
        // resolves to a focused note with one linked mention.
        "get_backlinks" => {
            let name = msg
                .get("params")
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if name == "missing" {
                Value::Null
            } else {
                json!({
                    "path": "notes/focused.md",
                    "title": "Focused Note",
                    "backlinks": [
                        {"name": "linker", "path": "notes/linker.md", "title": "Linker Note"}
                    ]
                })
            }
        }
        // Includes a self-mention ("Focused Note") that the backlinks route
        // must filter out of `unlinked`.
        "suggest_links" => json!({
            "suggestions": [
                {"mention": "Other Note", "target": "Other Note", "offset": 0},
                {"mention": "Focused Note", "target": "Focused Note", "offset": 20}
            ]
        }),
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
        "session.set_mode" => json!(null),
        "session.set_title" => json!(null),
        "session.generate_title" => json!({
            "session_id": "test-session-001",
            "title": "Merkle tree sync design"
        }),
        "session.search" => json!([{"session_id": "s1", "title": "Test Session"}]),
        // Mirrors the daemon's real response shape: a `history` array of
        // SessionEventMessage entries, NOT a `messages` array. Session id
        // "empty-session-001" yields an empty history for fallback tests.
        "session.resume_from_storage" => {
            let session_id = msg
                .get("params")
                .and_then(|p| p.get("session_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("test-session-001");
            if session_id == "empty-session-001" {
                json!({"session_id": session_id, "history": [], "total_events": 0})
            } else {
                json!({
                    "session_id": session_id,
                    "history": [
                        {
                            "type": "event",
                            "session_id": session_id,
                            "event": "user_message",
                            "data": {"message_id": "msg-001", "content": "Explain the merkle tree sync design"},
                            "timestamp": "2026-01-01T00:00:00Z",
                            "seq": 1
                        },
                        {
                            "type": "event",
                            "session_id": session_id,
                            "event": "agent_message",
                            "data": {"message_id": "msg-002", "content": "Sure — the merkle tree..."},
                            "timestamp": "2026-01-01T00:00:01Z",
                            "seq": 2
                        }
                    ],
                    "total_events": 2
                })
            }
        }
        "project.list" => json!([]),
        "fs.list_dir" => json!([]),
        "fs.move" => json!({"moved": true}),
        "fs.mkdir" => json!({"created": true}),
        "fs.trash" => json!({"trashed": true, "trash_path": ".crucible/trash/0-x"}),
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
        "models.list" => json!({"models": ["ollama/llama3.2", "openai/gpt-4o"]}),
        "agents.list_profiles" => json!({
            "profiles": [
                {"name": "claude", "description": "Claude Code via ACP", "command": "npx", "is_builtin": true, "available": false},
                {"name": "opencode", "description": "OpenCode AI (Go)", "command": "opencode", "is_builtin": true, "available": true},
            ]
        }),
        // Name "missing" is unknown (null); anything else resolves.
        "agents.resolve_profile" => {
            let name = msg
                .get("params")
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if name == "missing" {
                Value::Null
            } else {
                json!({
                    "name": name,
                    "description": "Mock ACP agent",
                    "command": "mock-agent",
                    "is_builtin": true,
                    "args": [],
                    "env": {},
                })
            }
        }
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

#[cfg(any(test, feature = "test-utils"))]
/// Build an AppState using a mock daemon client.
pub fn build_mock_state(client: DaemonClient) -> AppState {
    let broker = Arc::new(EventBroker::new());
    // Mock client has no live event stream; a dropped sender ends the router.
    let (_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<crucible_daemon::SessionEvent>();
    AppState {
        daemon: Arc::new(ReconnectingDaemon::new(client, event_rx, broker.clone())),
        events: broker,
        config: Arc::new(CliAppConfig::default()),
        http_client: reqwest::Client::new(),
        layout_path: Arc::new(unique_test_layout_path()),
    }
}

#[cfg(any(test, feature = "test-utils"))]
/// Per-call unique layout path so parallel tests never share a file.
/// Layout-specific tests build their own AppState over a TempDir instead.
pub fn unique_test_layout_path() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "crucible-test-layout-{}-{}.json",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

#[cfg(any(test, feature = "test-utils"))]
/// Build the full app router with mock state.
pub fn build_test_app(state: AppState) -> Router {
    Router::new()
        .merge(agents_routes())
        .merge(chat_routes())
        .merge(session_routes())
        .merge(project_routes())
        .merge(search_routes())
        .merge(fs_routes())
        .with_state(state)
        .merge(health_routes())
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #[test]
        fn smoke_arb_url_scheme(scheme in arb_url_scheme()) {
            prop_assert!(!scheme.is_empty());
        }

        #[test]
        fn smoke_arb_ipv4_private(ip in arb_ipv4_private()) {
            let parsed: Ipv4Addr = ip.parse().expect("strategy must produce valid IPv4");
            prop_assert!(parsed.is_private() || parsed.is_loopback());
        }

        #[test]
        fn smoke_arb_ipv4_public(ip in arb_ipv4_public()) {
            let parsed: Ipv4Addr = ip.parse().expect("strategy must produce valid IPv4");
            prop_assert!(!parsed.is_private());
            prop_assert!(!parsed.is_loopback());
            prop_assert!(!parsed.is_link_local());
            prop_assert!(!parsed.is_broadcast());
            prop_assert!(!parsed.is_unspecified());
        }

        #[test]
        fn smoke_arb_ipv6_loopback(ip in arb_ipv6_loopback()) {
            let host = ip.trim_matches(['[', ']']);
            let parsed: std::net::Ipv6Addr = host.parse().expect("strategy must produce valid IPv6");
            prop_assert!(parsed.is_loopback());
        }

        #[test]
        fn smoke_arb_hostname(host in arb_hostname()) {
            prop_assert!(host.len() <= 84);
        }

        #[test]
        fn smoke_arb_endpoint_url(url in arb_endpoint_url()) {
            prop_assert!(url.contains("://"));
        }

        #[test]
        fn smoke_arb_traversal_path(path in arb_traversal_path()) {
            prop_assert!(path.contains("..") || path.contains('\0'));
        }

        #[test]
        fn smoke_arb_safe_path(path in arb_safe_path()) {
            prop_assert!(!path.contains(".."));
            prop_assert!(!path.contains('\0'));
        }
    }
}
