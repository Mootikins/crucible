//! ACP Client Implementation for CLI
//!
//! Wraps crucible-acp's ChatSession for use in the CLI.

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use tracing::{debug, info};

use crucible_acp::{
    ChatSession, ChatConfig, CrucibleAcpClient as AcpClient,
    HistoryConfig, ContextConfig, StreamConfig,
};

use crate::acp::agent::AgentInfo;

/// ACP Client wrapper for CLI
///
/// Provides a CLI-friendly interface to crucible-acp's ChatSession.
pub struct CrucibleAcpClient {
    session: Option<ChatSession>,
    agent: AgentInfo,
    read_only: bool,
    config: ChatConfig,
    kiln_path: Option<PathBuf>,
}

impl CrucibleAcpClient {
    /// Create a new ACP client
    ///
    /// # Arguments
    /// * `agent` - Information about the agent to spawn
    /// * `read_only` - If true, deny all write operations
    pub fn new(agent: AgentInfo, read_only: bool) -> Self {
        let config = ChatConfig {
            history: HistoryConfig::default(),
            context: ContextConfig {
                enabled: true,
                context_size: 5,
                use_reranking: true,
                rerank_candidates: Some(15),
                enable_cache: true,
                cache_ttl_secs: 300,
            },
            streaming: StreamConfig::default(),
            auto_prune: true,
            enrich_prompts: true, // Enable context enrichment by default
        };

        Self {
            session: None,
            agent,
            read_only,
            config,
            kiln_path: None,
        }
    }

    /// Set the kiln path for tool initialization
    ///
    /// # Arguments
    /// * `path` - Path to the kiln directory
    pub fn with_kiln_path(mut self, path: PathBuf) -> Self {
        self.kiln_path = Some(path);
        self
    }

    /// Create a new ACP client with custom configuration
    ///
    /// # Arguments
    /// * `agent` - Information about the agent to spawn
    /// * `read_only` - If true, deny all write operations
    /// * `config` - Custom chat configuration
    pub fn with_config(agent: AgentInfo, read_only: bool, config: ChatConfig) -> Self {
        Self {
            session: None,
            agent,
            read_only,
            config,
            kiln_path: None,
        }
    }

    /// Spawn and connect to the agent
    ///
    /// Performs the full ACP protocol handshake.
    pub async fn spawn(&mut self) -> Result<()> {
        info!("Spawning agent: {} ({})", self.agent.name, self.agent.command);

        // Create the ACP client with the agent command path
        let agent_path = PathBuf::from(&self.agent.command);
        let client_config = crucible_acp::client::ClientConfig {
            agent_path,
            working_dir: None,
            env_vars: None,
            timeout_ms: None,
            max_retries: None,
        };
        let acp_client = AcpClient::new(client_config);

        // Create ChatSession with the ACP client
        let mut session = ChatSession::with_agent(self.config.clone(), acp_client);

        // Initialize tools if kiln path is available
        if let Some(kiln_path) = &self.kiln_path {
            info!("Initializing tools for kiln: {}", kiln_path.display());
            session.initialize_tools(kiln_path.clone())
                .map_err(|e| anyhow!("Failed to initialize tools: {}", e))?;
            info!("Tools initialized successfully");
        } else {
            info!("No kiln path provided - tools will not be available");
        }

        // Connect to the agent (performs protocol handshake)
        session.connect().await
            .map_err(|e| anyhow!("Failed to connect to agent: {}", e))?;

        info!("Agent connected successfully");

        // Store the session
        self.session = Some(session);

        Ok(())
    }

    /// Send a message to the agent
    ///
    /// # Arguments
    /// * `message` - The user message to send
    ///
    /// # Returns
    /// The agent's response
    pub async fn send_message(&mut self, message: &str) -> Result<String> {
        if let Some(session) = &mut self.session {
            debug!("Sending message to agent: {}", message);
            session.send_message(message).await
                .map_err(|e| anyhow!("Failed to send message: {}", e))
        } else {
            Err(anyhow!("Agent not running. Call spawn() first."))
        }
    }

    /// Start an interactive chat session
    ///
    /// # Arguments
    /// * `enriched_prompt` - The initial prompt (potentially enriched with context)
    pub async fn start_chat(&mut self, enriched_prompt: &str) -> Result<()> {
        info!("Starting chat session");
        info!("Mode: {}", if self.read_only { "Read-only (plan)" } else { "Write-enabled (act)" });

        // Send the initial prompt
        let response = self.send_message(enriched_prompt).await?;

        // Print the response
        println!("\n{}", response);

        Ok(())
    }

    /// Check if the session is connected
    pub fn is_connected(&self) -> bool {
        self.session.is_some()
    }

    /// Get the session ID
    pub fn session_id(&self) -> Option<&str> {
        self.session.as_ref().map(|s| s.session_id())
    }

    /// Get conversation statistics
    pub fn get_stats(&self) -> Option<(usize, usize, usize)> {
        self.session.as_ref().map(|s| {
            let state = s.state();
            (state.turn_count, state.total_tokens_used, state.prune_count)
        })
    }

    /// Clear conversation history
    pub fn clear_history(&mut self) {
        if let Some(session) = &mut self.session {
            session.clear_history();
            info!("Conversation history cleared");
        }
    }

    /// Set context enrichment enabled/disabled
    pub fn set_context_enrichment(&mut self, enabled: bool) {
        self.config.enrich_prompts = enabled;
        if let Some(session) = &mut self.session {
            session.set_enrichment_enabled(enabled);
        }
        info!("Context enrichment {}", if enabled { "enabled" } else { "disabled" });
    }

    /// Shutdown the agent process
    pub async fn shutdown(&mut self) -> Result<()> {
        if let Some(mut session) = self.session.take() {
            info!("Shutting down agent");
            session.disconnect().await
                .map_err(|e| anyhow!("Failed to disconnect: {}", e))?;
            info!("Agent shut down successfully");
        }
        Ok(())
    }
}

impl Drop for CrucibleAcpClient {
    fn drop(&mut self) {
        // Best effort cleanup - session will be dropped and its Drop impl
        // will handle cleanup
        if self.session.is_some() {
            debug!("CrucibleAcpClient dropped with active session");
        }
    }
}

// Tests are in ../acp/tests.rs
