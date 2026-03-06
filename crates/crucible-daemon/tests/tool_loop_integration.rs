use crucible_config::BackendType;
use crucible_core::session::SessionAgent;
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration};

mod streaming_mock;

use streaming_mock::{next_event, TestHarness};

fn make_internal_openai_agent(endpoint: String) -> SessionAgent {
    SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: None,
        provider: BackendType::OpenAI,
        model: "gpt-4o-mini".to_string(),
        system_prompt: "You are helpful.".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: Some(endpoint),
        env_overrides: HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
    }
}

fn openai_tool_call_sse() -> String {
    let first = json!({
        "id": "chatcmpl-tool",
        "object": "chat.completion.chunk",
        "created": 0,
        "model": "gpt-4o-mini",
        "choices": [{
            "index": 0,
            "delta": {
                "role": "assistant",
                "tool_calls": [{
                    "index": 0,
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"/tmp/test.txt\"}"
                    }
                }]
            },
            "finish_reason": null
        }]
    });

    let second = json!({
        "id": "chatcmpl-tool",
        "object": "chat.completion.chunk",
        "created": 0,
        "model": "gpt-4o-mini",
        "choices": [{
            "index": 0,
            "delta": {},
            "finish_reason": "tool_calls"
        }]
    });

    format!("data: {first}\n\ndata: {second}\n\ndata: [DONE]\n\n")
}

fn openai_text_sse(text: &str) -> String {
    let first = json!({
        "id": "chatcmpl-text",
        "object": "chat.completion.chunk",
        "created": 0,
        "model": "gpt-4o-mini",
        "choices": [{
            "index": 0,
            "delta": {"content": text},
            "finish_reason": null
        }]
    });

    let second = json!({
        "id": "chatcmpl-text",
        "object": "chat.completion.chunk",
        "created": 0,
        "model": "gpt-4o-mini",
        "choices": [{
            "index": 0,
            "delta": {},
            "finish_reason": "stop"
        }]
    });

    format!("data: {first}\n\ndata: {second}\n\ndata: [DONE]\n\n")
}

struct MockOpenAiServer {
    endpoint_v1: String,
    request_count: Arc<AtomicUsize>,
    request_bodies: Arc<Mutex<Vec<String>>>,
    task: JoinHandle<()>,
}

impl MockOpenAiServer {
    async fn start<F>(responder: F) -> Self
    where
        F: Fn(usize, &str) -> String + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock server");
        let addr = listener.local_addr().expect("local addr");
        let responder = Arc::new(responder);
        let request_count = Arc::new(AtomicUsize::new(0));
        let request_bodies: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        let responder_clone = responder.clone();
        let request_count_clone = request_count.clone();
        let request_bodies_clone = request_bodies.clone();

        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(v) => v,
                    Err(_) => break,
                };

                let mut buf = Vec::new();
                let mut tmp = [0u8; 1024];
                let headers_end;
                loop {
                    let n = match socket.read(&mut tmp).await {
                        Ok(0) => return,
                        Ok(n) => n,
                        Err(_) => return,
                    };
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        headers_end = pos + 4;
                        break;
                    }
                }

                let headers = String::from_utf8_lossy(&buf[..headers_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if name.eq_ignore_ascii_case("content-length") {
                            value.trim().parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);

                let mut body = buf[headers_end..].to_vec();
                while body.len() < content_length {
                    let n = match socket.read(&mut tmp).await {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(_) => break,
                    };
                    body.extend_from_slice(&tmp[..n]);
                }

                let body_str = String::from_utf8_lossy(&body).to_string();
                request_bodies_clone
                    .lock()
                    .expect("lock request bodies")
                    .push(body_str.clone());

                let req_num = request_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
                let sse_body = responder_clone(req_num, &body_str);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    sse_body.len(),
                    sse_body
                );

                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });

        Self {
            endpoint_v1: format!("http://{addr}/v1"),
            request_count,
            request_bodies,
            task,
        }
    }
}

impl Drop for MockOpenAiServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn tool_loop_single_call_executes_tool_and_continues() {
    std::fs::write("/tmp/test.txt", "tool loop integration file").expect("write /tmp/test.txt");

    let server = MockOpenAiServer::start(|request_number, _body| {
        if request_number == 1 {
            openai_tool_call_sse()
        } else {
            openai_text_sse("I read the file")
        }
    })
    .await;

    let harness = TestHarness::new().await;
    harness
        .agent_manager
        .configure_agent(
            &harness.session_id,
            make_internal_openai_agent(server.endpoint_v1.clone()),
        )
        .await
        .expect("configure agent");

    let (event_tx, mut event_rx) = broadcast::channel(128);
    harness
        .agent_manager
        .send_message(&harness.session_id, "read the file".to_string(), &event_tx)
        .await
        .expect("send message");

    let tool_call = next_event(&mut event_rx, "tool_call").await;
    assert_eq!(tool_call.data["tool"], "read_file");

    let tool_result = next_event(&mut event_rx, "tool_result").await;
    assert_eq!(tool_result.data["tool"], "read_file");

    let complete = next_event(&mut event_rx, "message_complete").await;
    assert_eq!(complete.data["full_response"], "I read the file");

    assert_eq!(server.request_count.load(Ordering::SeqCst), 2);
    let bodies = server.request_bodies.lock().expect("lock request bodies");
    assert!(
        bodies.iter().any(|body| body.contains("\"role\":\"tool\"")),
        "expected continuation request to include tool role message"
    );
}

#[tokio::test]
async fn tool_loop_stops_when_max_tool_depth_exceeded() {
    std::fs::write("/tmp/test.txt", "max depth integration file").expect("write /tmp/test.txt");

    let server = MockOpenAiServer::start(|_request_number, _body| openai_tool_call_sse()).await;

    let harness = TestHarness::new().await;
    harness
        .agent_manager
        .configure_agent(
            &harness.session_id,
            make_internal_openai_agent(server.endpoint_v1.clone()),
        )
        .await
        .expect("configure agent");

    let (event_tx, mut event_rx) = broadcast::channel(256);
    harness
        .agent_manager
        .send_message(&harness.session_id, "loop forever".to_string(), &event_tx)
        .await
        .expect("send message");

    let ended = next_event(&mut event_rx, "ended").await;
    let reason = ended.data["reason"]
        .as_str()
        .expect("ended reason should be string");
    assert!(
        reason.contains("max_tool_depth exceeded"),
        "unexpected end reason: {reason}"
    );

    assert_eq!(server.request_count.load(Ordering::SeqCst), 11);
}

#[tokio::test]
async fn tool_loop_without_tool_calls_streams_normal_response() {
    let server =
        MockOpenAiServer::start(|_request_number, _body| openai_text_sse("Plain response")).await;

    let harness = TestHarness::new().await;
    harness
        .agent_manager
        .configure_agent(
            &harness.session_id,
            make_internal_openai_agent(server.endpoint_v1.clone()),
        )
        .await
        .expect("configure agent");

    let (event_tx, mut event_rx) = broadcast::channel(128);
    harness
        .agent_manager
        .send_message(
            &harness.session_id,
            "plain response please".to_string(),
            &event_tx,
        )
        .await
        .expect("send message");

    let complete = next_event(&mut event_rx, "message_complete").await;
    assert_eq!(complete.data["full_response"], "Plain response");

    let no_tool_call_seen = timeout(Duration::from_millis(300), async {
        loop {
            let event = event_rx.recv().await.expect("event channel closed");
            if event.event == "tool_call" {
                return false;
            }
        }
    })
    .await
    .is_err();

    assert!(
        no_tool_call_seen,
        "unexpected tool_call event in plain response flow"
    );
    assert_eq!(server.request_count.load(Ordering::SeqCst), 1);
}
