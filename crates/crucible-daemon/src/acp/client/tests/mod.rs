use std::path::PathBuf;

use super::types::ClientConfig;
use super::CrucibleAcpClient;

mod connection;
mod creation;
mod io;
mod process_streaming;
mod protocol;
mod sdk_spike;
mod streaming;

/// Cross-platform test path helper
pub(super) fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("crucible_test_{}", name))
}

// Helper to get a simple command that runs and exits (like true/echo)
pub(super) fn get_simple_command() -> (PathBuf, Option<Vec<String>>) {
    #[cfg(windows)]
    {
        (
            PathBuf::from("cmd"),
            Some(vec!["/C".to_string(), "echo".to_string(), "ok".to_string()]),
        )
    }
    #[cfg(not(windows))]
    {
        (PathBuf::from("echo"), Some(vec!["ok".to_string()]))
    }
}

// Helper to get a command that echoes stdin to stdout (like cat)
pub(super) fn get_cat_command() -> (PathBuf, Option<Vec<String>>) {
    #[cfg(windows)]
    {
        // findstr can hang waiting for EOF. Use cmd hack to read one line and echo it.
        // This works for tests sending single messages.
        (
            PathBuf::from("cmd"),
            Some(vec![
                "/V".to_string(),
                "/C".to_string(),
                "set /p l= && echo !l!".to_string(),
            ]),
        )
    }
    #[cfg(not(windows))]
    {
        (PathBuf::from("cat"), None)
    }
}

/// A client wired to an in-process agent that answers each request with the
/// next result in `results`, echoing the request's `id`.
///
/// The task returns every frame the agent read, with `id` removed because a
/// process-wide counter assigns it. The agent reads exactly one frame per
/// result, so a client that sends fewer requests leaves the task waiting and
/// a test that awaits it fails on the missing frame.
pub(super) fn scripted_client(
    results: Vec<serde_json::Value>,
) -> (
    CrucibleAcpClient,
    tokio::task::JoinHandle<Vec<serde_json::Value>>,
) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let (client_end, agent_end) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = tokio::io::split(client_end);
    let (agent_read, mut agent_write) = tokio::io::split(agent_end);

    let client = CrucibleAcpClient::with_transport(
        ClientConfig::default(),
        Box::pin(client_write),
        Box::pin(BufReader::new(client_read)),
    );

    let agent = tokio::spawn(async move {
        let mut lines = BufReader::new(agent_read).lines();
        let mut frames = Vec::with_capacity(results.len());
        for result in results {
            let line = lines
                .next_line()
                .await
                .expect("the agent reads a line")
                .expect("the client sends a frame");
            let mut frame: serde_json::Value =
                serde_json::from_str(&line).expect("the frame is JSON");
            let id = frame
                .as_object_mut()
                .expect("the frame is an object")
                .remove("id")
                .expect("the frame is a request");
            let reply = serde_json::json!({"jsonrpc": "2.0", "id": id, "result": result});
            agent_write
                .write_all(format!("{reply}\n").as_bytes())
                .await
                .expect("the reply writes");
            frames.push(frame);
        }
        frames
    });

    (client, agent)
}
