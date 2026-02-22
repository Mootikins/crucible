//! CLI binary-level E2E coverage for delegation-related behavior.
//!
//! These tests focus on the `cru` executable surface area (help text, exit behavior,
//! and daemon-backed command wiring) rather than daemon-internal delegation logic.

#[allow(deprecated)]
mod cli_e2e_helpers;

use cli_e2e_helpers::*;
use predicates::prelude::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

fn start_openai_compat_delegate_tool_server() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_clone = Arc::clone(&calls);

    let handle = thread::spawn(move || {
        for stream in listener.incoming().take(2) {
            let mut stream = match stream {
                Ok(s) => s,
                Err(_) => continue,
            };

            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = [0u8; 16384];
            let _ = stream.read(&mut buf);

            let call_index = calls_clone.fetch_add(1, Ordering::SeqCst);
            let body = if call_index == 0 {
                concat!(
                    "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_delegate_1\",\"type\":\"function\",\"function\":{\"name\":\"delegate_session\",\"arguments\":\"{\\\"target\\\":\\\"opencode\\\",\\\"prompt\\\":\\\"delegate this task\\\"}\"}}]},\"finish_reason\":null}] }\n",
                    "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}] }\n",
                    "data: [DONE]\n"
                )
                .to_string()
            } else {
                concat!(
                    "data: {\"id\":\"chatcmpl-2\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"done\"},\"finish_reason\":null}] }\n",
                    "data: {\"id\":\"chatcmpl-2\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}] }\n",
                    "data: [DONE]\n"
                )
                .to_string()
            };

            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    (format!("http://{}", addr), handle)
}

#[test]
fn session_configure_help_exposes_expected_flags() {
    cru()
        .args(["session", "configure", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Configure agent for a session"))
        .stdout(predicate::str::contains("--provider"))
        .stdout(predicate::str::contains("--model"))
        .stdout(predicate::str::contains("--endpoint"));
}

#[test]
fn session_send_help_exposes_expected_usage() {
    cru()
        .args(["session", "send", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Send a message to a session"))
        .stdout(predicate::str::contains("<SESSION_ID>"))
        .stdout(predicate::str::contains("<MESSAGE>"))
        .stdout(predicate::str::contains("--raw"));
}

#[test]
fn session_configure_nonexistent_session_fails_gracefully() {
    let temp = tempfile::tempdir().unwrap();
    let config_path = write_config(temp.path(), "");
    let socket_path = temp.path().join("daemon.sock");

    cru()
        .env("CRUCIBLE_SOCKET", &socket_path)
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "session",
            "configure",
            "missing-session-id",
            "--provider",
            "ollama",
            "--model",
            "llama3.2",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error:"));
}

#[test]
#[ignore = "requires daemon configure RPC to succeed for existing sessions"]
fn session_configure_updates_existing_session_via_cli() {
    let daemon = TestDaemon::start();

    let create_output = daemon
        .command()
        .args(["session", "create", "--session-type", "chat"])
        .output()
        .unwrap();
    assert!(
        create_output.status.success(),
        "session create failed: {create_output:?}"
    );
    let session_id = extract_session_id(&create_output.stdout);

    daemon
        .command()
        .args([
            "session",
            "configure",
            &session_id,
            "--provider",
            "ollama",
            "--model",
            "llama3.2",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Configured agent: ollama / llama3.2",
        ));
}

#[test]
#[ignore = "requires daemon and mock OpenAI-compatible SSE server"]
fn session_send_surfaces_delegation_disabled_error() {
    let daemon = TestDaemon::start();
    let (endpoint, server_handle) = start_openai_compat_delegate_tool_server();

    let create_output = daemon
        .command()
        .args(["session", "create", "--session-type", "chat"])
        .output()
        .unwrap();
    assert!(
        create_output.status.success(),
        "session create failed: {create_output:?}"
    );
    let session_id = extract_session_id(&create_output.stdout);

    daemon
        .command()
        .args([
            "session",
            "configure",
            &session_id,
            "--provider",
            "openai",
            "--model",
            "gpt-4o-mini",
            "--endpoint",
            &endpoint,
        ])
        .assert()
        .success();

    daemon
        .command()
        .args(["session", "send", &session_id, "please delegate this task"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("delegation").and(predicate::str::contains("disabled")));

    let _ = server_handle.join();
}
