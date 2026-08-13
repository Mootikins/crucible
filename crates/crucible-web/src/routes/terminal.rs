//! Real PTY terminal over WebSocket for the web UI's xterm.js panel.
//!
//! One WebSocket = one shell in a PTY. Client→server messages are JSON
//! text frames: `{"t":"i","d":"<utf8 input>"}` for keystrokes and
//! `{"t":"r","cols":N,"rows":N}` for resizes. Server→client messages are
//! binary frames of raw PTY output (xterm.js writes bytes, preserving
//! ANSI). The child is killed when the socket closes.

use crate::services::daemon::AppState;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Deserialize;
use std::time::Duration;
use tokio::sync::{mpsc, Semaphore};
use tracing::{debug, warn};

/// Cap on concurrent PTY sessions. Each session spawns a shell child plus a
/// dedicated blocking OS thread, so an unbounded count is a fork-bomb / thread
/// exhaustion vector. Excess upgrades are rejected rather than queued.
const MAX_TERMINALS: usize = 8;
static TERMINAL_SLOTS: Semaphore = Semaphore::const_new(MAX_TERMINALS);

/// How often the server pings an idle peer, and how long silence is tolerated.
///
/// Without this a peer that vanishes WITHOUT a close frame — lid shut, Wi-Fi
/// dropped, NAT idle-timeout — parks `socket.recv()` forever. Nothing else in
/// the loop makes noise either, because an idle shell writes nothing. The task
/// then holds its permit for the process's lifetime, and eight of those make
/// every later terminal a permanent 503 plus eight orphan shells. Observed on a
/// developer box: one permit held by a five-hour-old `zsh` with a half-open
/// connection.
///
/// Any inbound frame counts as liveness — a Pong, a keystroke, a resize —
/// because tungstenite answers inbound Pings itself and the point is only to
/// learn that the far end is still there.
const KEEPALIVE: KeepAlive = KeepAlive {
    ping: Duration::from_secs(20),
    idle: Duration::from_secs(65),
    shell: None,
};

/// Per-session knobs, injectable so the regression test neither sleeps for a
/// minute nor depends on the developer's `$SHELL`. Test timings are
/// milliseconds; `shell: None` means read the environment, as production does.
#[derive(Clone)]
pub(crate) struct KeepAlive {
    ping: Duration,
    idle: Duration,
    shell: Option<String>,
}

pub fn terminal_routes() -> Router<AppState> {
    Router::new().route("/ws", get(terminal_ws))
}

#[derive(Debug, Deserialize)]
#[serde(tag = "t")]
enum ClientMsg {
    #[serde(rename = "i")]
    Input { d: String },
    #[serde(rename = "r")]
    Resize { cols: u16, rows: u16 },
}

async fn terminal_ws(ws: WebSocketUpgrade) -> impl IntoResponse {
    // Bound concurrent PTYs; hold the permit for the connection's lifetime.
    let permit = match TERMINAL_SLOTS.try_acquire() {
        Ok(permit) => permit,
        Err(_) => {
            warn!(
                max = MAX_TERMINALS,
                "Rejecting terminal: session limit reached"
            );
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Terminal session limit reached",
            )
                .into_response();
        }
    };
    ws.on_upgrade(move |socket| async move {
        handle_terminal(socket, KEEPALIVE).await;
        drop(permit);
    })
}

async fn handle_terminal(mut socket: WebSocket, keepalive: KeepAlive) {
    let pty_system = native_pty_system();
    let pair = match pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, "Failed to open PTY");
            let _ = socket
                .send(Message::Text(
                    format!("\r\nFailed to open PTY: {e}\r\n").into(),
                ))
                .await;
            return;
        }
    };

    let shell = keepalive
        .shell
        .clone()
        .or_else(|| std::env::var("SHELL").ok())
        .unwrap_or_else(|| "/bin/sh".to_string());
    let mut cmd = CommandBuilder::new(&shell);
    cmd.env("TERM", "xterm-256color");
    // xterm.js speaks 24-bit color, but Starship/Powerlevel10k-style prompts
    // gate truecolor on COLORTERM and silently downgrade to 256-color
    // approximations without it.
    cmd.env("COLORTERM", "truecolor");
    // Start where the server was launched (the project you're working in),
    // not $HOME — until terminals are session-aware, the launch directory is
    // the best guess at "where the user's work is".
    match std::env::current_dir() {
        Ok(cwd) => cmd.cwd(cwd),
        Err(_) => {
            if let Some(home) = dirs::home_dir() {
                cmd.cwd(home);
            }
        }
    }

    let mut child = match pair.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, shell = %shell, "Failed to spawn shell in PTY");
            let _ = socket
                .send(Message::Text(
                    format!("\r\nFailed to spawn {shell}: {e}\r\n").into(),
                ))
                .await;
            return;
        }
    };
    // The slave stays open in the child; drop our copy so reads see EOF
    // when the child exits.
    drop(pair.slave);

    let mut reader = match pair.master.try_clone_reader() {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "Failed to clone PTY reader");
            let _ = child.kill();
            return;
        }
    };
    let mut writer = match pair.master.take_writer() {
        Ok(w) => w,
        Err(e) => {
            warn!(error = %e, "Failed to take PTY writer");
            let _ = child.kill();
            return;
        }
    };

    // PTY reads are blocking — bridge through a channel from a blocking
    // thread. The thread ends when the PTY hits EOF (child exited) or the
    // receiver is dropped (socket closed).
    let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(64);
    std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if out_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    // An interval arm rather than wrapping `socket.recv()` in a timeout:
    // `WebSocket::recv` takes `&mut self`, so dropping it on every expiry would
    // cancel a partially-read multi-frame message. This never touches the recv
    // future. `Delay` on missed ticks so a burst of PTY output cannot queue a
    // backlog of pings to fire back-to-back.
    let mut keepalive_tick = tokio::time::interval(keepalive.ping);
    keepalive_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_seen = tokio::time::Instant::now();

    loop {
        tokio::select! {
            chunk = out_rx.recv() => {
                match chunk {
                    Some(bytes) => {
                        if socket.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                    // PTY EOF: shell exited.
                    None => {
                        let _ = socket
                            .send(Message::Text("\r\n[process exited]\r\n".into()))
                            .await;
                        break;
                    }
                }
            }
            msg = socket.recv() => {
                // Any inbound frame is proof of life, Pongs included — they
                // fall through the catch-all below and need no arm of their own.
                last_seen = tokio::time::Instant::now();
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientMsg>(&text) {
                            Ok(ClientMsg::Input { d }) => {
                                use std::io::Write;
                                if writer.write_all(d.as_bytes()).is_err() {
                                    break;
                                }
                            }
                            Ok(ClientMsg::Resize { cols, rows }) => {
                                let _ = pair.master.resize(PtySize {
                                    rows,
                                    cols,
                                    pixel_width: 0,
                                    pixel_height: 0,
                                });
                            }
                            Err(e) => debug!(error = %e, "Ignoring malformed terminal message"),
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            _ = keepalive_tick.tick() => {
                if last_seen.elapsed() > keepalive.idle {
                    warn!(
                        idle_secs = keepalive.idle.as_secs(),
                        "Terminal peer went silent past the keepalive window; closing to release the slot"
                    );
                    break;
                }
                // A send error means the socket is already gone, which the recv
                // arm may never learn on its own.
                if socket.send(Message::Ping(axum::body::Bytes::new())).await.is_err() {
                    break;
                }
            }
        }
    }

    // Kill the whole process group, not just the shell: a PTY spawn makes the
    // shell a session leader (pgid == pid), and backgrounded grandchildren can
    // keep the slave open. If only the shell is killed, the blocking reader
    // thread never sees EOF on the master and leaks (thread + fd) for the
    // grandchild's lifetime.
    //
    // On a blocking thread, and AWAITED rather than detached. `portable_pty`'s
    // `Child::wait` blocks — "Blocks execution until the child process has
    // completed" — so on a tokio worker a process wedged in uninterruptible
    // sleep parks that worker. And the await matters as much as the offload:
    // the caller drops this connection's permit the moment `handle_terminal`
    // returns, so returning before the shell is reaped would free a slot while
    // its process still lived, which is the exact accounting bug the cap exists
    // to prevent.
    let _ = tokio::task::spawn_blocking(move || {
        #[cfg(unix)]
        if let Some(pid) = child.process_id() {
            use nix::sys::signal::{killpg, Signal};
            use nix::unistd::Pid;
            let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
        }
        let _ = child.kill();
        let _ = child.wait();
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use futures::StreamExt;
    use std::time::Duration;
    use tokio::net::TcpListener;

    /// Fast timings and `/bin/sh`, so the test neither waits a minute nor depends
    /// on the developer's `$SHELL` (which it must not mutate).
    fn brisk() -> KeepAlive {
        KeepAlive {
            ping: Duration::from_millis(40),
            idle: Duration::from_millis(200),
            shell: Some("/bin/sh".to_string()),
        }
    }

    /// A terminal route on an ephemeral loopback port, bypassing auth: what is
    /// under test is the session loop, not the middleware around it.
    async fn serve_terminal(keepalive: KeepAlive) -> String {
        let app = axum::Router::new().route(
            "/ws",
            get(move |ws: WebSocketUpgrade| {
                let keepalive = keepalive.clone();
                async move {
                    // The permit is acquired exactly as the real route does, so
                    // the accounting under test is the accounting that ships.
                    let permit = TERMINAL_SLOTS.try_acquire().expect("a free terminal slot");
                    ws.on_upgrade(move |socket| async move {
                        handle_terminal(socket, keepalive).await;
                        drop(permit);
                    })
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        format!("ws://{addr}/ws")
    }

    /// Wait for the slot count to come back, so a pass is not a race.
    async fn wait_for_all_slots(within: Duration) -> usize {
        let deadline = tokio::time::Instant::now() + within;
        loop {
            let free = TERMINAL_SLOTS.available_permits();
            if free == MAX_TERMINALS || tokio::time::Instant::now() > deadline {
                return free;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn a_peer_that_goes_silent_releases_its_terminal_slot() {
        // The leak: a peer that vanishes WITHOUT a close frame parks
        // `socket.recv()` forever, and an idle shell writes nothing, so the task
        // held its permit for the process's lifetime. Eight of those made every
        // later terminal a permanent 503.
        //
        // `tokio-tungstenite` answers server Pings automatically, which is
        // exactly the liveness we must NOT have here — so the client is dropped
        // outright, leaving the server with a connection nothing will ever
        // answer on.
        let url = serve_terminal(brisk()).await;
        let before = TERMINAL_SLOTS.available_permits();

        let (stream, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("websocket upgrade");
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert!(
            TERMINAL_SLOTS.available_permits() < before,
            "the session should hold a slot while connected"
        );

        // Drop without a close frame: the far end simply stops existing.
        std::mem::forget(stream);

        let free = wait_for_all_slots(Duration::from_secs(5)).await;
        assert_eq!(
            free, MAX_TERMINALS,
            "the idle timeout must release the slot; before this it was held forever"
        );
    }

    #[tokio::test]
    async fn a_responsive_peer_is_left_alone() {
        // The other half: the timeout must not evict a live-but-quiet session.
        // Someone reading output without typing is silent at the application
        // level, and killing their shell would be worse than the leak.
        let url = serve_terminal(brisk()).await;
        let (mut stream, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("websocket upgrade");

        // Well past the idle window, answering pings (tungstenite does this for
        // us as the stream is polled) but sending nothing of our own.
        let deadline = tokio::time::Instant::now() + Duration::from_millis(700);
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(50), stream.next()).await {
                Ok(Some(Ok(_))) | Err(_) => {}
                // A close or an error here is the failure this test exists for.
                Ok(other) => panic!("session dropped a responsive peer: {other:?}"),
            }
        }

        let _ = stream.close(None).await;
        assert_eq!(
            wait_for_all_slots(Duration::from_secs(5)).await,
            MAX_TERMINALS,
            "a clean close must also return the slot"
        );
    }
}
