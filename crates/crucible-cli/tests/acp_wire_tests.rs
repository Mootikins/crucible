//! Pins the raw JSON that `cru acp` writes on stdout for the ACP handshake.
//!
//! The ACP SDK changes its Rust API between majors, but the v1 wire format
//! stays fixed. This test reads the bytes a host sees, so an SDK upgrade
//! cannot change the wire without a visible diff here.

use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdout, Command, Stdio};
use std::time::Duration;

use crucible_daemon::{rpc_client::SessionCreateParams, DaemonClient};
use serde_json::{json, Value};
use tempfile::TempDir;

/// Spawn `cru acp` against a throwaway kiln with all home directories
/// redirected into the temp directory, so the test never touches `~/.crucible`.
fn spawn_cru_acp(temp: &TempDir) -> std::process::Child {
    let kiln = temp.path().join("kiln");
    std::fs::create_dir_all(kiln.join(".crucible")).expect("create kiln dir");
    std::fs::write(kiln.join(".crucible").join("kiln.toml"), "").expect("write kiln.toml");

    Command::new(env!("CARGO_BIN_EXE_cru"))
        .arg("acp")
        .arg("--kiln")
        .arg(&kiln)
        .env("HOME", temp.path())
        .env("CRUCIBLE_HOME", temp.path().join("home"))
        .env("CRUCIBLE_CONFIG_DIR", temp.path().join("config"))
        .env("CRUCIBLE_LOG_FILE", temp.path().join("acp.log"))
        // Pin the daemon socket inside the temp dir too: with the developer's
        // XDG_RUNTIME_DIR inherited, a daemon LEAKED by any other test on the
        // shared default socket would answer this process's config fetch and
        // fail it with a root-mismatch refusal before the handshake.
        .env("CRUCIBLE_SOCKET", temp.path().join("daemon.sock"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn cru acp")
}

#[test]
fn initialize_reply_on_the_wire_is_pinned() {
    let temp = TempDir::new().expect("temp dir");
    let mut child = spawn_cru_acp(&temp);

    let mut stdin = child.stdin.take().expect("stdin");
    stdin
        .write_all(
            br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}"#,
        )
        .expect("write initialize");
    stdin.write_all(b"\n").expect("write newline");
    stdin.flush().expect("flush");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut line = String::new();
    stdout.read_line(&mut line).expect("read reply");
    let reply: Value = serde_json::from_str(line.trim()).expect("reply is JSON");

    assert_eq!(
        reply,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "protocolVersion": 1,
                "agentCapabilities": {
                    "loadSession": true,
                    "promptCapabilities": {
                        "image": false,
                        "audio": false,
                        "embeddedContext": false
                    },
                    "mcpCapabilities": { "http": false, "sse": false },
                    "sessionCapabilities": { "close": {} },
                    // Schema 1.5 adds the stable `auth` capability object.
                    // SDK 0.10 (schema 0.11) did not write it.
                    "auth": {}
                },
                "authMethods": []
            }
        })
    );

    // Closing stdin ends the connection; the process must exit on its own.
    drop(stdin);
    let status = child.wait().expect("wait for cru acp");
    assert!(status.success(), "cru acp exited with {status}");
}

/// Write one JSON-RPC message to the host→agent pipe.
fn send(stdin: &mut impl Write, msg: &Value) {
    stdin
        .write_all(msg.to_string().as_bytes())
        .expect("write request");
    stdin.write_all(b"\n").expect("write newline");
    stdin.flush().expect("flush");
}

/// Read agent→host lines until the reply with `id` arrives, returning every
/// notification that preceded it in order.
fn exchange(stdout: &mut BufReader<ChildStdout>, id: i64) -> (Vec<Value>, Value) {
    let mut notifications = Vec::new();
    loop {
        let mut line = String::new();
        let read = stdout.read_line(&mut line).expect("read line");
        assert!(read > 0, "cru acp closed stdout before replying to id {id}");
        let msg: Value = serde_json::from_str(line.trim()).expect("JSON-RPC line");
        if msg.get("id").and_then(|v| v.as_i64()) == Some(id) {
            return (notifications, msg);
        }
        notifications.push(msg);
    }
}

/// `session/load` must replay the recorded conversation as `session/update`
/// notifications, before the response. A host keeps no transcript across
/// restarts — the notifications are the ONLY copy of the conversation it will
/// ever draw, so the user's prompt is replayed too (the live path never sends
/// it because the host renders the text it just sent). Regression: load
/// resumed the daemon session and answered immediately, so a host resuming a
/// session drew an empty transcript tab.
#[test]
fn session_load_replays_the_recorded_transcript() {
    let temp = TempDir::new().expect("temp dir");
    let kiln = temp.path().join("kiln");
    let mut child = spawn_cru_acp(&temp);

    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    // `session/new` doubles as the trigger that makes the child start its
    // daemon; its own session stays untouched by what follows.
    send(
        &mut stdin,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}),
    );
    let (_, init_reply) = exchange(&mut stdout, 1);
    assert!(
        init_reply.get("error").is_none(),
        "initialize failed: {init_reply}"
    );

    send(
        &mut stdin,
        &json!({"jsonrpc":"2.0","id":2,"method":"session/new","params":{"cwd": kiln, "mcpServers": []}}),
    );
    let (_, new_reply) = exchange(&mut stdout, 2);
    assert!(
        new_reply.get("error").is_none(),
        "session/new failed: {new_reply}"
    );

    // Seed a second, finished session: one recorded turn, then paused — the
    // only state `session/load` can resume.
    let socket = temp.path().join("daemon.sock");
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let daemon_session = rt.block_on(async {
        let mut client = None;
        for _ in 0..50 {
            if let Ok(c) = DaemonClient::connect_to(&socket).await {
                client = Some(c);
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let client = client.expect("the child's daemon never came up");
        let created = client
            .session_create(SessionCreateParams {
                session_type: "chat".to_string(),
                kilns: Vec::new(),
                workspace: Some(kiln.clone()),
                recording_mode: None,
                recording_path: None,
                agent_type: Some("internal".to_string()),
                isolation: None,
            })
            .await
            .expect("session.create");
        let id = created["session_id"]
            .as_str()
            .expect("session_id")
            .to_string();
        client.session_pause(&id).await.expect("session.pause");
        id
    });

    // The recorded turn, byte-shaped like `persist_event` writes it.
    let session_dir = temp
        .path()
        .join("home")
        .join("sessions")
        .join(&daemon_session);
    std::fs::create_dir_all(&session_dir).expect("session dir");
    let envelope = |seq: u64, event: &str, data: Value| {
        json!({
            "type": "event",
            "session_id": daemon_session,
            "event": event,
            "data": data,
            "timestamp": "2026-08-11T12:00:01Z",
            "seq": seq,
        })
    };
    let mut log = String::new();
    for (seq, event, data) in [
        (
            1,
            "user_message",
            json!({"message_id": "m1", "content": "Fix the parser"}),
        ),
        (2, "thinking", json!({"content": "read parser.rs first"})),
        (3, "text_delta", json!({"content": "I will read "})),
        (4, "text_delta", json!({"content": "parser.rs"})),
        (
            5,
            "tool_call",
            json!({"call_id": "c1", "tool": "read_file", "args": {"path": "src/parser.rs"}}),
        ),
        (
            6,
            "tool_result",
            json!({"call_id": "c1", "tool": "read_file", "result": {"output": "fn parse() {}"}}),
        ),
        (
            7,
            "message_complete",
            json!({"message_id": "m2", "full_response": "I will read parser.rs"}),
        ),
    ] {
        log.push_str(&envelope(seq, event, data).to_string());
        log.push('\n');
    }
    std::fs::write(session_dir.join("session.jsonl"), log).expect("write log");

    send(
        &mut stdin,
        &json!({"jsonrpc":"2.0","id":3,"method":"session/load","params":{"sessionId": daemon_session, "cwd": kiln, "mcpServers": []}}),
    );
    let (notifications, reply) = exchange(&mut stdout, 3);
    assert!(reply.get("error").is_none(), "session/load failed: {reply}");

    let updates: Vec<Value> = notifications
        .iter()
        .filter(|n| {
            n["method"] == "session/update"
                && n["params"]["sessionId"].as_str() == Some(daemon_session.as_str())
        })
        .map(|n| n["params"]["update"].clone())
        .collect();
    let kinds: Vec<&str> = updates
        .iter()
        .map(|u| u["sessionUpdate"].as_str().expect("update tag"))
        .collect();
    assert_eq!(
        kinds,
        vec![
            "user_message_chunk",
            "agent_thought_chunk",
            "agent_message_chunk",
            "agent_message_chunk",
            "tool_call",
            "tool_call_update",
        ],
        "replayed transcript mismatch: {updates:?}"
    );
    assert_eq!(updates[0]["content"]["text"], "Fix the parser");
    assert_eq!(updates[1]["content"]["text"], "read parser.rs first");
    assert_eq!(updates[2]["content"]["text"], "I will read ");
    assert_eq!(updates[3]["content"]["text"], "parser.rs");
    assert_eq!(updates[4]["toolCallId"], "c1");
    assert_eq!(updates[4]["title"], "read file");
    assert_eq!(updates[4]["status"], "in_progress");
    assert_eq!(updates[4]["rawInput"], json!({"path": "src/parser.rs"}));
    assert_eq!(updates[5]["toolCallId"], "c1");
    assert_eq!(updates[5]["status"], "completed");
    assert_eq!(updates[5]["content"][0]["content"]["text"], "fn parse() {}");

    // Closing stdin ends the connection; a replay path that hangs or panics
    // shows up here.
    drop(stdin);
    let status = child.wait().expect("wait for cru acp");
    assert!(status.success(), "cru acp exited with {status}");
}
