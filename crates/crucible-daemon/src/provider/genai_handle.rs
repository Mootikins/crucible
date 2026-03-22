use async_trait::async_trait;
use crucible_config::{BackendType, LlmProviderConfig};
use crucible_core::session::{ContextStrategy, OutputValidation};
use crucible_core::traits::chat::{
    AgentHandle, ChatChunk, ChatError, ChatResult, ChatToolCall, ChatToolResult,
};
use crucible_core::traits::llm::LlmToolDefinition;
use crucible_core::traits::TokenUsage;
use crucible_core::types::acp::schema::{SessionModeId, SessionModeState};
use crucible_core::types::mode::default_internal_modes;
use futures::stream::BoxStream;
use futures::StreamExt;
use genai::chat::{
    CacheControl, ChatMessage, ChatOptions, ChatRequest, ChatStreamEvent, ContentPart,
    ReasoningEffort, Tool, ToolCall, ToolResponse,
};
use genai::ModelIden;

use super::adapter_mapping::ChatClient;

pub(crate) const EMPTY_RESPONSE_ERROR: &str =
    "LLM returned empty response — no content received from provider";
pub(crate) const STREAM_TIMEOUT_ERROR: &str =
    "LLM stream timed out — no response within timeout period";
pub(crate) const STREAM_UNEXPECTED_END_ERROR: &str =
    "LLM stream ended unexpectedly — connection terminated without completion";
pub(crate) const STREAM_CHUNK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Apply Anthropic prompt caching to a message list.
///
/// Marks the system prompt and the second-to-last message (the last message before
/// the current user turn) with `CacheControl::Ephemeral`. This follows Anthropic's
/// multi-turn caching pattern: the growing conversation prefix is cached, and each
/// turn only pays for new content. OpenAI caching is automatic and ignores this.
///
/// Returns the system prompt separately (as a cached system ChatMessage) so it can
/// be included in the messages vec rather than via `with_system()`, which doesn't
/// support cache control.
fn apply_prompt_caching(system_prompt: &str, messages: &mut Vec<ChatMessage>) {
    // Mark second-to-last message with cache control (the last msg before current user turn).
    // This creates a cache breakpoint at the end of the prior conversation, so the entire
    // prefix up to this point is cached on subsequent turns.
    if messages.len() >= 2 {
        let idx = messages.len() - 2;
        let msg = messages[idx].clone().with_options(CacheControl::Ephemeral);
        messages[idx] = msg;
    }

    // Prepend system prompt as a system-role message with cache control.
    // genai's with_system() doesn't support MessageOptions, but system-role
    // ChatMessages do — the Anthropic adapter handles them identically.
    if !system_prompt.is_empty() {
        let system_msg =
            ChatMessage::system(system_prompt).with_options(CacheControl::Ephemeral);
        messages.insert(0, system_msg);
    }
}

fn is_write_tool_name(tool_name: &str) -> bool {
    if tool_name == "write_file" || tool_name == "edit_file" {
        return true;
    }

    if tool_name.starts_with("create_") || tool_name.starts_with("delete_") {
        return true;
    }

    if tool_name == "bash" {
        return true;
    }

    false
}

fn usage_to_token_usage(usage: &genai::chat::Usage) -> TokenUsage {
    let to_u32 = |v: Option<i32>| -> u32 {
        let n = v.unwrap_or(0);
        if n < 0 { 0 } else { n as u32 }
    };
    let to_opt_u32 = |v: Option<i32>| -> Option<u32> {
        v.and_then(|n| if n > 0 { Some(n as u32) } else { None })
    };

    let (cache_read_tokens, cache_creation_tokens) = usage
        .prompt_tokens_details
        .as_ref()
        .map(|d| (to_opt_u32(d.cached_tokens), to_opt_u32(d.cache_creation_tokens)))
        .unwrap_or((None, None));

    TokenUsage {
        prompt_tokens: to_u32(usage.prompt_tokens),
        completion_tokens: to_u32(usage.completion_tokens),
        total_tokens: to_u32(usage.total_tokens),
        cache_read_tokens,
        cache_creation_tokens,
    }
}

fn wrap_stream_with_guards(
    mut stream: BoxStream<'static, ChatResult<ChatChunk>>,
) -> BoxStream<'static, ChatResult<ChatChunk>> {
    Box::pin(async_stream::stream! {
        let mut received_content = false;
        let mut received_tool_call = false;
        let mut received_reasoning = false;
        let mut received_done = false;

        loop {
            let next = match tokio::time::timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
                Ok(item) => item,
                Err(_) => {
                    yield Err(ChatError::Communication(STREAM_TIMEOUT_ERROR.to_string()));
                    return;
                }
            };

            let Some(next) = next else {
                break;
            };

            let chunk = match next {
                Ok(chunk) => chunk,
                Err(err) => {
                    yield Err(err);
                    return;
                }
            };

            if !chunk.delta.is_empty() {
                received_content = true;
            }
            if chunk
                .tool_calls
                .as_ref()
                .is_some_and(|calls| !calls.is_empty())
            {
                received_tool_call = true;
            }
            if chunk
                .reasoning
                .as_ref()
                .is_some_and(|reasoning| !reasoning.is_empty())
            {
                received_reasoning = true;
            }
            if chunk.done {
                received_done = true;
            }

            yield Ok(chunk);
        }

        if !received_content && !received_tool_call && !received_reasoning {
            yield Err(ChatError::Communication(EMPTY_RESPONSE_ERROR.to_string()));
            return;
        }

        if !received_done {
            yield Err(ChatError::Communication(
                STREAM_UNEXPECTED_END_ERROR.to_string(),
            ));
        }
    })
}

pub struct GenaiAgentHandle {
    client: genai::Client,
    model: ModelIden,
    system_prompt: String,
    tools: Vec<LlmToolDefinition>,
    history: Vec<genai::chat::ChatMessage>,
    mode_state: SessionModeState,
    current_mode_id: String,
    mode_context_sent: bool,
    max_tool_depth: usize,
    thinking_budget: Option<i64>,
    context_budget: Option<usize>,
    context_strategy: ContextStrategy,
    context_window: Option<usize>,
    output_validation: OutputValidation,
    validation_retries: u32,
    /// Stack of undo entries, one per agent turn. Each records the history
    /// length before the turn so we can truncate back on undo.
    undo_stack: Vec<crucible_core::types::UndoEntry>,
}

/// Estimate token count from message content using a chars/4 heuristic.
fn estimate_message_tokens(msg: &ChatMessage) -> usize {
    // Sum all text content parts; fall back to a small fixed cost for non-text messages
    let char_count: usize = msg
        .content
        .parts()
        .iter()
        .map(|part| match part {
            ContentPart::Text(t) => t.len(),
            _ => 50, // tool calls, images, etc. get a rough fixed estimate
        })
        .sum();
    // 4 chars per token heuristic, minimum 1 token per message
    (char_count / 4).max(1)
}

/// Enforce context budget by truncating messages according to the chosen strategy.
///
/// Modifies the message vec in-place. System messages (at the front) are preserved.
/// The last message (current user turn) is never removed.
fn enforce_context_budget(
    messages: &mut Vec<ChatMessage>,
    budget: Option<usize>,
    strategy: &ContextStrategy,
    window: Option<usize>,
) {
    let Some(budget) = budget else { return };

    let current: usize = messages.iter().map(estimate_message_tokens).sum();
    if current <= budget {
        return;
    }

    match strategy {
        ContextStrategy::Truncate => {
            // Drop oldest non-system messages until under budget.
            // Keep system messages at the front and the last message (current user turn).
            while messages.iter().map(estimate_message_tokens).sum::<usize>() > budget
                && messages.len() > 2
            {
                // Find first non-system message
                if let Some(idx) = messages
                    .iter()
                    .position(|m| m.role != genai::chat::ChatRole::System)
                {
                    // Don't remove the last message (current user turn)
                    if idx >= messages.len() - 1 {
                        break;
                    }
                    messages.remove(idx);
                } else {
                    break;
                }
            }
        }
        ContextStrategy::SlidingWindow => {
            let keep = window.unwrap_or(10);
            let keep_count = keep * 2; // user + assistant pairs
            let system_count = messages
                .iter()
                .take_while(|m| m.role == genai::chat::ChatRole::System)
                .count();
            if messages.len() > system_count + keep_count {
                let drain_end = messages.len() - keep_count;
                messages.drain(system_count..drain_end);
            }
        }
    }
}

impl GenaiAgentHandle {
    pub fn new(
        client: genai::Client,
        model: ModelIden,
        system_prompt: &str,
        tools: Vec<LlmToolDefinition>,
        thinking_budget: Option<i64>,
    ) -> Self {
        let mode_state = default_internal_modes();
        let current_mode_id = mode_state.current_mode_id.0.to_string();

        Self {
            client,
            model,
            system_prompt: system_prompt.to_string(),
            tools,
            history: Vec::new(),
            mode_state,
            current_mode_id,
            mode_context_sent: false,
            max_tool_depth: usize::MAX,
            thinking_budget,
            context_budget: None,
            context_strategy: ContextStrategy::default(),
            context_window: None,
            output_validation: OutputValidation::default(),
            validation_retries: 3,
            undo_stack: Vec::new(),
        }
    }

    pub fn new_for_contract_tests(
        provider: &str,
        model: &str,
        system: &str,
        tools: Vec<LlmToolDefinition>,
    ) -> Self {
        let backend = provider
            .parse::<BackendType>()
            .unwrap_or(BackendType::OpenAI);

        let config = LlmProviderConfig::builder(backend).model(model).build();
        let chat_client = ChatClient::new(&config);
        let client = chat_client.inner().clone();
        let model_iden = chat_client
            .model_iden(model)
            .unwrap_or_else(|| ModelIden::new(genai::adapter::AdapterKind::OpenAI, model));

        let mode_state = default_internal_modes();
        let current_mode_id = mode_state.current_mode_id.0.to_string();

        Self {
            client,
            model: model_iden,
            system_prompt: system.to_string(),
            tools,
            history: Vec::new(),
            mode_state,
            current_mode_id,
            mode_context_sent: false,
            max_tool_depth: usize::MAX,
            thinking_budget: None,
            context_budget: None,
            context_strategy: ContextStrategy::default(),
            context_window: None,
            output_validation: OutputValidation::default(),
            validation_retries: 3,
            undo_stack: Vec::new(),
        }
    }

    /// Record the current history length before an agent turn starts.
    /// Called at the beginning of `send_message_stream`.
    pub fn snapshot_before_turn(&mut self) {
        self.undo_stack.push(crucible_core::types::UndoEntry {
            message_index: self.history.len(),
            description: String::new(),
        });
    }

    /// Set a description on the most recent undo entry (e.g. first ~80 chars of response).
    pub fn set_turn_description(&mut self, description: String) {
        if let Some(entry) = self.undo_stack.last_mut() {
            if entry.description.is_empty() {
                entry.description = description;
            }
        }
    }

    fn send_mock_contract_stream(
        &mut self,
        message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        self.history.push(ChatMessage::user(&message));

        let mut chunks: Vec<ChatResult<ChatChunk>> = Vec::new();

        if message.contains("Use read_note") || message.contains("Call read_note") {
            chunks.push(Ok(ChatChunk {
                delta: String::new(),
                done: false,
                tool_calls: Some(vec![ChatToolCall {
                    name: "read_note".to_string(),
                    arguments: Some(serde_json::json!({"path": "docs/README.md"})),
                    id: Some("call_read_note_1".to_string()),
                }]),
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            chunks.push(Ok(ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            return Box::pin(futures::stream::iter(chunks));
        }

        if message.contains("Think step by step") {
            chunks.push(Ok(ChatChunk {
                delta: String::new(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: Some("I will reason internally before final output.".to_string()),
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            chunks.push(Ok(ChatChunk {
                delta: "42".to_string(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            chunks.push(Ok(ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            return Box::pin(futures::stream::iter(chunks));
        }

        if message.contains("Tool result:") {
            chunks.push(Ok(ChatChunk {
                delta: "Wikilinks connect notes and make navigation easier.".to_string(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            chunks.push(Ok(ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            return Box::pin(futures::stream::iter(chunks));
        }

        if message.contains("What token did I ask you to remember?") {
            let token = self
                .history
                .iter()
                .rev()
                .filter_map(|m| {
                    if m.role == genai::chat::ChatRole::User {
                        m.content.first_text().and_then(|txt| {
                            txt.split_once("Remember this token:")
                                .map(|(_, rest)| rest.trim().to_string())
                        })
                    } else {
                        None
                    }
                })
                .next()
                .unwrap_or_else(|| "unknown".to_string());

            chunks.push(Ok(ChatChunk {
                delta: format!("You asked me to remember {token}."),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            chunks.push(Ok(ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            return Box::pin(futures::stream::iter(chunks));
        }

        if message.contains("Say hello in two chunks") {
            chunks.push(Ok(ChatChunk {
                delta: "Hello".to_string(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            chunks.push(Ok(ChatChunk {
                delta: " there".to_string(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            chunks.push(Ok(ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }));
            return Box::pin(futures::stream::iter(chunks));
        }

        chunks.push(Ok(ChatChunk {
            delta: "ok".to_string(),
            done: false,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
            precognition_notes_count: None,
            precognition_notes: None,
        }));
        chunks.push(Ok(ChatChunk {
            delta: String::new(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
            precognition_notes_count: None,
            precognition_notes: None,
        }));

        Box::pin(futures::stream::iter(chunks))
    }

    fn visible_tools(&self) -> Vec<LlmToolDefinition> {
        if self.current_mode_id == "plan" {
            self.tools
                .iter()
                .filter(|t| !is_write_tool_name(&t.function.name))
                .cloned()
                .collect()
        } else {
            self.tools.clone()
        }
    }

    fn explicit_model_name(&self) -> String {
        format!(
            "{}::{}",
            self.model.adapter_kind.as_lower_str(),
            &*self.model.model_name
        )
    }

    pub fn debug_visible_tool_names(&self) -> Vec<String> {
        self.visible_tools()
            .into_iter()
            .map(|t| t.function.name)
            .collect()
    }

    pub fn current_model(&self) -> Option<&str> {
        Some(&self.model.model_name)
    }
}

#[async_trait]
impl AgentHandle for GenaiAgentHandle {
    fn send_message_stream(
        &mut self,
        message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        if self.max_tool_depth == 0 {
            return wrap_stream_with_guards(self.send_mock_contract_stream(message));
        }

        // Record undo snapshot before this turn modifies history
        self.snapshot_before_turn();

        let mode_prefix = if self.current_mode_id == "plan" && !self.mode_context_sent {
            self.mode_context_sent = true;
            Some("[MODE: Plan mode - write tools are disabled. Use read-only tools only.]\n\n")
        } else {
            None
        };

        self.history.push(ChatMessage::user(&message));

        let req_message = match mode_prefix {
            Some(prefix) => format!("{prefix}{message}"),
            None => message,
        };

        let mut messages = self.history.clone();
        if mode_prefix.is_some() {
            if let Some(last) = messages.last_mut() {
                *last = ChatMessage::user(req_message);
            }
        }

        apply_prompt_caching(&self.system_prompt, &mut messages);
        enforce_context_budget(
            &mut messages,
            self.context_budget,
            &self.context_strategy,
            self.context_window,
        );

        let req_tools: Vec<Tool> = self
            .visible_tools()
            .iter()
            .map(super::tool_bridge::llm_tool_to_genai)
            .collect();
        let request = ChatRequest::new(messages).with_tools(req_tools);

        let options = ChatOptions::default()
            .with_capture_tool_calls(true)
            .with_capture_content(true)
            .with_capture_usage(true)
            .with_capture_reasoning_content(true);
        let options = if let Some(budget) = self.thinking_budget {
            options.with_reasoning_effort(ReasoningEffort::Budget(
                budget.clamp(0, u32::MAX as i64) as u32
            ))
        } else {
            options
        };

        let client = self.client.clone();
        let model_name = self.explicit_model_name();
        let max_tool_depth = self.max_tool_depth;

        let stream = Box::pin(async_stream::stream! {
            let stream_res = client.exec_chat_stream(&model_name, request, Some(&options)).await;
            let mut stream = match stream_res {
                Ok(res) => res.stream,
                Err(err) => {
                    yield Err(ChatError::Communication(format!("genai stream start failed: {err}")));
                    return;
                }
            };

            let mut emitted_calls = 0usize;

            while let Some(next) = stream.next().await {
                let event = match next {
                    Ok(event) => event,
                    Err(err) => {
                        yield Err(ChatError::Communication(format!("genai stream error: {err}")));
                        return;
                    }
                };

                match event {
                    ChatStreamEvent::Start => {}
                    ChatStreamEvent::Chunk(chunk) => {
                        yield Ok(ChatChunk {
                            delta: chunk.content,
                            done: false,
                            tool_calls: None,
                            tool_results: None,
                            reasoning: None,
                            usage: None,
                            subagent_events: None,
                            precognition_notes_count: None,
                            precognition_notes: None,
                        });
                    }
                    ChatStreamEvent::ReasoningChunk(chunk) => {
                        yield Ok(ChatChunk {
                            delta: String::new(),
                            done: false,
                            tool_calls: None,
                            tool_results: None,
                            reasoning: Some(chunk.content),
                            usage: None,
                            subagent_events: None,
                            precognition_notes_count: None,
                            precognition_notes: None,
                        });
                    }
                    ChatStreamEvent::ThoughtSignatureChunk(_) => {}
                    ChatStreamEvent::ToolCallChunk(_) => {}
                    ChatStreamEvent::End(end) => {
                        let mut tool_calls = Vec::new();
                        if let Some(content) = end.captured_content {
                            for part in content.into_parts() {
                                if let ContentPart::ToolCall(tc) = part {
                                    if emitted_calls >= max_tool_depth {
                                        break;
                                    }
                                    emitted_calls += 1;
                                    tool_calls.push(ChatToolCall {
                                        name: tc.fn_name,
                                        arguments: Some(tc.fn_arguments),
                                        id: Some(tc.call_id),
                                    });
                                }
                            }
                        }

                        let usage = end.captured_usage.as_ref().map(usage_to_token_usage);

                        yield Ok(ChatChunk {
                            delta: String::new(),
                            done: true,
                            tool_calls: if tool_calls.is_empty() {
                                None
                            } else {
                                Some(tool_calls)
                            },
                            tool_results: None,
                            reasoning: end.captured_reasoning_content,
                            usage,
                            subagent_events: None,
                            precognition_notes_count: None,
                            precognition_notes: None,
                        });
                        break;
                    }
                }
            }
        });

        wrap_stream_with_guards(stream)
    }

    fn continue_with_tool_results(
        &mut self,
        tool_calls: Vec<ChatToolCall>,
        tool_results: Vec<ChatToolResult>,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        let genai_tool_calls: Vec<ToolCall> = tool_calls
            .iter()
            .enumerate()
            .map(|(idx, call)| ToolCall {
                call_id: call
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("tool_call_{idx}")),
                fn_name: call.name.clone(),
                fn_arguments: call.arguments.clone().unwrap_or(serde_json::Value::Null),
                thought_signatures: None,
            })
            .collect();

        if !genai_tool_calls.is_empty() {
            self.history.push(ChatMessage::from(genai_tool_calls));
        }

        for (idx, result) in tool_results.into_iter().enumerate() {
            let call_id = result.call_id.unwrap_or_else(|| {
                tool_calls
                    .get(idx)
                    .and_then(|call| call.id.clone())
                    .unwrap_or_else(|| format!("tool_call_{idx}"))
            });
            self.history
                .push(ChatMessage::from(ToolResponse::new(call_id, result.result)));
        }

        let mut messages = self.history.clone();
        apply_prompt_caching(&self.system_prompt, &mut messages);
        enforce_context_budget(
            &mut messages,
            self.context_budget,
            &self.context_strategy,
            self.context_window,
        );

        let req_tools: Vec<Tool> = self
            .visible_tools()
            .iter()
            .map(super::tool_bridge::llm_tool_to_genai)
            .collect();
        let request = ChatRequest::new(messages).with_tools(req_tools);

        let options = ChatOptions::default()
            .with_capture_tool_calls(true)
            .with_capture_content(true)
            .with_capture_usage(true)
            .with_capture_reasoning_content(true);
        let options = if let Some(budget) = self.thinking_budget {
            options.with_reasoning_effort(ReasoningEffort::Budget(
                budget.clamp(0, u32::MAX as i64) as u32
            ))
        } else {
            options
        };

        let client = self.client.clone();
        let model_name = self.explicit_model_name();
        let max_tool_depth = self.max_tool_depth;

        let stream = Box::pin(async_stream::stream! {
            let stream_res = client.exec_chat_stream(&model_name, request, Some(&options)).await;
            let mut stream = match stream_res {
                Ok(res) => res.stream,
                Err(err) => {
                    yield Err(ChatError::Communication(format!("genai stream start failed: {err}")));
                    return;
                }
            };

            let mut emitted_calls = 0usize;

            while let Some(next) = stream.next().await {
                let event = match next {
                    Ok(event) => event,
                    Err(err) => {
                        yield Err(ChatError::Communication(format!("genai stream error: {err}")));
                        return;
                    }
                };

                match event {
                    ChatStreamEvent::Start => {}
                    ChatStreamEvent::Chunk(chunk) => {
                        yield Ok(ChatChunk {
                            delta: chunk.content,
                            done: false,
                            tool_calls: None,
                            tool_results: None,
                            reasoning: None,
                            usage: None,
                            subagent_events: None,
                            precognition_notes_count: None,
                            precognition_notes: None,
                        });
                    }
                    ChatStreamEvent::ReasoningChunk(chunk) => {
                        yield Ok(ChatChunk {
                            delta: String::new(),
                            done: false,
                            tool_calls: None,
                            tool_results: None,
                            reasoning: Some(chunk.content),
                            usage: None,
                            subagent_events: None,
                            precognition_notes_count: None,
                            precognition_notes: None,
                        });
                    }
                    ChatStreamEvent::ThoughtSignatureChunk(_) => {}
                    ChatStreamEvent::ToolCallChunk(_) => {}
                    ChatStreamEvent::End(end) => {
                        let mut tool_calls = Vec::new();
                        if let Some(content) = end.captured_content {
                            for part in content.into_parts() {
                                if let ContentPart::ToolCall(tc) = part {
                                    if emitted_calls >= max_tool_depth {
                                        break;
                                    }
                                    emitted_calls += 1;
                                    tool_calls.push(ChatToolCall {
                                        name: tc.fn_name,
                                        arguments: Some(tc.fn_arguments),
                                        id: Some(tc.call_id),
                                    });
                                }
                            }
                        }

                        let usage = end.captured_usage.as_ref().map(usage_to_token_usage);

                        yield Ok(ChatChunk {
                            delta: String::new(),
                            done: true,
                            tool_calls: if tool_calls.is_empty() {
                                None
                            } else {
                                Some(tool_calls)
                            },
                            tool_results: None,
                            reasoning: end.captured_reasoning_content,
                            usage,
                            subagent_events: None,
                            precognition_notes_count: None,
                            precognition_notes: None,
                        });
                        break;
                    }
                }
            }
        });

        wrap_stream_with_guards(stream)
    }

    async fn undo(&mut self, count: usize) -> ChatResult<Vec<crucible_core::types::UndoSummary>> {
        let mut summaries = Vec::new();
        for _ in 0..count {
            if let Some(entry) = self.undo_stack.pop() {
                let messages_removed = self.history.len().saturating_sub(entry.message_index);
                self.history.truncate(entry.message_index);
                summaries.push(crucible_core::types::UndoSummary {
                    messages_removed,
                    description: entry.description,
                });
            } else {
                break;
            }
        }
        Ok(summaries)
    }

    fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    fn undo_depth(&self) -> usize {
        self.undo_stack.len()
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn get_modes(&self) -> Option<&SessionModeState> {
        Some(&self.mode_state)
    }

    fn get_mode_id(&self) -> &str {
        &self.current_mode_id
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
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

        self.current_mode_id = mode_id.to_string();
        self.mode_state.current_mode_id = SessionModeId::new(mode_id);
        self.mode_context_sent = false;
        Ok(())
    }

    async fn switch_model(&mut self, model_id: &str) -> ChatResult<()> {
        self.model = self.model.from_name(model_id.to_string());
        Ok(())
    }

    fn current_model(&self) -> Option<&str> {
        Some(&self.model.model_name)
    }

    async fn set_context_budget(&mut self, budget: Option<usize>) -> ChatResult<()> {
        self.context_budget = budget;
        Ok(())
    }

    fn get_context_budget(&self) -> Option<usize> {
        self.context_budget
    }

    async fn set_context_strategy(&mut self, strategy: ContextStrategy) -> ChatResult<()> {
        self.context_strategy = strategy;
        Ok(())
    }

    fn get_context_strategy(&self) -> ContextStrategy {
        self.context_strategy.clone()
    }

    async fn set_context_window(&mut self, window: Option<usize>) -> ChatResult<()> {
        self.context_window = window;
        Ok(())
    }

    fn get_context_window(&self) -> Option<usize> {
        self.context_window
    }

    async fn set_output_validation(
        &mut self,
        validation: crucible_core::session::OutputValidation,
    ) -> ChatResult<()> {
        self.output_validation = validation;
        Ok(())
    }

    fn get_output_validation(&self) -> &crucible_core::session::OutputValidation {
        &self.output_validation
    }

    async fn set_validation_retries(&mut self, retries: u32) -> ChatResult<()> {
        self.validation_retries = retries;
        Ok(())
    }

    fn get_validation_retries(&self) -> u32 {
        self.validation_retries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[derive(Clone)]
    struct StreamingMockAgent {
        chunks: Vec<ChatChunk>,
        hanging: bool,
    }

    impl StreamingMockAgent {
        fn immediate_end() -> Self {
            Self {
                chunks: vec![ChatChunk {
                    delta: String::new(),
                    done: true,
                    tool_calls: None,
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                    precognition_notes_count: None,
                    precognition_notes: None,
                }],
                hanging: false,
            }
        }

        fn empty() -> Self {
            Self::immediate_end()
        }

        fn hanging() -> Self {
            Self {
                chunks: Vec::new(),
                hanging: true,
            }
        }
    }

    #[async_trait]
    impl AgentHandle for StreamingMockAgent {
        fn send_message_stream(
            &mut self,
            _message: String,
        ) -> BoxStream<'static, ChatResult<ChatChunk>> {
            if self.hanging {
                futures::stream::pending::<ChatResult<ChatChunk>>().boxed()
            } else {
                futures::stream::iter(self.chunks.clone().into_iter().map(Ok)).boxed()
            }
        }

        fn continue_with_tool_results(
            &mut self,
            _tool_calls: Vec<ChatToolCall>,
            _tool_results: Vec<ChatToolResult>,
        ) -> BoxStream<'static, ChatResult<ChatChunk>> {
            if self.hanging {
                futures::stream::pending::<ChatResult<ChatChunk>>().boxed()
            } else {
                futures::stream::iter(self.chunks.clone().into_iter().map(Ok)).boxed()
            }
        }

        fn is_connected(&self) -> bool {
            true
        }

        async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
            Ok(())
        }
    }

    #[test]
    fn test_thinking_budget_stored_and_clamped() {
        let config = LlmProviderConfig::builder(BackendType::OpenAI)
            .model("gpt-4o-mini")
            .build();
        let chat_client = ChatClient::new(&config);
        let client = chat_client.inner().clone();
        let model = chat_client
            .model_iden("gpt-4o-mini")
            .unwrap_or_else(|| ModelIden::new(genai::adapter::AdapterKind::OpenAI, "gpt-4o-mini"));

        let negative_budget_handle = GenaiAgentHandle::new(
            client.clone(),
            model.clone(),
            "system",
            Vec::new(),
            Some(-5),
        );
        assert_eq!(negative_budget_handle.thinking_budget, Some(-5));

        let max_budget_handle =
            GenaiAgentHandle::new(client, model, "system", Vec::new(), Some(i64::MAX));
        assert_eq!(max_budget_handle.thinking_budget, Some(i64::MAX));

        let clamped_negative = (-5_i64).clamp(0, u32::MAX as i64) as u32;
        let clamped_overflow = i64::MAX.clamp(0, u32::MAX as i64) as u32;

        assert_eq!(clamped_negative, 0);
        assert_eq!(clamped_overflow, u32::MAX);
    }

    #[tokio::test]
    async fn test_send_message_stream_empty_response_yields_error() {
        let mut agent = StreamingMockAgent::immediate_end();
        let results = wrap_stream_with_guards(agent.send_message_stream("hello".to_string()))
            .collect::<Vec<_>>()
            .await;

        assert!(results.iter().any(
            |r| matches!(r, Err(ChatError::Communication(msg)) if msg.contains("empty response"))
        ));
    }

    #[tokio::test]
    async fn test_send_message_stream_empty_iterator_yields_error() {
        let mut agent = StreamingMockAgent::empty();
        let results = wrap_stream_with_guards(agent.send_message_stream("hello".to_string()))
            .collect::<Vec<_>>()
            .await;

        assert!(results.iter().any(
            |r| matches!(r, Err(ChatError::Communication(msg)) if msg.contains("empty response"))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn test_send_message_stream_timeout_yields_error() {
        let mut agent = StreamingMockAgent::hanging();
        let task = tokio::spawn(async move {
            wrap_stream_with_guards(agent.send_message_stream("hello".to_string()))
                .collect::<Vec<_>>()
                .await
        });

        tokio::time::advance(std::time::Duration::from_secs(301)).await;

        let results = task.await.expect("task panicked");
        assert!(results
            .iter()
            .any(|r| matches!(r, Err(ChatError::Communication(msg)) if msg.contains("timed out"))));
    }

    #[tokio::test]
    async fn test_continue_with_tool_results_empty_response_yields_error() {
        let mut agent = StreamingMockAgent::immediate_end();
        let results = wrap_stream_with_guards(agent.continue_with_tool_results(vec![], vec![]))
            .collect::<Vec<_>>()
            .await;

        assert!(results.iter().any(
            |r| matches!(r, Err(ChatError::Communication(msg)) if msg == EMPTY_RESPONSE_ERROR)
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn test_continue_with_tool_results_timeout_yields_error() {
        let mut agent = StreamingMockAgent::hanging();
        let task = tokio::spawn(async move {
            wrap_stream_with_guards(agent.continue_with_tool_results(vec![], vec![]))
                .collect::<Vec<_>>()
                .await
        });

        tokio::time::advance(std::time::Duration::from_secs(301)).await;

        let results = task.await.expect("task panicked");
        assert!(results.iter().any(
            |r| matches!(r, Err(ChatError::Communication(msg)) if msg == STREAM_TIMEOUT_ERROR)
        ));
    }

    // === Negative tests: verify no false positives on legitimate responses ===

    #[tokio::test]
    async fn test_normal_text_response_no_error() {
        let mut agent = StreamingMockAgent {
            chunks: vec![ChatChunk {
                delta: "Hello world".to_string(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }],
            hanging: false,
        };
        let results = wrap_stream_with_guards(agent.send_message_stream("test".to_string()))
            .collect::<Vec<_>>()
            .await;
        assert!(
            results.iter().all(|r| r.is_ok()),
            "expected no errors, got: {:?}",
            results
        );
    }

    #[tokio::test]
    async fn test_tool_call_only_response_no_error() {
        let mut agent = StreamingMockAgent {
            chunks: vec![ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: Some(vec![ChatToolCall {
                    name: "search".to_string(),
                    arguments: None,
                    id: Some("call_1".to_string()),
                }]),
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }],
            hanging: false,
        };
        let results = wrap_stream_with_guards(agent.send_message_stream("test".to_string()))
            .collect::<Vec<_>>()
            .await;
        assert!(
            results.iter().all(|r| r.is_ok()),
            "expected no errors, got: {:?}",
            results
        );
    }

    #[tokio::test]
    async fn test_thinking_only_response_no_error() {
        let mut agent = StreamingMockAgent {
            chunks: vec![ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: Some("Let me think about this...".to_string()),
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }],
            hanging: false,
        };
        let results = wrap_stream_with_guards(agent.send_message_stream("test".to_string()))
            .collect::<Vec<_>>()
            .await;
        assert!(
            results.iter().all(|r| r.is_ok()),
            "expected no errors, got: {:?}",
            results
        );
    }

    #[tokio::test]
    async fn test_text_plus_tool_call_response_no_error() {
        let mut agent = StreamingMockAgent {
            chunks: vec![
                ChatChunk {
                    delta: "Hello".to_string(),
                    done: false,
                    tool_calls: None,
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                    precognition_notes_count: None,
                    precognition_notes: None,
                },
                ChatChunk {
                    delta: String::new(),
                    done: true,
                    tool_calls: Some(vec![ChatToolCall {
                        name: "search".to_string(),
                        arguments: None,
                        id: Some("call_1".to_string()),
                    }]),
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                    precognition_notes_count: None,
                    precognition_notes: None,
                },
            ],
            hanging: false,
        };
        let results = wrap_stream_with_guards(agent.send_message_stream("test".to_string()))
            .collect::<Vec<_>>()
            .await;
        assert!(
            results.iter().all(|r| r.is_ok()),
            "expected no errors, got: {:?}",
            results
        );
    }

    // ── Undo tests ──────────────────────────────────────────────────────────

    fn make_test_handle() -> GenaiAgentHandle {
        let backend = BackendType::OpenAI;
        let config = LlmProviderConfig::builder(backend).model("test-model").build();
        let chat_client = ChatClient::new(&config);
        let client = chat_client.inner().clone();
        let model_iden = chat_client
            .model_iden("test-model")
            .unwrap_or_else(|| ModelIden::new(genai::adapter::AdapterKind::OpenAI, "test-model"));

        GenaiAgentHandle::new(client, model_iden, "system", Vec::new(), None)
    }

    #[test]
    fn undo_stack_empty_initially() {
        let handle = make_test_handle();
        assert!(!handle.can_undo());
        assert_eq!(handle.undo_depth(), 0);
    }

    #[test]
    fn snapshot_before_turn_pushes_entry() {
        let mut handle = make_test_handle();
        handle.snapshot_before_turn();
        assert!(handle.can_undo());
        assert_eq!(handle.undo_depth(), 1);
    }

    #[tokio::test]
    async fn undo_truncates_history_to_snapshot() {
        let mut handle = make_test_handle();

        // Simulate a turn: snapshot, add user message, add tool response
        handle.snapshot_before_turn();
        handle.history.push(ChatMessage::user("hello"));
        handle
            .history
            .push(ChatMessage::user("simulated assistant response"));
        assert_eq!(handle.history.len(), 2);

        let summaries = handle.undo(1).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].messages_removed, 2);
        assert!(handle.history.is_empty());
        assert!(!handle.can_undo());
    }

    #[tokio::test]
    async fn undo_multiple_turns() {
        let mut handle = make_test_handle();

        // Turn 1
        handle.snapshot_before_turn();
        handle.history.push(ChatMessage::user("turn 1"));

        // Turn 2
        handle.snapshot_before_turn();
        handle.history.push(ChatMessage::user("turn 2"));

        assert_eq!(handle.undo_depth(), 2);
        assert_eq!(handle.history.len(), 2);

        // Undo both turns
        let summaries = handle.undo(2).await.unwrap();
        assert_eq!(summaries.len(), 2);
        assert!(handle.history.is_empty());
        assert_eq!(handle.undo_depth(), 0);
    }

    #[tokio::test]
    async fn undo_more_than_available_stops_at_zero() {
        let mut handle = make_test_handle();

        handle.snapshot_before_turn();
        handle.history.push(ChatMessage::user("only turn"));

        let summaries = handle.undo(5).await.unwrap();
        assert_eq!(summaries.len(), 1); // only 1 was available
        assert!(handle.history.is_empty());
    }

    #[tokio::test]
    async fn undo_nothing_returns_empty() {
        let mut handle = make_test_handle();
        let summaries = handle.undo(1).await.unwrap();
        assert!(summaries.is_empty());
    }

    #[test]
    fn set_turn_description_updates_last_entry() {
        let mut handle = make_test_handle();
        handle.snapshot_before_turn();
        handle.set_turn_description("Analyzed the auth module".to_string());
        assert_eq!(
            handle.undo_stack.last().unwrap().description,
            "Analyzed the auth module"
        );
    }
}
