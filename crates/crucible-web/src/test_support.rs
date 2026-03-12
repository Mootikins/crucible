#[cfg(test)]
use crate::routes::{chat_routes, health_routes, project_routes, search_routes, session_routes};
#[cfg(test)]
use crate::services::daemon::{AppState, EventBroker, ReconnectingDaemon};
#[cfg(test)]
use axum::Router;
#[cfg(test)]
use crucible_config::CliAppConfig;
#[cfg(test)]
use crucible_daemon::DaemonClient;
#[cfg(test)]
use serde_json::{json, Value};
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use tempfile::TempDir;
#[cfg(test)]
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(test)]
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
    "[a-zA-Z0-9/_-]{1,64}".prop_filter("no traversal or null byte", |s| {
        !s.contains("..") && !s.contains('\0')
    })
}

#[cfg(test)]
/// A mock daemon that listens on a Unix socket and responds to JSON-RPC calls
/// with canned responses. This allows testing HTTP routes without a real daemon.
pub struct MockDaemon {
    _tmp: TempDir,
}

#[cfg(test)]
/// Start a mock daemon on a temporary Unix socket. Returns the mock daemon
/// handle (holds TempDir alive) and a connected DaemonClient.
pub async fn start_mock_daemon() -> (MockDaemon, DaemonClient) {
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

#[cfg(test)]
/// Generate mock RPC responses based on method name.
fn mock_rpc_response(method: &str, _msg: &Value) -> Value {
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
        "session.subscribe" => json!(null),
        "session.configure_agent" => json!(null),
        "session.send_message" => json!({"message_id": "msg-001"}),
        "session.interaction_respond" => json!(null),
        "session.list_models" => json!({"models": ["llama3.2", "mistral"]}),
        "session.switch_model" => json!(null),
        "session.set_title" => json!(null),
        "session.search" => json!([{"session_id": "s1", "title": "Test Session"}]),
        "session.resume_from_storage" => json!({"messages": [], "session_id": "test-session-001"}),
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
        "session.render_markdown" => json!({"markdown": "# Test Session\n\nExported content"}),
        "providers.list" => json!({"providers": []}),
        _ => json!(null),
    }
}

#[cfg(test)]
/// Build an AppState using a mock daemon client.
pub fn build_mock_state(client: DaemonClient) -> AppState {
    AppState {
        daemon: Arc::new(ReconnectingDaemon::new(client)),
        events: Arc::new(EventBroker::new()),
        config: Arc::new(CliAppConfig::default()),
        http_client: reqwest::Client::new(),
    }
}

#[cfg(test)]
/// Build the full app router with mock state.
pub fn build_test_app(state: AppState) -> Router {
    Router::new()
        .merge(chat_routes())
        .merge(session_routes())
        .merge(project_routes())
        .merge(search_routes())
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
