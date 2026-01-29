//! Rig Agent Handle
//!
//! Implements `AgentHandle` using Rig's agent abstractions.
//! This replaces `InternalAgentHandle` with Rig-based agent execution.
//!
//! ## Design Principles
//!
//! - **Stateless agents**: Conversation history is managed externally for multi-agent compatibility
//! - **Streaming-first**: Uses Rig's `stream_prompt()` for real-time responses
//! - **Mode-aware**: Tracks plan/act/auto modes (Rig doesn't have this natively)
//! - **Tool bridging**: Crucible tools are bridged to Rig's tool system

use async_trait::async_trait;
use crucible_core::traits::chat::{
    AgentHandle, ChatChunk, ChatError, ChatResult, ChatToolCall, ChatToolResult,
};
use crucible_core::traits::TokenUsage;
use crucible_core::types::acp::schema::SessionModeState;
use crucible_core::types::mode::default_internal_modes;
use futures::stream::BoxStream;
use futures::StreamExt;
use rig::agent::Agent;
use rig::completion::{AssistantContent, CompletionModel};
use rig::message::{Message, ToolCall as RigToolCall, ToolResult};
use rig::OneOrMany;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::agent::{build_agent_from_components_generic, AgentComponents};

/// Type alias for agent rebuild functions (reduces type complexity).
type RebuildFn<M> =
    Arc<dyn Fn(&AgentComponents, &str) -> Result<Agent<M>, ChatError> + Send + Sync>;
use crate::openai_reasoning::{self, ReasoningChunk};
use crate::providers::RigClient;
use crate::xml_tool_parser;

/// Check if a tool name represents a write operation
///
/// Write operations should be blocked in plan mode.
fn is_write_tool_name(tool_name: &str) -> bool {
    // Workspace write operations
    if tool_name == "write_file" || tool_name == "edit_file" {
        return true;
    }

    // Kiln write operations (if any)
    if tool_name.starts_with("create_") || tool_name.starts_with("delete_") {
        return true;
    }

    // Bash/shell execution (can write files)
    if tool_name == "bash" {
        return true;
    }

    false
}

/// Rig-based agent handle implementing `AgentHandle` trait.
///
/// This provides a bridge between Rig's agent abstraction and Crucible's
/// `AgentHandle` trait, enabling Rig agents to be used in the CLI and
/// other Crucible infrastructure.
///
/// ## Multi-Agent Compatibility
///
/// Conversation history is stored externally in `chat_history`, allowing:
/// - Session state to persist across agent switches
/// - History to be transferred between agents
/// - Clean separation of agent execution from state management
pub struct RigAgentHandle<M>
where
    M: CompletionModel + 'static,
{
    /// The Rig agent (wrapped in Arc for streaming)
    agent: Arc<Agent<M>>,

    /// Conversation history (external to agent for multi-agent support)
    chat_history: Arc<RwLock<Vec<Message>>>,

    /// Mode state (plan/act/auto) - managed by us, not Rig
    mode_state: SessionModeState,

    /// Current mode ID
    current_mode_id: String,

    /// Whether mode context has been sent for current mode
    /// Reset to false on mode change, set to true after first message
    mode_context_sent: AtomicBool,

    /// Maximum tool call depth (prevents infinite loops)
    max_tool_depth: usize,

    /// OpenAI-compatible endpoint URL for custom streaming with reasoning support
    reasoning_endpoint: Option<String>,

    /// Base Ollama API endpoint for model discovery
    ollama_endpoint: Option<String>,

    model_name: Option<String>,

    /// Thinking budget for reasoning models (-1=unlimited, 0=disabled, >0=max tokens)
    thinking_budget: Option<i64>,

    /// Temperature for response generation (0.0-2.0)
    temperature: Option<f64>,

    /// Maximum tokens for response generation
    max_tokens: Option<u32>,

    /// HTTP client for custom streaming
    http_client: reqwest::Client,

    /// Workspace context for mode synchronization with tools
    workspace_ctx: Option<crate::workspace_tools::WorkspaceContext>,

    /// Components for rebuilding the agent (enables model switching)
    components: Option<AgentComponents>,

    /// Flag indicating agent needs rebuild (set by switch_model, cleared by rebuild)
    needs_rebuild: AtomicBool,

    /// Type-erased rebuild function (set when components are provided)
    rebuild_fn: Option<RebuildFn<M>>,
}

impl<M> RigAgentHandle<M>
where
    M: CompletionModel + 'static,
    M::StreamingResponse: Clone + Send + Sync + rig::completion::GetTokenUsage,
{
    /// Create a new Rig agent handle
    ///
    /// # Arguments
    ///
    /// * `agent` - The Rig agent to wrap
    pub fn new(agent: Agent<M>) -> Self {
        let mode_state = default_internal_modes();
        let current_mode_id = mode_state.current_mode_id.0.to_string();

        Self {
            agent: Arc::new(agent),
            chat_history: Arc::new(RwLock::new(Vec::new())),
            mode_state,
            current_mode_id,
            mode_context_sent: AtomicBool::new(false),
            max_tool_depth: 50,
            reasoning_endpoint: None,
            ollama_endpoint: None,
            model_name: None,
            thinking_budget: None,
            temperature: None,
            max_tokens: None,
            http_client: reqwest::Client::new(),
            workspace_ctx: None,
            components: None,
            needs_rebuild: AtomicBool::new(false),
            rebuild_fn: None,
        }
    }

    /// Set the workspace context for mode synchronization
    pub fn with_workspace_context(mut self, ctx: crate::workspace_tools::WorkspaceContext) -> Self {
        self.workspace_ctx = Some(ctx);
        self
    }

    /// Set the initial mode (plan/normal/auto)
    ///
    /// This is a synchronous builder method for setting the mode at construction time.
    /// For runtime mode changes, use `set_mode_str` instead.
    pub fn with_initial_mode(mut self, mode_id: &str) -> Self {
        if self
            .mode_state
            .available_modes
            .iter()
            .any(|m| m.id.0.as_ref() == mode_id)
        {
            self.current_mode_id = mode_id.to_string();
            self.mode_state.current_mode_id =
                crucible_core::types::acp::schema::SessionModeId::new(mode_id);
            if let Some(ref ctx) = self.workspace_ctx {
                ctx.set_mode(mode_id);
            }
        }
        self
    }

    /// Set the maximum tool call depth
    ///
    /// This prevents infinite loops when the LLM keeps requesting tool calls.
    pub fn with_max_tool_depth(mut self, depth: usize) -> Self {
        self.max_tool_depth = depth;
        self
    }

    /// Enable custom streaming with reasoning_content extraction
    ///
    /// When set, uses our SSE parser instead of Rig's to extract the
    /// non-standard `reasoning_content` field from OpenAI-compatible APIs
    /// (Ollama, llama.cpp).
    pub fn with_reasoning_endpoint(mut self, endpoint: String, model: String) -> Self {
        self.reasoning_endpoint = Some(endpoint);
        self.model_name = Some(model);
        self
    }

    /// Set base Ollama endpoint for model discovery
    pub fn with_ollama_endpoint(mut self, endpoint: String) -> Self {
        self.ollama_endpoint = Some(endpoint);
        self
    }

    /// Set initial model name (enables custom streaming with model switching)
    pub fn with_model(mut self, model: String) -> Self {
        self.model_name = Some(model);
        self
    }

    /// Set thinking budget for reasoning models (-1=unlimited, 0=disabled, >0=max tokens)
    pub fn with_thinking_budget(mut self, budget: Option<i64>) -> Self {
        self.thinking_budget = budget;
        self
    }

    /// Set initial conversation history
    ///
    /// Useful for resuming sessions or multi-agent handoff.
    pub fn with_history(mut self, history: Vec<Message>) -> Self {
        self.chat_history = Arc::new(RwLock::new(history));
        self
    }

    /// Get a copy of the current conversation history
    ///
    /// Useful for session persistence or agent handoff.
    pub async fn get_history(&self) -> Vec<Message> {
        self.chat_history.read().await.clone()
    }

    /// Clear conversation history
    pub async fn clear_history(&self) {
        self.chat_history.write().await.clear();
    }
}

impl RigAgentHandle<rig::providers::ollama::CompletionModel> {
    /// Set components with Ollama-specific rebuild support
    pub fn with_ollama_components(mut self, components: AgentComponents) -> Self {
        self.workspace_ctx = Some(components.workspace_ctx.clone());
        self.ollama_endpoint = components.ollama_endpoint.clone();
        self.thinking_budget = components.thinking_budget;
        self.model_name = Some(components.config.model.clone());

        let rebuild_fn: RebuildFn<rig::providers::ollama::CompletionModel> =
            Arc::new(|comp, model| {
                let client = match &comp.client {
                    RigClient::Ollama(c) => c,
                    _ => {
                        return Err(ChatError::NotSupported(
                            "Ollama handle received non-Ollama client".into(),
                        ))
                    }
                };

                let built = build_agent_from_components_generic(comp, model, client)
                    .map_err(|e| ChatError::Internal(format!("Agent rebuild failed: {}", e)))?;

                Ok(built.agent)
            });

        self.rebuild_fn = Some(rebuild_fn);
        self.components = Some(components);
        self
    }
}

impl<M> RigAgentHandle<M>
where
    M: CompletionModel + 'static,
    M::StreamingResponse: Clone + Send + Sync + rig::completion::GetTokenUsage,
{
    /// Stream with custom SSE parsing for reasoning_content extraction
    ///
    /// This bypasses Rig's streaming to directly parse the `reasoning_content`
    /// field from OpenAI-compatible APIs (Ollama, llama.cpp). Use when you need
    /// to capture model thinking/reasoning output.
    ///
    /// Note: This currently doesn't support multi-turn tool execution. Tool calls
    /// are emitted but not automatically executed.
    #[allow(dead_code)]
    fn send_message_stream_with_reasoning(
        &self,
        message: String,
        endpoint: String,
        model: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        let history = Arc::clone(&self.chat_history);
        let http_client = self.http_client.clone();
        let current_mode_id = self.current_mode_id.clone();
        let thinking_budget = self.thinking_budget;

        // Check if we need to send mode context (only once per mode)
        let should_send_mode_context =
            current_mode_id == "plan" && !self.mode_context_sent.swap(true, Ordering::SeqCst);

        Box::pin(async_stream::stream! {
            // Build messages array from history + new message
            let current_history = history.read().await.clone();
            let mut messages: Vec<serde_json::Value> = Vec::new();

            // Convert Rig messages to OpenAI format
            for msg in current_history.iter() {
                match msg {
                    Message::User { content, .. } => {
                        // User messages: extract text from content
                        let text = content.iter()
                            .filter_map(|c| {
                                use rig::message::UserContent;
                                match c {
                                    UserContent::Text(t) => Some(t.text.clone()),
                                    _ => None,
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        messages.push(serde_json::json!({
                            "role": "user",
                            "content": text
                        }));
                    }
                    Message::Assistant { content, .. } => {
                        // Assistant messages: extract text and tool calls
                        let mut text_parts = Vec::new();
                        let mut tool_calls_json = Vec::new();

                        for c in content.iter() {
                            match c {
                                AssistantContent::Text(t) => {
                                    text_parts.push(t.text.clone());
                                }
                                AssistantContent::ToolCall(tc) => {
                                    tool_calls_json.push(serde_json::json!({
                                        "id": tc.call_id.as_deref().unwrap_or(""),
                                        "type": "function",
                                        "function": {
                                            "name": tc.function.name,
                                            "arguments": serde_json::to_string(&tc.function.arguments).unwrap_or_default()
                                        }
                                    }));
                                }
                                _ => {}
                            }
                        }

                        let mut msg_obj = serde_json::json!({
                            "role": "assistant",
                            "content": text_parts.join("")
                        });

                        if !tool_calls_json.is_empty() {
                            msg_obj["tool_calls"] = serde_json::Value::Array(tool_calls_json);
                        }

                        messages.push(msg_obj);
                    }
                }
            }

            // Add mode context for plan mode (only on first message after mode change)
            let prompt_message = if should_send_mode_context {
                format!(
                    "[MODE: Plan mode - write tools (bash, write_file, edit_file, create_*, delete_*) are DISABLED. Use read-only tools only. Switch to act mode with /act to enable writes.]\n\n{}",
                    message
                )
            } else {
                message.clone()
            };

            // Add new user message with mode context if applicable
            messages.push(serde_json::json!({
                "role": "user",
                "content": prompt_message
            }));

            // Add user message to history (original message, not mode-prefixed)
            {
                let mut h = history.write().await;
                h.push(Message::user(&message));
            }

            debug!(
                endpoint = %endpoint,
                model = %model,
                message_count = messages.len(),
                "Starting custom reasoning stream"
            );

            // Create custom stream with reasoning support
            let options = openai_reasoning::ReasoningOptions {
                tools: None, // TODO: Support tools in custom streaming
                thinking_budget,
            };
            let mut stream = openai_reasoning::stream_with_reasoning(
                http_client,
                &endpoint,
                &model,
                messages,
                options,
            );

            let mut accumulated_text = String::new();
            let mut accumulated_reasoning = String::new();

            while let Some(result) = stream.next().await {
                match result {
                    Ok(chunk) => {
                        match chunk {
                            ReasoningChunk::Text(text) => {
                                accumulated_text.push_str(&text);
                                yield Ok(ChatChunk {
                                    delta: text,
                                    done: false,
                                    tool_calls: None,
                                    tool_results: None,
                                    reasoning: None,
                                    usage: None,
                                    subagent_events: None,
                                });
                            }
                            ReasoningChunk::Reasoning(reasoning) => {
                                accumulated_reasoning.push_str(&reasoning);
                                yield Ok(ChatChunk {
                                    delta: String::new(),
                                    done: false,
                                    tool_calls: None,
                                    tool_results: None,
                                    reasoning: Some(reasoning),
                                    usage: None,
                                    subagent_events: None,
                                });
                            }
                            ReasoningChunk::ToolCall { id, name, arguments } => {
                                // Parse arguments as JSON
                                let args: serde_json::Value = serde_json::from_str(&arguments)
                                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                                // Note: This is custom streaming - track for potential result lookup
                                // (though tool execution not implemented in custom streaming yet)
                                yield Ok(ChatChunk {
                                    delta: String::new(),
                                    done: false,
                                    tool_calls: Some(vec![ChatToolCall {
                                        id: Some(id),
                                        name,
                                        arguments: Some(args),
                                    }]),
                                    tool_results: None,
                                    reasoning: None,
                                    usage: None,
                                    subagent_events: None,
                                });
                            }
                            ReasoningChunk::Done => {
                                // Update history with assistant response
                                {
                                    let mut h = history.write().await;
                                    if !accumulated_text.is_empty() {
                                        h.push(Message::assistant(accumulated_text.clone()));
                                    }
                                }

                                debug!(
                                    text_len = accumulated_text.len(),
                                    reasoning_len = accumulated_reasoning.len(),
                                    "Custom reasoning stream complete"
                                );

                                yield Ok(ChatChunk {
                                    delta: String::new(),
                                    done: true,
                                    tool_calls: None,
                                    tool_results: None,
                                    reasoning: None,
                                    usage: None,
                                    subagent_events: None,
                                });
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Custom reasoning stream error");
                        yield Err(ChatError::Communication(format!("Reasoning stream error: {}", e)));
                        return;
                    }
                }
            }

            // Stream ended without Done chunk - emit done anyway
            yield Ok(ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
            });
        })
    }
}

#[async_trait]
impl<M> AgentHandle for RigAgentHandle<M>
where
    M: CompletionModel + 'static,
    M::StreamingResponse: Clone + Send + Sync + Unpin + rig::completion::GetTokenUsage,
{
    fn send_message_stream(
        &mut self,
        message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        use rig::agent::MultiTurnStreamItem;
        use rig::streaming::{StreamedAssistantContent, StreamingPrompt};

        debug!("RigAgentHandle::send_message_stream called: {}", message);

        if self.needs_rebuild.swap(false, Ordering::SeqCst) {
            if let (Some(rebuild_fn), Some(components), Some(model)) =
                (&self.rebuild_fn, &self.components, &self.model_name)
            {
                match rebuild_fn(components, model) {
                    Ok(new_agent) => {
                        self.agent = Arc::new(new_agent);
                        info!(model = %model, "Agent rebuilt before streaming");
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to rebuild agent, continuing with old agent");
                    }
                }
            }
        }

        let agent = Arc::clone(&self.agent);
        let history = Arc::clone(&self.chat_history);
        let max_depth = self.max_tool_depth;
        let current_mode_id = self.current_mode_id.clone();

        // Check if we need to send mode context (only once per mode)
        let should_send_mode_context =
            current_mode_id == "plan" && !self.mode_context_sent.swap(true, Ordering::SeqCst);

        Box::pin(async_stream::stream! {
            // Get current history
            let current_history = history.read().await.clone();

            // Add mode context for plan mode (only on first message after mode change)
            let prompt_message = if should_send_mode_context {
                format!(
                    "[MODE: Plan mode - write tools (bash, write_file, edit_file, create_*, delete_*) are DISABLED. Use read-only tools only. Switch to act mode with /act to enable writes.]\n\n{}",
                    message
                )
            } else {
                message.clone()
            };

            // Add user message to history (original message, not the mode-prefixed one)
            {
                let mut h = history.write().await;
                h.push(Message::user(&message));
            }

            // Create streaming request with history (use mode-prefixed prompt)
            let mut stream = agent
                .stream_prompt(&prompt_message)
                .multi_turn(max_depth)
                .with_history(current_history)
                .await;

            let mut accumulated_text = String::new();
            let mut tool_calls: Vec<ChatToolCall> = Vec::new();
            let mut item_count = 0u64;
            let mut got_final_response = false;

            // Track Rig's native tool calls and results for proper history
            let mut rig_tool_calls: Vec<RigToolCall> = Vec::new();
            let mut tool_results: Vec<ToolResult> = Vec::new();

            // Map tool_call.id -> tool_name for looking up names when results arrive
            // ToolResult.id matches ToolCall.id (they are the same value)
            let mut tool_id_to_name: std::collections::HashMap<String, String> = std::collections::HashMap::new();

            // Track buffered text for XML tool call detection
            // We buffer text when we detect potential XML to avoid emitting partial fragments
            let mut is_buffering_xml = false;
            // Track how much of accumulated_text we've already emitted
            let mut emitted_text_len = 0usize;
            // Track if we just parsed a tool call and should suppress trailing </function>
            let mut suppress_trailing_function_close = false;

            info!(message_len = message.len(), "Rig stream starting");

            while let Some(item) = stream.next().await {
                item_count += 1;
                debug!(item_count, "Stream item received");

                match item {
                    Ok(MultiTurnStreamItem::StreamAssistantItem(content)) => {
                        match content {
                            StreamedAssistantContent::Text(text) => {
                                // Debug: log every text chunk we receive
                                debug!(chunk = %text.text.escape_debug(), "Received text chunk");

                                accumulated_text.push_str(&text.text);

                                // Check for XML-style tool calls in text output
                                // (fallback for models that don't use native function calling)
                                let might_have_xml = xml_tool_parser::might_contain_tool_calls(&accumulated_text);

                                if might_have_xml && !is_buffering_xml {
                                    // Start buffering - don't emit partial XML
                                    is_buffering_xml = true;
                                    debug!(
                                        acc_len = accumulated_text.len(),
                                        "Detected potential XML tool call, buffering"
                                    );
                                }

                                if is_buffering_xml {
                                    // Try to parse complete tool calls
                                    let parse_result = xml_tool_parser::parse_tool_calls(&accumulated_text);

                                    if !parse_result.tool_calls.is_empty() {
                                        info!(
                                            count = parse_result.tool_calls.len(),
                                            "Parsed XML tool calls from text output"
                                        );

                                        // Emit each parsed tool call
                                        for parsed_tc in &parse_result.tool_calls {
                                            let args_json: serde_json::Value = serde_json::to_value(&parsed_tc.arguments)
                                                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                                            let xml_id = format!("xml-{}", uuid::Uuid::new_v4());
                                            tool_id_to_name.insert(xml_id.clone(), parsed_tc.name.clone());

                                            let chat_tc = ChatToolCall {
                                                name: parsed_tc.name.clone(),
                                                arguments: Some(args_json),
                                                id: Some(xml_id.clone()),
                                            };
                                            tool_calls.push(chat_tc.clone());

                                            // Check if this is a write operation in plan mode
                                            let is_write = is_write_tool_name(&parsed_tc.name);
                                            if is_write && current_mode_id == "plan" {
                                                // Block write operations in plan mode
                                                // Emit tool call first (so it shows in UI)
                                                warn!(tool = %parsed_tc.name, "Blocking XML write tool in plan mode");
                                                yield Ok(ChatChunk {
                                                    delta: String::new(),
                                                    done: false,
                                                    tool_calls: Some(vec![chat_tc]),
                                                    tool_results: None,
                                                    reasoning: None,
                                                    usage: None,
                                                    subagent_events: None,
                                                });
                                                // Immediately emit error result (shows as red failed tool)
                                                yield Ok(ChatChunk {
                                                    delta: String::new(),
                                                    done: false,
                                                    tool_calls: None,
                                                    tool_results: Some(vec![ChatToolResult {
                                                        name: parsed_tc.name.clone(),
                                                        result: String::new(),
                                                        error: Some("Blocked in plan mode".to_string()),
                                                        call_id: Some(xml_id.clone()),
                                                    }]),
                                                    reasoning: None,
                                                    usage: None,
                                                    subagent_events: None,
                                                });
                                            } else {
                                                yield Ok(ChatChunk {
                                                    delta: String::new(),
                                                    done: false,
                                                    tool_calls: Some(vec![chat_tc]),
                                                    tool_results: None,
                                                    reasoning: None,
                                                    usage: None,
                                                    subagent_events: None,
                                                });
                                            }
                                        }

                                        // Update accumulated text to cleaned version
                                        accumulated_text = parse_result.cleaned_text.clone();
                                        emitted_text_len = 0; // Reset since we replaced accumulated_text

                                        // Emit cleaned text if any remains
                                        if !parse_result.cleaned_text.is_empty() {
                                            yield Ok(ChatChunk {
                                                delta: parse_result.cleaned_text.clone(),
                                                done: false,
                                                tool_calls: None,
                                                tool_results: None,
                                                reasoning: None,
                                                usage: None,
                                                subagent_events: None,
                                            });
                                            emitted_text_len = parse_result.cleaned_text.len();
                                        }

                                        // Stop buffering - we successfully parsed
                                        is_buffering_xml = false;
                                        // Set flag to suppress trailing </function> if it comes next
                                        suppress_trailing_function_close = true;
                                    }
                                    // If no complete tool calls yet, keep buffering (don't emit)
                                } else {
                                    // Check if we should suppress trailing XML closing tags
                                    let mut text_to_emit = text.text.clone();
                                    if suppress_trailing_function_close {
                                        // Remove trailing </function> and whitespace
                                        let trimmed = text_to_emit.trim();
                                        if trimmed == "</function>" || trimmed.is_empty() {
                                            // Suppress entirely
                                            debug!("Suppressing trailing </function> after tool call");
                                            suppress_trailing_function_close = false;
                                            emitted_text_len = accumulated_text.len();
                                            // Don't yield anything, continue to next chunk
                                        } else {
                                            // Has content beyond </function>, emit it (strip </function> if present)
                                            text_to_emit = text_to_emit.replace("</function>", "");
                                            suppress_trailing_function_close = false;
                                            if !text_to_emit.trim().is_empty() {
                                                yield Ok(ChatChunk {
                                                    delta: text_to_emit,
                                                    done: false,
                                                    tool_calls: None,
                                                    tool_results: None,
                                                    reasoning: None,
                                                    usage: None,
                                                    subagent_events: None,
                                                });
                                            }
                                            emitted_text_len = accumulated_text.len();
                                        }
                                    } else {
                                        // Normal emit
                                        yield Ok(ChatChunk {
                                            delta: text.text,
                                            done: false,
                                            tool_calls: None,
                                            tool_results: None,
                                            reasoning: None,
                                            usage: None,
                                            subagent_events: None,
                                        });
                                        emitted_text_len = accumulated_text.len();
                                    }
                                }
                            }
                            StreamedAssistantContent::ToolCall(tc) => {
                                info!(
                                    tool = %tc.function.name,
                                    id = %tc.id,
                                    call_id = ?tc.call_id,
                                    "Rig tool call received"
                                );

                                tool_id_to_name.insert(tc.id.clone(), tc.function.name.clone());
                                if let Some(ref call_id) = tc.call_id {
                                    tool_id_to_name.insert(call_id.clone(), tc.function.name.clone());
                                }

                                // Track for history (always, regardless of plan mode)
                                rig_tool_calls.push(tc.clone());
                                let chat_tc = ChatToolCall {
                                    name: tc.function.name.clone(),
                                    arguments: Some(tc.function.arguments.clone()),
                                    id: tc.call_id.clone(),
                                };
                                tool_calls.push(chat_tc.clone());

                                // Check if this is a write operation in plan mode
                                let is_write_tool = is_write_tool_name(&tc.function.name);
                                if is_write_tool && current_mode_id == "plan" {
                                    // Block write operations in plan mode
                                    // Emit tool call first (so it shows in UI)
                                    warn!(tool = %tc.function.name, "Blocking write tool in plan mode");
                                    yield Ok(ChatChunk {
                                        delta: String::new(),
                                        done: false,
                                        tool_calls: Some(vec![chat_tc]),
                                        tool_results: None,
                                        reasoning: None,
                                        usage: None,
                                        subagent_events: None,
                                    });
                                    // Immediately emit error result (shows as red failed tool)
                                    yield Ok(ChatChunk {
                                        delta: String::new(),
                                        done: false,
                                        tool_calls: None,
                                        tool_results: Some(vec![ChatToolResult {
                                            name: tc.function.name.clone(),
                                            result: String::new(),
                                            error: Some("Blocked in plan mode".to_string()),
                                            call_id: tc.call_id.clone(),
                                        }]),
                                        reasoning: None,
                                        usage: None,
                                        subagent_events: None,
                                    });
                                } else {
                                    // Emit tool call immediately via tool_calls field
                                    // (not as text delta - TUI handles tool display separately)
                                    yield Ok(ChatChunk {
                                        delta: String::new(),
                                        done: false,
                                        tool_calls: Some(vec![chat_tc]),
                                        tool_results: None,
                                        reasoning: None,
                                        usage: None,
                                        subagent_events: None,
                                    });
                                }
                            }
                            StreamedAssistantContent::Reasoning(r) => {
                                // Emit complete reasoning block
                                let reasoning_text = r.reasoning.join("");
                                if !reasoning_text.is_empty() {
                                    yield Ok(ChatChunk {
                                        delta: String::new(),
                                        done: false,
                                        tool_calls: None,
                                        tool_results: None,
                                        reasoning: Some(reasoning_text),
                                        usage: None,
                                        subagent_events: None,
                                    });
                                }
                            }
                            StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                                // Emit reasoning delta separately from main content
                                if !reasoning.is_empty() {
                                    yield Ok(ChatChunk {
                                        delta: String::new(),
                                        done: false,
                                        tool_calls: None,
                                        tool_results: None,
                                        reasoning: Some(reasoning),
                                        usage: None,
                                        subagent_events: None,
                                    });
                                }
                            }
                            StreamedAssistantContent::ToolCallDelta { .. } => {
                                // Ignore deltas, we get full tool call above
                            }
                            StreamedAssistantContent::Final(_) => {
                                // Final response marker, will handle in FinalResponse
                            }
                        }
                    }
                    Ok(MultiTurnStreamItem::StreamUserItem(ui)) => {
                        use rig::streaming::StreamedUserContent;
                        use rig::message::ToolResultContent;
                        // Capture tool results for history building and emit to TUI
                        let StreamedUserContent::ToolResult(tr) = ui;

                        // Extract text from OneOrMany<ToolResultContent>
                        let result_text: String = tr.content.iter()
                            .filter_map(|c| match c {
                                ToolResultContent::Text(t) => Some(t.text.clone()),
                                ToolResultContent::Image(_) => None, // Skip images for display
                            })
                            .collect::<Vec<_>>()
                            .join("\n");

                        // Look up tool name from id mapping (ToolResult.id == ToolCall.id)
                        let tool_name = tool_id_to_name
                            .get(&tr.id)
                            .cloned()
                            .unwrap_or_else(|| tr.id.clone());

                        info!(
                            tool_name = %tool_name,
                            result_id = %tr.id,
                            call_id = ?tr.call_id,
                            result_len = result_text.len(),
                            "Tool result received"
                        );

                        // Emit tool result to TUI
                        yield Ok(ChatChunk {
                            delta: String::new(),
                            done: false,
                            tool_calls: None,
                            tool_results: Some(vec![ChatToolResult {
                                name: tool_name,
                                result: result_text,
                                error: None, // Rig doesn't distinguish error results
                                call_id: tr.call_id.clone(),
                            }]),
                            reasoning: None,
                            usage: None,
                            subagent_events: None,
                        });

                        tool_results.push(tr);
                    }
                    Ok(MultiTurnStreamItem::FinalResponse(final_resp)) => {
                        info!(
                            item_count,
                            response_len = final_resp.response().len(),
                            "Received FinalResponse from Rig stream"
                        );
                        got_final_response = true;

                        // If we were buffering XML but never got a complete tool call,
                        // emit the buffered text as-is (it wasn't a valid tool call)
                        if is_buffering_xml && emitted_text_len < accumulated_text.len() {
                            let remaining = &accumulated_text[emitted_text_len..];
                            if !remaining.is_empty() {
                                debug!(
                                    remaining_len = remaining.len(),
                                    "Emitting buffered XML that didn't parse as tool call"
                                );
                                yield Ok(ChatChunk {
                                    delta: remaining.to_string(),
                                    done: false,
                                    tool_calls: None,
                                    tool_results: None,
                                    reasoning: None,
                                    usage: None,
                                    subagent_events: None,
                                });
                            }
                        }

                        debug!(
                            item_count,
                            response_len = final_resp.response().len(),
                            tool_count = tool_calls.len(),
                            "Rig stream complete"
                        );

                        // Build proper history with text AND tool calls
                        {
                            info!("Acquiring history write lock...");
                            let mut h = history.write().await;
                            info!("History write lock acquired");

                            // Build assistant content with both text and tool calls
                            let mut assistant_content: Vec<AssistantContent> = Vec::new();

                            // Add text content if non-empty
                            let response_text_for_history = final_resp.response();
                            if !response_text_for_history.is_empty() {
                                assistant_content.push(AssistantContent::text(response_text_for_history));
                            }

                            // Add all tool calls
                            for tc in rig_tool_calls.iter() {
                                assistant_content.push(AssistantContent::ToolCall(tc.clone()));
                            }

                            // Push assistant message with combined content
                            if !assistant_content.is_empty() {
                                let content = if assistant_content.len() == 1 {
                                    OneOrMany::one(assistant_content.remove(0))
                                } else {
                                    // Safe to unwrap: we checked non-empty above
                                    OneOrMany::many(assistant_content).expect("assistant_content is non-empty")
                                };
                                h.push(Message::from(content));
                            }

                            // Add tool results as user messages
                            for tr in tool_results.iter() {
                                h.push(Message::from(tr.clone()));
                            }
                        }

                        let rig_usage = final_resp.usage();
                        let usage = Some(TokenUsage {
                            prompt_tokens: rig_usage.input_tokens as u32,
                            completion_tokens: rig_usage.output_tokens as u32,
                            total_tokens: rig_usage.total_tokens as u32,
                        });

                        info!(
                            input_tokens = rig_usage.input_tokens,
                            output_tokens = rig_usage.output_tokens,
                            total_tokens = rig_usage.total_tokens,
                            "Yielding final done=true chunk with usage"
                        );
                        yield Ok(ChatChunk {
                            delta: String::new(),
                            done: true,
                            tool_calls: None,
                            tool_results: None,
                            reasoning: None,
                            usage,
                            subagent_events: None,
                        });
                    }
                    Err(e) => {
                        warn!(
                            item_count,
                            error = ?e,
                            error_display = %e,
                            "Rig stream error"
                        );
                        yield Err(ChatError::Communication(format!("Rig LLM error: {}", e)));
                        return;
                    }
                    // Catch-all for future MultiTurnStreamItem variants (non-exhaustive enum)
                    Ok(_) => {
                        // Ignore unknown variants for forward compatibility
                    }
                }
            }

            info!(
                item_count,
                got_final_response,
                accumulated_len = accumulated_text.len(),
                tool_call_count = tool_calls.len(),
                tool_result_count = tool_results.len(),
                "Rig stream loop exited (stream.next() returned None)"
            );

            // Safety net: If stream ended without FinalResponse (e.g., network timeout,
            // unexpected termination, or empty response), ensure we still emit a done
            // chunk so TUI doesn't get stuck in "Generating..." state.
            if !got_final_response {
                warn!(
                    item_count,
                    accumulated_len = accumulated_text.len(),
                    tool_count = tool_calls.len(),
                    "Rig stream ended without FinalResponse - emitting done chunk"
                );

                yield Ok(ChatChunk {
                    delta: String::new(),
                    done: true,
                    tool_calls: None,
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                });
            }
        })
    }

    fn is_connected(&self) -> bool {
        // Rig agents are always "connected" (no persistent connection)
        true
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn get_modes(&self) -> Option<&SessionModeState> {
        Some(&self.mode_state)
    }

    fn get_mode_id(&self) -> &str {
        &self.current_mode_id
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        // Validate mode exists in our advertised modes
        let exists = self
            .mode_state
            .available_modes
            .iter()
            .any(|m| m.id.0.as_ref() == mode_id);

        if !exists {
            return Err(ChatError::InvalidMode(format!(
                "Unknown mode '{}'. Available: {:?}",
                mode_id,
                self.mode_state
                    .available_modes
                    .iter()
                    .map(|m| m.id.0.as_ref())
                    .collect::<Vec<_>>()
            )));
        }

        let mode_changed = self.current_mode_id != mode_id;

        self.current_mode_id = mode_id.to_string();
        self.mode_context_sent.store(false, Ordering::SeqCst);

        if let Some(ref ctx) = self.workspace_ctx {
            ctx.set_mode(mode_id);
        }

        if mode_changed {
            if let Some(ref mut comp) = self.components {
                comp.mode_id = mode_id.to_string();
                self.needs_rebuild.store(true, Ordering::SeqCst);
                info!(mode = %mode_id, "Mode changed, agent will rebuild with new tool set");
            }
        }

        Ok(())
    }

    fn clear_history(&mut self) {
        if let Ok(mut history) = self.chat_history.try_write() {
            history.clear();
            debug!("Cleared chat history");
        } else {
            warn!("Could not acquire write lock to clear chat history");
        }
    }

    async fn switch_model(&mut self, model_id: &str) -> ChatResult<()> {
        info!(model = %model_id, "Switching model");
        self.model_name = Some(model_id.to_string());
        if let Some(ref mut comp) = self.components {
            comp.config.model = model_id.to_string();
            self.needs_rebuild.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    fn current_model(&self) -> Option<&str> {
        self.model_name.as_deref()
    }

    async fn set_temperature(&mut self, temperature: f64) -> ChatResult<()> {
        info!(temperature = %temperature, "Setting temperature");
        self.temperature = Some(temperature);
        if let Some(ref mut comp) = self.components {
            comp.config.temperature = Some(temperature);
            self.needs_rebuild.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    fn get_temperature(&self) -> Option<f64> {
        self.temperature
    }

    async fn set_max_tokens(&mut self, max_tokens: Option<u32>) -> ChatResult<()> {
        info!(max_tokens = ?max_tokens, "Setting max tokens");
        self.max_tokens = max_tokens;
        if let Some(ref mut comp) = self.components {
            comp.config.max_tokens = max_tokens;
            self.needs_rebuild.store(true, Ordering::SeqCst);
        }
        Ok(())
    }

    fn get_max_tokens(&self) -> Option<u32> {
        self.max_tokens
    }

    async fn fetch_available_models(&mut self) -> Vec<String> {
        let endpoint = self
            .ollama_endpoint
            .as_deref()
            .or(self.reasoning_endpoint.as_deref())
            .unwrap_or("http://localhost:11434");

        let base = endpoint.trim_end_matches('/').trim_end_matches("/v1");
        let url = format!("{}/api/tags", base);
        debug!(url = %url, "Fetching available models from Ollama");

        #[derive(serde::Deserialize)]
        struct TagsResponse {
            models: Vec<ModelInfo>,
        }
        #[derive(serde::Deserialize)]
        struct ModelInfo {
            name: String,
        }

        match self.http_client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => match resp.json::<TagsResponse>().await {
                Ok(tags) => {
                    let models: Vec<String> = tags.models.into_iter().map(|m| m.name).collect();
                    info!(count = models.len(), "Fetched available models");
                    models
                }
                Err(e) => {
                    warn!(error = %e, "Failed to parse Ollama models response");
                    Vec::new()
                }
            },
            Ok(resp) => {
                warn!(status = %resp.status(), "Ollama API returned non-success status");
                Vec::new()
            }
            Err(e) => {
                warn!(error = %e, "Failed to fetch models from Ollama");
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::client::{CompletionClient, Nothing};
    use rig::providers::ollama;

    fn create_test_agent() -> Agent<ollama::CompletionModel> {
        let client = create_test_client();

        client
            .agent("llama3.2")
            .preamble("You are a test assistant.")
            .build()
    }

    fn create_test_client() -> ollama::Client {
        ollama::Client::builder().api_key(Nothing).build().unwrap()
    }

    fn create_remote_client() -> ollama::Client {
        ollama::Client::builder()
            .api_key(Nothing)
            .base_url("https://llama.krohnos.io")
            .build()
            .unwrap()
    }

    fn create_openai_compatible_client() -> rig::providers::openai::CompletionsClient {
        // Use CompletionsClient for standard /chat/completions API
        // (not the new OpenAI "responses" API)
        rig::providers::openai::CompletionsClient::builder()
            .api_key("not-needed")
            .base_url("https://llama.krohnos.io/v1")
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn test_rig_agent_handle_creation() {
        let agent = create_test_agent();
        let handle = RigAgentHandle::new(agent);

        assert!(handle.is_connected());
        assert_eq!(handle.get_mode_id(), "normal");
        assert!(handle.get_modes().is_some());
    }

    #[tokio::test]
    async fn test_rig_agent_handle_mode_switching() {
        let agent = create_test_agent();
        let mut handle = RigAgentHandle::new(agent);

        assert_eq!(handle.get_mode_id(), "normal");

        handle.set_mode_str("plan").await.unwrap();
        assert_eq!(handle.get_mode_id(), "plan");

        handle.set_mode_str("auto").await.unwrap();
        assert_eq!(handle.get_mode_id(), "auto");

        // Invalid mode should error
        let result = handle.set_mode_str("invalid").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rig_agent_handle_history_management() {
        let agent = create_test_agent();
        let handle = RigAgentHandle::new(agent);

        // Initially empty
        assert!(handle.get_history().await.is_empty());

        // Can set initial history
        let agent2 = create_test_agent();
        let handle2 = RigAgentHandle::new(agent2).with_history(vec![Message::user("Hello")]);

        assert_eq!(handle2.get_history().await.len(), 1);
    }

    #[tokio::test]
    async fn test_rig_agent_handle_with_max_depth() {
        let agent = create_test_agent();
        let handle = RigAgentHandle::new(agent).with_max_tool_depth(5);

        assert_eq!(handle.max_tool_depth, 5);
    }

    // Integration test requiring running Ollama
    #[tokio::test]
    #[ignore = "requires running Ollama"]
    async fn test_rig_agent_handle_streaming() {
        let agent = create_test_agent();
        let mut handle = RigAgentHandle::new(agent);

        let mut stream = handle.send_message_stream("Say hello".to_string());
        let mut chunks = Vec::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(chunk) => chunks.push(chunk),
                Err(e) => panic!("Stream error: {}", e),
            }
        }

        // Should have at least one chunk
        assert!(!chunks.is_empty());

        // Last chunk should be done
        assert!(chunks.last().unwrap().done);

        // History should be updated
        let history = handle.get_history().await;
        assert_eq!(history.len(), 2); // User + Assistant
    }

    // Test streaming with tools - reproduce the empty response issue
    #[tokio::test]
    #[ignore = "requires running Ollama with tool-capable model"]
    async fn test_rig_agent_streaming_with_tools() {
        use crate::workspace_tools::{ReadFileTool, WorkspaceContext};
        use rig::streaming::StreamingPrompt;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let ctx = WorkspaceContext::new(temp.path());

        // Create a test file
        std::fs::write(temp.path().join("test.txt"), "Hello from test").unwrap();

        // Build agent with tool
        let client = create_remote_client();

        let agent = client
            .agent("qwen3-4b-instruct-2507-q8_0")
            .preamble("You are a helpful assistant. Use the read_file tool to read files.")
            .tool(ReadFileTool::new(ctx))
            .build();

        println!("=== Testing direct Rig streaming (bypassing RigAgentHandle) ===");

        // Test directly with Rig's stream_prompt
        let mut stream = agent
            .stream_prompt("Read the file test.txt")
            .multi_turn(5)
            .await;

        let mut items = Vec::new();
        while let Some(item) = stream.next().await {
            match item {
                Ok(item) => {
                    println!("Direct item: {:?}", item);
                    items.push(format!("{:?}", item));
                }
                Err(e) => {
                    println!("Direct error: {:?}", e);
                    panic!("Stream error: {:?}", e);
                }
            }
        }

        println!("Got {} items", items.len());

        // The issue: if we get an empty FinalResponse immediately, something is wrong
        assert!(
            items.len() > 1 || !items[0].contains("FinalResponse"),
            "Expected more than just an empty FinalResponse. Got: {:?}",
            items
        );
    }

    // Test streaming WITHOUT tools to verify basic streaming works
    #[tokio::test]
    #[ignore = "requires running Ollama"]
    async fn test_rig_agent_streaming_without_tools() {
        use rig::streaming::StreamingPrompt;

        let client = create_remote_client();

        let agent = client
            .agent("qwen3-4b-instruct-2507-q8_0")
            .preamble("You are a helpful assistant.")
            .build();

        println!("=== Testing streaming WITHOUT tools ===");

        let mut stream = agent.stream_prompt("Say hello in one word").await;

        let mut items = Vec::new();
        while let Some(item) = stream.next().await {
            match item {
                Ok(item) => {
                    println!("Item: {:?}", item);
                    items.push(format!("{:?}", item));
                }
                Err(e) => {
                    println!("Error: {:?}", e);
                    panic!("Stream error: {:?}", e);
                }
            }
        }

        println!("Got {} items", items.len());
        assert!(items.len() > 1, "Expected multiple stream items");
    }

    // Test NON-streaming with tools to verify tools work without streaming
    #[tokio::test]
    #[ignore = "requires running Ollama with tool-capable model"]
    async fn test_rig_agent_prompt_with_tools() {
        use crate::workspace_tools::{ReadFileTool, WorkspaceContext};
        use rig::completion::Prompt;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let ctx = WorkspaceContext::new(temp.path());

        // Create a test file
        std::fs::write(temp.path().join("test.txt"), "Hello from test").unwrap();

        let client = create_remote_client();

        let agent = client
            .agent("qwen3-4b-instruct-2507-q8_0")
            .preamble("You are a helpful assistant. Use the read_file tool to read files.")
            .tool(ReadFileTool::new(ctx))
            .build();

        println!("=== Testing NON-streaming prompt with tools ===");

        match agent.prompt("Read the file test.txt").await {
            Ok(response) => {
                println!("Response: {}", response);
                assert!(
                    !response.is_empty(),
                    "Expected non-empty response when using tools"
                );
            }
            Err(e) => {
                println!("Error: {:?}", e);
                panic!("Prompt error: {:?}", e);
            }
        }
    }

    // Test OpenAI-compatible endpoint for streaming with tools
    #[tokio::test]
    #[ignore = "requires llama.cpp with OpenAI-compatible endpoint"]
    async fn test_rig_openai_streaming_with_tools() {
        use crate::workspace_tools::{ReadFileTool, WorkspaceContext};
        use rig::streaming::StreamingPrompt;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let ctx = WorkspaceContext::new(temp.path());

        // Create a test file
        std::fs::write(temp.path().join("test.txt"), "Hello from test").unwrap();

        // Use OpenAI provider with custom endpoint (llama.cpp)
        let client = create_openai_compatible_client();

        let agent = client
            .agent("qwen3-4b-instruct-2507-q8_0")
            .preamble("You are a helpful assistant. Use the read_file tool to read files.")
            .tool(ReadFileTool::new(ctx))
            .build();

        println!("=== Testing OpenAI-compatible streaming with tools ===");

        let mut stream = agent
            .stream_prompt("Read the file test.txt")
            .multi_turn(5)
            .await;

        use rig::agent::MultiTurnStreamItem;

        let mut item_count = 0;
        let mut got_tool_call = false;
        let mut got_final = false;
        let mut got_text = false;

        while let Some(item) = stream.next().await {
            match item {
                Ok(MultiTurnStreamItem::StreamAssistantItem(content)) => {
                    item_count += 1;
                    use rig::streaming::StreamedAssistantContent;
                    match content {
                        StreamedAssistantContent::Text(t) => {
                            println!("Text: {}", t.text);
                            got_text = true;
                        }
                        StreamedAssistantContent::ToolCall(tc) => {
                            println!("ToolCall: {} ({:?})", tc.function.name, tc.call_id);
                            got_tool_call = true;
                        }
                        _ => {
                            println!("Other content");
                        }
                    }
                }
                Ok(MultiTurnStreamItem::FinalResponse(final_resp)) => {
                    println!("FinalResponse: {}", final_resp.response());
                    got_final = true;
                    item_count += 1;
                }
                Ok(MultiTurnStreamItem::StreamUserItem(_)) => {
                    println!("StreamUserItem (tool result)");
                    item_count += 1;
                }
                Ok(_) => {
                    println!("Unknown item");
                    item_count += 1;
                }
                Err(e) => {
                    println!("Error: {:?}", e);
                    panic!("Streaming error: {:?}", e);
                }
            }
        }

        println!(
            "Got {} items, tool_call={}, final={}, text={}",
            item_count, got_tool_call, got_final, got_text
        );

        // Should have received tool calls and final response
        assert!(got_tool_call, "Expected to receive tool calls");
        assert!(got_final, "Expected to receive final response");
    }

    // Test multi-turn tool calling to reproduce 400 error
    //
    // ROOT CAUSE IDENTIFIED (Rig bug):
    // - Ollama returns tool calls with an `id` field
    // - Rig's ToolCall struct doesn't capture `id` (ollama.rs line 717-722)
    // - Line 679 uses String::new() as placeholder instead of the actual id
    // - This empty string becomes tool_name in subsequent requests
    // - Ollama rejects requests with empty tool_name
    //
    // Fix needed in rig-core:
    // 1. Add `id` field to ollama::ToolCall struct
    // 2. Use tool_call.id instead of String::new() in line 679
    //
    // See: https://github.com/0xPlaygrounds/rig/issues/XXXX
    #[tokio::test]
    #[ignore = "requires running Ollama with tool-capable model"]
    async fn test_rig_agent_multi_turn_tool_calls() {
        use crate::workspace_tools::{BashTool, WorkspaceContext};
        use rig::agent::MultiTurnStreamItem;
        use rig::streaming::{StreamedAssistantContent, StreamingPrompt};
        use tempfile::TempDir;
        use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

        // Initialize tracing to see request details
        let _ = tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_target(true))
            .with(tracing_subscriber::EnvFilter::new("rig=trace"))
            .try_init();

        let temp = TempDir::new().unwrap();
        let ctx = WorkspaceContext::new(temp.path());

        // Use OpenAI-compatible client for better tool calling support
        let client = create_openai_compatible_client();

        // Use a model that tends to make multiple tool calls
        let agent = client
            .agent("Qwen3-Coder-30B-A3B-Instruct-UD-IQ4_NL")
            .preamble(
                "You are a helpful assistant. You MUST use tools to answer questions. \
                 Always run commands to verify your answers.",
            )
            .tool(BashTool::new(ctx))
            .build();

        println!("=== Testing multi-turn tool calls ===");

        // This prompt is designed to trigger multiple tool calls
        let mut stream = agent
            .stream_prompt(
                "Run 'pwd' to check the current directory, then run 'ls' to list files. \
                 Report both results.",
            )
            .multi_turn(10) // Allow many turns
            .await;

        let mut round = 0;
        let mut item_count = 0;
        let mut tool_call_count = 0;
        let mut text_chunks = Vec::new();
        let mut errors = Vec::new();

        while let Some(item) = stream.next().await {
            item_count += 1;
            match item {
                Ok(MultiTurnStreamItem::StreamAssistantItem(content)) => match content {
                    StreamedAssistantContent::Text(t) => {
                        text_chunks.push(t.text.clone());
                        print!("{}", t.text);
                    }
                    StreamedAssistantContent::ToolCall(tc) => {
                        tool_call_count += 1;
                        println!(
                            "\n[Round {} - Tool call #{}: {}]",
                            round, tool_call_count, tc.function.name
                        );
                    }
                    StreamedAssistantContent::Final(_) => {
                        round += 1;
                        println!("\n[Round {} complete]", round);
                    }
                    _ => {}
                },
                Ok(MultiTurnStreamItem::StreamUserItem(_)) => {
                    println!("[Tool result received]");
                }
                Ok(MultiTurnStreamItem::FinalResponse(final_resp)) => {
                    println!("\n[Final response: {} chars]", final_resp.response().len());
                }
                Ok(_) => {}
                Err(e) => {
                    println!("\n[ERROR at round {}, item {}: {:?}]", round, item_count, e);
                    errors.push(format!("{:?}", e));
                }
            }
        }

        println!("\n=== Summary ===");
        println!(
            "Items: {}, Rounds: {}, Tool calls: {}",
            item_count, round, tool_call_count
        );
        println!("Errors: {:?}", errors);

        // The test should pass without 400 errors
        assert!(
            errors.is_empty(),
            "Expected no errors during multi-turn tool calls. Got: {:?}",
            errors
        );

        // Should have made at least 2 tool calls (pwd and ls)
        assert!(
            tool_call_count >= 2,
            "Expected at least 2 tool calls, got {}",
            tool_call_count
        );
    }

    // Test streaming with tools WITHOUT multi_turn()
    #[tokio::test]
    #[ignore = "requires running Ollama with tool-capable model"]
    async fn test_rig_agent_streaming_tools_no_multiturn() {
        use crate::workspace_tools::{ReadFileTool, WorkspaceContext};
        use rig::streaming::StreamingPrompt;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let ctx = WorkspaceContext::new(temp.path());

        // Create a test file
        std::fs::write(temp.path().join("test.txt"), "Hello from test").unwrap();

        let client = create_remote_client();

        let agent = client
            .agent("qwen3-4b-instruct-2507-q8_0")
            .preamble("You are a helpful assistant. Use the read_file tool to read files.")
            .tool(ReadFileTool::new(ctx))
            .build();

        println!("=== Testing streaming with tools WITHOUT multi_turn() ===");

        // Try streaming without multi_turn() to see if that's the issue
        let mut stream = agent.stream_prompt("Read the file test.txt").await;

        let mut items = Vec::new();
        while let Some(item) = stream.next().await {
            match item {
                Ok(item) => {
                    println!("Item: {:?}", item);
                    items.push(format!("{:?}", item));
                }
                Err(e) => {
                    println!("Error: {:?}", e);
                    // Don't panic - just note the error
                    items.push(format!("Error: {:?}", e));
                }
            }
        }

        println!("Got {} items", items.len());
        // Just print what we got - this is exploratory
        for (i, item) in items.iter().enumerate() {
            println!("  [{}] {}", i, item);
        }
    }

    // Test custom reasoning stream extraction
    #[tokio::test]
    #[ignore = "requires remote endpoint with thinking model"]
    async fn test_reasoning_stream_extraction() {
        use crate::openai_reasoning::{stream_with_reasoning, ReasoningChunk, ReasoningOptions};

        let client = reqwest::Client::new();
        let endpoint = "https://llama.krohnos.io/v1";
        let model = "qwen3-4b-thinking-2507-q8_0";
        let messages = vec![serde_json::json!({
            "role": "user",
            "content": "What is 2+2?"
        })];

        println!("=== Testing reasoning extraction ===");

        let options = ReasoningOptions::default();
        let mut stream = stream_with_reasoning(client, endpoint, model, messages, options);

        let mut reasoning_chunks = 0u32;
        let mut text_chunks = 0u32;
        let mut reasoning_text = String::new();
        let mut response_text = String::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(chunk) => match chunk {
                    ReasoningChunk::Text(t) => {
                        text_chunks += 1;
                        response_text.push_str(&t);
                    }
                    ReasoningChunk::Reasoning(r) => {
                        reasoning_chunks += 1;
                        reasoning_text.push_str(&r);
                    }
                    ReasoningChunk::Done => {
                        println!("Stream complete");
                        break;
                    }
                    ReasoningChunk::ToolCall { .. } => {}
                },
                Err(e) => {
                    panic!("Stream error: {}", e);
                }
            }
        }

        println!("Reasoning chunks: {}", reasoning_chunks);
        println!("Text chunks: {}", text_chunks);
        println!(
            "Reasoning: {}",
            &reasoning_text[..reasoning_text.len().min(200)]
        );
        println!("Response: {}", response_text);

        // With a thinking model, we should get reasoning content
        assert!(
            reasoning_chunks > 0,
            "Expected reasoning chunks from thinking model"
        );
        assert!(text_chunks > 0, "Expected text chunks for response");
        assert!(
            response_text.contains("4"),
            "Response should contain the answer"
        );
    }

    /// Helper to check if history has consecutive assistant messages
    /// Returns (has_consecutive, description) where description explains the issue
    fn check_no_consecutive_assistant_messages(history: &[Message]) -> (bool, String) {
        let mut last_was_assistant = false;
        let mut consecutive_count = 0;

        for (i, msg) in history.iter().enumerate() {
            let is_assistant = matches!(msg, Message::Assistant { .. });

            if is_assistant {
                if last_was_assistant {
                    consecutive_count += 1;
                    return (
                        false,
                        format!(
                            "Found {} consecutive assistant messages ending at index {}",
                            consecutive_count + 1,
                            i
                        ),
                    );
                }
                last_was_assistant = true;
            } else {
                last_was_assistant = false;
                consecutive_count = 0;
            }
        }

        // Also check if history ends with consecutive assistant messages
        // (the specific error: "Cannot have 2 or more assistant messages at the end")
        let mut trailing_assistant_count = 0;
        for msg in history.iter().rev() {
            if matches!(msg, Message::Assistant { .. }) {
                trailing_assistant_count += 1;
            } else {
                break;
            }
        }

        if trailing_assistant_count > 1 {
            return (
                false,
                format!(
                    "History ends with {} consecutive assistant messages",
                    trailing_assistant_count
                ),
            );
        }

        (true, "No consecutive assistant messages found".to_string())
    }

    /// Helper to describe message roles in history
    fn describe_history(history: &[Message]) -> String {
        history
            .iter()
            .enumerate()
            .map(|(i, msg)| {
                let role = match msg {
                    Message::User { .. } => "User",
                    Message::Assistant { .. } => "Assistant",
                };
                format!("[{}] {}", i, role)
            })
            .collect::<Vec<_>>()
            .join(" -> ")
    }

    // Test that validates message ordering after multi-turn tool conversations
    #[tokio::test]
    #[ignore = "requires running Ollama with tool-capable model"]
    async fn test_message_ordering_after_tool_calls() {
        use crate::workspace_tools::{BashTool, ReadFileTool, WorkspaceContext};
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let ctx = WorkspaceContext::new(temp.path());

        // Create test files
        std::fs::write(temp.path().join("file1.txt"), "Content of file 1").unwrap();
        std::fs::write(temp.path().join("file2.txt"), "Content of file 2").unwrap();

        let client = create_openai_compatible_client();

        let agent = client
            .agent("qwen3-4b-instruct-2507-q8_0")
            .preamble(
                "You are a helpful assistant with file and shell tools. \
                 Use tools when asked to read files or run commands.",
            )
            .tool(ReadFileTool::new(ctx.clone()))
            .tool(BashTool::new(ctx))
            .build();

        let mut handle = RigAgentHandle::new(agent);

        println!("=== Testing message ordering after tool calls ===\n");

        // Turn 1: Simple question (no tools)
        println!("--- Turn 1: Simple greeting ---");
        {
            let mut stream = handle.send_message_stream("Say hi briefly.".to_string());
            while let Some(result) = stream.next().await {
                match result {
                    Ok(chunk) if chunk.done => break,
                    Ok(chunk) => print!("{}", chunk.delta),
                    Err(e) => {
                        println!("\nError: {:?}", e);
                        break;
                    }
                }
            }
            println!();
        }

        let history1 = handle.get_history().await;
        println!("History after turn 1: {}", describe_history(&history1));
        let (ok1, msg1) = check_no_consecutive_assistant_messages(&history1);
        println!(
            "Ordering check: {} - {}\n",
            if ok1 { "✓" } else { "✗" },
            msg1
        );
        assert!(ok1, "Turn 1 failed: {}", msg1);

        // Turn 2: Request tool use (read file)
        println!("--- Turn 2: Read file (tool call) ---");
        {
            let mut stream = handle.send_message_stream("Read file1.txt".to_string());
            while let Some(result) = stream.next().await {
                match result {
                    Ok(chunk) => {
                        if let Some(ref tcs) = chunk.tool_calls {
                            for tc in tcs {
                                println!("[Tool: {}]", tc.name);
                            }
                        }
                        if let Some(ref trs) = chunk.tool_results {
                            for tr in trs {
                                println!("[Result: {} bytes]", tr.result.len());
                            }
                        }
                        if !chunk.delta.is_empty() {
                            print!("{}", chunk.delta);
                        }
                        if chunk.done {
                            break;
                        }
                    }
                    Err(e) => {
                        println!("\nError: {:?}", e);
                        break;
                    }
                }
            }
            println!();
        }

        let history2 = handle.get_history().await;
        println!("History after turn 2: {}", describe_history(&history2));
        let (ok2, msg2) = check_no_consecutive_assistant_messages(&history2);
        println!(
            "Ordering check: {} - {}\n",
            if ok2 { "✓" } else { "✗" },
            msg2
        );
        assert!(ok2, "Turn 2 failed: {}", msg2);

        // Turn 3: Another question (should still work)
        println!("--- Turn 3: Follow-up question ---");
        {
            let mut stream = handle.send_message_stream("What did the file contain?".to_string());
            while let Some(result) = stream.next().await {
                match result {
                    Ok(chunk) if chunk.done => break,
                    Ok(chunk) => print!("{}", chunk.delta),
                    Err(e) => {
                        println!("\nError: {:?}", e);
                        break;
                    }
                }
            }
            println!();
        }

        let history3 = handle.get_history().await;
        println!("History after turn 3: {}", describe_history(&history3));
        let (ok3, msg3) = check_no_consecutive_assistant_messages(&history3);
        println!(
            "Ordering check: {} - {}\n",
            if ok3 { "✓" } else { "✗" },
            msg3
        );
        assert!(ok3, "Turn 3 failed: {}", msg3);

        // Turn 4: Multiple tool calls in one turn
        println!("--- Turn 4: Multiple tool calls ---");
        {
            let mut stream =
                handle.send_message_stream("Run 'pwd' and then read file2.txt".to_string());
            while let Some(result) = stream.next().await {
                match result {
                    Ok(chunk) => {
                        if let Some(ref tcs) = chunk.tool_calls {
                            for tc in tcs {
                                println!("[Tool: {}]", tc.name);
                            }
                        }
                        if let Some(ref trs) = chunk.tool_results {
                            for tr in trs {
                                println!("[Result: {} bytes]", tr.result.len());
                            }
                        }
                        if !chunk.delta.is_empty() {
                            print!("{}", chunk.delta);
                        }
                        if chunk.done {
                            break;
                        }
                    }
                    Err(e) => {
                        println!("\nError: {:?}", e);
                        break;
                    }
                }
            }
            println!();
        }

        let history4 = handle.get_history().await;
        println!("History after turn 4: {}", describe_history(&history4));
        let (ok4, msg4) = check_no_consecutive_assistant_messages(&history4);
        println!(
            "Ordering check: {} - {}\n",
            if ok4 { "✓" } else { "✗" },
            msg4
        );
        assert!(ok4, "Turn 4 failed: {}", msg4);

        println!("=== All turns passed message ordering check ===");
    }

    // Unit test for the helper function itself
    #[test]
    fn test_consecutive_message_detection() {
        use rig::message::AssistantContent;

        // Valid: User -> Assistant
        let valid1 = vec![Message::user("hi"), Message::assistant("hello")];
        let (ok, _) = check_no_consecutive_assistant_messages(&valid1);
        assert!(ok, "User -> Assistant should be valid");

        // Valid: User -> Assistant -> User -> Assistant
        let valid2 = vec![
            Message::user("q1"),
            Message::assistant("a1"),
            Message::user("q2"),
            Message::assistant("a2"),
        ];
        let (ok, _) = check_no_consecutive_assistant_messages(&valid2);
        assert!(ok, "Alternating should be valid");

        // Invalid: User -> Assistant -> Assistant
        let invalid1 = vec![
            Message::user("q"),
            Message::assistant("a1"),
            Message::assistant("a2"),
        ];
        let (ok, msg) = check_no_consecutive_assistant_messages(&invalid1);
        assert!(!ok, "Consecutive assistants should be invalid: {}", msg);

        // Invalid: Ends with multiple assistants
        let invalid2 = vec![
            Message::user("q1"),
            Message::assistant("a1"),
            Message::user("q2"),
            Message::assistant("a2"),
            Message::assistant("a3"),
        ];
        let (ok, msg) = check_no_consecutive_assistant_messages(&invalid2);
        assert!(
            !ok,
            "Trailing consecutive assistants should be invalid: {}",
            msg
        );

        // Valid with tool results (tool results are User messages in Rig)
        // User -> Assistant (with tool call) -> User (tool result) -> Assistant
        let tool_function = rig::message::ToolFunction {
            name: "test".to_string(),
            arguments: serde_json::json!({}),
        };
        let tool_call = rig::message::ToolCall::new("tc1".to_string(), tool_function);
        let valid3 = vec![
            Message::user("read file"),
            Message::from(rig::OneOrMany::one(AssistantContent::ToolCall(tool_call))),
            Message::user("tool result here"), // Tool results become user messages
            Message::assistant("here is the content"),
        ];
        let (ok, _) = check_no_consecutive_assistant_messages(&valid3);
        assert!(ok, "Tool call flow should be valid");
    }

    /// Test that model switching is wired correctly and affects the streaming path
    #[tokio::test]
    async fn test_model_switching_configuration() {
        let agent = create_test_agent();

        // Create handle WITHOUT endpoints - should NOT use custom streaming
        let handle = RigAgentHandle::new(agent);
        assert!(handle.ollama_endpoint.is_none());
        assert!(handle.reasoning_endpoint.is_none());
        assert!(handle.model_name.is_none());

        // Create new handle WITH endpoints - should use custom streaming
        let agent2 = create_test_agent();
        let handle2 = RigAgentHandle::new(agent2)
            .with_ollama_endpoint("https://llama.krohnos.io".to_string())
            .with_model("initial-model".to_string());

        assert!(handle2.ollama_endpoint.is_some());
        assert_eq!(
            handle2.ollama_endpoint.as_deref(),
            Some("https://llama.krohnos.io")
        );
        assert_eq!(handle2.model_name.as_deref(), Some("initial-model"));

        // After switch_model, model_name should change
        let agent3 = create_test_agent();
        let mut handle3 = RigAgentHandle::new(agent3)
            .with_ollama_endpoint("https://llama.krohnos.io".to_string())
            .with_model("initial-model".to_string());

        handle3.switch_model("new-model").await.unwrap();
        assert_eq!(handle3.model_name.as_deref(), Some("new-model"));

        // current_model() should reflect the switch
        assert_eq!(handle3.current_model(), Some("new-model"));
    }

    /// Integration test: verify custom streaming path is taken when endpoints are set
    #[tokio::test]
    #[ignore = "requires remote Ollama endpoint"]
    async fn test_model_switching_uses_custom_streaming() {
        use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

        let _ = tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_target(true))
            .with(tracing_subscriber::EnvFilter::new("crucible_rig=debug"))
            .try_init();

        let client = create_openai_compatible_client();
        let agent = client
            .agent("gpt-oss-20b-q8_k_xl")
            .preamble("You are a test assistant.")
            .build();

        let mut handle = RigAgentHandle::new(agent)
            .with_ollama_endpoint("https://llama.krohnos.io".to_string())
            .with_model("gpt-oss-20b-q8_k_xl".to_string());

        // Switch to a different model
        handle.switch_model("glm-4.7-flash-q8_0").await.unwrap();
        assert_eq!(handle.current_model(), Some("glm-4.7-flash-q8_0"));

        // Send a message - this should use the NEW model via custom streaming
        let mut stream =
            handle.send_message_stream("Say just 'test' and nothing else.".to_string());

        let mut got_response = false;
        while let Some(result) = stream.next().await {
            match result {
                Ok(chunk) => {
                    if !chunk.delta.is_empty() {
                        println!("Got delta: {}", chunk.delta);
                        got_response = true;
                    }
                    if chunk.done {
                        break;
                    }
                }
                Err(e) => {
                    panic!("Stream error: {}", e);
                }
            }
        }

        assert!(got_response, "Should have received response from new model");
    }

    #[tokio::test]
    async fn test_model_switching_with_components_sets_rebuild_flag() {
        use crate::agent::{AgentComponents, AgentConfig};
        use crate::providers::RigClient;
        use crate::workspace_tools::WorkspaceContext;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let client = create_test_client();
        let agent = client.agent("llama3.2").preamble("Test").build();

        let config = AgentConfig::new("llama3.2", "Test");
        let ws_ctx = WorkspaceContext::new(temp.path());
        let components = AgentComponents::new(config, RigClient::Ollama(client), ws_ctx);

        let mut handle = RigAgentHandle::new(agent).with_ollama_components(components);

        assert!(!handle.needs_rebuild.load(Ordering::SeqCst));
        assert_eq!(handle.current_model(), Some("llama3.2"));

        handle.switch_model("qwen3-8b").await.unwrap();

        assert!(handle.needs_rebuild.load(Ordering::SeqCst));
        assert_eq!(handle.current_model(), Some("qwen3-8b"));
    }

    #[tokio::test]
    async fn test_model_switching_preserves_history() {
        use crate::agent::{AgentComponents, AgentConfig};
        use crate::providers::RigClient;
        use crate::workspace_tools::WorkspaceContext;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let client = create_test_client();
        let agent = client.agent("llama3.2").preamble("Test").build();

        let config = AgentConfig::new("llama3.2", "Test");
        let ws_ctx = WorkspaceContext::new(temp.path());
        let components = AgentComponents::new(config, RigClient::Ollama(client), ws_ctx);

        let mut handle = RigAgentHandle::new(agent)
            .with_ollama_components(components)
            .with_history(vec![
                Message::user("Hello"),
                Message::assistant("Hi there!"),
            ]);

        assert_eq!(handle.get_history().await.len(), 2);

        handle.switch_model("qwen3-8b").await.unwrap();

        assert_eq!(
            handle.get_history().await.len(),
            2,
            "History should be preserved after model switch"
        );
    }

    #[tokio::test]
    async fn test_without_components_model_switch_does_not_set_rebuild() {
        let agent = create_test_agent();
        let mut handle = RigAgentHandle::new(agent);

        handle.switch_model("new-model").await.unwrap();

        assert!(
            !handle.needs_rebuild.load(Ordering::SeqCst),
            "Without components, rebuild flag should not be set"
        );
        assert_eq!(handle.current_model(), Some("new-model"));
    }
}
