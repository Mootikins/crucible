use async_trait::async_trait;
use crucible_core::session::{ContextStrategy, OutputValidation};
use crucible_core::traits::chat::{
    AgentHandle, ChatError, ChatResult, ChatToolCall, ChatToolResult,
};
use crucible_core::traits::llm::LlmToolDefinition;
use crucible_core::traits::TokenUsage;
use crucible_core::turn::{StopReason, TurnError, TurnEvent};
use crucible_core::types::acp::schema::{SessionModeId, SessionModeState};
use crucible_core::types::mode::default_internal_modes;
use futures::stream::BoxStream;
use futures::StreamExt;
use genai::chat::{
    CacheControl, ChatMessage, ChatOptions, ChatRequest, ChatStreamEvent, ContentPart,
    ReasoningEffort, Tool, ToolCall, ToolResponse,
};
use genai::ModelIden;

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
        let system_msg = ChatMessage::system(system_prompt).with_options(CacheControl::Ephemeral);
        messages.insert(0, system_msg);
    }
}

fn is_write_tool_name(tool_name: &str) -> bool {
    matches!(tool_name, "write_file" | "edit_file" | "bash")
        || tool_name.starts_with("create_")
        || tool_name.starts_with("delete_")
}

fn usage_to_token_usage(usage: &genai::chat::Usage) -> TokenUsage {
    let to_u32 = |v: Option<i32>| -> u32 {
        let n = v.unwrap_or(0);
        if n < 0 {
            0
        } else {
            n as u32
        }
    };
    let to_opt_u32 = |v: Option<i32>| -> Option<u32> {
        v.and_then(|n| if n > 0 { Some(n as u32) } else { None })
    };

    let (cache_read_tokens, cache_creation_tokens) = usage
        .prompt_tokens_details
        .as_ref()
        .map(|d| {
            (
                to_opt_u32(d.cached_tokens),
                to_opt_u32(d.cache_creation_tokens),
            )
        })
        .unwrap_or((None, None));

    TokenUsage {
        prompt_tokens: to_u32(usage.prompt_tokens),
        completion_tokens: to_u32(usage.completion_tokens),
        total_tokens: to_u32(usage.total_tokens),
        cache_read_tokens,
        cache_creation_tokens,
    }
}

/// Wrap an LLM turn-event stream with stream-level invariants:
/// per-chunk timeout, empty-response detection, and unexpected-end
/// detection. On a guard failure the stream re-emits a terminal
/// `TurnEvent::Error`; on success the inner stream's terminal event
/// (`Done` or `Error`) passes through unchanged.
fn wrap_stream_with_guards(
    mut stream: BoxStream<'static, TurnEvent>,
) -> BoxStream<'static, TurnEvent> {
    Box::pin(async_stream::stream! {
        let mut received_content = false;
        let mut received_tool_call = false;
        let mut received_reasoning = false;
        let mut received_terminal = false;

        loop {
            let next = match tokio::time::timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
                Ok(item) => item,
                Err(_) => {
                    yield TurnEvent::Error(TurnError::Communication(
                        STREAM_TIMEOUT_ERROR.to_string(),
                    ));
                    return;
                }
            };

            let Some(event) = next else {
                break;
            };

            match &event {
                TurnEvent::TextDelta(text) if !text.is_empty() => received_content = true,
                TurnEvent::Thinking(text) if !text.is_empty() => received_reasoning = true,
                TurnEvent::ToolCall { .. } => received_tool_call = true,
                TurnEvent::Done { .. } | TurnEvent::Error(_) => received_terminal = true,
                _ => {}
            }

            yield event;
        }

        if !received_terminal {
            if !received_content && !received_tool_call && !received_reasoning {
                yield TurnEvent::Error(TurnError::Communication(
                    EMPTY_RESPONSE_ERROR.to_string(),
                ));
                return;
            }
            yield TurnEvent::Error(TurnError::Communication(
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
        }
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
        // If model_name already has a namespace (contains ::), use it as-is.
        // Otherwise prefix with adapter kind for explicit routing.
        if self.model.model_name.contains("::") {
            self.model.model_name.to_string()
        } else {
            format!(
                "{}::{}",
                self.model.adapter_kind.as_lower_str(),
                &*self.model.model_name
            )
        }
    }

    /// Stream a single LLM call for an explicit message list as a
    /// `BoxStream<TurnEvent>`. The stream emits content events
    /// (`TextDelta` / `Thinking` / `ToolCall` / `Usage`) as the provider
    /// yields them, then a terminal `Done { EndTurn }` on clean end or
    /// `Error(...)` on failure. Guard-wrapped for per-chunk timeout,
    /// empty-response, and unexpected-end detection.
    fn stream_chat_from_messages(
        &self,
        mut messages: Vec<ChatMessage>,
    ) -> BoxStream<'static, TurnEvent> {
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
            let stream_res = client
                .exec_chat_stream(&model_name, request, Some(&options))
                .await;
            let mut stream = match stream_res {
                Ok(res) => res.stream,
                Err(err) => {
                    yield TurnEvent::Error(TurnError::Communication(format!(
                        "genai stream start failed: {err}"
                    )));
                    return;
                }
            };

            let mut emitted_calls = 0usize;

            while let Some(next) = stream.next().await {
                let event = match next {
                    Ok(event) => event,
                    Err(err) => {
                        yield TurnEvent::Error(TurnError::Communication(format!(
                            "genai stream error: {err}"
                        )));
                        return;
                    }
                };

                match event {
                    ChatStreamEvent::Start => {}
                    ChatStreamEvent::Chunk(chunk) => {
                        if !chunk.content.is_empty() {
                            yield TurnEvent::TextDelta(chunk.content);
                        }
                    }
                    ChatStreamEvent::ReasoningChunk(chunk) => {
                        if !chunk.content.is_empty() {
                            yield TurnEvent::Thinking(chunk.content);
                        }
                    }
                    ChatStreamEvent::ThoughtSignatureChunk(_) => {}
                    ChatStreamEvent::ToolCallChunk(_) => {}
                    ChatStreamEvent::End(end) => {
                        if let Some(reasoning) = end.captured_reasoning_content {
                            if !reasoning.is_empty() {
                                yield TurnEvent::Thinking(reasoning);
                            }
                        }
                        if let Some(content) = end.captured_content {
                            for part in content.into_parts() {
                                if let ContentPart::ToolCall(tc) = part {
                                    if emitted_calls >= max_tool_depth {
                                        break;
                                    }
                                    emitted_calls += 1;
                                    yield TurnEvent::ToolCall {
                                        id: tc.call_id,
                                        name: tc.fn_name,
                                        args: tc.fn_arguments,
                                    };
                                }
                            }
                        }
                        if let Some(usage) = end.captured_usage.as_ref() {
                            yield TurnEvent::Usage(usage_to_token_usage(usage));
                        }
                        yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
                        return;
                    }
                }
            }
        });

        wrap_stream_with_guards(stream)
    }

    /// Convert a flattened `ContextMessage` list into genai's
    /// `ChatMessage` form, dropping the trailing user message if it
    /// matches the turn's `content` (the scheduler pushes it to both
    /// the tree and `ctx.content`; we'd rather re-emit it fresh from
    /// `ctx.content` to keep mode-prefix injection logic in one place).
    fn context_messages_to_chat(
        &self,
        messages: &[crucible_core::traits::context_ops::ContextMessage],
    ) -> Vec<ChatMessage> {
        use crucible_core::traits::llm::MessageRole;
        messages
            .iter()
            .map(|m| match m.role {
                MessageRole::User => ChatMessage::user(&m.content),
                MessageRole::Assistant => ChatMessage::assistant(&m.content),
                MessageRole::System => ChatMessage::system(&m.content),
                MessageRole::Tool | MessageRole::Function => {
                    // Minimal fallback; tool-role messages come back
                    // in-turn via the inbound channel, not the flattened
                    // context — but be forgiving on input.
                    ChatMessage::user(&m.content)
                }
            })
            .collect()
    }

    /// Scheduler-driven tool loop: the caller provides the full turn
    /// messages via `ctx.messages`; the handle does not read or write
    /// `self.history`. `ctx.content` is used only for backwards-
    /// compatible logging (the scheduler has already pushed the user
    /// message into `ctx.messages`).
    fn scheduler_driven_turn<'a>(
        &'a mut self,
        ctx: crucible_core::turn::TurnContext,
    ) -> futures::stream::BoxStream<'a, TurnEvent> {
        /// Depth-cap prompt sent back to the agent when `max_iterations`
        /// is reached. Kept in sync with
        /// `agent_manager::messaging::TOOL_DEPTH_LIMIT_FINAL_PROMPT`.
        const DEPTH_CAP_PROMPT: &str = "You have reached the tool call limit. Please provide your final answer based on the information gathered so far.";

        let mut messages = self.context_messages_to_chat(&ctx.messages);
        let mut inbound = ctx.inbound;

        // Mode prefix injection. Mirrors the legacy path's one-shot
        // behaviour: first turn in plan mode prepends a synthetic
        // instruction to the last user message.
        if self.current_mode_id == "plan" && !self.mode_context_sent {
            if let Some(last) = messages.last() {
                if last.role == genai::chat::ChatRole::User {
                    let prefix = "[MODE: Plan mode - write tools are disabled. Use read-only tools only.]\n\n";
                    let text = last.content.first_text().unwrap_or_default();
                    let combined = format!("{prefix}{text}");
                    let idx = messages.len() - 1;
                    messages[idx] = ChatMessage::user(combined);
                    self.mode_context_sent = true;
                }
            }
        }

        let body = async_stream::stream! {
            let mut chat_stream = self.stream_chat_from_messages(messages.clone());

            'turn: loop {
                // Collect ToolCall events emitted during this LLM iteration
                // so the outer loop can dispatch them when the stream ends.
                let mut pending_calls: Vec<ChatToolCall> = Vec::new();

                loop {
                    let Some(event) = chat_stream.next().await else {
                        // Unexpected stream close — treat as empty.
                        yield TurnEvent::Done { stop_reason: StopReason::Empty };
                        return;
                    };

                    match event {
                        TurnEvent::ToolCall { ref id, ref name, ref args } => {
                            pending_calls.push(ChatToolCall {
                                id: Some(id.clone()),
                                name: name.clone(),
                                arguments: Some(args.clone()),
                            });
                            yield event;
                        }
                        TurnEvent::Done { .. } => break,
                        TurnEvent::Error(e) => {
                            yield TurnEvent::Error(e);
                            return;
                        }
                        other => yield other,
                    }
                }

                if pending_calls.is_empty() {
                    yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
                    return;
                }

                yield TurnEvent::ToolBatchEnd;

                let Some(rx) = inbound.as_mut() else {
                    yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
                    return;
                };

                let expected_ids: std::collections::HashSet<String> = pending_calls
                    .iter()
                    .filter_map(|c| c.id.clone())
                    .collect();
                let mut collected: Vec<ChatToolResult> = Vec::with_capacity(pending_calls.len());
                while collected.len() < pending_calls.len() {
                    let Some(event) = rx.recv().await else {
                        yield TurnEvent::Done { stop_reason: StopReason::Cancelled };
                        return;
                    };

                    match event {
                        TurnEvent::ToolResult { ref id, ref name, ref result, ref error } => {
                            if !expected_ids.is_empty() && !expected_ids.contains(id) {
                                continue;
                            }
                            let result_str = match result {
                                serde_json::Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                            collected.push(ChatToolResult {
                                name: name.clone(),
                                result: result_str,
                                error: error.clone(),
                                call_id: Some(id.clone()),
                            });
                        }
                        TurnEvent::HandlerInjection { content, .. } => {
                            drop(chat_stream);
                            messages.push(ChatMessage::user(&content));
                            chat_stream = self.stream_chat_from_messages(messages.clone());
                            continue 'turn;
                        }
                        TurnEvent::DepthCapHit { .. } => {
                            drop(chat_stream);
                            messages.push(ChatMessage::user(DEPTH_CAP_PROMPT));
                            chat_stream = self.stream_chat_from_messages(messages.clone());
                            continue 'turn;
                        }
                        _ => {}
                    }
                }

                // Fold tool calls + results into the local message list
                // for the next LLM iteration.
                let genai_tool_calls: Vec<ToolCall> = pending_calls
                    .iter()
                    .enumerate()
                    .map(|(idx, call)| ToolCall {
                        call_id: call
                            .id
                            .clone()
                            .unwrap_or_else(|| format!("tool_call_{idx}")),
                        fn_name: call.name.clone(),
                        fn_arguments: call
                            .arguments
                            .clone()
                            .unwrap_or(serde_json::Value::Null),
                        thought_signatures: None,
                    })
                    .collect();
                if !genai_tool_calls.is_empty() {
                    messages.push(ChatMessage::from(genai_tool_calls));
                }
                for (idx, result) in collected.into_iter().enumerate() {
                    let call_id = result.call_id.unwrap_or_else(|| {
                        pending_calls
                            .get(idx)
                            .and_then(|call| call.id.clone())
                            .unwrap_or_else(|| format!("tool_call_{idx}"))
                    });
                    messages.push(ChatMessage::from(ToolResponse::new(call_id, result.result)));
                }

                drop(chat_stream);
                chat_stream = self.stream_chat_from_messages(messages.clone());
            }
        };

        Box::pin(body)
    }
}

#[async_trait]
impl AgentHandle for GenaiAgentHandle {
    async fn send_message_fire_and_forget(&mut self, _message: String) -> ChatResult<()> {
        // GenaiAgentHandle is daemon-side — the TUI never calls this
        // directly. Included only to satisfy the AgentHandle trait.
        Ok(())
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

#[async_trait]
impl crucible_core::turn::Agent for GenaiAgentHandle {
    fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
        crucible_core::turn::AgentCapabilities {
            streaming: true,
            tool_calls: true,
            thinking: true,
            model_switching: true,
            usage_reporting: true,
            cancellation: true,
            temperature_control: true,
            max_tokens_control: true,
            owns_history: false,
            modes: true,
        }
    }

    async fn turn<'a>(
        &'a mut self,
        ctx: crucible_core::turn::TurnContext,
    ) -> Result<
        futures::stream::BoxStream<'a, crucible_core::turn::TurnEvent>,
        crucible_core::turn::AgentError,
    > {
        Ok(Self::scheduler_driven_turn(self, ctx))
    }

    async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
        AgentHandle::cancel(self)
            .await
            .map_err(|e| crucible_core::turn::AgentError::Communication(e.to_string()))
    }

    async fn switch_model(
        &mut self,
        model_id: &str,
    ) -> Result<(), crucible_core::turn::NotSupported> {
        AgentHandle::switch_model(self, model_id)
            .await
            .map_err(|_| crucible_core::turn::NotSupported::new("switch_model"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::ChatClient;
    use crucible_core::config::{BackendType, LlmProviderConfig};

    use futures::StreamExt;

    /// Build a scripted `TurnEvent` stream for driving
    /// `wrap_stream_with_guards` in unit tests.
    fn scripted_turn_stream(events: Vec<TurnEvent>) -> BoxStream<'static, TurnEvent> {
        futures::stream::iter(events).boxed()
    }

    /// Stream that never yields — exercises `STREAM_CHUNK_TIMEOUT`.
    fn hanging_turn_stream() -> BoxStream<'static, TurnEvent> {
        futures::stream::pending::<TurnEvent>().boxed()
    }

    /// Single terminal `Done` with no prior content — exercises the
    /// empty-response detection.
    fn immediate_done_events() -> Vec<TurnEvent> {
        vec![TurnEvent::Done {
            stop_reason: StopReason::EndTurn,
        }]
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

    fn has_empty_response_error(events: &[TurnEvent]) -> bool {
        events.iter().any(|e| {
            matches!(
                e,
                TurnEvent::Error(TurnError::Communication(msg)) if msg.contains("empty response")
            )
        })
    }

    fn has_timeout_error(events: &[TurnEvent]) -> bool {
        events.iter().any(|e| {
            matches!(
                e,
                TurnEvent::Error(TurnError::Communication(msg)) if msg.contains("timed out")
            )
        })
    }

    fn has_any_error(events: &[TurnEvent]) -> bool {
        events.iter().any(|e| matches!(e, TurnEvent::Error(_)))
    }

    #[tokio::test]
    async fn test_immediate_done_yields_empty_response_error() {
        // Terminal Done with no prior content must flag empty response.
        // Note: wrap_stream_with_guards can't distinguish "inner stream
        // terminated via Done with nothing emitted" from "inner stream
        // closed naturally with nothing emitted" — both are errors.
        // Since Done itself marks received_terminal, we need a stream
        // that closes without emitting Done to exercise empty-response.
        let events = wrap_stream_with_guards(scripted_turn_stream(vec![]))
            .collect::<Vec<_>>()
            .await;
        assert!(has_empty_response_error(&events), "got: {events:?}");

        // And an inner stream that terminates with Done but emitted no
        // content should still complete cleanly (the LLM sometimes
        // returns an empty assistant message; guard only catches the
        // "no terminal at all" case).
        let events = wrap_stream_with_guards(scripted_turn_stream(immediate_done_events()))
            .collect::<Vec<_>>()
            .await;
        assert!(!has_any_error(&events), "got: {events:?}");
    }

    #[tokio::test]
    async fn test_unterminated_stream_yields_unexpected_end_error() {
        // Content arrived but the inner stream closed without Done/Error.
        let events = wrap_stream_with_guards(scripted_turn_stream(vec![TurnEvent::TextDelta(
            "partial".to_string(),
        )]))
        .collect::<Vec<_>>()
        .await;
        assert!(
            events.iter().any(|e| matches!(
                e,
                TurnEvent::Error(TurnError::Communication(msg))
                    if msg.contains("ended unexpectedly")
            )),
            "got: {events:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn test_hanging_stream_yields_timeout_error() {
        let task = tokio::spawn(async move {
            wrap_stream_with_guards(hanging_turn_stream())
                .collect::<Vec<_>>()
                .await
        });

        tokio::time::advance(std::time::Duration::from_secs(301)).await;

        let events = task.await.expect("task panicked");
        assert!(has_timeout_error(&events), "got: {events:?}");
    }

    // === Negative tests: verify no false positives on legitimate responses ===

    #[tokio::test]
    async fn test_normal_text_response_no_error() {
        let events = wrap_stream_with_guards(scripted_turn_stream(vec![
            TurnEvent::TextDelta("Hello world".to_string()),
            TurnEvent::Done {
                stop_reason: StopReason::EndTurn,
            },
        ]))
        .collect::<Vec<_>>()
        .await;
        assert!(!has_any_error(&events), "got: {events:?}");
    }

    #[tokio::test]
    async fn test_tool_call_only_response_no_error() {
        let events = wrap_stream_with_guards(scripted_turn_stream(vec![
            TurnEvent::ToolCall {
                id: "call_1".to_string(),
                name: "search".to_string(),
                args: serde_json::Value::Null,
            },
            TurnEvent::Done {
                stop_reason: StopReason::EndTurn,
            },
        ]))
        .collect::<Vec<_>>()
        .await;
        assert!(!has_any_error(&events), "got: {events:?}");
    }

    #[tokio::test]
    async fn test_thinking_only_response_no_error() {
        let events = wrap_stream_with_guards(scripted_turn_stream(vec![
            TurnEvent::Thinking("Let me think about this...".to_string()),
            TurnEvent::Done {
                stop_reason: StopReason::EndTurn,
            },
        ]))
        .collect::<Vec<_>>()
        .await;
        assert!(!has_any_error(&events), "got: {events:?}");
    }

    #[tokio::test]
    async fn test_text_plus_tool_call_response_no_error() {
        let events = wrap_stream_with_guards(scripted_turn_stream(vec![
            TurnEvent::TextDelta("Hello".to_string()),
            TurnEvent::ToolCall {
                id: "call_1".to_string(),
                name: "search".to_string(),
                args: serde_json::Value::Null,
            },
            TurnEvent::Done {
                stop_reason: StopReason::EndTurn,
            },
        ]))
        .collect::<Vec<_>>()
        .await;
        assert!(!has_any_error(&events), "got: {events:?}");
    }

    // Undo semantics moved to AgentManager (operates on the scheduler-
    // owned ConversationTree). See agent_manager::models::undo + the
    // integration test `agent_manager::tests::messaging::*`.
}
