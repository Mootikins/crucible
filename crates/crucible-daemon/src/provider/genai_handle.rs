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

/// Tracks emitted tool calls during a stream so we can:
///   1. Emit each tool call live (on `ToolCallChunk`) instead of waiting for
///      `End` — the user sees a permission prompt as soon as each tool call's
///      args finish streaming, rather than after the whole response block.
///   2. Avoid double-emission when the provider also replays tool calls in
///      `captured_content` at `End` (deduplication by `call_id`).
///   3. Honor `max_tool_depth` consistently across both paths.
///   4. Synthesize `FileDiff`s for file-mutating tools so the TUI can show
///      the pending change in scrollback before the model receives the
///      tool result.
struct ToolCallEmitter {
    emitted_call_ids: std::collections::HashSet<String>,
    emitted_count: usize,
    max_depth: usize,
    workspace_root: std::path::PathBuf,
}

impl ToolCallEmitter {
    fn new(max_depth: usize, workspace_root: std::path::PathBuf) -> Self {
        Self {
            emitted_call_ids: std::collections::HashSet::new(),
            emitted_count: 0,
            max_depth,
            workspace_root,
        }
    }

    /// Try to emit a `TurnEvent::ToolCall` for `tc`. Returns `None` when the
    /// `call_id` was already emitted or the depth cap has been reached.
    fn try_emit(&mut self, tc: ToolCall) -> Option<TurnEvent> {
        if self.emitted_count >= self.max_depth {
            return None;
        }
        if !self.emitted_call_ids.insert(tc.call_id.clone()) {
            return None;
        }
        self.emitted_count += 1;
        let normalized_args = normalize_tool_args(tc.fn_arguments);
        // Pure helper — returns an empty Vec for unknown tools, malformed
        // args, or oversized content. Mirrors the permission flow's
        // synthesis at `agent_manager::messaging::permission`.
        let diffs = crate::tools::diff_synth::synthesize_diffs(
            &tc.fn_name,
            &normalized_args,
            &self.workspace_root,
        );
        Some(TurnEvent::ToolCall {
            id: tc.call_id,
            name: tc.fn_name,
            args: normalized_args,
            diffs,
        })
    }

    #[cfg(test)]
    fn emitted_count(&self) -> usize {
        self.emitted_count
    }
}

/// Tracks whether any reasoning chunks have been emitted live during a stream
/// so we don't double-emit at `End` for providers that *also* replay the full
/// reasoning content there. If no live chunks fired (the End-only path),
/// the End-time replay is the user's only source of reasoning, so we still
/// emit it.
#[derive(Default)]
struct ReasoningEmissionState {
    emitted_live: bool,
}

impl ReasoningEmissionState {
    fn new() -> Self {
        Self::default()
    }

    fn note_live_chunk(&mut self) {
        self.emitted_live = true;
    }

    fn should_emit_end_replay(&self) -> bool {
        !self.emitted_live
    }
}

/// Normalize tool arguments coming back from the provider into a JSON object.
///
/// Some OpenAI-compatible providers serialize the arguments as a *string*
/// containing JSON rather than as a native object. The downstream tool
/// dispatcher expects an object with named fields (`args.get("command")`),
/// so we unwrap one level of string-encoding when we recognise it. Anything
/// we can't massage into an object becomes an empty object — that gives the
/// dispatcher a clean "missing parameter" error instead of a panic-y
/// "expected object, got string".
fn normalize_tool_args(args: serde_json::Value) -> serde_json::Value {
    match args {
        serde_json::Value::Object(_) => args,
        serde_json::Value::String(ref s) => match serde_json::from_str::<serde_json::Value>(s) {
            Ok(parsed) if parsed.is_object() => parsed,
            _ => {
                tracing::warn!(
                    target: "provider",
                    raw = %s,
                    "tool args were a string but didn't decode to a JSON object; \
                     coercing to {{}} — dispatcher will surface this as a \
                     missing-parameter error"
                );
                serde_json::Value::Object(serde_json::Map::new())
            }
        },
        serde_json::Value::Null => serde_json::Value::Object(serde_json::Map::new()),
        other => {
            tracing::warn!(
                target: "provider",
                kind = ?other,
                "unexpected tool args shape; coercing to {{}}"
            );
            serde_json::Value::Object(serde_json::Map::new())
        }
    }
}

/// Translate a single `ChatStreamEvent` into the equivalent `TurnEvent`(s).
/// Returns `(events, terminal)` where `terminal == true` indicates the stream
/// should be consumed no further (an `End` event was seen).
///
/// Stateful concerns — tool-call dedup and depth capping — are delegated to
/// `emitter`, so the caller threads the same emitter across every event in
/// one stream lifetime.
fn translate_chat_stream_event(
    event: ChatStreamEvent,
    emitter: &mut ToolCallEmitter,
    reasoning: &mut ReasoningEmissionState,
) -> (Vec<TurnEvent>, bool) {
    let mut out = Vec::new();
    match event {
        ChatStreamEvent::Start => {}
        ChatStreamEvent::Chunk(chunk) => {
            if !chunk.content.is_empty() {
                out.push(TurnEvent::TextDelta(chunk.content));
            }
        }
        ChatStreamEvent::ReasoningChunk(chunk) => {
            if !chunk.content.is_empty() {
                reasoning.note_live_chunk();
                out.push(TurnEvent::Thinking(chunk.content));
            }
        }
        ChatStreamEvent::ThoughtSignatureChunk(_) => {}
        ChatStreamEvent::ToolCallChunk(chunk) => {
            if let Some(ev) = emitter.try_emit(chunk.tool_call) {
                out.push(ev);
            }
        }
        ChatStreamEvent::End(end) => {
            // Skip End's reasoning replay when chunks already streamed it —
            // the model's chunks already populated the TUI's thinking block,
            // and re-emitting the full text creates a duplicate "Thought"
            // node.
            if reasoning.should_emit_end_replay() {
                if let Some(text) = end.captured_reasoning_content {
                    if !text.is_empty() {
                        out.push(TurnEvent::Thinking(text));
                    }
                }
            }
            if let Some(content) = end.captured_content {
                for part in content.into_parts() {
                    if let ContentPart::ToolCall(tc) = part {
                        if let Some(ev) = emitter.try_emit(tc) {
                            out.push(ev);
                        }
                    }
                }
            }
            if let Some(usage) = end.captured_usage.as_ref() {
                out.push(TurnEvent::Usage(usage_to_token_usage(usage)));
            }
            out.push(TurnEvent::Done {
                stop_reason: StopReason::EndTurn,
            });
            return (out, true);
        }
    }
    (out, false)
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
    /// Working directory used to resolve relative `path` arguments when
    /// synthesizing diffs for file-mutating tool calls
    /// (`edit_file`, `write_file`, etc.). Empty `PathBuf` falls back to
    /// `synthesize_diffs` resolving paths verbatim.
    workspace_root: std::path::PathBuf,
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
        Self::with_workspace(
            client,
            model,
            system_prompt,
            tools,
            thinking_budget,
            std::path::PathBuf::new(),
        )
    }

    /// Construct a handle with an explicit workspace root used for
    /// resolving relative file paths in synthesized diffs. The
    /// daemon's `agent_factory` calls this with the session's
    /// working directory; tests can pass any directory.
    pub fn with_workspace(
        client: genai::Client,
        model: ModelIden,
        system_prompt: &str,
        tools: Vec<LlmToolDefinition>,
        thinking_budget: Option<i64>,
        workspace_root: std::path::PathBuf,
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
            workspace_root,
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
        let workspace_root = self.workspace_root.clone();

        let stream = Box::pin(async_stream::stream! {
            let provider_start = std::time::Instant::now();
            tracing::info!(target: "ttft", stage = "provider_call_start", model = %model_name, "ttft");
            let stream_res = client
                .exec_chat_stream(&model_name, request, Some(&options))
                .await;
            tracing::info!(
                target: "ttft",
                stage = "provider_call_returned",
                elapsed_ms = provider_start.elapsed().as_millis() as u64,
                "ttft"
            );
            let mut stream = match stream_res {
                Ok(res) => res.stream,
                Err(err) => {
                    yield TurnEvent::Error(TurnError::Communication(format!(
                        "genai stream start failed: {err}"
                    )));
                    return;
                }
            };
            let mut first_chunk_logged = false;

            let mut tool_emitter = ToolCallEmitter::new(max_tool_depth, workspace_root.clone());
            let mut reasoning_state = ReasoningEmissionState::new();

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

                if !first_chunk_logged {
                    tracing::info!(
                        target: "ttft",
                        stage = "provider_first_chunk",
                        elapsed_ms = provider_start.elapsed().as_millis() as u64,
                        kind = ?std::mem::discriminant(&event),
                        "ttft"
                    );
                    first_chunk_logged = true;
                }
                tracing::info!(
                    target: "ttft",
                    stage = "raw_chat_stream_event",
                    elapsed_ms = provider_start.elapsed().as_millis() as u64,
                    kind = ?std::mem::discriminant(&event),
                    "ttft"
                );

                let (events, terminal) =
                    translate_chat_stream_event(event, &mut tool_emitter, &mut reasoning_state);
                for ev in events {
                    yield ev;
                }
                if terminal {
                    return;
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
                        TurnEvent::ToolCall { ref id, ref name, ref args, .. } => {
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

    // ─── ToolCallEmitter contract ──────────────────────────────────────

    fn tc(id: &str, name: &str) -> ToolCall {
        ToolCall {
            call_id: id.to_string(),
            fn_name: name.to_string(),
            fn_arguments: serde_json::json!({}),
            thought_signatures: None,
        }
    }

    /// Build an emitter with no workspace root — synthesized diffs will
    /// resolve relative paths verbatim, which is fine for tests that
    /// don't exercise diff synthesis (the tool name is a non-edit tool
    /// like "bash" or "read", so synthesis returns an empty Vec).
    fn emitter(max_depth: usize) -> ToolCallEmitter {
        ToolCallEmitter::new(max_depth, std::path::PathBuf::new())
    }

    #[test]
    fn emitter_unwraps_double_encoded_json_string_args() {
        // Some OpenAI-compatible providers (e.g. GLM-style endpoints over
        // OpenAI-compat) return tool call arguments as a *JSON-encoded
        // string* rather than a JSON object. The downstream tool dispatcher
        // expects an object with named fields, so the emitter must unwrap
        // the string-shaped payload before forwarding it.
        let raw = ToolCall {
            call_id: "call-1".to_string(),
            fn_name: "bash".to_string(),
            fn_arguments: serde_json::Value::String(
                r#"{"command":"ls -la","timeout_ms":5000}"#.to_string(),
            ),
            thought_signatures: None,
        };
        let mut e = emitter(10);
        let ev = e.try_emit(raw).expect("must emit");
        match ev {
            TurnEvent::ToolCall { args, .. } => {
                let obj = args.as_object().expect("args must be parsed into object");
                assert_eq!(obj.get("command").and_then(|v| v.as_str()), Some("ls -la"));
                assert_eq!(obj.get("timeout_ms").and_then(|v| v.as_u64()), Some(5000));
            }
            other => panic!("expected ToolCall, got {other:?}"),
        }
    }

    #[test]
    fn emitter_passes_object_args_through_unchanged() {
        let args_obj = serde_json::json!({"command": "ls"});
        let raw = ToolCall {
            call_id: "call-1".to_string(),
            fn_name: "bash".to_string(),
            fn_arguments: args_obj.clone(),
            thought_signatures: None,
        };
        let mut e = emitter(10);
        let ev = e.try_emit(raw).expect("must emit");
        match ev {
            TurnEvent::ToolCall { args, .. } => assert_eq!(args, args_obj),
            other => panic!("expected ToolCall, got {other:?}"),
        }
    }

    #[test]
    fn emitter_leaves_unparseable_string_args_as_object() {
        // String args that aren't valid JSON should still produce an object
        // (empty) so the dispatcher's `args.get("...")` calls don't blow up.
        let raw = ToolCall {
            call_id: "call-1".to_string(),
            fn_name: "bash".to_string(),
            fn_arguments: serde_json::Value::String("not really json".to_string()),
            thought_signatures: None,
        };
        let mut e = emitter(10);
        let ev = e.try_emit(raw).expect("must emit");
        match ev {
            TurnEvent::ToolCall { args, .. } => {
                assert!(args.is_object(), "must coerce to object: got {args:?}");
            }
            other => panic!("expected ToolCall, got {other:?}"),
        }
    }

    #[test]
    fn emitter_emits_first_chunk() {
        let mut e = emitter(10);
        let ev = e.try_emit(tc("call-1", "bash"));
        assert!(matches!(ev, Some(TurnEvent::ToolCall { ref id, .. }) if id == "call-1"));
    }

    #[test]
    fn emitter_dedupes_same_call_id() {
        let mut e = emitter(10);
        assert!(e.try_emit(tc("call-1", "bash")).is_some());
        assert!(
            e.try_emit(tc("call-1", "bash")).is_none(),
            "second emission of same call_id must be deduplicated"
        );
    }

    #[test]
    fn emitter_distinct_ids_both_emit() {
        let mut e = emitter(10);
        assert!(e.try_emit(tc("a", "bash")).is_some());
        assert!(e.try_emit(tc("b", "read")).is_some());
    }

    #[test]
    fn emitter_caps_at_max_depth() {
        let mut e = emitter(2);
        assert!(e.try_emit(tc("a", "x")).is_some());
        assert!(e.try_emit(tc("b", "x")).is_some());
        assert!(
            e.try_emit(tc("c", "x")).is_none(),
            "third call past max_depth must not emit"
        );
    }

    #[test]
    fn emitter_chunk_then_end_no_double_emit() {
        // Real-world: provider streams ToolCallChunk live AND replays the
        // same tool calls in captured_content at End. Emitter dedupes by id.
        let mut e = emitter(10);
        let chunk_ev = e.try_emit(tc("call-1", "bash"));
        assert!(chunk_ev.is_some());
        let end_ev = e.try_emit(tc("call-1", "bash"));
        assert!(
            end_ev.is_none(),
            "End-time replay of already-emitted tool call must be skipped"
        );
    }

    #[test]
    fn emitter_end_only_path_still_works() {
        // Provider that does NOT emit ToolCallChunks (only End): emitter
        // sees the tool calls for the first time at End and emits.
        let mut e = emitter(10);
        let ev = e.try_emit(tc("call-1", "bash"));
        assert!(ev.is_some());
        assert_eq!(e.emitted_count(), 1);
    }

    #[test]
    fn emitter_synthesizes_diff_for_write_tool() {
        // Regression: a `write_file` (or similar) tool call should arrive at
        // the TUI scrollback with `diffs` populated so the user sees the
        // pending file contents alongside the call header. The synthesizer
        // is pure and gracefully degrades to empty for unknown tools, but
        // for known edit-style tools it must produce one entry per file.
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let mut e = ToolCallEmitter::new(10, tmp.path().to_path_buf());

        let raw = ToolCall {
            call_id: "call-1".to_string(),
            fn_name: "write_file".to_string(),
            fn_arguments: serde_json::json!({
                "path": "new_file.txt",
                "content": "hello world\n",
            }),
            thought_signatures: None,
        };
        let ev = e.try_emit(raw).expect("must emit ToolCall");
        match ev {
            TurnEvent::ToolCall { diffs, .. } => {
                assert_eq!(diffs.len(), 1, "should synthesize one FileDiff");
                let diff = &diffs[0];
                // synthesize_diffs resolves relative paths against
                // workspace_root, so the path should be absolute.
                assert!(
                    diff.path.ends_with("new_file.txt"),
                    "diff path: {}",
                    diff.path
                );
                // File didn't exist on disk → old_content is None
                // (treated as a "create").
                assert!(diff.old_content.is_none());
                assert_eq!(diff.new_content, "hello world\n");
            }
            other => panic!("expected ToolCall, got {other:?}"),
        }
    }

    #[test]
    fn emitter_emits_empty_diffs_for_non_file_tools() {
        // For tools that aren't file-mutating (`bash`, `read`, etc.),
        // synthesize_diffs returns an empty Vec — emitter forwards that
        // unchanged so the TUI doesn't render a diff section.
        let mut e = emitter(10);
        let ev = e.try_emit(tc("call-1", "bash")).expect("must emit");
        match ev {
            TurnEvent::ToolCall { diffs, .. } => {
                assert!(
                    diffs.is_empty(),
                    "non-file-mutating tools must not synthesize diffs"
                );
            }
            other => panic!("expected ToolCall, got {other:?}"),
        }
    }

    // ─── translate_chat_stream_event wiring ────────────────────────────

    use genai::chat::{MessageContent, StreamChunk, StreamEnd, ToolChunk};

    fn drive_translate(events: Vec<ChatStreamEvent>) -> Vec<TurnEvent> {
        let mut emitter = self::emitter(10);
        let mut reasoning = ReasoningEmissionState::new();
        let mut out = Vec::new();
        for ev in events {
            let (translated, terminal) =
                translate_chat_stream_event(ev, &mut emitter, &mut reasoning);
            out.extend(translated);
            if terminal {
                break;
            }
        }
        out
    }

    #[test]
    fn reasoning_chunk_then_end_replay_does_not_duplicate() {
        // Regression: providers that stream reasoning via ReasoningChunk
        // ALSO put the same content in End.captured_reasoning_content. That
        // replay would previously emit a second TurnEvent::Thinking, causing
        // the TUI to render two identical "Thought (N words)" blocks.
        let mut emitter = self::emitter(10);
        let mut state = ReasoningEmissionState::new();

        let chunk = ChatStreamEvent::ReasoningChunk(StreamChunk {
            content: "deliberate reasoning".to_string(),
        });
        let (live, _) = translate_chat_stream_event(chunk, &mut emitter, &mut state);
        assert_eq!(live.len(), 1);
        assert!(matches!(live[0], TurnEvent::Thinking(ref s) if s == "deliberate reasoning"));

        let end = ChatStreamEvent::End(StreamEnd {
            captured_usage: None,
            captured_content: None,
            captured_reasoning_content: Some("deliberate reasoning".to_string()),
        });
        let (events, terminal) = translate_chat_stream_event(end, &mut emitter, &mut state);
        assert!(terminal);
        let thinking_replay: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, TurnEvent::Thinking(_)))
            .collect();
        assert!(
            thinking_replay.is_empty(),
            "End must not replay reasoning when chunks already streamed it: {:?}",
            events
        );
    }

    #[test]
    fn end_only_provider_still_emits_reasoning() {
        // Provider that only delivers reasoning at End (no live chunks)
        // must still surface it once.
        let mut emitter = self::emitter(10);
        let mut state = ReasoningEmissionState::new();

        let end = ChatStreamEvent::End(StreamEnd {
            captured_usage: None,
            captured_content: None,
            captured_reasoning_content: Some("after-the-fact reasoning".to_string()),
        });
        let (events, _) = translate_chat_stream_event(end, &mut emitter, &mut state);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TurnEvent::Thinking(s) if s == "after-the-fact reasoning")),
            "End-only provider must still emit reasoning: {:?}",
            events
        );
    }

    #[test]
    fn translate_text_chunk_yields_text_delta() {
        let out = drive_translate(vec![ChatStreamEvent::Chunk(StreamChunk {
            content: "hello".to_string(),
        })]);
        assert!(matches!(&out[..], [TurnEvent::TextDelta(s)] if s == "hello"));
    }

    #[test]
    fn translate_empty_text_chunk_yields_nothing() {
        let out = drive_translate(vec![ChatStreamEvent::Chunk(StreamChunk {
            content: String::new(),
        })]);
        assert!(out.is_empty());
    }

    #[test]
    fn translate_reasoning_chunk_yields_thinking() {
        let out = drive_translate(vec![ChatStreamEvent::ReasoningChunk(StreamChunk {
            content: "thinking".to_string(),
        })]);
        assert!(matches!(&out[..], [TurnEvent::Thinking(s)] if s == "thinking"));
    }

    #[test]
    fn translate_tool_call_chunk_emits_live_tool_call() {
        // Regression: previously ToolCallChunk was a no-op, and tool calls
        // only fired at End. Now each chunk emits a TurnEvent::ToolCall
        // immediately so the user sees a permission prompt as soon as the
        // tool call's args finish streaming.
        let out = drive_translate(vec![ChatStreamEvent::ToolCallChunk(ToolChunk {
            tool_call: tc("call-1", "bash"),
        })]);
        assert!(
            matches!(&out[..], [TurnEvent::ToolCall { id, name, .. }] if id == "call-1" && name == "bash"),
            "ToolCallChunk must emit a live TurnEvent::ToolCall: got {:?}",
            out
        );
    }

    #[test]
    fn translate_three_tool_call_chunks_emit_in_order() {
        // Parallel tool batch: each chunk's permission prompt should fire
        // as soon as that tool call streams in.
        let out = drive_translate(vec![
            ChatStreamEvent::ToolCallChunk(ToolChunk {
                tool_call: tc("a", "bash"),
            }),
            ChatStreamEvent::ToolCallChunk(ToolChunk {
                tool_call: tc("b", "read_file"),
            }),
            ChatStreamEvent::ToolCallChunk(ToolChunk {
                tool_call: tc("c", "grep"),
            }),
        ]);
        assert_eq!(out.len(), 3);
        let ids: Vec<&str> = out
            .iter()
            .filter_map(|e| match e {
                TurnEvent::ToolCall { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(ids, ["a", "b", "c"]);
    }

    #[test]
    fn translate_chunk_then_end_no_double_emission() {
        // Provider that streams ToolCallChunks AND replays them in End's
        // captured_content (the genai default with capture_tool_calls=true).
        // The emitter must dedupe so the same tool call only fires once.
        let live_call = tc("call-1", "bash");
        let end_replay = ContentPart::ToolCall(tc("call-1", "bash"));
        let out = drive_translate(vec![
            ChatStreamEvent::ToolCallChunk(ToolChunk {
                tool_call: live_call,
            }),
            ChatStreamEvent::End(StreamEnd {
                captured_usage: None,
                captured_content: Some(MessageContent::from_parts(vec![end_replay])),
                captured_reasoning_content: None,
            }),
        ]);
        let tool_calls: Vec<_> = out
            .iter()
            .filter(|e| matches!(e, TurnEvent::ToolCall { .. }))
            .collect();
        assert_eq!(
            tool_calls.len(),
            1,
            "live + replay must dedupe to one TurnEvent::ToolCall: {:?}",
            out
        );
        // Done must still fire from End.
        assert!(out.iter().any(|e| matches!(e, TurnEvent::Done { .. })));
    }

    #[test]
    fn translate_end_only_provider_still_emits_tool_calls() {
        // Provider that only delivers tool calls in End (older behavior).
        let out = drive_translate(vec![ChatStreamEvent::End(StreamEnd {
            captured_usage: None,
            captured_content: Some(MessageContent::from_parts(vec![
                ContentPart::ToolCall(tc("call-1", "bash")),
                ContentPart::ToolCall(tc("call-2", "read_file")),
            ])),
            captured_reasoning_content: None,
        })]);
        let ids: Vec<&str> = out
            .iter()
            .filter_map(|e| match e {
                TurnEvent::ToolCall { id, .. } => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(ids, ["call-1", "call-2"]);
    }

    #[test]
    fn translate_end_yields_done_terminal() {
        let mut emitter = self::emitter(10);
        let mut reasoning = ReasoningEmissionState::new();
        let (out, terminal) = translate_chat_stream_event(
            ChatStreamEvent::End(StreamEnd {
                captured_usage: None,
                captured_content: None,
                captured_reasoning_content: None,
            }),
            &mut emitter,
            &mut reasoning,
        );
        assert!(terminal, "End must be terminal");
        assert!(matches!(out.last(), Some(TurnEvent::Done { .. })));
    }

    #[test]
    fn translate_pre_end_events_are_not_terminal() {
        let mut emitter = self::emitter(10);
        let mut reasoning = ReasoningEmissionState::new();
        for ev in [
            ChatStreamEvent::Start,
            ChatStreamEvent::Chunk(StreamChunk {
                content: "x".to_string(),
            }),
            ChatStreamEvent::ReasoningChunk(StreamChunk {
                content: "y".to_string(),
            }),
            ChatStreamEvent::ToolCallChunk(ToolChunk {
                tool_call: tc("zz", "bash"),
            }),
        ] {
            let (_out, terminal) = translate_chat_stream_event(ev, &mut emitter, &mut reasoning);
            assert!(!terminal, "non-End events must not be terminal");
        }
    }

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
                diffs: Vec::new(),
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
                diffs: Vec::new(),
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
