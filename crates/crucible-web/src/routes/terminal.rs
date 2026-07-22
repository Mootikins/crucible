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
use tokio::sync::{mpsc, Semaphore};
use tracing::{debug, warn};

/// Cap on concurrent PTY sessions. Each session spawns a shell child plus a
/// dedicated blocking OS thread, so an unbounded count is a fork-bomb / thread
/// exhaustion vector. Excess upgrades are rejected rather than queued.
const MAX_TERMINALS: usize = 8;
static TERMINAL_SLOTS: Semaphore = Semaphore::const_new(MAX_TERMINALS);

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
        handle_terminal(socket).await;
        drop(permit);
    })
}

async fn handle_terminal(mut socket: WebSocket) {
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

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let mut cmd = CommandBuilder::new(&shell);
    cmd.env("TERM", "xterm-256color");
    // xterm.js speaks 24-bit color, but Starship/Powerlevel10k-style prompts
    // gate truecolor on COLORTERM and silently downgrade to 256-color
    // approximations without it.
    cmd.env("COLORTERM", "truecolor");
    if let Some(home) = dirs::home_dir() {
        cmd.cwd(home);
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
        }
    }

    // Kill the whole process group, not just the shell: a PTY spawn makes the
    // shell a session leader (pgid == pid), and backgrounded grandchildren can
    // keep the slave open. If only the shell is killed, the blocking reader
    // thread never sees EOF on the master and leaks (thread + fd) for the
    // grandchild's lifetime.
    #[cfg(unix)]
    if let Some(pid) = child.process_id() {
        use nix::sys::signal::{killpg, Signal};
        use nix::unistd::Pid;
        let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}
