//! Common test utilities for daemon E2E tests

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::sleep;

/// A JSON-RPC connection to a test daemon.
///
/// **The read buffer belongs to the connection, not to a single call.** That is
/// the entire reason this type exists rather than a free `rpc_call(&mut stream)`
/// function.
///
/// The daemon multiplexes replies and server-pushed notifications down one
/// socket, and one `read()` routinely returns a reply followed by the head of
/// the next message. A helper that allocates its buffer per call answers the
/// reply and then drops those trailing bytes on the floor — so the *following*
/// call starts reading from the middle of a message, finds that message's
/// newline, and tries to parse a fragment.
///
/// That is not hypothetical. `plugin.reload` pushes a `ui_style_changed`
/// notification of roughly 2KB, comfortably more than the 1KB read chunk. On a
/// contended machine it coalesces with the preceding reply, and the next call
/// parsed a fragment beginning mid-string:
///
/// ```text
/// Failed to parse JSON line: Error("expected value", line: 1, column: 1)
/// ```
///
/// which is what took CI red on run 30315863433 — both attempts, since a retry
/// re-runs the same racy sequence. Reproducing it needs contention rather than
/// repetition: 64 sequential runs on an idle box were clean, while 12 copies
/// pinned to 2 CPUs (`taskset -c 0,1`, approximating a GitHub runner) failed 54
/// times out of 72.
pub struct RpcConn {
    stream: UnixStream,
    /// Bytes read past the last returned message. Carrying these across calls
    /// is the fix; see the type docs.
    buffered: Vec<u8>,
}

impl RpcConn {
    /// Connect to a daemon's socket.
    pub async fn connect(socket_path: &Path) -> Result<Self> {
        Ok(Self {
            stream: UnixStream::connect(socket_path).await.with_context(|| {
                format!("connecting to daemon socket at {}", socket_path.display())
            })?,
            buffered: Vec::new(),
        })
    }

    /// Send a raw JSON-RPC request line and return **that request's** response.
    ///
    /// Skips server-pushed notifications, which is what a real client does:
    /// a reply is not necessarily the next line on the wire.
    ///
    /// It also matches the response's `id` against the request's, rather than
    /// taking the first reply on the wire. `&mut self` means only one request is
    /// ever outstanding here, so arrival order and request order agree — but the
    /// daemon serves requests concurrently now, and "there is only one in flight"
    /// is an invariant of this type, not of the wire. Checking it is what makes
    /// it an invariant rather than an assumption; a mismatch means a stale reply
    /// from an abandoned call is being handed to the wrong assertion.
    pub async fn call(&mut self, request: &str) -> serde_json::Value {
        // `None` for a request with no id — a notification, which has no reply
        // to wait for; such a call falls back to the first reply on the wire.
        let expected_id = serde_json::from_str::<serde_json::Value>(request)
            .ok()
            .and_then(|v| v.get("id").cloned());

        self.stream
            .write_all(format!("{}\n", request).as_bytes())
            .await
            .expect("Failed to write request");

        loop {
            while let Some(line_end) = self.buffered.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.buffered.drain(..=line_end).collect();
                let trimmed = &line[..line.len() - 1];
                if trimmed.is_empty() {
                    continue;
                }
                let value: serde_json::Value =
                    serde_json::from_slice(trimmed).unwrap_or_else(|e| {
                        // Print the offending bytes. The bare `expect` this
                        // replaces reported only "expected value at line 1
                        // column 1", which says nothing about *which* message
                        // was mangled and cost a full reproduction to diagnose.
                        panic!(
                            "Failed to parse JSON line ({e}); {} bytes: {:?}",
                            trimmed.len(),
                            String::from_utf8_lossy(trimmed)
                        )
                    });
                // No `id` means a notification (e.g. `ui_style_changed`).
                let Some(got_id) = value.get("id") else {
                    continue;
                };
                match &expected_id {
                    Some(want) if want != got_id => {
                        eprintln!(
                            "RpcConn: skipping a reply for id {got_id} while waiting for {want}"
                        );
                    }
                    _ => return value,
                }
            }

            let mut chunk = [0u8; 1024];
            let n = self
                .stream
                .read(&mut chunk)
                .await
                .expect("Failed to read response");
            assert!(n > 0, "Connection closed before full response");
            self.buffered.extend_from_slice(&chunk[..n]);
        }
    }

    /// Send a JSON-RPC request built from its parts.
    ///
    /// `common` is compiled into each integration binary separately, so a
    /// helper only one of them uses reads as dead code in the others.
    #[allow(dead_code)]
    pub async fn call_method(
        &mut self,
        method: &str,
        params: serde_json::Value,
        id: i64,
    ) -> serde_json::Value {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.call(&serde_json::to_string(&request).expect("request should serialize"))
            .await
    }
}

/// Test daemon fixture that manages an isolated daemon instance
pub struct TestDaemon {
    pub socket_path: PathBuf,
    process: Option<Child>,
    /// Held to keep temp directory alive for duration of test
    #[allow(dead_code)]
    temp_dir: tempfile::TempDir,
}

/// Block until the daemon at `socket_path` actually accepts a connection.
///
/// Waiting for the socket *file* to appear and then sleeping a fixed 50ms is
/// not the same test: `bind()` creates the path, and the daemon is not
/// listening until some time after that. On an idle box the sleep covers the
/// gap; on a loaded one it does not, and the arbitrary constant is doing all
/// the work. `rpc_session_e2e.rs` already replaced its own copy of that pattern
/// after the same intermittent failures — this is the remaining one, shared by
/// 17 tests across four files.
async fn wait_until_accepting(socket_path: &Path) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if socket_path.exists() && UnixStream::connect(socket_path).await.is_ok() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("Daemon did not start accepting connections within 10 seconds");
        }
        sleep(Duration::from_millis(20)).await;
    }
}

impl TestDaemon {
    /// Start a test daemon with isolated socket path
    ///
    /// This spawns a real daemon process in a temporary directory,
    /// ensuring test isolation.
    pub async fn start() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let socket_path = temp_dir.path().join("daemon.sock");

        // Single binary: daemon runs via `cru daemon serve`.
        // Cannot use env!("CARGO_BIN_EXE_cru") — crucible-daemon cannot depend on crucible-cli
        // (circular: crucible-cli -> crucible-daemon). Locate `cru` at runtime instead.
        let cru_exe = std::env::var("CARGO_BIN_EXE_cru").unwrap_or_else(|_| {
            let test_exe = std::env::current_exe().expect("current_exe");
            let target_dir = test_exe
                .parent() // deps/
                .and_then(|p| p.parent()) // debug/ or release/
                .expect("target dir");
            target_dir.join("cru").to_string_lossy().to_string()
        });
        // Spawn daemon with a HERMETIC environment: env_clear + allowlist so
        // the developer's real provider credentials (which could drive real
        // API calls) and real ~/.crucible state never reach the child.
        let mut cmd = Command::new(&cru_exe);
        cmd.env_clear();
        for (k, v) in crucible_core::test_support::hermetic_env_pairs(temp_dir.path()) {
            cmd.env(k, v);
        }
        let process = cmd
            .args(["daemon", "serve"])
            .env("CRUCIBLE_SOCKET", &socket_path)
            // Scope the config home to this test's TempDir (child-process env, no
            // global mutation) so the spawned daemon never reads the developer's
            // real ~/.crucible registry.
            .env("CRUCIBLE_HOME", temp_dir.path())
            // And the DATA home. The daemon extracts the compiled-in runtime
            // tree and help corpus to `<data_dir>/crucible/...` at startup;
            // without this every daemon-spawning test writes them into the
            // developer's real ~/.local/share, and a parallel run has many
            // processes writing one path at once.
            .env("XDG_DATA_HOME", temp_dir.path().join("data"))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        wait_until_accepting(&socket_path).await?;
        Ok(Self {
            socket_path,
            process: Some(process),
            temp_dir,
        })
    }

    /// Start a test daemon with additional environment variables.
    #[allow(dead_code)]
    pub async fn start_with_env(env_vars: Vec<(&str, &str)>) -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let socket_path = temp_dir.path().join("daemon.sock");

        let cru_exe = std::env::var("CARGO_BIN_EXE_cru").unwrap_or_else(|_| {
            let test_exe = std::env::current_exe().expect("current_exe");
            let target_dir = test_exe
                .parent()
                .and_then(|p| p.parent())
                .expect("target dir");
            target_dir.join("cru").to_string_lossy().to_string()
        });

        let mut cmd = Command::new(&cru_exe);
        // Hermetic base (cleared + allowlist) — see start(); caller-supplied
        // vars are applied on top and win.
        cmd.env_clear();
        for (k, v) in crucible_core::test_support::hermetic_env_pairs(temp_dir.path()) {
            cmd.env(k, v);
        }
        cmd.args(["daemon", "serve"])
            .env("CRUCIBLE_SOCKET", &socket_path)
            // Default the config home to this test's TempDir; a caller-supplied
            // CRUCIBLE_HOME in env_vars still overrides via the loop below.
            .env("CRUCIBLE_HOME", temp_dir.path())
            // Same for the data home, which is where the compiled-in runtime
            // tree and help corpus extract to. See `start()`.
            .env("XDG_DATA_HOME", temp_dir.path().join("data"))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        for (key, value) in &env_vars {
            cmd.env(key, value);
        }

        let process = cmd.spawn()?;

        wait_until_accepting(&socket_path).await?;
        Ok(Self {
            socket_path,
            process: Some(process),
            temp_dir,
        })
    }

    /// Manually stop the daemon process
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(mut p) = self.process.take() {
            p.kill()?;
            p.wait()?;
        }
        Ok(())
    }

    /// Check if the daemon process is still running
    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut p) = self.process {
            match p.try_wait() {
                Ok(Some(_)) => false, // Process has exited
                Ok(None) => true,     // Still running
                Err(_) => false,      // Error checking status
            }
        } else {
            false
        }
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        if let Some(mut p) = self.process.take() {
            let _ = p.kill();
            let _ = p.wait();
        }
    }
}

#[allow(unused_imports)]
pub use crucible_daemon::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_daemon_fixture_starts_and_stops() {
        let mut daemon = TestDaemon::start()
            .await
            .expect("Failed to start test daemon");

        // Verify socket exists
        assert!(daemon.socket_path.exists(), "Socket should exist");

        // Verify daemon is running
        assert!(daemon.is_running(), "Daemon should be running");

        // Stop daemon
        daemon.stop().await.expect("Failed to stop daemon");

        // Verify daemon stopped
        assert!(!daemon.is_running(), "Daemon should be stopped");
    }

    /// A reply that arrives in the same read as the head of a notification must
    /// not cost us the rest of that notification.
    ///
    /// Deterministic stand-in for the production race: the fake server answers
    /// the first request with the reply *and* a notification bigger than the
    /// client's 1KB read chunk, in a single write. The client's first `read()`
    /// therefore returns the whole reply plus a partial notification — exactly
    /// the interleaving a contended daemon produces.
    ///
    /// A per-call buffer answers request 1 and drops the notification's head,
    /// so request 2 reads from the middle of it, hits that message's newline,
    /// and parses a fragment: `expected value at line 1 column 1`.
    #[tokio::test]
    async fn reply_read_together_with_a_notification_does_not_corrupt_the_next_call() {
        let dir = tempfile::tempdir().expect("temp dir");
        let socket_path = dir.path().join("fake.sock");
        let listener = tokio::net::UnixListener::bind(&socket_path).expect("bind");

        // Comfortably larger than the 1024-byte read chunk, so the first read
        // is guaranteed to leave this message incomplete.
        let filler = "x".repeat(4000);
        let notification = format!(
            r#"{{"jsonrpc":"2.0","method":"ui_style_changed","params":{{"s":"{filler}"}}}}"#
        );

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut seen = Vec::new();
            let mut replies = 0;
            let mut chunk = [0u8; 1024];
            loop {
                let n = stream.read(&mut chunk).await.expect("server read");
                if n == 0 {
                    return;
                }
                seen.extend_from_slice(&chunk[..n]);
                while let Some(end) = seen.iter().position(|&b| b == b'\n') {
                    seen.drain(..=end);
                    replies += 1;
                    let out = if replies == 1 {
                        // Reply and notification in ONE write: this is the
                        // coalescing the test exists to pin down.
                        format!("{{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"first\"}}\n{notification}\n")
                    } else {
                        "{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":\"second\"}\n".to_string()
                    };
                    stream
                        .write_all(out.as_bytes())
                        .await
                        .expect("server write");
                    if replies == 2 {
                        return;
                    }
                }
            }
        });

        let mut conn = RpcConn::connect(&socket_path).await.expect("connect");

        let first = conn
            .call(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#)
            .await;
        assert_eq!(first["id"], 1, "first call should get its own reply");

        // Without the carried-over buffer this call parses a fragment of the
        // notification and panics before it ever sees id 2.
        let second = conn
            .call(r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#)
            .await;
        assert_eq!(
            second["id"], 2,
            "second call must skip the notification and return its own reply"
        );
        assert_eq!(second["result"], "second");

        server.await.expect("server task");
    }

    /// A reply left over from an abandoned call must not be handed to the next
    /// one. The daemon serves requests concurrently, so "the next reply is mine"
    /// is not something the wire guarantees any more.
    #[tokio::test]
    async fn a_stale_reply_is_skipped_rather_than_answered_to_the_wrong_call() {
        let dir = tempfile::tempdir().expect("temp dir");
        let socket_path = dir.path().join("fake.sock");
        let listener = tokio::net::UnixListener::bind(&socket_path).expect("bind");

        // Answers the first request twice: once with a stale id nobody is waiting
        // for, then with the real one. Deterministic — one write, one read.
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut chunk = [0u8; 1024];
            let n = stream.read(&mut chunk).await.expect("server read");
            assert!(n > 0);
            stream
                .write_all(
                    b"{\"jsonrpc\":\"2.0\",\"id\":99,\"result\":\"stale\"}\n\
                      {\"jsonrpc\":\"2.0\",\"id\":7,\"result\":\"mine\"}\n",
                )
                .await
                .expect("server write");
        });

        let mut conn = RpcConn::connect(&socket_path).await.expect("connect");
        let reply = conn
            .call(r#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#)
            .await;

        assert_eq!(reply["id"], 7, "must not answer with id 99's reply");
        assert_eq!(reply["result"], "mine");
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn test_daemon_fixture_cleanup_on_drop() {
        let daemon = TestDaemon::start()
            .await
            .expect("Failed to start test daemon");
        let _socket_path = daemon.socket_path.clone();

        // Drop daemon (should clean up)
        drop(daemon);

        // Give OS time to cleanup
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Socket might still exist briefly but daemon should be gone
        // This is best-effort cleanup test
    }
}
