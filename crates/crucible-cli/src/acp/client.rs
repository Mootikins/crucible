//! ACP Client Implementation
//!
//! Implements the Agent Client Protocol client for Crucible.

use anyhow::{anyhow, Result};
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::acp::agent::AgentInfo;

/// ACP Client for Crucible
///
/// Manages communication with an external ACP agent process.
pub struct CrucibleAcpClient {
    agent: AgentInfo,
    process: Arc<Mutex<Option<Child>>>,
    read_only: bool,
}

impl CrucibleAcpClient {
    /// Create a new ACP client
    ///
    /// # Arguments
    /// * `agent` - Information about the agent to spawn
    /// * `read_only` - If true, deny all write operations
    pub fn new(agent: AgentInfo, read_only: bool) -> Self {
        Self {
            agent,
            process: Arc::new(Mutex::new(None)),
            read_only,
        }
    }

    /// Spawn the agent process
    pub async fn spawn(&self) -> Result<()> {
        info!("Spawning agent: {} ({})", self.agent.name, self.agent.command);

        let child = Command::new(&self.agent.command)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn agent '{}': {}", self.agent.command, e))?;

        let mut process = self.process.lock().await;
        *process = Some(child);

        info!("Agent spawned successfully");
        Ok(())
    }

    /// Send a message to the agent
    ///
    /// # Arguments
    /// * `message` - The message to send (JSON-RPC format)
    pub async fn send_message(&self, message: &str) -> Result<()> {
        let mut process = self.process.lock().await;

        if let Some(child) = process.as_mut() {
            if let Some(stdin) = child.stdin.as_mut() {
                debug!("Sending message to agent: {}", message);
                stdin.write_all(message.as_bytes()).await?;
                stdin.write_all(b"\n").await?;
                stdin.flush().await?;
                Ok(())
            } else {
                Err(anyhow!("Agent stdin not available"))
            }
        } else {
            Err(anyhow!("Agent not running"))
        }
    }

    /// Read a response from the agent
    ///
    /// # Returns
    /// The response message (JSON-RPC format)
    pub async fn read_response(&self) -> Result<String> {
        let mut process = self.process.lock().await;

        if let Some(child) = process.as_mut() {
            if let Some(stdout) = child.stdout.as_mut() {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                reader.read_line(&mut line).await?;
                debug!("Received response from agent: {}", line.trim());
                Ok(line)
            } else {
                Err(anyhow!("Agent stdout not available"))
            }
        } else {
            Err(anyhow!("Agent not running"))
        }
    }

    /// Start an interactive chat session
    ///
    /// # Arguments
    /// * `enriched_prompt` - The initial prompt (potentially enriched with context)
    pub async fn start_chat(&self, enriched_prompt: &str) -> Result<()> {
        info!("Starting chat session");
        info!("Mode: {}", if self.read_only { "Read-only (chat)" } else { "Write-enabled (act)" });

        // For MVP, we'll implement a simplified version
        // A full implementation would use the agent-client-protocol crate's
        // JSON-RPC message handling

        // Print the enriched prompt for the user to see what context was added
        println!("\n--- Enriched Prompt ---");
        println!("{}", enriched_prompt);
        println!("--- End Enriched Prompt ---\n");

        // TODO: Implement full ACP protocol integration
        // For now, just indicate what would happen
        println!("🚧 ACP Integration - MVP Placeholder");
        println!("In full implementation, this would:");
        println!("  1. Send enriched prompt to agent");
        println!("  2. Stream responses back to user");
        println!("  3. Handle file read/write requests");
        println!("  4. Manage permissions (read-only: {})", self.read_only);

        Ok(())
    }

    /// Check if the process is still running
    pub async fn is_running(&self) -> bool {
        let process = self.process.lock().await;
        if let Some(child) = process.as_ref() {
            child.id().is_some()
        } else {
            false
        }
    }

    /// Shutdown the agent process
    pub async fn shutdown(&self) -> Result<()> {
        let mut process = self.process.lock().await;

        if let Some(mut child) = process.take() {
            info!("Shutting down agent");
            child.kill().await.ok(); // Ignore errors on kill
            child.wait().await?;
            info!("Agent shut down successfully");
        }

        Ok(())
    }
}

impl Drop for CrucibleAcpClient {
    fn drop(&mut self) {
        // Best effort cleanup
        if let Some(mut child) = self.process.try_lock().ok().and_then(|mut p| p.take()) {
            let _ = child.start_kill();
        }
    }
}

// NOTE: Full ACP protocol implementation would implement the
// agent_client_protocol::Client trait here. For MVP, we have
// a simplified version to demonstrate the architecture.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::agent::AgentInfo;

    #[test]
    fn test_client_creation() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
        };
        let client = CrucibleAcpClient::new(agent, true);
        assert!(client.read_only);
    }
}
