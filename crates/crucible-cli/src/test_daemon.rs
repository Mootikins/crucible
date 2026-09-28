//! A fake daemon for client tests.
//!
//! It listens on a Unix socket in a temporary directory, records each
//! JSON-RPC request, and answers it through a closure. A test gets a real
//! `DaemonClient` connected to it, so the code under test makes its real
//! calls, and the test asserts on the methods and params that reached the
//! socket.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

use crate::session::LiveSession;

/// The answer to one request: a result, or an error message.
pub(crate) type Answer = Result<Value, String>;

pub(crate) struct FakeDaemon {
    /// A session on this daemon.
    pub(crate) session: LiveSession,
    calls: Arc<Mutex<Vec<(String, Value)>>>,
    _dir: tempfile::TempDir,
    server: tokio::task::JoinHandle<()>,
}

impl FakeDaemon {
    /// Start a fake daemon with a session named `session_id`. `answer`
    /// gives the reply to each method and params.
    pub(crate) async fn start(
        session_id: &str,
        answer: impl Fn(&str, &Value) -> Answer + Send + Sync + 'static,
    ) -> Self {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let socket = dir.path().join("daemon.sock");
        let listener = UnixListener::bind(&socket).expect("bind the fake daemon socket");
        let calls: Arc<Mutex<Vec<(String, Value)>>> = Arc::default();
        let server = tokio::spawn(serve(listener, calls.clone(), Arc::new(answer)));
        let client = crucible_daemon::DaemonClient::connect_to(&socket)
            .await
            .expect("connect to the fake daemon");
        Self {
            session: LiveSession {
                client: Arc::new(client),
                id: session_id.to_string(),
            },
            calls,
            _dir: dir,
            server,
        }
    }

    /// Start a fake daemon that answers `null` to every method.
    pub(crate) async fn answering_null(session_id: &str) -> Self {
        Self::start(session_id, |_, _| Ok(Value::Null)).await
    }

    /// The methods that reached the daemon, in order.
    pub(crate) fn methods(&self) -> Vec<String> {
        self.calls().into_iter().map(|(method, _)| method).collect()
    }

    /// The methods and params that reached the daemon, in order.
    pub(crate) fn calls(&self) -> Vec<(String, Value)> {
        self.calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Drop for FakeDaemon {
    fn drop(&mut self) {
        self.server.abort();
    }
}

type AnswerFn = dyn Fn(&str, &Value) -> Answer + Send + Sync;

async fn serve(
    listener: UnixListener,
    calls: Arc<Mutex<Vec<(String, Value)>>>,
    answer: Arc<AnswerFn>,
) {
    while let Ok((stream, _)) = listener.accept().await {
        let calls = calls.clone();
        let answer = answer.clone();
        tokio::spawn(async move {
            let (read, mut write) = stream.into_split();
            let mut lines = BufReader::new(read).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(request) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                let method = request["method"].as_str().unwrap_or_default().to_string();
                let params = request["params"].clone();
                calls
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push((method.clone(), params.clone()));
                let reply = match answer(&method, &params) {
                    Ok(result) => json!({"jsonrpc": "2.0", "id": request["id"], "result": result}),
                    Err(message) => json!({"jsonrpc": "2.0", "id": request["id"],
                        "error": {"code": -32000, "message": message}}),
                };
                if write
                    .write_all(format!("{reply}\n").as_bytes())
                    .await
                    .is_err()
                {
                    return;
                }
            }
        });
    }
}
