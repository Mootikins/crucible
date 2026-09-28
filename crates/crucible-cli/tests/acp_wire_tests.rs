//! Pins the raw JSON that `cru acp` writes on stdout.
//!
//! The ACP SDK changes its Rust API between majors, but the v1 wire format
//! stays fixed. These tests read the bytes a host sees, so an SDK upgrade
//! cannot change the wire without a visible diff here. They cover the
//! handshake, the `session/load` replay, and full prompt turns against a
//! mock provider.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crucible_core::protocol::requests::SessionCreateParams;
use crucible_daemon::DaemonClient;
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

    // A session that an earlier daemon recorded: the paused session's
    // record under a new id, with one recorded turn. This daemon never held
    // it, so it folds the log on load. A session that this daemon holds
    // answers its live fold, and a log written behind its back is not in it.
    let sessions = temp.path().join("home").join("sessions");
    let created_session = daemon_session;
    let daemon_session = format!("{created_session}-rec");
    let session_dir = sessions.join(&daemon_session);
    std::fs::create_dir_all(&session_dir).expect("session dir");
    let meta = std::fs::read_to_string(sessions.join(&created_session).join("meta.json"))
        .expect("the paused session's record");
    std::fs::write(
        session_dir.join("meta.json"),
        meta.replace(&created_session, &daemon_session),
    )
    .expect("write record");

    // The recorded turn, byte-shaped like `persist_event` writes it.
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
    let mut kinds: Vec<&str> = updates
        .iter()
        .map(|u| u["sessionUpdate"].as_str().expect("update tag"))
        .collect();
    // The command list follows the replayed transcript.
    assert_eq!(
        kinds.pop(),
        Some("available_commands_update"),
        "{updates:?}"
    );
    assert_eq!(
        kinds,
        vec![
            "user_message_chunk",
            "agent_thought_chunk",
            "agent_message_chunk",
            "tool_call",
            "tool_call_update",
        ],
        "replayed transcript mismatch: {updates:?}"
    );
    // The replay draws the daemon's folded transcript: one chunk for each
    // answer segment, in the place it had before the tool.
    assert_eq!(updates[0]["content"]["text"], "Fix the parser");
    assert_eq!(updates[1]["content"]["text"], "read parser.rs first");
    assert_eq!(updates[2]["content"]["text"], "I will read parser.rs");
    assert_eq!(updates[3]["toolCallId"], "c1");
    assert_eq!(updates[3]["title"], "read file");
    assert_eq!(updates[3]["status"], "in_progress");
    assert_eq!(updates[3]["rawInput"], json!({"path": "src/parser.rs"}));
    assert_eq!(updates[4]["toolCallId"], "c1");
    assert_eq!(updates[4]["status"], "completed");
    assert_eq!(updates[4]["content"][0]["content"]["text"], "fn parse() {}");

    // Closing stdin ends the connection; a replay path that hangs or panics
    // shows up here.
    drop(stdin);
    let status = child.wait().expect("wait for cru acp");
    assert!(status.success(), "cru acp exited with {status}");
}

// ---------------------------------------------------------------------------
// A full prompt turn through the real `cru acp` process.
//
// `cru acp` serves the internal agent, so a turn needs an LLM provider. The
// provider here is an OpenAI-compatible server on a local port that answers
// with a scripted SSE stream. Every hop is real: host pipe, `cru acp`, the
// daemon it starts, the provider HTTP call, and the path back.
// ---------------------------------------------------------------------------

/// What the mock provider does with one chat request.
#[derive(Clone, Copy)]
enum ProviderScript {
    /// Stream `PROVIDER_REPLY` in two chunks, then stop.
    Reply,
    /// Stream the first chunk, then hold the response open until the test
    /// ends. A turn on this script ends only by cancellation.
    Hold,
    /// Answer the first request with one `write_file` call for
    /// `PERMISSION_FILE`, and every later request as `Reply` does.
    WriteThenReply,
}

const PERMISSION_FILE: &str = "permission-probe.txt";

const PROVIDER_REPLY: [&str; 2] = ["pong from ", "the mock provider"];

/// An OpenAI-compatible chat endpoint on a local port.
struct MockProvider {
    endpoint: String,
    requests: std::sync::mpsc::Receiver<Value>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for MockProvider {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let result = thread.join();
            if !std::thread::panicking() {
                result.expect("mock provider panicked");
            }
        }
    }
}

fn sse_chunk(delta: Value, finish: Option<&str>) -> String {
    let frame = json!({
        "id": "chatcmpl-mock",
        "object": "chat.completion.chunk",
        "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]
    });
    format!("data: {frame}\n\n")
}

fn start_mock_provider(script: ProviderScript) -> MockProvider {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock provider");
    listener
        .set_nonblocking(true)
        .expect("nonblocking listener");
    let endpoint = format!("http://{}", listener.local_addr().expect("local addr"));
    let (request_tx, requests) = std::sync::mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));

    let thread = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            let mut held = Vec::new();
            let mut calls = 0;
            while !stop.load(Ordering::SeqCst) {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(e) => panic!("accept failed: {e}"),
                };
                stream.set_nonblocking(false).expect("blocking stream");
                let Some(body) = read_http_body(&mut stream) else {
                    continue;
                };
                let _ = request_tx.send(body);
                calls += 1;

                let header = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
                if matches!(script, ProviderScript::WriteThenReply) && calls == 1 {
                    let args = json!({"path": PERMISSION_FILE, "content": "written"}).to_string();
                    let call = [
                        sse_chunk(
                            json!({"tool_calls": [{
                                "index": 0,
                                "id": "call_write_1",
                                "type": "function",
                                "function": {"name": "write_file", "arguments": args}
                            }]}),
                            None,
                        ),
                        sse_chunk(json!({}), Some("tool_calls")),
                        "data: [DONE]\n\n".to_string(),
                    ]
                    .concat();
                    let _ = stream.write_all(header.as_bytes());
                    let _ = stream.write_all(call.as_bytes());
                    let _ = stream.flush();
                    continue;
                }

                let first = sse_chunk(json!({"content": PROVIDER_REPLY[0]}), None);
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(first.as_bytes());
                let _ = stream.flush();
                match script {
                    ProviderScript::Reply | ProviderScript::WriteThenReply => {
                        let rest = [
                            sse_chunk(json!({"content": PROVIDER_REPLY[1]}), None),
                            sse_chunk(json!({}), Some("stop")),
                            "data: [DONE]\n\n".to_string(),
                        ]
                        .concat();
                        let _ = stream.write_all(rest.as_bytes());
                        let _ = stream.flush();
                    }
                    // Keep the socket so the response never ends.
                    ProviderScript::Hold => held.push(stream),
                }
            }
        })
    };

    MockProvider {
        endpoint,
        requests,
        stop,
        thread: Some(thread),
    }
}

/// Read one HTTP request and return its JSON body.
fn read_http_body(stream: &mut std::net::TcpStream) -> Option<Value> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    let mut reader = BufReader::new(stream);
    let mut content_length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().ok()?;
            }
        }
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

/// `cru acp` with a hermetic environment and a config whose default provider
/// is `provider`. The daemon the child starts inherits both.
struct AcpUnderTest {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<ChildStdout>,
    kiln: std::path::PathBuf,
    temp: TempDir,
}

impl AcpUnderTest {
    fn start(provider: &MockProvider) -> Self {
        let temp = TempDir::new().expect("temp dir");
        let kiln = temp.path().join("kiln");
        std::fs::create_dir_all(kiln.join(".crucible")).expect("create kiln dir");
        std::fs::write(kiln.join(".crucible").join("kiln.toml"), "").expect("write kiln.toml");
        let config = temp.path().join("init.lua");
        std::fs::write(
            &config,
            format!(
                "cru.config.set({{\n  kiln_path = {kiln:?},\n  llm = {{ default = \"mock\", providers = {{ mock = {{ type = \"openai\", endpoint = {endpoint:?}, default_model = \"gpt-4o-mini\", api_key = \"test-key\" }} }} }},\n}})\n",
                kiln = kiln.display().to_string(),
                endpoint = provider.endpoint,
            ),
        )
        .expect("write config");

        let mut child = Self::command(&temp)
            .arg("--config")
            .arg(&config)
            .arg("acp")
            .arg("--kiln")
            .arg(&kiln)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn cru acp");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self {
            child,
            stdin,
            stdout,
            kiln,
            temp,
        }
    }

    /// A `cru` command in this test's hermetic environment.
    fn command(temp: &TempDir) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_cru"));
        cmd.env_clear();
        for (key, value) in crucible_core::test_support::hermetic_env_pairs(temp.path()) {
            cmd.env(key, value);
        }
        cmd.env("CRUCIBLE_LOG_FILE", temp.path().join("acp.log"))
            .env("CRUCIBLE_SOCKET", temp.path().join("daemon.sock"));
        cmd
    }

    fn send(&mut self, msg: &Value) {
        send(&mut self.stdin, msg);
    }

    fn exchange(&mut self, id: i64) -> (Vec<Value>, Value) {
        exchange(&mut self.stdout, id)
    }

    /// `initialize` then `session/new`; returns the session id.
    fn open_session(&mut self) -> String {
        self.send(
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}),
        );
        let (_, init) = self.exchange(1);
        assert!(init.get("error").is_none(), "initialize failed: {init}");

        let cwd = self.kiln.clone();
        self.send(&json!({"jsonrpc":"2.0","id":2,"method":"session/new","params":{"cwd": cwd, "mcpServers": []}}));
        let (_, created) = self.exchange(2);
        created["result"]["sessionId"]
            .as_str()
            .unwrap_or_else(|| panic!("session/new failed: {created}"))
            .to_string()
    }

    /// Close stdin and wait for a clean exit, then stop the daemon the child
    /// started so it does not outlive the test.
    fn finish(self) {
        let Self {
            mut child,
            stdin,
            temp,
            ..
        } = self;
        drop(stdin);
        let status = child.wait().expect("wait for cru acp");
        let _ = Self::command(&temp)
            .args(["daemon", "stop"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        assert!(status.success(), "cru acp exited with {status}");
    }
}

/// The text of every `agent_message_chunk` update for `session`, in order.
fn agent_text(notifications: &[Value], session: &str) -> String {
    notifications
        .iter()
        .filter(|n| n["method"] == "session/update" && n["params"]["sessionId"] == session)
        .filter(|n| n["params"]["update"]["sessionUpdate"] == "agent_message_chunk")
        .filter_map(|n| n["params"]["update"]["content"]["text"].as_str())
        .collect()
}

/// initialize → session/new → session/prompt: the provider's text reaches
/// the host as `agent_message_chunk` updates before the reply, and the turn
/// ends with `end_turn`.
#[test]
fn a_prompt_turn_streams_the_provider_text_and_ends_the_turn() {
    let provider = start_mock_provider(ProviderScript::Reply);
    let mut acp = AcpUnderTest::start(&provider);
    let session = acp.open_session();

    acp.send(&json!({
        "jsonrpc": "2.0", "id": 3, "method": "session/prompt",
        "params": {"sessionId": session, "prompt": [{"type": "text", "text": "ping over acp"}]}
    }));
    let (notifications, reply) = acp.exchange(3);

    assert_eq!(
        reply["result"]["stopReason"], "end_turn",
        "unexpected prompt reply: {reply}"
    );
    assert_eq!(
        agent_text(&notifications, &session),
        PROVIDER_REPLY.concat()
    );
    let request = provider
        .requests
        .try_iter()
        .find(|r| r.to_string().contains("ping over acp"))
        .expect("the prompt reached the provider");
    assert_eq!(request["model"], "gpt-4o-mini");

    acp.finish();
}

/// After `session/new` the host gets the session's commands, without the
/// built-in ones. A prompt that names a mode command switches the mode in the
/// daemon: the result comes back as agent text, and no model call happens.
#[test]
fn the_host_gets_the_command_catalog_and_a_mode_command_runs_in_the_daemon() {
    let provider = start_mock_provider(ProviderScript::Reply);
    let mut acp = AcpUnderTest::start(&provider);
    let session = acp.open_session();

    acp.send(&json!({
        "jsonrpc": "2.0", "id": 3, "method": "session/prompt",
        "params": {"sessionId": session, "prompt": [{"type": "text", "text": "/plan"}]}
    }));
    let (notifications, reply) = acp.exchange(3);

    let advertised: Vec<&str> = notifications
        .iter()
        .filter(|n| n["params"]["update"]["sessionUpdate"] == "available_commands_update")
        .flat_map(|n| {
            n["params"]["update"]["availableCommands"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert!(advertised.contains(&"plan"), "{notifications:#?}");
    assert!(!advertised.contains(&"help"), "{advertised:?}");

    assert_eq!(reply["result"]["stopReason"], "end_turn", "{reply}");
    assert_eq!(agent_text(&notifications, &session), "Mode: plan");
    assert!(
        provider.requests.try_iter().next().is_none(),
        "a mode command calls no model"
    );

    acp.finish();
}

/// A prompt for a session this process never opened is refused with
/// `invalid_params` (-32602), and the connection stays usable.
#[test]
fn a_prompt_for_an_unknown_session_is_invalid_params() {
    let provider = start_mock_provider(ProviderScript::Reply);
    let mut acp = AcpUnderTest::start(&provider);
    acp.send(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}));
    let _ = acp.exchange(1);

    acp.send(&json!({
        "jsonrpc": "2.0", "id": 2, "method": "session/prompt",
        "params": {"sessionId": "no-such-session", "prompt": [{"type": "text", "text": "hi"}]}
    }));
    let (_, reply) = acp.exchange(2);
    assert_eq!(reply["error"]["code"], -32602, "unexpected reply: {reply}");

    acp.send(&json!({"jsonrpc":"2.0","id":3,"method":"initialize","params":{"protocolVersion":1}}));
    let (_, again) = acp.exchange(3);
    assert!(
        again.get("result").is_some(),
        "the connection broke: {again}"
    );
    assert!(
        provider.requests.try_iter().next().is_none(),
        "a refused prompt must not reach the provider"
    );

    acp.finish();
}

/// `session/cancel` during a turn ends it with `cancelled`. The provider
/// holds its stream open, so nothing else can end the turn.
#[test]
fn a_cancel_during_a_turn_ends_it_cancelled() {
    let provider = start_mock_provider(ProviderScript::Hold);
    let mut acp = AcpUnderTest::start(&provider);
    let session = acp.open_session();

    acp.send(&json!({
        "jsonrpc": "2.0", "id": 3, "method": "session/prompt",
        "params": {"sessionId": session, "prompt": [{"type": "text", "text": "wait for me"}]}
    }));
    // The first chunk proves the turn is live before the cancel goes out.
    loop {
        let mut line = String::new();
        let read = acp.stdout.read_line(&mut line).expect("read line");
        assert!(read > 0, "cru acp closed stdout mid-turn");
        let msg: Value = serde_json::from_str(line.trim()).expect("JSON-RPC line");
        assert!(msg.get("id").is_none(), "the turn ended early: {msg}");
        if agent_text(std::slice::from_ref(&msg), &session) == PROVIDER_REPLY[0] {
            break;
        }
    }

    acp.send(&json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId": session}}));
    let (_, reply) = acp.exchange(3);
    assert_eq!(
        reply["result"]["stopReason"], "cancelled",
        "unexpected prompt reply: {reply}"
    );

    acp.finish();
}

/// Run one turn whose tool call needs permission, answer the host's
/// `session/request_permission` with `option_id`, and return the prompt
/// reply and what the tool wrote, if anything.
fn permission_turn(option_id: &str) -> (Value, Option<String>) {
    let provider = start_mock_provider(ProviderScript::WriteThenReply);
    let mut acp = AcpUnderTest::start(&provider);
    let session = acp.open_session();

    acp.send(&json!({
        "jsonrpc": "2.0", "id": 3, "method": "session/prompt",
        "params": {"sessionId": session, "prompt": [{"type": "text", "text": "write the probe"}]}
    }));
    let request = loop {
        let mut line = String::new();
        let read = acp.stdout.read_line(&mut line).expect("read line");
        assert!(read > 0, "cru acp closed stdout mid-turn");
        let msg: Value = serde_json::from_str(line.trim()).expect("JSON-RPC line");
        assert_ne!(msg["id"], 3, "the turn ended before asking: {msg}");
        if msg["method"] == "session/request_permission" {
            break msg;
        }
    };

    assert_eq!(request["params"]["sessionId"], session.as_str());
    let options: Vec<&str> = request["params"]["options"]
        .as_array()
        .expect("options")
        .iter()
        .map(|o| o["optionId"].as_str().expect("optionId"))
        .collect();
    // No "reject always": the daemon cannot store a deny rule, so that
    // option would give only a one-time deny under a wider name.
    assert_eq!(options, ["allow_once", "allow_always", "reject_once"]);
    assert_eq!(request["params"]["toolCall"]["kind"], "edit", "{request}");

    acp.send(&json!({
        "jsonrpc": "2.0", "id": request["id"],
        "result": {"outcome": {"outcome": "selected", "optionId": option_id}}
    }));
    let (_, reply) = acp.exchange(3);
    // Read before `finish`, which removes the temp directory.
    let written = std::fs::read_to_string(acp.kiln.join(PERMISSION_FILE)).ok();
    acp.finish();
    (reply, written)
}

/// The host's "allow once" reaches the daemon: the tool runs, and the turn
/// goes on to its end.
#[test]
fn an_allowed_permission_request_lets_the_tool_run() {
    let (reply, written) = permission_turn("allow_once");
    assert_eq!(reply["result"]["stopReason"], "end_turn", "{reply}");
    assert_eq!(written.as_deref(), Some("written"));
}

/// The host's "reject once" reaches the daemon: the tool does not run, and
/// the turn still ends normally.
#[test]
fn a_rejected_permission_request_keeps_the_tool_from_running() {
    let (reply, written) = permission_turn("reject_once");
    assert_eq!(reply["result"]["stopReason"], "end_turn", "{reply}");
    assert_eq!(written, None, "a rejected write must not land");
}
