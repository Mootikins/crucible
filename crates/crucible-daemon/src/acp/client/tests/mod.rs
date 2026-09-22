use agent_client_protocol::ByteStreams;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use super::types::ClientConfig;
use super::CrucibleAcpClient;

mod handshake;
mod permission_name;
mod streaming;

/// The agent end of an in-process pipe: raw lines, with no SDK.
pub(super) struct RawAgent {
    pub(super) lines: tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>,
    pub(super) write: tokio::io::WriteHalf<tokio::io::DuplexStream>,
}

impl RawAgent {
    pub(super) async fn read(&mut self) -> serde_json::Value {
        let line = self
            .lines
            .next_line()
            .await
            .expect("the agent reads a line")
            .expect("the client sends a frame");
        serde_json::from_str(&line).expect("the frame is JSON")
    }

    pub(super) async fn write(&mut self, frame: serde_json::Value) {
        self.write
            .write_all(format!("{frame}\n").as_bytes())
            .await
            .expect("the frame writes");
    }
}

/// A client connected to a [`RawAgent`] over an in-process pipe.
pub(super) async fn raw_client() -> (CrucibleAcpClient, RawAgent) {
    let (client_end, agent_end) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = tokio::io::split(client_end);
    let (agent_read, agent_write) = tokio::io::split(agent_end);
    let client = CrucibleAcpClient::connect(
        ClientConfig::default(),
        ByteStreams::new(client_write.compat_write(), client_read.compat()),
        "raw",
        None,
    )
    .await
    .expect("the client connects");
    let agent = RawAgent {
        lines: BufReader::new(agent_read).lines(),
        write: agent_write,
    };
    (client, agent)
}

/// A client whose agent answers each request with the next body in
/// `replies`. A body is `{"result": …}` or `{"error": …}`.
///
/// The task returns every frame the agent read, without `id`, because the
/// SDK makes a new id for each request.
pub(super) async fn scripted_client(
    replies: Vec<serde_json::Value>,
) -> (
    CrucibleAcpClient,
    tokio::task::JoinHandle<Vec<serde_json::Value>>,
) {
    let (client, mut agent) = raw_client().await;
    let task = tokio::spawn(async move {
        let mut frames = Vec::with_capacity(replies.len());
        for mut reply in replies {
            let mut frame = agent.read().await;
            let id = frame
                .as_object_mut()
                .expect("the frame is an object")
                .remove("id")
                .expect("the frame is a request");
            reply["jsonrpc"] = "2.0".into();
            reply["id"] = id;
            agent.write(reply).await;
            frames.push(frame);
        }
        frames
    });
    (client, task)
}
