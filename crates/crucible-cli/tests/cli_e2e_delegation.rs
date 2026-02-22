//! CLI binary-level E2E coverage for delegation-related behavior.
//!
//! These tests focus on the `cru` executable surface area (help text, exit behavior,
//! and daemon-backed command wiring) rather than daemon-internal delegation logic.

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as StdCommand, Stdio};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

fn cru() -> Command {
    Command::cargo_bin("cru").unwrap()
}

fn write_test_config(dir: &Path) -> PathBuf {
    let kiln_path = dir.join("kiln");
    fs::create_dir_all(&kiln_path).unwrap();

    let config_path = dir.join("config.toml");
    fs::write(
        &config_path,
        format!(
            "kiln_path = \"{}\"\n",
            kiln_path.to_string_lossy().replace('\\', "\\\\")
        ),
    )
    .unwrap();

    config_path
}

fn start_daemon(socket_path: &Path) -> Child {
    let daemon_exe = std::env::var("CARGO_BIN_EXE_cru-server").unwrap_or_else(|_| {
        let current_exe = std::env::current_exe().expect("current_exe should be available");
        let deps_dir = current_exe
            .parent()
            .expect("test binary should have parent dir");
        let direct = deps_dir.join("cru-server");
        if direct.exists() {
            return direct.to_string_lossy().to_string();
        }

        deps_dir
            .join("..")
            .join("cru-server")
            .canonicalize()
            .expect("cru-server binary should exist in target/debug")
            .to_string_lossy()
            .to_string()
    });
    let mut daemon = StdCommand::new(daemon_exe)
        .env("CRUCIBLE_SOCKET", socket_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    for _ in 0..50 {
        if socket_path.exists() {
            return daemon;
        }
        thread::sleep(Duration::from_millis(100));
    }

    let _ = daemon.kill();
    let _ = daemon.wait();
    panic!("daemon failed to start within timeout");
}

fn create_session(config_path: &Path, socket_path: &Path) -> String {
    let output = cru()
        .env("CRUCIBLE_SOCKET", socket_path)
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "session",
            "create",
            "--session-type",
            "chat",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "session create failed: {output:?}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("Created session: "))
        .map(ToOwned::to_owned)
        .expect("session id should be present in create output")
}

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
    let temp = TempDir::new().unwrap();
    let config_path = write_test_config(temp.path());
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
    let temp = TempDir::new().unwrap();
    let config_path = write_test_config(temp.path());
    let socket_path = temp.path().join("daemon.sock");
    let mut daemon = start_daemon(&socket_path);

    let session_id = create_session(&config_path, &socket_path);

    cru()
        .env("CRUCIBLE_SOCKET", &socket_path)
        .args([
            "--config",
            config_path.to_str().unwrap(),
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

    let _ = daemon.kill();
    let _ = daemon.wait();
}

#[test]
#[ignore = "requires daemon and mock OpenAI-compatible SSE server"]
fn session_send_surfaces_delegation_disabled_error() {
    let temp = TempDir::new().unwrap();
    let config_path = write_test_config(temp.path());
    let socket_path = temp.path().join("daemon.sock");
    let mut daemon = start_daemon(&socket_path);
    let (endpoint, server_handle) = start_openai_compat_delegate_tool_server();

    let session_id = create_session(&config_path, &socket_path);

    cru()
        .env("CRUCIBLE_SOCKET", &socket_path)
        .args([
            "--config",
            config_path.to_str().unwrap(),
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

    cru()
        .env("CRUCIBLE_SOCKET", &socket_path)
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "session",
            "send",
            &session_id,
            "please delegate this task",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("delegation").and(predicate::str::contains("disabled")));

    let _ = daemon.kill();
    let _ = daemon.wait();
    let _ = server_handle.join();
}
