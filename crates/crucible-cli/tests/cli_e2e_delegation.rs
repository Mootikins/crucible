//! CLI binary-level E2E coverage for delegation-related behavior.
//!
//! These tests focus on the `cru` executable surface area (help text, exit behavior,
//! and daemon-backed command wiring) rather than daemon-internal delegation logic.

mod cli_e2e_helpers;

use cli_e2e_helpers::*;
use predicates::prelude::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

struct DelegateMock {
    endpoint: String,
    requests: std::sync::mpsc::Receiver<serde_json::Value>,
    stop: std::sync::mpsc::Sender<()>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for DelegateMock {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let result = thread.join();
            if !std::thread::panicking() {
                result.expect("delegate mock panicked");
            }
        }
    }
}

fn start_openai_compat_delegate_tool_server() -> DelegateMock {
    let (stop, stopped) = std::sync::mpsc::channel();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (request_tx, requests) = std::sync::mpsc::channel();

    let handle = thread::spawn(move || {
        // Accept up to 2 connections with a per-accept timeout.
        // Without this, the thread blocks forever if the daemon makes fewer requests
        // (e.g., when delegation is disabled and there's no second LLM call).
        for call_index in 0..2 {
            // Use non-blocking accept with a manual timeout loop
            listener.set_nonblocking(true).ok();
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let stream = loop {
                match listener.accept() {
                    Ok((s, _)) => break Some(s),
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            break None;
                        }
                        if stopped.recv_timeout(Duration::from_millis(20)).is_ok() {
                            return;
                        }
                    }
                    Err(_) => break None,
                }
            };

            let mut stream = match stream {
                Some(s) => s,
                None => return,
            };

            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut reader = BufReader::new(&mut stream);
            let mut content_length = None;
            loop {
                let mut line = String::new();
                assert!(
                    reader.read_line(&mut line).unwrap() > 0,
                    "incomplete HTTP request"
                );
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        content_length = Some(value.trim().parse::<usize>().unwrap());
                    }
                }
            }
            let mut body = vec![0; content_length.expect("request content length")];
            reader.read_exact(&mut body).unwrap();
            request_tx
                .send(serde_json::from_slice(&body).unwrap())
                .unwrap();
            let body = if call_index == 0 {
                concat!(
                    "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_delegate_1\",\"type\":\"function\",\"function\":{\"name\":\"delegate_session\",\"arguments\":\"{\\\"target\\\":\\\"opencode\\\",\\\"prompt\\\":\\\"delegate this task\\\"}\"}}]},\"finish_reason\":null}] }\n\n",
                    "data: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}] }\n\n",
                    "data: [DONE]\n\n"
                )
                .to_string()
            } else {
                concat!(
                    "data: {\"id\":\"chatcmpl-2\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"done\"},\"finish_reason\":null}] }\n\n",
                    "data: {\"id\":\"chatcmpl-2\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}] }\n\n",
                    "data: [DONE]\n\n"
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

    DelegateMock {
        endpoint: format!("http://{}", addr),
        requests,
        stop,
        thread: Some(handle),
    }
}

#[test]
fn session_configure_help_exposes_expected_flags() {
    cru()
        .args(["session", "configure", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Configure agent backend"))
        .stdout(predicate::str::contains("--provider"))
        .stdout(predicate::str::contains("--model"))
        .stdout(predicate::str::contains("--endpoint"))
        .stdout(predicate::str::contains("--format"));
}

#[test]
fn session_send_help_exposes_expected_usage() {
    cru()
        .args(["session", "send", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Send a message to a session"))
        .stdout(predicate::str::contains("[MESSAGE]"))
        .stdout(predicate::str::contains("--raw"));
}

#[test]
fn session_configure_nonexistent_session_fails_gracefully() {
    let daemon = TestDaemon::start();

    daemon
        .command()
        .args([
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
#[ignore = "requires: cru binary"]
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
#[ignore = "requires: cru binary — starts an in-process OpenAI-compatible SSE mock"]
fn session_send_completes_after_an_unavailable_tool_call() {
    let daemon = TestDaemon::start();
    let mock = start_openai_compat_delegate_tool_server();

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
            &mock.endpoint,
        ])
        .assert()
        .success();

    let output = daemon
        .command()
        .args([
            "session",
            "send",
            &session_id,
            "please delegate this task",
            "--raw",
            "--permissions",
            "allow",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains("message_complete") && text.contains("done"),
        "the turn must complete after the unavailable tool call: {text}"
    );
    let requests: Vec<_> = mock.requests.try_iter().collect();
    assert_eq!(requests.len(), 2, "{requests:?}");
    let messages = requests[1]["messages"].as_array().expect("conversation");
    assert!(
        messages.iter().any(|message| {
            message["role"] == "tool"
                && message["tool_call_id"] == "call_delegate_1"
                && message["content"]
                    .as_str()
                    .is_some_and(|content| content.starts_with("Error:"))
        }),
        "the model must receive the correlated tool error: {messages:?}"
    );
}
