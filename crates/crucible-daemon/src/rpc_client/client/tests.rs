use crate::rpc_client::client::session::build_create_request;
use crate::rpc_client::client::*;
use crate::Server;
use tempfile::TempDir;

/// Approval criterion 1.3: a cold-start failure must surface in seconds,
/// not the 51.15s the old uncapped schedule took.
#[test]
fn the_connect_backoff_gives_up_in_seconds_not_minutes() {
    let delays: Vec<Duration> = DaemonClient::connect_backoff().collect();
    let total: Duration = delays.iter().sum();

    assert!(
        total < Duration::from_secs(6),
        "backoff must give up quickly; total was {total:?}"
    );
    assert!(
        delays.first().unwrap() < &Duration::from_millis(100),
        "first retry must be fast for the healthy-start case"
    );
}

/// The failure message must carry the daemon's own words — the log tail —
/// not just the restart incantation.
#[test]
fn the_connect_failure_message_shows_the_daemon_log_tail() {
    let msg = DaemonClient::compose_connect_failure(
        8,
        std::path::Path::new("/home/user/.crucible/daemon.log"),
        Some("Error: invalid config: unknown field `chat.provider`".to_string()),
    );

    assert!(msg.contains("Try: cru daemon stop && cru daemon start"));
    assert!(msg.contains("cru daemon logs"));
    assert!(msg.contains("cru doctor"));
    assert!(
        msg.contains("unknown field `chat.provider`"),
        "the daemon's actual error text must be in the message"
    );
}

#[test]
fn the_connect_failure_message_admits_when_there_is_no_log() {
    let msg = DaemonClient::compose_connect_failure(
        8,
        std::path::Path::new("/home/user/.crucible/daemon.log"),
        None,
    );

    assert!(msg.contains("No daemon output captured"));
    assert!(msg.contains("/home/user/.crucible/daemon.log"));
}

#[test]
fn daemon_serve_args_forwards_config() {
    assert_eq!(
        DaemonClient::daemon_serve_args(Some("/etc/crucible.toml")),
        vec!["--config", "/etc/crucible.toml", "daemon", "serve"]
    );
    // No/empty config → plain `daemon serve`.
    assert_eq!(
        DaemonClient::daemon_serve_args(None),
        vec!["daemon", "serve"]
    );
    assert_eq!(
        DaemonClient::daemon_serve_args(Some("")),
        vec!["daemon", "serve"]
    );
}

async fn setup_test_server() -> (TempDir, std::path::PathBuf, tokio::task::JoinHandle<()>) {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    // Inject an isolated data root (no env) so the daemon never loads the
    // developer's real ~/.crucible registry — else test_client_kiln_list_
    // initially_empty sees the real registered kilns.
    let kiln = tmp.path().join("kiln");
    std::fs::create_dir_all(&kiln).unwrap();
    let server = Server::bind_with_data_home_and_kilns(
        &sock_path,
        tmp.path().to_path_buf(),
        &[("kiln", &kiln)],
    )
    .await
    .unwrap();
    let _shutdown_handle = server.shutdown_handle();

    let handle = tokio::spawn(async move {
        let _ = server.run().await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    (tmp, sock_path.clone(), handle)
}

#[tokio::test]
async fn test_client_ping() {
    let (_tmp, sock_path, _handle) = setup_test_server().await;

    let client = DaemonClient::connect_to(&sock_path).await.unwrap();
    let result = client.ping().await.unwrap();
    assert_eq!(result, "pong");
}

#[test]
fn validate_socket_path_rejects_overlong_path() {
    // 220-char path exceeds sun_path (108 on Linux / 104 on macOS).
    let tmp = TempDir::new().unwrap();
    let bad = tmp.path().join("s".repeat(220));
    let err = validate_socket_path(&bad).unwrap_err();
    let msg = format!("{:#}", err);
    assert!(
        msg.contains("invalid daemon socket path"),
        "expected path-validation error, got: {msg}"
    );
}

#[test]
fn validate_socket_path_accepts_short_path() {
    let tmp = TempDir::new().unwrap();
    let ok = tmp.path().join("short.sock");
    validate_socket_path(&ok).expect("short path should validate");
}

// ---- Wire-format tests for the types the SERVER deserializes ----
//
// These two request types are not just what the client sends: gate A6
// requires the handler to deserialize them rather than re-derive their field
// names, so a change here changes what the daemon accepts. `session.create`
// omitting `type`, and `lua.init_session` omitting `kiln_path` or spelling
// it `kiln`, are all payloads the hand-plucked handlers accepted — the
// serde attributes are what keeps accepting them.

#[test]
fn session_create_request_without_type_defaults_to_chat() {
    // The handler used to do `optional_param!(req, "type", …).unwrap_or("chat")`.
    // Without `#[serde(default)]` this payload would now be INVALID_PARAMS.
    let req: SessionCreateRequest = serde_json::from_value(serde_json::json!({
        "kilns": ["/tmp/kiln"],
    }))
    .unwrap();
    assert_eq!(req.session_type, "chat");
}

#[test]
fn lua_init_session_request_without_kiln_path_deserializes_as_none() {
    // Absent means "fall back to the daemon's data root", which is what the
    // handler does. A required `String` would reject this payload.
    let req: crate::rpc_client::LuaInitSessionRequest =
        serde_json::from_value(serde_json::json!({ "session_id": "s1" })).unwrap();
    assert_eq!(req.session_id, "s1");
    assert_eq!(req.kiln_path, None);
}

#[test]
fn lua_init_session_request_accepts_the_kiln_alias() {
    // The handler read `kiln_path` OR `kiln`. No in-tree caller sends `kiln`,
    // but the method is public RPC, so dropping the second spelling would be
    // a silent break for anyone who used it.
    let req: crate::rpc_client::LuaInitSessionRequest = serde_json::from_value(serde_json::json!({
        "session_id": "s1",
        "kiln": "/tmp/kiln",
    }))
    .unwrap();
    assert_eq!(req.kiln_path.as_deref(), Some("/tmp/kiln"));
}

#[test]
fn lua_init_session_request_omits_an_absent_kiln_path() {
    let req = crate::rpc_client::LuaInitSessionRequest {
        session_id: "s1".to_string(),
        kiln_path: None,
    };
    let json = serde_json::to_value(&req).unwrap();
    assert!(
        json.get("kiln_path").is_none(),
        "an absent kiln_path must not be sent as null: {json}"
    );
}

// ---- SessionCreateRequest wire-format tests (Task 1.2a) ----
// The daemon and CLI may be at different versions; the `agent_type` field
// must be forward/backward compatible.

#[test]
fn session_create_request_without_agent_type_deserializes_as_none() {
    // Old-style payload (pre-Task 1.2a) — no `agent_type`.
    let json = serde_json::json!({
        "type": "chat",
        "kilns": ["/tmp/kiln"],
    });
    let req: SessionCreateRequest = serde_json::from_value(json).unwrap();
    assert_eq!(req.session_type, "chat");
    assert_eq!(
        req.kilns.as_deref(),
        Some(["/tmp/kiln".to_string()].as_slice())
    );
    assert_eq!(req.agent_type, None);
}

#[test]
fn session_create_request_with_agent_type_acp_roundtrips() {
    let json = serde_json::json!({
        "type": "chat",
        "kilns": ["/tmp/kiln"],
        "agent_type": "acp",
    });
    let req: SessionCreateRequest = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(req.agent_type.as_deref(), Some("acp"));
    // Re-serialize and confirm the field survives the round-trip.
    let roundtrip = serde_json::to_value(&req).unwrap();
    assert_eq!(roundtrip["agent_type"], "acp");
}

#[test]
fn session_create_request_with_agent_type_internal_roundtrips() {
    let json = serde_json::json!({
        "type": "chat",
        "kilns": ["/tmp/kiln"],
        "agent_type": "internal",
    });
    let req: SessionCreateRequest = serde_json::from_value(json).unwrap();
    assert_eq!(req.agent_type.as_deref(), Some("internal"));
    let roundtrip = serde_json::to_value(&req).unwrap();
    assert_eq!(roundtrip["agent_type"], "internal");
}

#[test]
fn session_create_request_omits_agent_type_when_none() {
    // Ensure over-the-wire backward compatibility: a None `agent_type`
    // must not appear in the serialized payload, so old daemons don't
    // see an unexpected field.
    let req = SessionCreateRequest {
        session_type: "chat".to_string(),
        kilns: Some(vec!["/tmp/kiln".to_string()]),
        workspace: None,
        recording_mode: None,
        recording_path: None,
        agent_type: None,
        ..Default::default()
    };
    let json = serde_json::to_value(&req).unwrap();
    assert!(
        json.get("agent_type").is_none(),
        "agent_type should be omitted when None, got: {json}"
    );
    // The agent-spec fields default to absent too, so an old daemon sees
    // the same minimal payload it always did.
    assert!(json.get("configure_agent").is_none());
    assert!(json.get("agent_name").is_none());
    assert!(json.get("agent_card").is_none());
    assert!(json.get("tool_policy").is_none());
}

/// `cru session create --agent` is the only Rust caller that names a card, and
/// it must not land in `agent_name` — that field launches an ACP subprocess.
#[test]
fn agent_spec_card_reaches_the_wire_as_agent_card() {
    let req = build_create_request(
        SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: Some("internal".to_string()),
            isolation: None,
        },
        Some(SessionAgentSpec {
            agent_card: Some("researcher".to_string()),
            ..Default::default()
        }),
    );
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["agent_card"], "researcher");
    assert_eq!(json["configure_agent"], serde_json::json!(true));
    assert!(json.get("agent_name").is_none());
}

/// The plugin surface's field: a per-session tool policy that survives an
/// agent card instead of needing a whole-agent `configure_agent` afterwards.
#[test]
fn session_create_request_tool_policy_roundtrips() {
    use crucible_core::agent::ToolPolicy;

    let req: SessionCreateRequest = serde_json::from_value(serde_json::json!({
        "type": "chat",
        "tool_policy": { "bash": "deny", "read_file": "allow" },
    }))
    .unwrap();
    let policy = req.tool_policy.clone().expect("tool_policy deserialized");
    assert_eq!(policy.get("bash"), Some(&ToolPolicy::Deny));
    assert_eq!(policy.get("read_file"), Some(&ToolPolicy::Allow));
    assert_eq!(
        serde_json::to_value(&req).unwrap()["tool_policy"]["bash"],
        "deny"
    );
}

#[test]
fn session_create_request_agent_card_roundtrips_and_is_distinct_from_agent_name() {
    let json = serde_json::json!({
        "type": "chat",
        "configure_agent": true,
        "agent_card": "researcher",
    });
    let req: SessionCreateRequest = serde_json::from_value(json).unwrap();
    assert_eq!(req.agent_card.as_deref(), Some("researcher"));
    // The card must not land in `agent_name`: on an ACP session that field
    // launches a subprocess by profile name.
    assert_eq!(req.agent_name, None);
    let roundtrip = serde_json::to_value(&req).unwrap();
    assert_eq!(roundtrip["agent_card"], "researcher");
}

/// `cru.session.create{ kilns = {...} }` is the spelling every plugin uses,
/// and it is now the wire name too — the flatten collapsed `kiln` +
/// `connect_kilns` into it, so the binding sends its table through untouched.
/// Order is preserved because the daemon takes the first entry as the
/// default kiln.
#[test]
fn session_create_request_takes_kilns_as_an_ordered_set() {
    let req: SessionCreateRequest =
        serde_json::from_value(serde_json::json!({ "kilns": ["/a", "/b"] })).unwrap();
    assert_eq!(
        req.kilns.as_deref(),
        Some(["/a".to_string(), "/b".to_string()].as_slice())
    );
    assert_eq!(
        serde_json::to_value(&req).unwrap()["kilns"],
        serde_json::json!(["/a", "/b"])
    );
}

#[test]
fn session_create_request_omits_kilns_when_none() {
    // An absent kiln set must not appear on the wire: the daemon resolves its
    // own default (home kiln), and clients must never pre-empt it.
    let req = SessionCreateRequest {
        session_type: "chat".to_string(),
        kilns: None,
        workspace: None,
        recording_mode: None,
        recording_path: None,
        agent_type: None,
        ..Default::default()
    };
    let json = serde_json::to_value(&req).unwrap();
    assert!(
        json.get("kilns").is_none(),
        "kilns should be omitted when None, got: {json}"
    );
}

#[tokio::test]
async fn test_client_capabilities() {
    let (_tmp, sock_path, _handle) = setup_test_server().await;

    let client = DaemonClient::connect_to(&sock_path).await.unwrap();
    let caps = client.capabilities().await.unwrap();

    assert_eq!(caps.protocol_version, "1.0");
    assert!(caps.capabilities.kilns);
    assert!(caps.capabilities.sessions);
    assert!(caps.capabilities.agents);
    assert!(caps.capabilities.events);
    assert!(caps.capabilities.model_switching);
    assert!(caps.methods.contains(&"ping".to_string()));
    assert!(caps
        .methods
        .contains(&"session.set_context_strategy".to_string()));
}

#[tokio::test]
async fn test_client_version_check_matches() {
    let (_tmp, sock_path, _handle) = setup_test_server().await;

    let client = DaemonClient::connect_to(&sock_path).await.unwrap();
    let check = client.check_version().await.unwrap();

    assert!(check.is_match());
}

#[tokio::test]
async fn test_client_ping_event_mode() {
    let (_tmp, sock_path, _handle) = setup_test_server().await;

    let (client, _event_rx) = DaemonClient::connect_to_with_events(&sock_path)
        .await
        .unwrap();
    let result = client.ping().await.unwrap();
    assert_eq!(result, "pong");
}

#[tokio::test]
async fn test_client_kiln_list_initially_empty() {
    let (_tmp, sock_path, _handle) = setup_test_server().await;

    let client = DaemonClient::connect_to(&sock_path).await.unwrap();
    let list = client.kiln_list().await.unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn test_client_connect_fails_without_server() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("nonexistent.sock");

    let result = DaemonClient::connect_to(&sock_path).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_multiple_sequential_calls_event_mode() {
    let (_tmp, sock_path, _handle) = setup_test_server().await;

    let (client, _event_rx) = DaemonClient::connect_to_with_events(&sock_path)
        .await
        .unwrap();

    for _ in 0..5 {
        let result = client.ping().await.unwrap();
        assert_eq!(result, "pong");
    }
}

#[tokio::test]
async fn test_session_create_and_get() {
    let (_srv, sock, _handle) = setup_test_server().await;
    let client = DaemonClient::connect_to(&sock).await.unwrap();
    let _tmp = TempDir::new().unwrap();

    let result = client
        .session_create(SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![crate::test_support::kiln_name("kiln")],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await
        .unwrap();
    let session_id = result["session_id"].as_str().unwrap();

    let session = client.session_get(session_id).await.unwrap();
    assert_eq!(session["session_id"], session_id);
    assert_eq!(session["type"], "chat");
}

#[tokio::test]
async fn test_session_list() {
    let (_srv, sock, _handle) = setup_test_server().await;
    let client = DaemonClient::connect_to(&sock).await.unwrap();
    let result = client
        .session_list(None, None, None, None, None)
        .await
        .unwrap();
    assert!(result.is_array() || result.is_object());
}

#[tokio::test]
async fn test_session_lifecycle() {
    let (_srv, sock, _handle) = setup_test_server().await;
    let client = DaemonClient::connect_to(&sock).await.unwrap();
    let _tmp = TempDir::new().unwrap();

    let result = client
        .session_create(SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![crate::test_support::kiln_name("kiln")],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await
        .unwrap();
    let session_id = result["session_id"].as_str().unwrap();

    let pause_result = client.session_pause(session_id).await;
    assert!(pause_result.is_ok());

    let resume_result = client.session_resume(session_id).await;
    assert!(resume_result.is_ok());

    let end_result = client.session_end(session_id).await;
    assert!(end_result.is_ok());
}

#[tokio::test]
async fn test_session_subscribe_unsubscribe() {
    let (_srv, sock, _handle) = setup_test_server().await;
    let client = DaemonClient::connect_to(&sock).await.unwrap();
    let _tmp = TempDir::new().unwrap();

    let result = client
        .session_create(SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![crate::test_support::kiln_name("kiln")],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await
        .unwrap();
    let session_id = result["session_id"].as_str().unwrap();

    let sub_result = client.session_subscribe(&[session_id]).await;
    assert!(sub_result.is_ok());

    let unsub_result = client.session_unsubscribe(&[session_id]).await;
    assert!(unsub_result.is_ok());

    let _ = client.session_end(session_id).await;
}

#[tokio::test]
async fn test_call_with_retry_succeeds_on_valid_method() {
    let (_tmp, sock_path, _handle) = setup_test_server().await;

    let client = DaemonClient::connect_to(&sock_path).await.unwrap();
    let result = client
        .call_with_retry("ping", serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(result, "pong");
}

#[tokio::test]
async fn test_call_with_retry_does_not_retry_rpc_errors() {
    let (_tmp, sock_path, _handle) = setup_test_server().await;

    let client = DaemonClient::connect_to(&sock_path).await.unwrap();
    let result = client
        .call_with_retry("nonexistent.method", serde_json::json!({}))
        .await;
    assert!(result.is_err(), "Unknown method should fail without retry");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        !err_msg.contains("timeout"),
        "Error should not be timeout-related: {}",
        err_msg
    );
}

#[test]
fn version_check_enum_match() {
    let check = VersionCheck::Match;
    assert!(check.is_match());
}

#[test]
fn version_check_enum_mismatch() {
    let check = VersionCheck::Mismatch {
        client: "abc1234".to_string(),
        daemon: "def5678".to_string(),
    };
    assert!(!check.is_match());
}

#[test]
fn version_check_mismatch_dev_vs_real() {
    // This is the exact scenario that was broken before build.rs was added:
    // client had "dev" (no build.rs), daemon had "dev" → false Match
    let check_dev_match = VersionCheck::Match; // "dev" == "dev"
    assert!(check_dev_match.is_match());

    // After build.rs: client has real SHA, daemon has old "dev" → correct Mismatch
    let check_dev_mismatch = VersionCheck::Mismatch {
        client: "abc1234".to_string(),
        daemon: "dev".to_string(),
    };
    assert!(!check_dev_mismatch.is_match());
}

#[test]
fn build_sha_is_set_by_build_rs() {
    // After Task 2 added build.rs, this should NOT be "dev" anymore.
    // This is the CRITICAL test — proves the SHA is actually embedded.
    let sha = option_env!("CRUCIBLE_BUILD_SHA");
    assert!(
        sha.is_some(),
        "CRUCIBLE_BUILD_SHA should be set by build.rs"
    );
    let sha = sha.unwrap();
    assert_ne!(sha, "dev", "Should be a real git SHA, not 'dev'");
    assert!(sha.len() >= 7, "SHA should be at least 7 chars: got {sha}");
    assert!(
        sha.chars().all(|c| c.is_ascii_hexdigit()),
        "SHA should be hex chars only: got {sha}"
    );
}

/// Response correlation in simple mode.
///
/// Simple mode had no correlation at all: `read_response_simple` returned the
/// first line carrying an `id`, whichever request it belonged to. That was
/// invisible while the daemon answered strictly FIFO, and it is the reason a
/// per-request spawn on the server could not land — it would have started
/// mis-delivering every simple-mode client's responses, silently, with no error
/// anywhere.
mod simple_mode_correlation {
    use super::*;
    use std::sync::Arc;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;

    /// A daemon that waits for `n` requests and then answers them in reverse
    /// arrival order, echoing each request's `method` as its result.
    ///
    /// Reverse is the point: it is exactly what a server that spawns per request
    /// produces when one handler is slower than another, and it is fully
    /// deterministic here — nothing is written until both requests are in.
    fn stub_answering_in_reverse(
        listener: UnixListener,
        n: usize,
    ) -> tokio::task::JoinHandle<Vec<(u64, String)>> {
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let (read, mut write) = stream.into_split();
            let mut reader = BufReader::new(read);

            let mut requests: Vec<(u64, String)> = Vec::new();
            while requests.len() < n {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).await.expect("read") > 0);
                let req: serde_json::Value = serde_json::from_str(&line).expect("parse request");
                requests.push((
                    req["id"].as_u64().expect("numeric id"),
                    req["method"].as_str().expect("method").to_string(),
                ));
            }

            let answers: Vec<(u64, String)> = requests.into_iter().rev().collect();
            for (id, method) in &answers {
                write
                    .write_all(
                        format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":\"{method}\"}}\n")
                            .as_bytes(),
                    )
                    .await
                    .expect("write response");
            }
            answers
        })
    }

    /// Each caller must get its own answer, not the first one on the wire.
    #[tokio::test]
    async fn each_caller_gets_its_own_response_when_answers_arrive_reversed() {
        let dir = TempDir::new().expect("temp dir");
        let sock = dir.path().join("stub.sock");
        let listener = UnixListener::bind(&sock).expect("bind");
        let server = stub_answering_in_reverse(listener, 2);

        let client = Arc::new(
            DaemonClient::connect_to(&sock)
                .await
                .expect("connect simple mode"),
        );
        assert!(
            client.simple_reader.is_some() && client.reader_task.is_none(),
            "this test is about simple mode; event mode already correlates"
        );

        let a = tokio::spawn({
            let client = client.clone();
            async move { client.call("alpha", serde_json::Value::Null).await }
        });
        let b = tokio::spawn({
            let client = client.clone();
            async move { client.call("beta", serde_json::Value::Null).await }
        });

        let a = a.await.expect("task a").expect("call alpha");
        let b = b.await.expect("task b").expect("call beta");

        assert_eq!(a, serde_json::json!("alpha"), "caller a got b's response");
        assert_eq!(b, serde_json::json!("beta"), "caller b got a's response");

        // The hazard, asserted present: if the stub had answered in arrival
        // order this test would pass without any correlation at all.
        let answered = server.await.expect("server task");
        let ids: Vec<u64> = answered.iter().map(|(id, _)| *id).collect();
        let mut ascending = ids.clone();
        ascending.sort_unstable();
        assert_ne!(
            ids, ascending,
            "the stub must actually have answered out of order, or this test \
             proves nothing"
        );
    }

    /// Server-pushed notifications carry no `id` and must be skipped, not
    /// mistaken for a reply. This was the one thing the old code got right.
    #[tokio::test]
    async fn a_notification_between_request_and_reply_is_skipped() {
        let dir = TempDir::new().expect("temp dir");
        let sock = dir.path().join("stub.sock");
        let listener = UnixListener::bind(&sock).expect("bind");

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let (read, mut write) = stream.into_split();
            let mut reader = BufReader::new(read);
            let mut line = String::new();
            assert!(reader.read_line(&mut line).await.expect("read") > 0);
            let req: serde_json::Value = serde_json::from_str(&line).expect("parse");
            let id = req["id"].as_u64().expect("id");
            write
                .write_all(
                    format!(
                        "{{\"type\":\"event\",\"session_id\":\"s\",\"event\":\"ui_style_changed\",\"data\":{{}}}}\n\
                         {{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":\"pong\"}}\n"
                    )
                    .as_bytes(),
                )
                .await
                .expect("write");
        });

        let client = DaemonClient::connect_to(&sock).await.expect("connect");
        let result = client
            .call("ping", serde_json::Value::Null)
            .await
            .expect("call");
        assert_eq!(result, serde_json::json!("pong"));
        server.await.expect("server task");
    }
}

/// Ask the operating system whether `pid` still names a live process.
///
/// Signal 0 delivers nothing; it performs only the existence and permission
/// checks. Reading the guard's own bookkeeping would prove nothing about the
/// process it was supposed to reap.
#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    // SAFETY: `kill` with signal 0 sends no signal. Its only effects are the
    // return value and `errno`.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// A child that outlives any plausible test, so "still alive" means the guard
/// let it live rather than that it had not finished yet.
#[cfg(unix)]
fn spawn_long_lived_child() -> std::process::Child {
    std::process::Command::new("sleep")
        .arg("300")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn a long-lived child")
}

/// The give-up path: the daemon never answered, the client is about to return
/// an error, and nothing else in the system holds that process.
#[cfg(unix)]
#[test]
fn a_daemon_that_never_answers_is_reaped_by_the_guard() {
    let child = spawn_long_lived_child();
    let pid = child.id();

    drop(SpawnedDaemon(Some(child)));

    assert!(
        !process_is_alive(pid),
        "pid {pid} survived the guard; a daemon that never answered is an orphan"
    );
}

/// The success path: the daemon is serving, and outliving this client is the
/// entire reason it was spawned detached.
#[cfg(unix)]
#[test]
fn a_daemon_that_answers_is_left_running() {
    let child = spawn_long_lived_child();
    let pid = child.id();

    SpawnedDaemon(Some(child)).detach();

    assert!(
        process_is_alive(pid),
        "pid {pid} was killed; a daemon that answered must outlive its client"
    );

    // Detaching means nothing waits on it any more. Kill it here; the test
    // process exits immediately afterwards (nextest runs one test per
    // process), so init reaps what is left.
    // SAFETY: `kill` on a pid this test spawned and still owns.
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGKILL);
    }
}

/// Spawn `script` under `sh` and return once it says it is ready.
///
/// The readiness line is not decoration. A shell that has been spawned but has
/// not yet parsed its `trap` still carries SIGTERM's default disposition, so a
/// signal delivered in that window kills it outright and the test reads the
/// answer it was written to catch.
#[cfg(unix)]
fn spawn_ready_child(script: &str) -> std::process::Child {
    use std::io::{BufRead, BufReader};

    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{script}\necho ready\nwait_forever"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn the child shell");

    let stdout = child.stdout.take().expect("piped stdout");
    let mut line = String::new();
    BufReader::new(stdout)
        .read_line(&mut line)
        .expect("read the readiness line");
    assert_eq!(line.trim(), "ready", "the child never became ready");

    child
}

/// A child that catches SIGTERM and exits with a status of its own choosing.
///
/// It stands in for the daemon, whose shutdown path (cancel the tasks, join
/// them, release the socket lock) runs only on SIGTERM. SIGKILL skips all of
/// it, so the exit code below is the evidence that the request was made.
#[cfg(unix)]
fn spawn_child_that_handles_sigterm() -> std::process::Child {
    spawn_ready_child("trap 'exit 42' TERM\nwait_forever() { while :; do sleep 0.05; done; }")
}

/// A child that refuses to stop. The reaper must not wait on it for ever.
#[cfg(unix)]
fn spawn_child_that_ignores_sigterm() -> std::process::Child {
    spawn_ready_child("trap '' TERM\nwait_forever() { while :; do sleep 0.05; done; }")
}

/// A daemon that outlived its client's patience is ASKED to stop.
///
/// The client cannot tell "my child is slow" from "my child bound its socket a
/// moment ago and is now serving somebody else". SIGKILL takes the second one
/// down mid-write and bypasses the graceful path entirely; SIGTERM lets it run
/// that path. The exit code proves which signal arrived.
#[cfg(unix)]
#[test]
fn a_daemon_that_never_answers_is_asked_to_stop_before_it_is_killed() {
    use std::os::unix::process::ExitStatusExt;

    let mut child = spawn_child_that_handles_sigterm();
    let status = reap_spawned_daemon(&mut child).expect("reap the child");

    assert_eq!(
        status.code(),
        Some(42),
        "the daemon must run its own shutdown path; it exited by signal {:?}",
        status.signal()
    );
}

/// And the escalation still exists: a daemon that ignores the request is
/// killed rather than waited on for ever.
#[cfg(unix)]
#[test]
fn a_daemon_that_ignores_the_request_is_still_killed() {
    use std::os::unix::process::ExitStatusExt;

    let mut child = spawn_child_that_ignores_sigterm();
    let started = std::time::Instant::now();
    let status = reap_spawned_daemon(&mut child).expect("reap the child");
    let elapsed = started.elapsed();

    assert_eq!(
        status.signal(),
        Some(libc::SIGKILL),
        "a daemon that ignores SIGTERM must still be killed"
    );
    assert!(
        elapsed >= REAP_GRACE,
        "the kill must come after the grace period, not instead of it; took {elapsed:?}"
    );
    assert!(
        elapsed < REAP_GRACE * 4,
        "the reaper must not wait far beyond the grace period; took {elapsed:?}"
    );
}
