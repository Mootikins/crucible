use crate::services::daemon::AppState;
use crate::{WebError, error::WebResultExt};
use axum::{
    Json, Router,
    extract::State,
    response::sse::{Event, Sse},
    routing::post,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::ReceiverStream};

const DEFAULT_TIMEOUT_SECS: u64 = 30;

pub fn shell_routes() -> Router<AppState> {
    Router::new().route("/exec", post(shell_exec))
}

#[derive(Debug, Deserialize)]
struct ShellExecRequest {
    command: String,
    timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ShellEvent {
    Stdout { data: String },
    Stderr { data: String },
    Exit { code: i32 },
    Error { message: String },
}

impl ShellEvent {
    fn event_name(&self) -> &'static str {
        match self {
            ShellEvent::Stdout { .. } => "stdout",
            ShellEvent::Stderr { .. } => "stderr",
            ShellEvent::Exit { .. } => "exit",
            ShellEvent::Error { .. } => "error",
        }
    }
}

async fn shell_exec(
    State(state): State<AppState>,
    Json(req): Json<ShellExecRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, WebError> {
    let command = req.command.trim();
    if command.is_empty() {
        return Err(WebError::Validation("Command cannot be empty".to_string()));
    }

    let timeout_secs = req.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
    if timeout_secs == 0 {
        return Err(WebError::Validation(
            "timeout_secs must be greater than 0".to_string(),
        ));
    }

    let daemon_caps = state.daemon.capabilities().await.daemon_err()?;
    let has_shell_exec_rpc = daemon_caps
        .methods
        .iter()
        .any(|method| method == "shell.exec");

    let (tx, rx) = mpsc::channel::<ShellEvent>(128);
    let command = command.to_string();

    tokio::spawn(async move {
        if has_shell_exec_rpc {
            let _ = tx
                .send(ShellEvent::Error {
                    message: "Daemon shell.exec RPC is not wired in web yet".to_string(),
                })
                .await;
            return;
        }

        let _ = run_local_shell_command(command, Duration::from_secs(timeout_secs), tx).await;
    });

    let stream = ReceiverStream::new(rx).map(|shell_event| {
        let event_name = shell_event.event_name();
        let data = serde_json::to_string(&shell_event).unwrap_or_else(|_| {
            "{\"type\":\"error\",\"message\":\"failed to serialize shell event\"}".to_string()
        });
        Ok(Event::default().event(event_name).data(data))
    });

    Ok(Sse::new(stream))
}

async fn run_local_shell_command(
    command: String,
    timeout: Duration,
    tx: mpsc::Sender<ShellEvent>,
) -> Result<(), WebError> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| WebError::Internal(format!("Failed to start command: {e}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| WebError::Internal("Failed to capture stdout".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| WebError::Internal("Failed to capture stderr".to_string()))?;

    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut stderr_lines = BufReader::new(stderr).lines();
    let mut stdout_done = false;
    let mut stderr_done = false;

    let timeout_at = tokio::time::Instant::now() + timeout;
    let timeout_sleep = tokio::time::sleep_until(timeout_at);
    tokio::pin!(timeout_sleep);

    while !stdout_done || !stderr_done {
        tokio::select! {
            _ = tx.closed() => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Ok(());
            }
            _ = &mut timeout_sleep => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                let _ = tx.send(ShellEvent::Error {
                    message: format!("Command timed out after {} seconds", timeout.as_secs()),
                }).await;
                return Ok(());
            }
            line = stdout_lines.next_line(), if !stdout_done => {
                match line {
                    Ok(Some(data)) => {
                        if tx.send(ShellEvent::Stdout { data }).await.is_err() {
                            let _ = child.kill().await;
                            let _ = child.wait().await;
                            return Ok(());
                        }
                    }
                    Ok(None) => {
                        stdout_done = true;
                    }
                    Err(err) => {
                        let _ = tx.send(ShellEvent::Error {
                            message: format!("Failed to read stdout: {err}"),
                        }).await;
                        stdout_done = true;
                    }
                }
            }
            line = stderr_lines.next_line(), if !stderr_done => {
                match line {
                    Ok(Some(data)) => {
                        if tx.send(ShellEvent::Stderr { data }).await.is_err() {
                            let _ = child.kill().await;
                            let _ = child.wait().await;
                            return Ok(());
                        }
                    }
                    Ok(None) => {
                        stderr_done = true;
                    }
                    Err(err) => {
                        let _ = tx.send(ShellEvent::Error {
                            message: format!("Failed to read stderr: {err}"),
                        }).await;
                        stderr_done = true;
                    }
                }
            }
        }
    }

    let exit_status = tokio::select! {
        _ = tx.closed() => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Ok(());
        }
        _ = &mut timeout_sleep => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            let _ = tx.send(ShellEvent::Error {
                message: format!("Command timed out after {} seconds", timeout.as_secs()),
            }).await;
            return Ok(());
        }
        status = child.wait() => {
            status.map_err(|e| WebError::Internal(format!("Failed waiting for command: {e}")))?
        }
    };

    let code = exit_status.code().unwrap_or(-1);
    let _ = tx.send(ShellEvent::Exit { code }).await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    async fn collect_events(mut rx: mpsc::Receiver<ShellEvent>) -> Vec<ShellEvent> {
        let mut events = Vec::new();
        while let Some(event) = rx.recv().await {
            events.push(event);
        }
        events
    }

    #[tokio::test]
    async fn echo_hello_emits_stdout_then_exit_zero() {
        let (tx, rx) = mpsc::channel(32);
        run_local_shell_command("echo hello".to_string(), Duration::from_secs(5), tx)
            .await
            .unwrap();

        let events = collect_events(rx).await;
        assert!(matches!(
            events.first(),
            Some(ShellEvent::Stdout { data }) if data.trim() == "hello"
        ));
        assert!(matches!(events.last(), Some(ShellEvent::Exit { code: 0 })));
    }

    #[tokio::test]
    async fn false_command_emits_non_zero_exit() {
        let (tx, rx) = mpsc::channel(32);
        run_local_shell_command("false".to_string(), Duration::from_secs(5), tx)
            .await
            .unwrap();

        let events = collect_events(rx).await;
        assert!(events
            .iter()
            .any(|event| matches!(event, ShellEvent::Exit { code: 1 })));
    }

    #[tokio::test]
    async fn stderr_output_emits_stderr_event() {
        let (tx, rx) = mpsc::channel(32);
        run_local_shell_command("echo err >&2".to_string(), Duration::from_secs(5), tx)
            .await
            .unwrap();

        let events = collect_events(rx).await;
        assert!(events.iter().any(
            |event| matches!(event, ShellEvent::Stderr { data } if data.trim() == "err")
        ));
        assert!(events
            .iter()
            .any(|event| matches!(event, ShellEvent::Exit { code: 0 })));
    }

    #[tokio::test]
    async fn sleep_timeout_emits_timeout_error_quickly() {
        let (tx, rx) = mpsc::channel(32);
        let start = tokio::time::Instant::now();

        run_local_shell_command("sleep 10".to_string(), Duration::from_secs(1), tx)
            .await
            .unwrap();

        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_secs(3));

        let events = collect_events(rx).await;
        assert!(events.iter().any(
            |event| matches!(event, ShellEvent::Error { message } if message.contains("timed out"))
        ));
    }

    #[tokio::test]
    async fn shell_event_error_serializes_validation_message_shape() {
        let event = ShellEvent::Error {
            message: "Command cannot be empty".to_string(),
        };

        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["type"], "error");
        assert_eq!(value["message"], "Command cannot be empty");
    }

    #[tokio::test]
    async fn stdout_event_name_is_stdout() {
        let event = ShellEvent::Stdout {
            data: "hi".to_string(),
        };
        assert_eq!(event.event_name(), "stdout");
    }

    #[tokio::test]
    async fn stderr_event_name_is_stderr() {
        let event = ShellEvent::Stderr {
            data: "e".to_string(),
        };
        assert_eq!(event.event_name(), "stderr");
    }

    #[tokio::test]
    async fn exit_event_name_is_exit() {
        let event = ShellEvent::Exit { code: 0 };
        assert_eq!(event.event_name(), "exit");
    }

    #[tokio::test]
    async fn error_event_name_is_error() {
        let event = ShellEvent::Error {
            message: "m".to_string(),
        };
        assert_eq!(event.event_name(), "error");
    }

    #[tokio::test]
    async fn cat_dev_null_exits_zero_without_stdout() {
        let (tx, rx) = mpsc::channel(32);
        run_local_shell_command("cat /dev/null".to_string(), Duration::from_secs(5), tx)
            .await
            .unwrap();

        let events = collect_events(rx).await;
        assert!(!events
            .iter()
            .any(|event| matches!(event, ShellEvent::Stdout { .. })));
        assert!(events
            .iter()
            .any(|event| matches!(event, ShellEvent::Exit { code: 0 })));
    }

    #[tokio::test]
    async fn multiline_echo_emits_two_stdout_events_and_exit() {
        let (tx, rx) = mpsc::channel(32);
        run_local_shell_command(
            "echo line1; echo line2".to_string(),
            Duration::from_secs(5),
            tx,
        )
        .await
        .unwrap();

        let events = collect_events(rx).await;
        let stdout_lines: Vec<String> = events
            .iter()
            .filter_map(|event| {
                if let ShellEvent::Stdout { data } = event {
                    Some(data.clone())
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(stdout_lines.len(), 2);
        assert_eq!(stdout_lines[0].trim(), "line1");
        assert_eq!(stdout_lines[1].trim(), "line2");
        assert!(events
            .iter()
            .any(|event| matches!(event, ShellEvent::Exit { code: 0 })));
    }
}
