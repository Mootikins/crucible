//! Production-wiring delegation e2e.
//!
//! Drives `delegate_session` through the REAL server construction
//! (`Server::bind_with_data_home` → RPC → AgentManager → DelegationService)
//! with a scripted OpenAI-compatible SSE endpoint standing in for the LLM.
//! This is the test class whose absence let the original "subagent factory
//! never wired in production" bug ship: every prior delegation test injected
//! its own manager wiring.

use super::*;
use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::thread;

/// Scripted OpenAI-compatible SSE server:
/// call 1 (parent) → `delegate_session` tool call,
/// call 2 (child) → text "CHILD-SAYS-HI",
/// call 3 (parent continuation) → text "PARENT-DONE".
fn start_scripted_llm_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    thread::spawn(move || {
        listener.set_nonblocking(true).ok();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut call_index = 0usize;
        while call_index < 3 && std::time::Instant::now() < deadline {
            let mut stream = match listener.accept() {
                Ok((s, _)) => s,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(std::time::Duration::from_millis(20));
                    continue;
                }
                Err(_) => return,
            };
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
            let mut buf = [0u8; 65536];
            let _ = stream.read(&mut buf);

            let body = match call_index {
                0 => concat!(
                    "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_d1\",\"type\":\"function\",\"function\":{\"name\":\"delegate_session\",\"arguments\":\"{\\\"prompt\\\":\\\"summarize the notes\\\",\\\"description\\\":\\\"e2e delegation\\\"}\"}}]},\"finish_reason\":null}] }\n\n",
                    "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}] }\n\n",
                    "data: [DONE]\n\n"
                ),
                1 => concat!(
                    "data: {\"id\":\"c2\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"CHILD-SAYS-HI\"},\"finish_reason\":null}] }\n\n",
                    "data: {\"id\":\"c2\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}] }\n\n",
                    "data: [DONE]\n\n"
                ),
                _ => concat!(
                    "data: {\"id\":\"c3\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"PARENT-DONE\"},\"finish_reason\":null}] }\n\n",
                    "data: {\"id\":\"c3\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}] }\n\n",
                    "data: [DONE]\n\n"
                ),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
            call_index += 1;
        }
    });

    format!("http://{}", addr)
}

#[tokio::test(flavor = "multi_thread")]
async fn delegate_session_works_through_production_server_wiring() {
    let endpoint = start_scripted_llm_server();
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let parent_id = create_chat_session(&mut client, &server.kiln_path, 900).await;

    // Configure a delegation-enabled internal agent against the scripted LLM.
    let configure = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 901,
            "method": "session.configure_agent",
            "params": {
                "session_id": parent_id,
                "agent": {
                    "agent_type": "internal",
                    "provider": "openai",
                    "provider_key": "openai",
                    "model": "gpt-4o-mini",
                    "endpoint": endpoint,
                    "system_prompt": "You are a delegation e2e test agent.",
                    "precognition_enabled": false,
                    "delegation_config": {
                        "enabled": true,
                        "max_depth": 1,
                        "result_max_bytes": 51200,
                        "max_concurrent_delegations": 3,
                        "timeout_secs": 60
                    }
                }
            }
        }),
    )
    .await;
    assert!(
        configure["error"].is_null(),
        "configure_agent failed: {configure:?}"
    );

    // Fire the turn. The scripted LLM makes the parent call delegate_session
    // (blocking, no target → child clones the parent's agent config).
    let send = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 902,
            "method": "session.send_message",
            "params": {
                "session_id": parent_id,
                "content": "please delegate",
                "is_interactive": false,
                "permission_mode": "allow"
            }
        }),
    )
    .await;
    assert!(send["error"].is_null(), "send_message failed: {send:?}");

    // Poll: a hidden child session must appear, linked to the parent, and
    // the parent's transcript must eventually record the delegation result.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut child_seen = false;
    let mut result_seen = false;
    let mut req_id = 910i64;
    while tokio::time::Instant::now() < deadline && !(child_seen && result_seen) {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        if !child_seen {
            req_id += 1;
            let list = rpc_call(
                &mut client,
                json!({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "method": "session.list",
                    "params": {
                        "kiln": server.kiln_path.to_string_lossy(),
                        "include_children": true
                    }
                }),
            )
            .await;
            if let Some(sessions) = list["result"]["sessions"].as_array() {
                child_seen = sessions
                    .iter()
                    .any(|s| s["parent_session_id"].as_str() == Some(parent_id.as_str()));
            }
        }

        if !result_seen {
            req_id += 1;
            let events = rpc_call(
                &mut client,
                json!({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "method": "session.resume_from_storage",
                    "params": {
                        "session_id": parent_id,
                        "kiln": server.kiln_path.to_string_lossy()
                    }
                }),
            )
            .await;
            let history = events["result"]["history"].to_string();
            result_seen = history.contains("CHILD-SAYS-HI");
        }
    }

    assert!(
        child_seen,
        "a parent-linked child session must be created through production wiring"
    );
    assert!(
        result_seen,
        "the child's result must reach the parent transcript"
    );
}
