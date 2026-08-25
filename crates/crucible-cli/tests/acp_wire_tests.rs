//! Pins the raw JSON that `cru acp` writes on stdout for the ACP handshake.
//!
//! The ACP SDK changes its Rust API between majors, but the v1 wire format
//! stays fixed. This test reads the bytes a host sees, so an SDK upgrade
//! cannot change the wire without a visible diff here.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

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
