//! One scripted ACP agent on the SDK `Agent` role.
//!
//! A [`MockScript`] says what the agent advertises and what each prompt turn
//! does. The same code serves two ways:
//!
//! - in process, over a pipe: [`connect`] gives a `CrucibleAcpClient`;
//! - as the `mock-acp-agent` binary, which reads the script as JSON from
//!   `CRU_MOCK_SCRIPT` (see [`MockScript::env`]).
//!
//! With `log` set, the agent appends one JSON line per inbound frame to that
//! file: `{"method": …, "params": …}`. It also logs the answer to its own
//! permission request (`permission/answer`) and the reply to its MCP call
//! (`mcp/result`). A test reads the log with [`read_log`] or [`logged`].
//!
//! `acp_support` is `#[path]`-included by the binary and by several test
//! binaries, and each uses a different part of this file, so the allows are
//! per item.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::v1::{
    CancelNotification, CloseSessionRequest, CloseSessionResponse, ContentBlock, ContentChunk,
    Error, InitializeRequest, InitializeResponse, NewSessionRequest, NewSessionResponse,
    PromptRequest, PromptResponse, RequestPermissionRequest, ResumeSessionRequest,
    ResumeSessionResponse, SessionId, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, SetSessionConfigOptionResponse, SetSessionModeRequest,
    SetSessionModeResponse, StopReason,
};
use agent_client_protocol::{
    on_receive_notification, on_receive_request, Agent, Client, ConnectTo, ConnectionTo,
    JsonRpcMessage, UntypedMessage,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{watch, Notify};

/// What the agent advertises, and what each prompt turn does.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MockScript {
    /// `agentInfo.name`.
    pub name: String,
    pub mcp_http: bool,
    pub mcp_sse: bool,
    /// Answer `initialize` with an error.
    pub fail_initialize: bool,
    /// Advertise and answer `session/close`. Off: `-32601`.
    pub session_close: bool,
    /// How to answer `session/resume`. `None`: `-32601`.
    pub session_resume: Option<Resume>,
    /// The current mode of a declared mode set. `None`: no modes.
    pub mode: Option<String>,
    /// The offered mode ids. `None`: `default`, `acceptEdits`, `plan`. The
    /// current mode is not added: an agent whose current mode is not offered
    /// is malformed, and a client has to survive it.
    pub mode_ids: Option<Vec<String>>,
    /// Advertise a model selector in `configOptions`.
    pub models: bool,
    /// Advertise the agent's own options, with this `thought_level`.
    pub agent_options: Option<String>,
    /// What each prompt turn does, in order. The turn ends with `end_turn`
    /// unless a step says otherwise.
    pub turn: Vec<Step>,
    /// The frame log. See the module docs.
    pub log: Option<PathBuf>,
}

impl Default for MockScript {
    fn default() -> Self {
        Self {
            name: "mock-agent".to_string(),
            mcp_http: true,
            mcp_sse: false,
            fail_initialize: false,
            session_close: false,
            session_resume: None,
            mode: None,
            mode_ids: None,
            models: false,
            agent_options: None,
            turn: Vec::new(),
            log: None,
        }
    }
}

/// How the agent answers `session/resume`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resume {
    /// Continue the requested session.
    Adopt,
    /// `-32002`: the agent knows the method but not the session.
    Unknown,
    /// `-32602`: a refusal that is not a fallback signal.
    Reject,
}

/// One step of a prompt turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    /// An `agent_message_chunk`.
    Text(String),
    /// An `agent_thought_chunk`.
    Thought(String),
    /// A raw `session/update` payload, sent as it is.
    Update(Value),
    /// `session/request_permission` with these params; `sessionId` is added.
    /// The next steps run while the question is open. The turn waits for
    /// every answer before it ends.
    Permission(Value),
    /// Call a tool on the HTTP MCP server that `session/new` offered.
    McpCall { tool: String, args: Value },
    /// Hold the turn until `session/cancel`, then end it with `cancelled`.
    /// Only a turn that starts before the first cancel holds. `tick_ms`
    /// streams a "." chunk per tick while it holds. `ignore_cancel` keeps
    /// the turn open after the cancel.
    Hold {
        #[serde(default)]
        tick_ms: Option<u64>,
        #[serde(default)]
        ignore_cancel: bool,
    },
    /// End the connection. The binary process then exits.
    Exit,
    /// End the turn with this stop reason.
    Stop(StopReason),
}

impl MockScript {
    /// The env var that gives this script to the `mock-acp-agent` binary.
    #[allow(dead_code)]
    pub fn env(&self) -> (String, String) {
        (
            "CRU_MOCK_SCRIPT".to_string(),
            serde_json::to_string(self).expect("the script serializes"),
        )
    }

    /// The script in `CRU_MOCK_SCRIPT`, or the default script.
    #[allow(dead_code)]
    pub fn from_env() -> Self {
        match std::env::var("CRU_MOCK_SCRIPT") {
            Ok(json) => serde_json::from_str(&json).expect("CRU_MOCK_SCRIPT holds a MockScript"),
            Err(_) => Self::default(),
        }
    }
}

/// The frames that the agent logged, in order.
#[allow(dead_code)]
pub fn read_log(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// The params of each logged frame with `method`, in order.
#[allow(dead_code)]
pub fn logged(path: &Path, method: &str) -> Vec<Value> {
    read_log(path)
        .into_iter()
        .filter(|frame| frame["method"] == method)
        .map(|frame| frame["params"].clone())
        .collect()
}

/// A client connected to a mock agent that serves `script` in process, over
/// a pipe. `timeout_ms` is the client's `ClientConfig::timeout_ms`. Abort the
/// returned task to kill the agent.
#[allow(dead_code)]
pub async fn connect(
    script: MockScript,
    timeout_ms: Option<u64>,
    permission: Option<crucible_daemon::acp::client::PermissionRequestHandler>,
) -> (
    crucible_daemon::acp::CrucibleAcpClient,
    tokio::task::JoinHandle<()>,
) {
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    let (client_end, agent_end) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = tokio::io::split(client_end);
    let (agent_read, agent_write) = tokio::io::split(agent_end);
    let agent = tokio::spawn(serve(
        script,
        agent_client_protocol::ByteStreams::new(agent_write.compat_write(), agent_read.compat()),
    ));
    let config = crucible_daemon::acp::client::ClientConfig {
        agent_path: PathBuf::from("mock-agent"),
        timeout_ms,
        ..Default::default()
    };
    let client = crucible_daemon::acp::CrucibleAcpClient::connect(
        config,
        agent_client_protocol::ByteStreams::new(client_write.compat_write(), client_read.compat()),
        "mock-agent",
        permission,
    )
    .await
    .expect("the client connects");
    (client, agent)
}

struct State {
    script: MockScript,
    mcp_url: Option<String>,
}

type Shared = Arc<Mutex<State>>;

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, State> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Append one frame to the log. One `write_all` of the whole line on an
/// append-mode file, so a reader never sees half a line.
fn log(shared: &Shared, method: &str, params: impl Serialize) {
    let Some(path) = lock(shared).script.log.clone() else {
        return;
    };
    let line = json!({"method": method, "params": params}).to_string() + "\n";
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let _ = file.write_all(line.as_bytes());
    }
}

/// A typed answer from its JSON shape.
fn answer<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).expect("the mock builds a valid answer")
}

/// The model selector, in the shape claude-agent-acp sends.
fn model_options(current: &str) -> Value {
    json!([{
        "id": "model",
        "name": "Model",
        "category": "model",
        "type": "select",
        "currentValue": current,
        "options": [
            {"value": "mock-sonnet", "name": "Mock Sonnet"},
            {"value": "mock-opus", "name": "Mock Opus"}
        ]
    }])
}

/// Options that are the agent's own: a `thought_level` select, a category
/// Crucible has no knob for, and a plain toggle.
fn agent_options(thought_level: &str) -> Value {
    json!([
        {
            "id": "thought_level",
            "name": "Reasoning",
            "description": "How long the agent thinks before answering",
            "category": "thought_level",
            "type": "select",
            "currentValue": thought_level,
            "options": [{"value": "low", "name": "Low"}, {"value": "high", "name": "High"}]
        },
        {"id": "verbose_logs", "name": "Verbose logs", "type": "boolean", "currentValue": false}
    ])
}

fn mode_state(script: &MockScript) -> Value {
    let Some(current) = &script.mode else {
        return Value::Null;
    };
    let available: Vec<Value> = match &script.mode_ids {
        Some(ids) => ids.iter().map(|id| json!({"id": id, "name": id})).collect(),
        None => vec![
            json!({"id": "default", "name": "Manual", "description": "Always ask first"}),
            json!({"id": "acceptEdits", "name": "Accept edits", "description": "Take file edits"}),
            json!({"id": "plan", "name": "Plan", "description": "Plan before changing"}),
        ],
    };
    json!({"currentModeId": current, "availableModes": available})
}

/// Serve the agent role over `transport` until the client closes it or a
/// turn runs an [`Step::Exit`].
#[allow(dead_code)]
pub async fn serve(script: MockScript, transport: impl ConnectTo<Agent> + 'static) {
    let shared: Shared = Arc::new(Mutex::new(State {
        script,
        mcp_url: None,
    }));
    // The number of `session/cancel` notifications so far.
    let (cancels, _) = watch::channel(0_u32);
    let exit = Arc::new(Notify::new());

    let connection = Agent
        .builder()
        .on_receive_request(
            {
                let shared = shared.clone();
                async move |req: InitializeRequest, responder, _cx| {
                    log(&shared, req.method(), &req);
                    let script = lock(&shared).script.clone();
                    if script.fail_initialize {
                        return responder.respond_with_error(Error::new(
                            -32000,
                            "Simulated initialization error",
                        ));
                    }
                    let mut session = serde_json::Map::new();
                    if script.session_close {
                        session.insert("close".into(), json!({}));
                    }
                    if script.session_resume.is_some() {
                        session.insert("resume".into(), json!({}));
                    }
                    responder.respond(answer::<InitializeResponse>(json!({
                        "protocolVersion": req.protocol_version,
                        "agentCapabilities": {
                            "mcpCapabilities": {"http": script.mcp_http, "sse": script.mcp_sse},
                            "sessionCapabilities": session
                        },
                        "authMethods": [],
                        "agentInfo": {"name": script.name, "version": "1.0.0"}
                    })))
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let shared = shared.clone();
                async move |req: NewSessionRequest, responder, _cx| {
                    log(&shared, req.method(), &req);
                    let mut state = lock(&shared);
                    state.mcp_url =
                        serde_json::to_value(&req.mcp_servers)
                            .ok()
                            .and_then(|servers| {
                                servers.as_array()?.iter().find_map(|s| {
                                    s.get("url").and_then(Value::as_str).map(str::to_string)
                                })
                            });
                    let mut options = Vec::new();
                    if state.script.models {
                        options.extend(
                            model_options("mock-sonnet")
                                .as_array()
                                .cloned()
                                .unwrap_or_default(),
                        );
                    }
                    if let Some(level) = &state.script.agent_options {
                        options
                            .extend(agent_options(level).as_array().cloned().unwrap_or_default());
                    }
                    let options = (!options.is_empty()).then_some(options);
                    responder.respond(answer::<NewSessionResponse>(json!({
                        "sessionId": format!("mock-session-{}", uuid::Uuid::new_v4()),
                        "modes": mode_state(&state.script),
                        "configOptions": options
                    })))
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let shared = shared.clone();
                async move |req: ResumeSessionRequest, responder, _cx| {
                    log(&shared, req.method(), &req);
                    let state = lock(&shared);
                    match state.script.session_resume {
                        None => responder.respond_with_error(Error::method_not_found()),
                        Some(Resume::Unknown) => responder.respond_with_error(Error::new(
                            -32002,
                            format!("Resource not found: {}", req.session_id),
                        )),
                        Some(Resume::Reject) => {
                            responder.respond_with_error(Error::new(-32602, "no such session"))
                        }
                        Some(Resume::Adopt) => responder.respond(answer::<ResumeSessionResponse>(
                            json!({"modes": mode_state(&state.script)}),
                        )),
                    }
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let shared = shared.clone();
                async move |req: SetSessionModeRequest, responder, _cx| {
                    log(&shared, req.method(), &req);
                    responder.respond(SetSessionModeResponse::new())
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let shared = shared.clone();
                async move |req: SetSessionConfigOptionRequest, responder, _cx| {
                    log(&shared, req.method(), &req);
                    // The typed value serializes as `{"value": id}`.
                    let value = serde_json::to_value(&req.value).unwrap_or_default();
                    let value = value
                        .get("value")
                        .unwrap_or(&value)
                        .as_str()
                        .unwrap_or_default();
                    responder.respond(answer::<SetSessionConfigOptionResponse>(
                        json!({"configOptions": model_options(value)}),
                    ))
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let shared = shared.clone();
                async move |req: CloseSessionRequest, responder, _cx| {
                    log(&shared, req.method(), &req);
                    if lock(&shared).script.session_close {
                        responder.respond(CloseSessionResponse::new())
                    } else {
                        responder.respond_with_error(Error::method_not_found())
                    }
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let shared = shared.clone();
                let cancels = cancels.clone();
                let exit = exit.clone();
                async move |req: PromptRequest, responder, cx: ConnectionTo<Client>| {
                    log(&shared, req.method(), &req);
                    let turn = Turn {
                        shared: shared.clone(),
                        cx: cx.clone(),
                        session_id: req.session_id.clone(),
                        cancels: cancels.subscribe(),
                        exit: exit.clone(),
                    };
                    // The dispatch loop waits for a handler, and a held turn
                    // must still read `session/cancel`.
                    cx.spawn(async move {
                        if let Some(stop) = turn.run().await {
                            let _ = responder.respond(PromptResponse::new(stop));
                        }
                        Ok(())
                    })
                }
            },
            on_receive_request!(),
        )
        .on_receive_notification(
            {
                let shared = shared.clone();
                async move |notification: CancelNotification, _cx| {
                    log(&shared, notification.method(), &notification);
                    cancels.send_modify(|count| *count += 1);
                    Ok(())
                }
            },
            on_receive_notification!(),
        )
        .connect_to(transport);

    tokio::select! {
        result = connection => {
            if let Err(error) = result {
                eprintln!("mock agent connection ended: {error}");
            }
        }
        () = exit.notified() => {}
    }
}

/// One prompt turn.
struct Turn {
    shared: Shared,
    cx: ConnectionTo<Client>,
    session_id: SessionId,
    cancels: watch::Receiver<u32>,
    exit: Arc<Notify>,
}

impl Turn {
    /// Run the steps. `None` means the turn never answers.
    async fn run(self) -> Option<StopReason> {
        let steps = lock(&self.shared).script.turn.clone();
        let cancels_at_start = *self.cancels.borrow();
        let mut stop = StopReason::EndTurn;
        let mut questions = Vec::new();
        for step in steps {
            match step {
                Step::Text(text) => self.update(SessionUpdate::AgentMessageChunk(
                    ContentChunk::new(ContentBlock::from(text)),
                )),
                Step::Thought(text) => self.update(SessionUpdate::AgentThoughtChunk(
                    ContentChunk::new(ContentBlock::from(text)),
                )),
                Step::Update(update) => {
                    let params = json!({"sessionId": self.session_id, "update": update});
                    let message = UntypedMessage::new("session/update", params)
                        .expect("an update serializes");
                    let _ = self.cx.send_notification(message);
                }
                Step::Permission(mut params) => {
                    params["sessionId"] = json!(self.session_id);
                    let request: RequestPermissionRequest =
                        serde_json::from_value(params).expect("valid permission params");
                    let answer = self.cx.send_request(request).block_task();
                    let shared = self.shared.clone();
                    questions.push(tokio::spawn(async move {
                        let result = match answer.await {
                            Ok(response) => json!({"result": response}),
                            Err(error) => json!({"error": error}),
                        };
                        log(&shared, "permission/answer", result);
                    }));
                }
                Step::McpCall { tool, args } => {
                    let url = lock(&self.shared).mcp_url.clone();
                    let result = tokio::task::spawn_blocking(move || match url {
                        Some(url) => mcp_tool_call(&url, &tool, &args),
                        None => Err("session/new offered no HTTP MCP server".to_string()),
                    })
                    .await
                    .expect("the MCP call task");
                    let result = result.unwrap_or_else(|e| format!("MOCK-MCP-ERROR: {e}"));
                    log(&self.shared, "mcp/result", result);
                }
                Step::Hold {
                    tick_ms,
                    ignore_cancel,
                } => {
                    // A turn after a cancel does not hold, so a test can
                    // cancel one turn and run the next to its end.
                    if cancels_at_start > 0 {
                        continue;
                    }
                    if !self.hold(tick_ms, cancels_at_start).await || ignore_cancel {
                        std::future::pending::<()>().await;
                    }
                    return Some(StopReason::Cancelled);
                }
                Step::Exit => {
                    // The client must read what the turn sent before the
                    // exit. SDK 2.0.0 has no public drain, so one request
                    // makes a round trip: the wire keeps its order, and the
                    // client answers an unknown method with `-32601`.
                    let flush = UntypedMessage::new("_mock/flush", json!({}))
                        .expect("an empty request serializes");
                    let _ = self.cx.send_request(flush).block_task().await;
                    self.exit.notify_one();
                    return None;
                }
                Step::Stop(reason) => stop = reason,
            }
        }
        for question in questions {
            let _ = question.await;
        }
        Some(stop)
    }

    fn update(&self, update: SessionUpdate) {
        let _ = self
            .cx
            .send_notification(SessionNotification::new(self.session_id.clone(), update));
    }

    /// Wait for a cancel after `seen`. Stream a "." chunk per tick while it
    /// waits. `false` when the connection ends first.
    async fn hold(&self, tick_ms: Option<u64>, seen: u32) -> bool {
        let mut cancels = self.cancels.clone();
        loop {
            let tick = async {
                match tick_ms {
                    Some(ms) => tokio::time::sleep(std::time::Duration::from_millis(ms)).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                changed = cancels.wait_for(|count| *count > seen) => return changed.is_ok(),
                () = tick => self.update(SessionUpdate::AgentMessageChunk(ContentChunk::new(
                    ContentBlock::from("."),
                ))),
            }
        }
    }
}

/// Call one tool on the streamable HTTP MCP server at `url`, and return the
/// `tools/call` reply as JSON text.
///
/// This speaks HTTP/1.0 over a blocking `TcpStream`, because the binary has
/// no HTTP client. With HTTP/1.0 the server closes the connection at the end
/// of the body and sends no chunks.
fn mcp_tool_call(url: &str, tool: &str, args: &Value) -> Result<String, String> {
    let (_, session) = mcp_post(
        url,
        None,
        &json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": "mock-acp-agent", "version": "0.1.0" }
            }
        }),
    )?;
    let session = session.ok_or("initialize returned no mcp-session-id")?;
    let initialized = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    mcp_post(url, Some(&session), &initialized)?;
    let call = json!({
        "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": { "name": tool, "arguments": args }
    });
    Ok(mcp_post(url, Some(&session), &call)?.0)
}

/// POST one JSON-RPC frame. Return the JSON-RPC payload of the reply (the
/// first SSE `data:` line, or the whole body) and the `mcp-session-id` header.
fn mcp_post(
    url: &str,
    session: Option<&str>,
    body: &Value,
) -> Result<(String, Option<String>), String> {
    use std::io::{Read, Write};
    let rest = url
        .strip_prefix("http://")
        .ok_or("expected an http:// URL")?;
    let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let mut stream = std::net::TcpStream::connect(host).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    let body = body.to_string();
    let session_header = session
        .map(|id| format!("Mcp-Session-Id: {id}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "POST {path} HTTP/1.0\r\nHost: {host}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\n{session_header}\
         Content-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .map_err(|e| e.to_string())?;
    let mut reply = String::new();
    stream
        .read_to_string(&mut reply)
        .map_err(|e| e.to_string())?;
    let (head, body) = reply.split_once("\r\n\r\n").ok_or("no HTTP header end")?;
    if !head.starts_with("HTTP/1.1 2") && !head.starts_with("HTTP/1.0 2") {
        return Err(format!(
            "MCP status: {}",
            head.lines().next().unwrap_or_default()
        ));
    }
    let session = head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("mcp-session-id")
            .then(|| value.trim().to_string())
    });
    let payload = body
        .lines()
        .find_map(|line| line.strip_prefix("data: ").filter(|d| d.starts_with('{')))
        .unwrap_or(body);
    Ok((payload.to_string(), session))
}

/// A one-text-block prompt for `session_id`.
#[allow(dead_code)]
pub fn make_prompt_request(
    session_id: &str,
    text: &str,
) -> agent_client_protocol::schema::v1::PromptRequest {
    agent_client_protocol::schema::v1::PromptRequest::new(
        SessionId::from(session_id.to_string()),
        vec![ContentBlock::from(text.to_string())],
    )
}

/// A `tool_call` step, in progress, with optional raw input.
#[allow(dead_code)]
pub fn tool_call(id: &str, title: &str, raw_input: Option<Value>) -> Step {
    let mut update = json!({
        "sessionUpdate": "tool_call",
        "toolCallId": id,
        "title": title,
        "status": "in_progress"
    });
    if let Some(input) = raw_input {
        update["rawInput"] = input;
    }
    Step::Update(update)
}

/// A `tool_call_update` step that ends a call with `status`, with optional
/// raw output.
#[allow(dead_code)]
pub fn tool_call_update(id: &str, status: &str, raw_output: Option<Value>) -> Step {
    let mut update = json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": id,
        "status": status
    });
    if let Some(output) = raw_output {
        update["rawOutput"] = output;
    }
    Step::Update(update)
}

/// Run one turn on `client`, and call `on_chunk` for each chunk. A `false`
/// from `on_chunk` drops the turn, the way the daemon cancels one.
#[allow(dead_code)]
pub async fn prompt_with(
    client: &crucible_daemon::acp::CrucibleAcpClient,
    request: agent_client_protocol::schema::v1::PromptRequest,
    mut on_chunk: impl FnMut(crucible_core::turn::TurnEvent) -> bool,
) -> Result<
    (
        crucible_daemon::acp::TurnSummary,
        agent_client_protocol::schema::v1::PromptResponse,
    ),
    crucible_daemon::acp::ClientError,
> {
    let (out, mut chunks) = tokio::sync::mpsc::unbounded_channel();
    let turn = client.prompt(request, &out);
    tokio::pin!(turn);
    let mut open = true;
    loop {
        tokio::select! {
            result = &mut turn => {
                while let Ok(chunk) = chunks.try_recv() {
                    if open {
                        on_chunk(chunk);
                    }
                }
                return result;
            }
            Some(chunk) = chunks.recv(), if open => {
                if !on_chunk(chunk) {
                    open = false;
                    chunks.close();
                }
            }
        }
    }
}
