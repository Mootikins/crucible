//! ACP Client Implementation for CLI
//!
//! Wraps crucible-acp's ChatSession for use in the CLI.

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::chat::{AgentHandle, ChatChunk, ChatError, ChatResult, ChatToolCall};
use crucible_acp::{
    channel_callback, AgentInfo, ChatSession, ChatSessionConfig, ContextConfig,
    CrucibleAcpClient as AcpClient, HistoryConfig, InProcessMcpHost, StreamConfig, StreamingChunk,
};
use crucible_config::AcpConfig;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use crucible_core::types::acp::schema::AvailableCommand;

/// ACP Client wrapper for CLI
///
/// Provides a CLI-friendly interface to crucible-acp's ChatSession.
pub struct CrucibleAcpClient {
    session: Option<ChatSession>,
    agent: AgentInfo,
    mode_id: String,
    config: ChatSessionConfig,
    acp_config: AcpConfig,
    kiln_path: Option<PathBuf>,
    working_dir: Option<PathBuf>,
    knowledge_repo: Option<Arc<dyn KnowledgeRepository>>,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    mcp_host: Option<InProcessMcpHost>,
    cached_temperature: Option<f64>,
    cached_max_tokens: Option<u32>,
    cached_thinking_budget: Option<i64>,
}

impl CrucibleAcpClient {
    /// Create a new ACP client
    ///
    /// # Arguments
    /// * `agent` - Information about the agent to spawn
    /// * `read_only` - If true, deny all write operations
    pub fn new(agent: AgentInfo, read_only: bool) -> Self {
        Self::with_acp_config(agent, read_only, AcpConfig::default())
    }

    /// Create a new ACP client with custom ACP configuration
    ///
    /// # Arguments
    /// * `agent` - Information about the agent to spawn
    /// * `read_only` - If true, deny all write operations
    /// * `acp_config` - ACP configuration (includes streaming timeout)
    pub fn with_acp_config(agent: AgentInfo, read_only: bool, acp_config: AcpConfig) -> Self {
        let config = ChatSessionConfig {
            history: HistoryConfig::default(),
            context: ContextConfig {
                enabled: true,
                context_size: 5,
                use_reranking: true,
                rerank_candidates: Some(15),
                enable_cache: true,
                cache_ttl_secs: 300,
                inject_context: false, // ACP agents get tools via MCP
            },
            streaming: StreamConfig::default(),
            auto_prune: true,
            enrich_prompts: true, // Enable context enrichment by default
        };

        let mode_id = if read_only {
            "plan".to_string()
        } else {
            "normal".to_string()
        };

        Self {
            session: None,
            agent,
            mode_id,
            config,
            acp_config,
            kiln_path: None,
            working_dir: None,
            knowledge_repo: None,
            embedding_provider: None,
            mcp_host: None,
            cached_temperature: None,
            cached_max_tokens: None,
            cached_thinking_budget: None,
        }
    }

    /// Set the working directory for the agent
    ///
    /// This is where the agent will operate (for file operations, git, etc.).
    /// It is distinct from the kiln path, which is where knowledge is stored.
    ///
    /// # Arguments
    /// * `path` - The directory where the agent should work
    pub fn with_working_dir(mut self, path: PathBuf) -> Self {
        self.working_dir = Some(path);
        self
    }

    /// Set the kiln path for tool initialization
    ///
    /// # Arguments
    /// * `path` - Path to the kiln directory
    pub fn with_kiln_path(mut self, path: PathBuf) -> Self {
        self.kiln_path = Some(path);
        self
    }

    /// Set the MCP dependencies for in-process tool execution
    ///
    /// # Arguments
    /// * `knowledge_repo` - Repository for semantic search
    /// * `embedding_provider` - Provider for generating embeddings
    pub fn with_mcp_dependencies(
        mut self,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        self.knowledge_repo = Some(knowledge_repo);
        self.embedding_provider = Some(embedding_provider);
        self
    }

    /// Get the agent name for display
    pub fn agent_name(&self) -> &str {
        &self.agent.name
    }

    /// Create a new ACP client with custom chat configuration
    ///
    /// # Arguments
    /// * `agent` - Information about the agent to spawn
    /// * `read_only` - If true, deny all write operations
    /// * `config` - Custom chat configuration
    pub fn with_config(agent: AgentInfo, read_only: bool, config: ChatSessionConfig) -> Self {
        let mode_id = if read_only {
            "plan".to_string()
        } else {
            "act".to_string()
        };

        Self {
            session: None,
            agent,
            mode_id,
            config,
            acp_config: AcpConfig::default(),
            kiln_path: None,
            working_dir: None,
            knowledge_repo: None,
            embedding_provider: None,
            mcp_host: None,
            cached_temperature: None,
            cached_max_tokens: None,
            cached_thinking_budget: None,
        }
    }

    /// Build the ACP client configuration from the agent info
    ///
    /// This method converts the AgentInfo into a ClientConfig suitable for
    /// spawning the ACP agent process. Environment variables from the agent
    /// are passed through to the subprocess.
    pub fn build_client_config(&self) -> crucible_acp::client::ClientConfig {
        let agent_path = PathBuf::from(&self.agent.command);
        let agent_args = if self.agent.args.is_empty() {
            None
        } else {
            Some(self.agent.args.clone())
        };

        // Convert env_vars HashMap to Vec<(String, String)> for ClientConfig
        let env_vars = if self.agent.env_vars.is_empty() {
            None
        } else {
            Some(
                self.agent
                    .env_vars
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            )
        };

        // Convert streaming timeout from minutes to milliseconds
        // The lower-level client multiplies timeout_ms by 10 for overall streaming timeout
        // So we divide by 10 here: minutes * 60 * 1000 / 10 = minutes * 6000
        let timeout_ms = self.acp_config.streaming_timeout_minutes * 6000;

        crucible_acp::client::ClientConfig {
            agent_path,
            agent_args,
            working_dir: self.working_dir.clone(),
            env_vars,
            timeout_ms: Some(timeout_ms),
            max_retries: None,
        }
    }

    /// Spawn and connect to the agent
    ///
    /// Performs the full ACP protocol handshake. If MCP dependencies are configured,
    /// starts an in-process SSE MCP server for tool execution. Otherwise, falls back
    /// to stdio-based MCP.
    pub async fn spawn(&mut self) -> Result<()> {
        info!(
            "Spawning agent: {} ({})",
            self.agent.name, self.agent.command
        );

        let client_config = self.build_client_config();
        info!(
            "Streaming timeout configured: {} minutes ({} ms base)",
            self.acp_config.streaming_timeout_minutes,
            client_config.timeout_ms.unwrap_or(0)
        );

        // Log env vars being passed (without values for security)
        if let Some(ref env_vars) = client_config.env_vars {
            let keys: Vec<_> = env_vars.iter().map(|(k, _)| k.as_str()).collect();
            info!("Passing environment variables to agent: {:?}", keys);
        }

        let acp_client = AcpClient::new(client_config);

        // Create ChatSession with the ACP client
        let mut session = ChatSession::with_agent(self.config.clone(), acp_client);

        // Connect to the agent - use in-process SSE if dependencies are available
        if let (Some(kiln_path), Some(knowledge_repo), Some(embedding_provider)) = (
            &self.kiln_path,
            &self.knowledge_repo,
            &self.embedding_provider,
        ) {
            // Start in-process MCP server
            info!("Starting in-process MCP server...");
            let mcp_host = InProcessMcpHost::start(
                kiln_path.clone(),
                knowledge_repo.clone(),
                embedding_provider.clone(),
            )
            .await
            .map_err(|e| anyhow!("Failed to start in-process MCP server: {}", e))?;

            let sse_url = mcp_host.sse_url();
            info!("In-process MCP server running at {}", sse_url);

            // Connect with SSE MCP
            session
                .connect_with_sse_mcp(&sse_url)
                .await
                .map_err(|e| anyhow!("Failed to connect to agent: {}", e))?;

            // Keep the MCP host alive
            self.mcp_host = Some(mcp_host);
        } else {
            // Fall back to stdio MCP (subprocess)
            info!("Using stdio MCP server (no in-process dependencies configured)");
            session
                .connect()
                .await
                .map_err(|e| anyhow!("Failed to connect to agent: {}", e))?;
        }

        info!("Agent connected successfully");

        // Store the session
        self.session = Some(session);

        Ok(())
    }

    /// Send a message to the agent (ACP-specific, returns raw tool calls)
    ///
    /// # Arguments
    /// * `message` - The user message to send
    ///
    /// # Returns
    /// Tuple of (response, tool_calls)
    pub async fn send_message_acp(
        &mut self,
        message: &str,
    ) -> Result<(String, Vec<crucible_acp::ToolCallInfo>)> {
        if let Some(session) = &mut self.session {
            debug!("Sending message to agent: {}", message);
            session
                .send_message(message)
                .await
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
        info!("Mode: {}", self.mode_id);

        // Send the initial prompt
        let (response, _tool_calls) = self.send_message_acp(enriched_prompt).await?;

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
        info!(
            "Context enrichment {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    /// Get the current chat mode ID
    pub fn mode_id(&self) -> &str {
        &self.mode_id
    }

    /// Set the chat mode by ID, propagating to the ACP agent if connected.
    ///
    /// # Design: best-effort propagation
    ///
    /// Local state is always updated. If the ACP agent rejects the mode change
    /// the local and remote states will diverge — this is intentional.
    /// ACP agents are external processes; we prefer a responsive local UX over
    /// blocking on a remote round-trip that may never succeed.
    pub async fn set_mode_by_id(&mut self, mode_id: &str) -> Result<()> {
        info!("Changing mode from {} to {}", self.mode_id, mode_id);

        if let Some(session) = &mut self.session {
            match session.set_session_mode(mode_id).await {
                Ok(_) => info!("Mode change propagated to ACP agent"),
                Err(e) => warn!(
                    "Failed to propagate mode to ACP agent: {} (continuing with local change)",
                    e
                ),
            }
        }

        self.mode_id = mode_id.to_string();
        Ok(())
    }

    /// Shutdown the agent process
    pub async fn shutdown(&mut self) -> Result<()> {
        if let Some(mut session) = self.session.take() {
            info!("Shutting down agent");
            session
                .disconnect()
                .await
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

// Implement AgentHandle trait for backend-agnostic chat interface
#[async_trait::async_trait]
impl AgentHandle for CrucibleAcpClient {
    fn send_message_stream(
        &mut self,
        message: String,
    ) -> futures::stream::BoxStream<'static, ChatResult<ChatChunk>> {
        use futures::StreamExt;
        use std::future::Future;
        use std::pin::Pin;
        use tokio::sync::mpsc;

        // Create channel for streaming chunks
        let (tx, rx) = mpsc::unbounded_channel::<StreamingChunk>();

        match self.session.as_mut() {
            Some(session) => {
                // Create a callback that sends chunks to the channel
                let callback = channel_callback(tx.clone());

                // Create the future that runs the streaming call
                type FutureOutput =
                    Result<(String, Vec<crucible_acp::ToolCallInfo>), crucible_acp::ClientError>;
                let fut: Pin<Box<dyn Future<Output = FutureOutput> + '_>> = Box::pin(async move {
                    session.send_message_with_callback(&message, callback).await
                });

                // SAFETY: Transmuting lifetime - safe because session lives as long as self
                // and stream must be consumed before self is dropped (&mut self exclusivity)
                let static_fut: Pin<Box<dyn Future<Output = FutureOutput> + Send>> =
                    unsafe { std::mem::transmute(fut) };

                // Spawn a task to run the streaming call in the background
                // This allows us to yield chunks as they arrive
                let (result_tx, result_rx) = tokio::sync::oneshot::channel();
                tokio::spawn(async move {
                    let result = static_fut.await;
                    let _ = result_tx.send(result);
                });

                // State for unfold: Option contains (rx, result_rx, tool_calls, terminated_flag)
                // When terminated_flag is true, the next poll will return None to end stream
                type UnfoldState = Option<(
                    mpsc::UnboundedReceiver<StreamingChunk>,
                    tokio::sync::oneshot::Receiver<
                        Result<
                            (String, Vec<crucible_acp::ToolCallInfo>),
                            crucible_acp::ClientError,
                        >,
                    >,
                    Vec<ChatToolCall>,
                    bool, // terminated
                )>;

                // Convert the channel receiver into a stream of ChatChunks
                Box::pin(futures::stream::unfold(
                    Some((rx, result_rx, Vec::new(), false)) as UnfoldState,
                    move |state| async move {
                        // Check if we're in terminated state or no state
                        let (mut rx, result_rx, mut tool_calls, terminated) = state?;
                        if terminated {
                            return None; // Stream properly terminated
                        }

                        // Try to receive a chunk
                        match rx.recv().await {
                            Some(chunk) => {
                                let chat_chunk = match chunk {
                                    StreamingChunk::Text(text) => ChatChunk {
                                        delta: text,
                                        done: false,
                                        tool_calls: None,
                                        tool_results: None,
                                        reasoning: None,
                                        usage: None,
                                        subagent_events: None,
                                    },
                                    StreamingChunk::Thinking(text) => ChatChunk {
                                        delta: String::new(),
                                        done: false,
                                        tool_calls: None,
                                        tool_results: None,
                                        reasoning: Some(text),
                                        usage: None,
                                        subagent_events: None,
                                    },
                                    StreamingChunk::ToolStart { name, id } => {
                                        tool_calls.push(ChatToolCall {
                                            name: name.clone(),
                                            arguments: None,
                                            id: Some(id),
                                        });
                                        ChatChunk {
                                            delta: String::new(),
                                            done: false,
                                            tool_calls: Some(vec![ChatToolCall {
                                                name,
                                                arguments: None,
                                                id: None,
                                            }]),
                                            tool_results: None,
                                            reasoning: None,
                                            usage: None,
                                            subagent_events: None,
                                        }
                                    }
                                    StreamingChunk::ToolEnd { id: _ } => ChatChunk {
                                        delta: String::new(),
                                        done: false,
                                        tool_calls: None,
                                        tool_results: None,
                                        reasoning: None,
                                        usage: None,
                                        subagent_events: None,
                                    },
                                };
                                Some((Ok(chat_chunk), Some((rx, result_rx, tool_calls, false))))
                            }
                            None => {
                                // Channel closed - check if streaming completed successfully
                                match result_rx.await {
                                    Ok(Ok((content, acp_tool_calls))) => {
                                        // Log completion details for debugging
                                        tracing::debug!(
                                            content_len = content.len(),
                                            tool_count = acp_tool_calls.len(),
                                            "ACP stream completed"
                                        );
                                        if content.is_empty() && acp_tool_calls.is_empty() {
                                            tracing::warn!(
                                                "ACP stream completed with empty response"
                                            );
                                        }

                                        // Emit final chunk with all tool calls, mark terminated
                                        let final_tool_calls: Vec<ChatToolCall> = acp_tool_calls
                                            .into_iter()
                                            .map(|t| ChatToolCall {
                                                name: t.title,
                                                arguments: t.arguments,
                                                id: t.id,
                                            })
                                            .collect();
                                        Some((
                                            Ok(ChatChunk {
                                                delta: String::new(),
                                                done: true,
                                                tool_calls: if final_tool_calls.is_empty() {
                                                    None
                                                } else {
                                                    Some(final_tool_calls)
                                                },
                                                tool_results: None,
                                                reasoning: None,
                                                usage: None,
                                                subagent_events: None,
                                            }),
                                            None,
                                        ))
                                    }
                                    Ok(Err(e)) => {
                                        tracing::warn!(error = %e, "ACP stream error");
                                        Some((
                                            Err(ChatError::Communication(format!(
                                                "ACP error: {}",
                                                e
                                            ))),
                                            None, // Terminate stream after error
                                        ))
                                    }
                                    Err(_) => {
                                        tracing::warn!(
                                            "ACP streaming task failed (oneshot dropped)"
                                        );
                                        Some((
                                            Err(ChatError::Communication(
                                                "ACP streaming task failed".to_string(),
                                            )),
                                            None, // Terminate stream after error
                                        ))
                                    }
                                }
                            }
                        }
                    },
                ))
            }
            None => {
                // No session - return error stream
                Box::pin(futures::stream::once(async {
                    Err(ChatError::AgentUnavailable(
                        "Agent not running. Call spawn() first.".to_string(),
                    ))
                }))
            }
        }
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        self.set_mode_by_id(mode_id)
            .await
            .map_err(|e| ChatError::ModeChange(e.to_string()))
    }

    fn get_mode_id(&self) -> &str {
        self.mode_id()
    }

    fn is_connected(&self) -> bool {
        self.is_connected()
    }

    fn get_commands(&self) -> &[AvailableCommand] {
        if let Some(session) = &self.session {
            session.available_commands()
        } else {
            &[]
        }
    }

    async fn clear_history(&mut self) {
        if let Some(session) = &mut self.session {
            session.clear_history();
            info!("Conversation history cleared");
        }
    }

    async fn set_temperature(&mut self, temperature: f64) -> ChatResult<()> {
        info!(
            temperature,
            "Setting temperature (cached locally for ACP agent)"
        );
        self.cached_temperature = Some(temperature);
        Ok(())
    }

    fn get_temperature(&self) -> Option<f64> {
        self.cached_temperature
    }

    async fn set_thinking_budget(&mut self, budget: i64) -> ChatResult<()> {
        info!(
            budget,
            "Setting thinking budget (cached locally for ACP agent)"
        );
        self.cached_thinking_budget = Some(budget);
        Ok(())
    }

    fn get_thinking_budget(&self) -> Option<i64> {
        self.cached_thinking_budget
    }

    async fn set_max_tokens(&mut self, max_tokens: Option<u32>) -> ChatResult<()> {
        info!(
            ?max_tokens,
            "Setting max tokens (cached locally for ACP agent)"
        );
        self.cached_max_tokens = max_tokens;
        Ok(())
    }

    fn get_max_tokens(&self) -> Option<u32> {
        self.cached_max_tokens
    }

    async fn switch_model(&mut self, model_id: &str) -> ChatResult<()> {
        Err(ChatError::NotSupported(format!(
            "ACP agents manage their own model selection. Cannot switch to '{}'",
            model_id
        )))
    }
}

// Tests are in ../acp/tests.rs
