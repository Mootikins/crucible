//! Threaded mock agent for integration testing without subprocess spawning
//!
//! This module provides an in-process mock agent that communicates via
//! tokio DuplexStream pipes, eliminating the need to build and spawn
//! a separate binary for integration tests.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::support::{ThreadedMockAgent, MockStdioAgentConfig};
//! use crucible_acp::CrucibleAcpClient;
//!
//! let config = MockStdioAgentConfig::opencode();
//! let (client, _agent_handle) = ThreadedMockAgent::spawn_with_client(config).await;
//!
//! // Now use client normally - it's connected to the in-process mock agent
//! let result = client.connect_with_handshake().await;
//! ```

use super::mock_stdio_agent::{MockStdioAgent, MockStdioAgentConfig};
use serde_json::Value;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// Handle to a running threaded mock agent
///
/// When dropped, signals the agent task to shut down.
pub struct ThreadedMockAgentHandle {
    /// Task handle for the agent background task
    task: JoinHandle<()>,
    /// Shutdown signal sender
    _shutdown_tx: oneshot::Sender<()>,
}

impl ThreadedMockAgentHandle {
    /// Wait for the agent task to complete
    pub async fn join(self) -> Result<(), tokio::task::JoinError> {
        self.task.await
    }

    /// Abort the agent task immediately
    pub fn abort(&self) {
        self.task.abort();
    }
}

/// A mock agent transport consisting of connected pipe streams
pub struct MockAgentTransport {
    /// Client-side reader (receives agent responses)
    pub client_reader: BufReader<tokio::io::ReadHalf<DuplexStream>>,
    /// Client-side writer (sends requests to agent)
    pub client_writer: tokio::io::WriteHalf<DuplexStream>,
}

impl MockAgentTransport {
    /// Create a new pair of connected duplex streams for client-agent communication
    ///
    /// Returns (client_transport, agent_reader, agent_writer)
    fn new_pair() -> (
        Self,
        BufReader<tokio::io::ReadHalf<DuplexStream>>,
        tokio::io::WriteHalf<DuplexStream>,
    ) {
        // Create two duplex streams - they're bidirectional pipes
        // client_to_agent: client writes, agent reads
        // agent_to_client: agent writes, client reads
        let (client_to_agent_client, client_to_agent_agent) = tokio::io::duplex(8192);
        let (agent_to_client_agent, agent_to_client_client) = tokio::io::duplex(8192);

        // Split streams for half-duplex usage
        let (_ctaa_read, ctaa_write) = tokio::io::split(client_to_agent_client);
        let (ctaa_agent_read, _ctaa_agent_write) = tokio::io::split(client_to_agent_agent);

        let (_atcc_read, atcc_write) = tokio::io::split(agent_to_client_agent);
        let (atcc_client_read, _atcc_client_write) = tokio::io::split(agent_to_client_client);

        let client_transport = MockAgentTransport {
            client_reader: BufReader::new(atcc_client_read),
            client_writer: ctaa_write,
        };

        let agent_reader = BufReader::new(ctaa_agent_read);
        let agent_writer = atcc_write;

        (client_transport, agent_reader, agent_writer)
    }
}

/// Threaded mock agent that runs in a background tokio task
///
/// This provides the same functionality as spawning the mock-acp-agent binary,
/// but runs entirely in-process using async pipes.
pub struct ThreadedMockAgent;

impl ThreadedMockAgent {
    /// Spawn a mock agent in a background task and return connected transport
    ///
    /// # Arguments
    ///
    /// * `config` - Mock agent configuration
    ///
    /// # Returns
    ///
    /// A tuple of (transport, agent_handle) where:
    /// - transport: Contains reader/writer for communicating with the agent
    /// - agent_handle: Handle to the background task (drop to signal shutdown)
    pub fn spawn(config: MockStdioAgentConfig) -> (MockAgentTransport, ThreadedMockAgentHandle) {
        let (transport, agent_reader, agent_writer) = MockAgentTransport::new_pair();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let task = tokio::spawn(async move {
            Self::run_agent_loop(config, agent_reader, agent_writer, shutdown_rx).await;
        });

        let handle = ThreadedMockAgentHandle {
            task,
            _shutdown_tx: shutdown_tx,
        };

        (transport, handle)
    }

    /// Spawn a mock agent and create a pre-configured CrucibleAcpClient
    ///
    /// This is the most convenient way to create a test client that's
    /// already connected to an in-process mock agent.
    ///
    /// # Arguments
    ///
    /// * `config` - Mock agent configuration
    ///
    /// # Returns
    ///
    /// A tuple of (client, agent_handle) where the client is already connected
    /// to the in-process mock agent.
    pub fn spawn_with_client(
        config: MockStdioAgentConfig,
    ) -> (crucible_acp::CrucibleAcpClient, ThreadedMockAgentHandle) {
        let (transport, handle) = Self::spawn(config);

        // Create client with pre-connected transport
        let client_config = crucible_acp::client::ClientConfig {
            agent_path: PathBuf::from("mock-threaded-agent"),
            agent_args: None,
            working_dir: None,
            env_vars: None,
            timeout_ms: Some(5000),
            max_retries: Some(1),
        };

        // Use with_transport to inject the in-process reader/writer
        let client = crucible_acp::CrucibleAcpClient::with_transport(
            client_config,
            Box::pin(transport.client_writer),
            Box::pin(transport.client_reader),
        );

        (client, handle)
    }

    /// Run the agent event loop
    async fn run_agent_loop(
        config: MockStdioAgentConfig,
        mut reader: BufReader<tokio::io::ReadHalf<DuplexStream>>,
        mut writer: tokio::io::WriteHalf<DuplexStream>,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) {
        let mut agent = MockStdioAgent::new(config);
        let mut line = String::new();

        loop {
            line.clear();

            tokio::select! {
                // Check for shutdown signal
                _ = &mut shutdown_rx => {
                    tracing::debug!("Threaded mock agent received shutdown signal");
                    break;
                }

                // Read next request line
                result = reader.read_line(&mut line) => {
                    match result {
                        Ok(0) => {
                            // EOF - client closed connection
                            tracing::debug!("Threaded mock agent: client closed connection");
                            break;
                        }
                        Ok(_) => {
                            let trimmed = line.trim();
                            if trimmed.is_empty() {
                                continue;
                            }

                            // Parse and handle request
                            match serde_json::from_str::<Value>(trimmed) {
                                Ok(request) => {
                                    // Simulate delay if configured
                                    if let Some(delay_ms) = agent.config.response_delay_ms {
                                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                                    }

                                    // Handle request
                                    let response = agent.handle_request(&request);

                                    // Send response
                                    let response_json = serde_json::to_string(&response).unwrap();
                                    if writer.write_all(response_json.as_bytes()).await.is_err() {
                                        break;
                                    }
                                    if writer.write_all(b"\n").await.is_err() {
                                        break;
                                    }
                                    if writer.flush().await.is_err() {
                                        break;
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("Threaded mock agent: failed to parse request: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Threaded mock agent: read error: {}", e);
                            break;
                        }
                    }
                }
            }
        }
    }
}

/// Trait extension for MockStdioAgent to access config
trait MockStdioAgentExt {
    fn config(&self) -> &MockStdioAgentConfig;
}

// Extend MockStdioAgent with a config field accessor
// Note: This requires MockStdioAgent.config to be pub
impl MockStdioAgentExt for MockStdioAgent {
    fn config(&self) -> &MockStdioAgentConfig {
        // Access via the public field
        &self.config
    }
}

// Add pub to MockStdioAgent.config field - we need to modify mock_stdio_agent.rs
// For now, we'll work around this by storing config separately in run_agent_loop

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_threaded_agent_initialize() {
        let config = MockStdioAgentConfig::opencode();
        let (mut transport, _handle) = ThreadedMockAgent::spawn(config);

        // Send initialize request
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": 1,
                "clientInfo": null,
                "clientCapabilities": {},
                "meta": null
            }
        });

        let request_str = serde_json::to_string(&request).unwrap();
        transport
            .client_writer
            .write_all(request_str.as_bytes())
            .await
            .unwrap();
        transport.client_writer.write_all(b"\n").await.unwrap();
        transport.client_writer.flush().await.unwrap();

        // Read response
        let mut response_line = String::new();
        transport
            .client_reader
            .read_line(&mut response_line)
            .await
            .unwrap();

        let response: Value = serde_json::from_str(&response_line).unwrap();
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 1);
        assert!(response.get("result").is_some());
        assert_eq!(response["result"]["agentInfo"]["name"], "mock-opencode");
    }

    #[tokio::test]
    async fn test_threaded_agent_new_session() {
        let config = MockStdioAgentConfig::opencode();
        let (mut transport, _handle) = ThreadedMockAgent::spawn(config);

        // Send new session request
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session/new",
            "params": {
                "cwd": "/test",
                "mcpServers": [],
                "meta": null
            }
        });

        let request_str = serde_json::to_string(&request).unwrap();
        transport
            .client_writer
            .write_all(request_str.as_bytes())
            .await
            .unwrap();
        transport.client_writer.write_all(b"\n").await.unwrap();
        transport.client_writer.flush().await.unwrap();

        // Read response
        let mut response_line = String::new();
        transport
            .client_reader
            .read_line(&mut response_line)
            .await
            .unwrap();

        let response: Value = serde_json::from_str(&response_line).unwrap();
        assert_eq!(response["jsonrpc"], "2.0");
        assert!(response.get("result").is_some());
        let session_id = response["result"]["sessionId"].as_str().unwrap();
        assert!(session_id.starts_with("mock-session-"));
    }

    #[tokio::test]
    async fn test_threaded_agent_complete_handshake() {
        let config = MockStdioAgentConfig::opencode();
        let (mut transport, _handle) = ThreadedMockAgent::spawn(config);

        // Step 1: Initialize
        let init_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": 1,
                "clientInfo": null,
                "clientCapabilities": {},
                "meta": null
            }
        });

        transport
            .client_writer
            .write_all(serde_json::to_string(&init_request).unwrap().as_bytes())
            .await
            .unwrap();
        transport.client_writer.write_all(b"\n").await.unwrap();
        transport.client_writer.flush().await.unwrap();

        let mut line = String::new();
        transport.client_reader.read_line(&mut line).await.unwrap();
        let init_response: Value = serde_json::from_str(&line).unwrap();
        assert!(init_response.get("result").is_some());

        // Step 2: New Session
        let session_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "session/new",
            "params": {
                "cwd": "/test",
                "mcpServers": [],
                "meta": null
            }
        });

        transport
            .client_writer
            .write_all(serde_json::to_string(&session_request).unwrap().as_bytes())
            .await
            .unwrap();
        transport.client_writer.write_all(b"\n").await.unwrap();
        transport.client_writer.flush().await.unwrap();

        line.clear();
        transport.client_reader.read_line(&mut line).await.unwrap();
        let session_response: Value = serde_json::from_str(&line).unwrap();
        assert!(session_response.get("result").is_some());
        assert!(session_response["result"]["sessionId"].as_str().is_some());
    }
}
