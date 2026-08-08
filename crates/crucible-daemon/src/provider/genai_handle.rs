use async_trait::async_trait;
use crucible_core::session::{ContextStrategy, OutputValidation};
use crucible_core::traits::chat::{
    AgentHandle, ChatError, ChatResult, ChatToolCall, ChatToolResult,
};
use crucible_core::traits::llm::LlmToolDefinition;
use crucible_core::traits::TokenUsage;
use crucible_core::turn::{StopReason, TurnError, TurnEvent};
use crucible_core::types::acp::schema::{SessionMode, SessionModeId, SessionModeState};
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

/// Attached tool-schema token estimate above this share of the effective
/// context budget triggers progressive tool disclosure: deferrable tools are
/// dropped from the request and replaced by the discovery bridge.
const TOOL_SCHEMA_BUDGET_SHARE: f64 = 0.15;

/// Effective-budget fallback used when a session sets neither `context_budget`
/// nor `context_window`. A conservative modern context size.
const DEFAULT_ASSUMED_CONTEXT: usize = 128_000;

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
fn apply_prompt_caching(
    system_prompt: &str,
    session_context: &str,
    messages: &mut Vec<ChatMessage>,
) {
    // Mark second-to-last message with cache control (the last msg before current user turn).
    // This creates a cache breakpoint at the end of the prior conversation, so the entire
    // prefix up to this point is cached on subsequent turns.
    if messages.len() >= 2 {
        let idx = messages.len() - 2;
        let msg = messages[idx].clone().with_options(CacheControl::Ephemeral);
        messages[idx] = msg;
    }

    // Two system messages, each with its own breakpoint, stable one first.
    // genai collects every system-role message into Anthropic's `system` array
    // keeping per-message cache control (`adapter_shared.rs`), so this is two
    // real breakpoints rather than one — and nothing is dropped.
    //
    // The split is the point. A cache entry is keyed on the whole prefix up to
    // its breakpoint, so with a single breakpoint covering both halves, a
    // different workspace missed on the persona and the project rules too.
    // Providers that cache prefixes automatically (OpenAI) benefit from the
    // ordering alone.
    if !session_context.is_empty() {
        let ctx_msg = ChatMessage::system(session_context).with_options(CacheControl::Ephemeral);
        messages.insert(0, ctx_msg);
    }
    // genai's with_system() doesn't support MessageOptions, but system-role
    // ChatMessages do.
    if !system_prompt.is_empty() {
        let system_msg = ChatMessage::system(system_prompt).with_options(CacheControl::Ephemeral);
        messages.insert(0, system_msg);
    }
}

/// What the model is shown for one finished tool call.
///
/// Every failure path sets `result` empty and puts the text in `error` —
/// permission denials, dispatch timeouts, containment refusals, unknown tools,
/// plugin cancels. This function used to be the line `result.result`, so the
/// model received `""` for all of them and either repeated the identical call
/// or invented an outcome, while the TUI was shown the real message through a
/// separate event channel. Every carefully-worded refusal in this codebase was
/// addressed to a reader that never saw it.
fn tool_response_payload(result: &ChatToolResult) -> String {
    match &result.error {
        None => result.result.clone(),
        // Content AND an error: a non-zero exit with output on the way is
        // still output. Keep both rather than choosing.
        Some(err) if !result.result.is_empty() => {
            format!("{}\n\nError: {err}", result.result)
        }
        Some(err) => format!("Error: {err}"),
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
}

impl ToolCallEmitter {
    fn new(max_depth: usize) -> Self {
        Self {
            emitted_call_ids: std::collections::HashSet::new(),
            emitted_count: 0,
            max_depth,
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
        let diffs = crate::tools::diff_synth::synthesize_diffs(&tc.fn_name, &normalized_args);
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
    /// This session's workspace/kiln header. Held apart from `system_prompt`
    /// so the stable half can be cached across sessions; see
    /// `apply_prompt_caching`.
    session_context: String,
    tools: Vec<LlmToolDefinition>,
    mode_state: SessionModeState,
    current_mode_id: String,
    /// Lua-declared modes, for the per-request tool filter. `None` (tests,
    /// direct constructions) keeps the shipped plan-mode behaviour.
    modes: Option<crucible_lua::ModeRegistry>,
    mode_context_sent: bool,
    max_tool_depth: usize,
    thinking_budget: Option<i64>,
    /// Sampling temperature for every request this handle issues. `None`
    /// leaves it to the provider's own default rather than picking one here —
    /// providers disagree on what neutral means.
    temperature: Option<f64>,
    /// Cap on generated tokens per response. `None` leaves it to the provider.
    max_tokens: Option<u32>,
    context_budget: Option<usize>,
    context_strategy: ContextStrategy,
    context_window: Option<usize>,
    output_validation: OutputValidation,
    validation_retries: u32,
    autocompact_threshold: Option<f32>,
    /// Session scope for diff previews of file-mutating tool calls
    /// (`edit_file`, `write_file`, etc.): relative `path` arguments resolve
    /// against it, and it also **bounds** what the preview may open — a
    /// target `synthesize_diffs` cannot prove is inside it yields no diff at
    /// all. An empty `PathBuf` is therefore deny-all, not "resolve against
    /// the daemon's CWD"; it does not canonicalize, so every path is refused.
    /// That matters here because this emitter has no permission gate in front
    /// of it, so a preview read is published unconditionally.
    /// Tool names eligible for progressive disclosure. The daemon's agent
    /// factory populates this with the gateway (user MCP) tool names; kiln
    /// and workspace tools are never deferrable. Empty means the handle
    /// never defers, regardless of schema size.
    deferrable_tool_names: std::collections::HashSet<String>,
    /// Plugin-declared tool names among `tools`. Plan mode excludes these
    /// per-request — the write-name blocklist cannot classify a plugin
    /// tool's side effects, so plan fails closed on all of them.
    plugin_tool_names: std::collections::HashSet<String>,
}

/// The tool set attached to a single request, plus how many tools were
/// deferred behind the discovery bridge (zero when no deferral occurred).
struct VisibleToolSet {
    tools: Vec<LlmToolDefinition>,
    deferred_count: usize,
}

/// Rough token cost of attaching `defs` as function schemas: the serialized
/// name, description, and parameter schema, via the shared chars/4 heuristic.
fn tool_schema_tokens(defs: &[LlmToolDefinition]) -> usize {
    use crucible_core::traits::context_ops::estimate_tokens;
    defs.iter()
        .map(|d| {
            let schema_tokens = d
                .function
                .parameters
                .as_ref()
                .map(|p| p.to_string().len().div_ceil(4))
                .unwrap_or(0);
            estimate_tokens(&d.function.name)
                + estimate_tokens(&d.function.description)
                + schema_tokens
        })
        .sum()
}

/// The three discovery-bridge tool definitions attached in place of the
/// deferred tools. `discover_tools`/`get_tool_schema` mirror
/// `ExtendedMcpServer::discovery_tools()`; `invoke_tool` is a generic proxy
/// the daemon unwraps to the real tool *before* hooks and permissions run.
fn bridge_tool_defs() -> Vec<LlmToolDefinition> {
    use serde_json::json;
    vec![
        LlmToolDefinition::new(
            "discover_tools",
            "Search available tools by name, description, or source. Some tools are \
             deferred to save context — use this to find them before calling them with \
             invoke_tool.",
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query to filter by name or description"
                    },
                    "source": {
                        "type": "string",
                        "description": "Filter by tool source"
                    },
                    "limit": {
                        "type": "integer",
                        "default": 50,
                        "description": "Maximum results to return"
                    }
                }
            }),
        ),
        LlmToolDefinition::new(
            "get_tool_schema",
            "Get the full JSON Schema for a specific tool's input parameters.",
            json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The name of the tool to get schema for"
                    }
                },
                "required": ["name"]
            }),
        ),
        LlmToolDefinition::new(
            "invoke_tool",
            "Call a deferred tool by name. Routes through the normal permission and hook \
             pipeline exactly as a direct call would. Use discover_tools and get_tool_schema \
             first to find the tool and its parameters.",
            json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The exact name of the tool to invoke"
                    },
                    "args": {
                        "type": "object",
                        "description": "Arguments object for the tool, matching its input schema"
                    }
                },
                "required": ["name"]
            }),
        ),
    ]
}

/// System-prompt line appended when deferral is active for a request. Kept
/// static except for the count so the prompt stays cache-friendly.
fn deferral_prompt_note(count: usize) -> String {
    format!(
        "{count} additional tools are deferred to save context. Find them with \
         discover_tools, inspect with get_tool_schema, call with invoke_tool."
    )
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

/// Outcome of `enforce_context_budget`. The Summarize variant carries
/// the drained messages so an async caller can replace the inserted
/// placeholder with an LLM-generated recap before the actual
/// completion request goes out.
pub(crate) enum BudgetAction {
    /// Budget was respected without changes (no budget set, or under).
    NoChange,
    /// Mutated in place (Truncate or SlidingWindow). The caller has
    /// nothing to do.
    Mutated,
    /// Summarize selected: a placeholder was inserted at
    /// `placeholder_idx`, and `drained` holds the messages that were
    /// removed. The async caller may replace the placeholder content
    /// with an LLM-generated recap, or leave it as-is on error.
    NeedsSummarize {
        placeholder_idx: usize,
        drained: Vec<ChatMessage>,
    },
}

/// Drop oldest non-system messages until the running token estimate is at or
/// under `budget`, always keeping the system prefix and the final (current)
/// message.
fn truncate_to_budget(messages: &mut Vec<ChatMessage>, budget: usize) {
    while messages.iter().map(estimate_message_tokens).sum::<usize>() > budget && messages.len() > 2
    {
        let Some(idx) = messages
            .iter()
            .position(|m| m.role != genai::chat::ChatRole::System)
        else {
            break;
        };
        // Don't remove the last message (current user turn).
        if idx >= messages.len() - 1 {
            break;
        }
        messages.remove(idx);
    }
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
) -> BudgetAction {
    let Some(budget) = budget else {
        return BudgetAction::NoChange;
    };

    let current: usize = messages.iter().map(estimate_message_tokens).sum();
    if current <= budget {
        return BudgetAction::NoChange;
    }

    match strategy {
        ContextStrategy::Truncate => {
            truncate_to_budget(messages, budget);
            BudgetAction::Mutated
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
            BudgetAction::Mutated
        }
        ContextStrategy::Summarize => {
            // Drain old non-system non-keep messages, insert a static
            // placeholder, and return the drained vec so an async caller
            // can replace the placeholder with an LLM summary. Ignoring
            // the returned `NeedsSummarize` is safe — the placeholder
            // is itself a usable (if static) elision marker.
            let keep = window.unwrap_or(10);
            let keep_count = keep * 2;
            let system_count = messages
                .iter()
                .take_while(|m| m.role == genai::chat::ChatRole::System)
                .count();
            let action = if messages.len() > system_count + keep_count {
                let drain_end = messages.len() - keep_count;
                let n_dropped = drain_end - system_count;
                let drained: Vec<ChatMessage> = messages.drain(system_count..drain_end).collect();
                let placeholder = ChatMessage::system(format!(
                    "[summary placeholder] {n_dropped} earlier turn{} elided to fit context budget",
                    if n_dropped == 1 { "" } else { "s" }
                ));
                messages.insert(system_count, placeholder);
                BudgetAction::NeedsSummarize {
                    placeholder_idx: system_count,
                    drained,
                }
            } else {
                BudgetAction::NoChange
            };
            // Belt-and-braces: if even the kept window plus the
            // placeholder + system prompt exceed the budget, fall back
            // to Truncate behaviour to avoid over-budget prompts.
            truncate_to_budget(messages, budget);
            action
        }
    }
}

/// System prompt used for LLM-driven summarization in the Summarize
/// context strategy. Kept brief; the conversation transcript is passed
/// in the user message and dwarfs this anyway.
const SUMMARIZE_SYSTEM_PROMPT: &str =
    "You are summarizing earlier turns of an ongoing conversation so the assistant can keep \
     context after older messages are dropped. Produce a concise factual recap of the \
     transcript below. Preserve names, decisions, file paths, code references, and unresolved \
     questions. Use 3-6 sentences. No preamble, no closing, no quotes — just the recap.";

/// Format drained messages as a transcript for summarization.
fn drained_transcript(drained: &[ChatMessage]) -> String {
    drained
        .iter()
        .map(|m| {
            let role = match m.role {
                genai::chat::ChatRole::System => "system",
                genai::chat::ChatRole::User => "user",
                genai::chat::ChatRole::Assistant => "assistant",
                genai::chat::ChatRole::Tool => "tool",
            };
            let body: String = m
                .content
                .parts()
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text(t) => Some(t.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");
            format!("{role}: {body}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Ask the same backend the agent uses to summarize `drained` into a
/// single recap string. The genai client is shared (cheap clone). On
/// any error this returns `Err`; the caller falls back to keeping the
/// static placeholder rather than propagating the failure into the
/// user's turn.
/// Explicit routing name for a model: if it already carries a namespace
/// (contains `::`), use it as-is; otherwise prefix the adapter kind so
/// genai routes by configuration, not by model-name prefix heuristics.
pub(crate) fn explicit_model_name(model: &ModelIden) -> String {
    if model.model_name.contains("::") {
        model.model_name.to_string()
    } else {
        format!(
            "{}::{}",
            model.adapter_kind.as_lower_str(),
            &*model.model_name
        )
    }
}

pub(crate) async fn summarize_via_backend(
    client: &genai::Client,
    model_name: &str,
    drained: &[ChatMessage],
) -> Result<String, String> {
    if drained.is_empty() {
        return Ok(String::new());
    }
    let transcript = drained_transcript(drained);
    let request = ChatRequest::new(vec![
        ChatMessage::system(SUMMARIZE_SYSTEM_PROMPT),
        ChatMessage::user(transcript),
    ]);
    let options = ChatOptions::default().with_capture_content(true);
    let resp = client
        .exec_chat(model_name, request, Some(&options))
        .await
        .map_err(|e| format!("summarize call failed: {e}"))?;
    let text: String = resp.content.texts().join("");
    if text.trim().is_empty() {
        Err("summarize call returned empty content".to_string())
    } else {
        Ok(text)
    }
}

impl GenaiAgentHandle {
    /// Attach this session's workspace/kiln header.
    ///
    /// Kept out of `system_prompt` so the stable half can be cached across
    /// sessions and projects rather than invalidated by a path.
    pub fn with_session_context(mut self, session_context: String) -> Self {
        self.session_context = session_context;
        self
    }

    /// Attach the Lua mode registry.
    ///
    /// Two jobs, and the second was missing: a declared mode's `tools`
    /// selector becomes the per-request filter, AND `mode_state` is rebuilt
    /// so `set_mode_str` will actually *accept* a declared mode. Without the
    /// rebuild the handle validated against `default_internal_modes()`, so a
    /// user-defined mode was rejected on every handle re-creation — the
    /// session reported one mode and enforced another, with only a `warn!`
    /// on the resume path to show for it.
    ///
    /// An empty registry keeps the shipped built-ins: a missing registry must
    /// restrict, not open up.
    #[must_use]
    pub fn with_modes(mut self, modes: crucible_lua::ModeRegistry) -> Self {
        let declared = modes.all();
        if !declared.is_empty() {
            let available = declared
                .iter()
                .map(|m| {
                    let title = {
                        let mut c = m.name.chars();
                        match c.next() {
                            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                            None => m.name.clone(),
                        }
                    };
                    let mode = SessionMode::new(SessionModeId::new(m.name.as_str()), title);
                    match &m.description {
                        Some(d) => mode.description(d.clone()),
                        None => mode,
                    }
                })
                .collect();
            // Keep the handle's current mode if it is still declared;
            // otherwise fall back to the first declared one rather than
            // leaving a `current_mode_id` no longer in `available_modes`.
            let current = if declared.iter().any(|m| m.name == self.current_mode_id) {
                self.current_mode_id.clone()
            } else {
                declared[0].name.clone()
            };
            self.current_mode_id = current.clone();
            self.mode_state =
                SessionModeState::new(SessionModeId::new(current.as_str()), available);
        }
        self.modes = Some(modes);
        self
    }

    /// Construct a handle with an explicit workspace root used for
    /// containing the file paths a synthesized diff may read. The
    /// daemon's `agent_factory` calls this with the session's working
    /// directory; tests can pass any directory. An empty or relative
    /// `PathBuf` now DENIES every preview rather than resolving against
    /// the daemon's CWD — that fallback was the exfiltration path, since
    /// this emitter runs on tool-call emission with no permission gate
    /// anywhere downstream of it.
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
            session_context: String::new(),
            tools,
            mode_state,
            current_mode_id,
            modes: None,
            mode_context_sent: false,
            max_tool_depth: usize::MAX,
            thinking_budget,
            temperature: None,
            max_tokens: None,
            context_budget: None,
            context_strategy: ContextStrategy::default(),
            context_window: None,
            output_validation: OutputValidation::default(),
            validation_retries: 3,
            autocompact_threshold: None,
            deferrable_tool_names: std::collections::HashSet::new(),
            plugin_tool_names: std::collections::HashSet::new(),
        }
    }

    /// Declare which attached tools may be deferred behind the discovery
    /// bridge when their schemas would consume too much of the context
    /// budget. The daemon's agent factory passes the gateway (user MCP) tool
    /// names; kiln and workspace tools are never deferrable.
    pub fn with_deferrable_tools(mut self, names: std::collections::HashSet<String>) -> Self {
        self.deferrable_tool_names = names;
        self
    }

    /// Sampling settings the session chose. `None` for either leaves that one
    /// to the provider's default.
    #[must_use]
    pub fn with_generation_settings(
        mut self,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
    ) -> Self {
        self.temperature = temperature;
        self.max_tokens = max_tokens;
        self
    }

    /// Context-window settings the session chose: the budget to keep under,
    /// what to do when it is exceeded, and the model's window when the
    /// provider does not report one.
    #[must_use]
    pub fn with_context_settings(
        mut self,
        budget: Option<usize>,
        strategy: ContextStrategy,
        window: Option<usize>,
    ) -> Self {
        self.context_budget = budget;
        self.context_strategy = strategy;
        self.context_window = window;
        self
    }

    /// The per-request generation settings, as one place instead of inline in
    /// the stream body — the only way to assert them without a live provider.
    ///
    /// Each `Option` is applied only when set, so an unset value means "the
    /// provider decides" rather than a value chosen here.
    fn build_chat_options(&self) -> ChatOptions {
        let mut options = ChatOptions::default()
            .with_capture_tool_calls(true)
            .with_capture_content(true)
            .with_capture_usage(true)
            .with_capture_reasoning_content(true);
        if let Some(budget) = self.thinking_budget {
            options = options.with_reasoning_effort(ReasoningEffort::Budget(
                budget.clamp(0, u32::MAX as i64) as u32,
            ));
        }
        if let Some(temperature) = self.temperature {
            options = options.with_temperature(temperature);
        }
        if let Some(max_tokens) = self.max_tokens {
            options = options.with_max_tokens(max_tokens);
        }
        options
    }

    /// Declare which attached tools are plugin-declared. Plan mode excludes
    /// them categorically — their side effects are unknown, so the write-name
    /// blocklist cannot classify them — and per-request, so a session
    /// switched to plan mid-run stops advertising them (the mode captured at
    /// creation used to be the only filter).
    pub fn with_plugin_tools(mut self, names: std::collections::HashSet<String>) -> Self {
        self.plugin_tool_names = names;
        self
    }

    /// Compute the tool set for a single request: apply the plan-mode filter,
    /// then — if a deferrable set is present and the mode-filtered schemas
    /// exceed the budget share — drop the deferrable defs and append the
    /// discovery bridge. Pure over `&self`; the decision is recomputed each
    /// request so runtime `context_budget` changes are respected.
    fn visible_tools(&self) -> VisibleToolSet {
        let in_plan = self.current_mode_id == "plan";
        // A declared mode's `tools` selector narrows the set first. It can only
        // narrow: plan's write-name blocklist and the plugin-tool exclusion
        // below still apply on top, so a mode cannot widen its way past the
        // floor.
        let declared = self
            .modes
            .as_ref()
            .and_then(|m| m.get(&self.current_mode_id));
        // A registry is attached but does not know this mode: the session is
        // running a mode whose declaration has gone away (edited init.lua,
        // different machine, shadowed defaults). We cannot know what it
        // permitted, so fail CLOSED — apply the same write-tool floor plan
        // mode uses. Returning "no selector" here let the most restrictive
        // mode silently become the most permissive.
        // Built-in modes need no declaration: an un-configured daemon (defaults
        // not yet loaded, failed to parse, shadowed) is in `normal`, and
        // treating that as "unknown" stripped every agent's tools. Emptiness
        // cannot be the discriminator — removing the last declaration empties
        // the registry too.
        let mode_unknown = self.modes.is_some()
            && declared.is_none()
            && !crate::tools::tool_modes::is_builtin_mode(&self.current_mode_id);
        let mode_selector = declared.map(|m| m.tools);

        // Native attach candidates after the plan-mode filter: the write-name
        // blocklist for built-ins, plus plugin tools the mode has not named
        // (unknown side effects — plan mode fails closed on them). This set
        // still contains gateway tools; the budget trigger is computed over it
        // so a large upstream tool set forces deferral even in plan mode.
        //
        // The admission rule has to be applied HERE as well as at dispatch.
        // Lifting only the dispatch ban left a tool the operator had named
        // absent from the advertised set, so the model never saw it and the
        // grant did nothing — a half-landed permission is worse than none,
        // because the config says it worked.
        let selected: Vec<LlmToolDefinition> = match &mode_selector {
            Some(selector) => self
                .tools
                .iter()
                .filter(|t| selector.matches(&t.function.name))
                .cloned()
                .collect(),
            None => self.tools.clone(),
        };
        let write_filtered: Vec<LlmToolDefinition> = if in_plan || mode_unknown {
            selected
                .iter()
                .filter(|t| {
                    let is_plugin = self.plugin_tool_names.contains(&t.function.name);
                    // An UNKNOWN mode has no declaration, so nothing can have
                    // granted anything and every plugin tool is stripped —
                    // `plugin_tool_barred` alone would not do it, because it
                    // answers only for `plan` and returns false for any other
                    // mode id. Wiring it in without this arm quietly reopened
                    // plugin tools for a mode that had gone away, which is the
                    // most permissive possible reading of a missing config.
                    let barred = if mode_unknown {
                        is_plugin
                    } else {
                        crate::tools::tool_modes::plugin_tool_barred(
                            &self.current_mode_id,
                            &t.function.name,
                            &self.plugin_tool_names,
                            self.modes.as_ref(),
                        )
                    };
                    !is_write_tool_name(&t.function.name) && !barred
                })
                .cloned()
                .collect()
        } else {
            selected
        };

        if self.deferrable_tool_names.is_empty() {
            return VisibleToolSet {
                tools: write_filtered,
                deferred_count: 0,
            };
        }

        let effective_budget = self
            .context_budget
            .or(self.context_window)
            .unwrap_or(DEFAULT_ASSUMED_CONTEXT);
        let threshold = (TOOL_SCHEMA_BUDGET_SHARE * effective_budget as f64) as usize;
        let over_budget = tool_schema_tokens(&write_filtered) > threshold;

        // Drop deferrable (gateway/user MCP) tools when the schemas exceed the
        // budget share, in plan mode, or in a mode whose declaration vanished.
        // Plan mode excludes upstream tools categorically — fail-closed,
        // because we can't tell which upstream tools mutate state and plan mode
        // must stay read-only. An unknown mode needs the same treatment for the
        // same reason: the write-name blocklist above is built from OUR tool
        // names and matches none of `gh_create_pr`, `jira_delete_issue`,
        // `linear_update_issue`. The bridge's invoke_tool enforces the ban for
        // plan; this is the advertisement half.
        if !over_budget && !in_plan && !mode_unknown {
            return VisibleToolSet {
                tools: write_filtered,
                deferred_count: 0,
            };
        }

        let before = write_filtered.len();
        let mut kept: Vec<LlmToolDefinition> = write_filtered
            .into_iter()
            .filter(|t| !self.deferrable_tool_names.contains(&t.function.name))
            .collect();
        let dropped = before - kept.len();
        if dropped == 0 {
            return VisibleToolSet {
                tools: kept,
                deferred_count: 0,
            };
        }

        // Attach the discovery bridge only when the drop was budget-driven. A
        // purely categorical plan-mode drop (under budget) leaves no bridge:
        // upstream tools are disabled, not deferred, and invoke_tool would deny
        // them anyway — so there's nothing to reach through the bridge.
        if over_budget {
            kept.extend(bridge_tool_defs());
            VisibleToolSet {
                tools: kept,
                deferred_count: dropped,
            }
        } else {
            VisibleToolSet {
                tools: kept,
                deferred_count: 0,
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn visible_tool_names_for_test(&self) -> (Vec<String>, usize) {
        let visible = self.visible_tools();
        (
            visible
                .tools
                .iter()
                .map(|t| t.function.name.clone())
                .collect(),
            visible.deferred_count,
        )
    }

    fn explicit_model_name(&self) -> String {
        explicit_model_name(&self.model)
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
        let visible = self.visible_tools();
        let system_prompt = if visible.deferred_count > 0 {
            format!(
                "{}\n\n{}",
                self.system_prompt,
                deferral_prompt_note(visible.deferred_count)
            )
        } else {
            self.system_prompt.clone()
        };
        apply_prompt_caching(&system_prompt, &self.session_context, &mut messages);

        let req_tools: Vec<Tool> = visible
            .tools
            .iter()
            .map(super::tool_bridge::llm_tool_to_genai)
            .collect();

        let options = self.build_chat_options();

        let client = self.client.clone();
        let model_name = self.explicit_model_name();
        let max_tool_depth = self.max_tool_depth;
        let context_budget = self.context_budget;
        let context_strategy = self.context_strategy.clone();
        let context_window = self.context_window;

        let stream = Box::pin(async_stream::stream! {
            // Budget enforcement happens here (inside async) so the
            // Summarize strategy can `.await` the LLM. Truncate /
            // SlidingWindow are sync transforms and complete instantly.
            let action = enforce_context_budget(
                &mut messages,
                context_budget,
                &context_strategy,
                context_window,
            );
            if let BudgetAction::NeedsSummarize { placeholder_idx, drained } = action {
                match summarize_via_backend(&client, &model_name, &drained).await {
                    Ok(summary) if !summary.trim().is_empty() => {
                        let n = drained.len();
                        messages[placeholder_idx] = ChatMessage::system(format!(
                            "[summary of {} earlier turn{}] {}",
                            n,
                            if n == 1 { "" } else { "s" },
                            summary.trim(),
                        ));
                    }
                    Ok(_) => {
                        tracing::warn!(
                            "Summarize backend returned empty content; keeping static placeholder"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "Summarize backend call failed; keeping static placeholder"
                        );
                    }
                }
            }

            let request = ChatRequest::new(messages).with_tools(req_tools);

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

            let mut tool_emitter = ToolCallEmitter::new(max_tool_depth);
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
                tracing::trace!(
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

            // Knowledge attached by Lua handlers mid-turn. Declared outside
            // `'turn: loop` because both `continue 'turn` paths below restart
            // the iteration — a per-iteration binding silently dropped whatever
            // had been attached, and the registry has already charged its
            // budget and burned its dedup key, so it can never be re-supplied.
            let mut attached: Vec<String> = Vec::new();

            // Whether this turn put anything in front of the user. Declared
            // outside `'turn: loop` for the same reason as `attached`: a later
            // iteration that yields nothing still belongs to a turn that
            // already showed text or ran a tool, and that turn is not empty.
            //
            // Without this the only site that reported `StopReason::Empty` was
            // the unexpected-stream-close arm below, so a *well-formed* turn
            // that produced nothing reported `EndTurn` — the same "silence
            // reported as natural completion" that `Empty` exists to name, and
            // a divergence from the ACP path, which reports `Empty`
            // (`acp_handle/translate.rs::turn_stop_reason`).
            let mut produced_content = false;

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
                            produced_content = true;
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
                        other => {
                            // Whitespace-only deltas are what a provider emits
                            // while producing nothing; `stream.rs`'s own
                            // empty-response check trims for the same reason.
                            // Shared with `AcpAgentHandle` so the two agents
                            // cannot drift on what "produced content" means.
                            produced_content |= match &other {
                                TurnEvent::TextDelta(t) | TurnEvent::Thinking(t) => {
                                    crucible_core::turn::is_visible_content(t)
                                }
                                _ => false,
                            };
                            yield other;
                        }
                    }
                }

                if pending_calls.is_empty() {
                    let stop_reason = if produced_content {
                        StopReason::EndTurn
                    } else {
                        StopReason::Empty
                    };
                    yield TurnEvent::Done { stop_reason };
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
                                terminate: false,
                            });
                        }
                        TurnEvent::HandlerInjection { content, .. } => {
                            drop(chat_stream);
                            for attachment in attached.drain(..) {
                                messages.push(ChatMessage::system(&attachment));
                            }
                            messages.push(ChatMessage::user(&content));
                            chat_stream = self.stream_chat_from_messages(messages.clone());
                            continue 'turn;
                        }
                        TurnEvent::ContextAttach { content } => {
                            // Retrieved reference material, not a user turn —
                            // system role. Buffered rather than restarting the
                            // stream: we are mid tool-result collection, and
                            // the attachment is folded in below so it lands
                            // after the tool responses, at the end of the
                            // message list. Restarting here would drop the
                            // tool results we are still waiting on.
                            attached.push(content);
                        }
                        TurnEvent::DepthCapHit { .. } => {
                            drop(chat_stream);
                            for attachment in attached.drain(..) {
                                messages.push(ChatMessage::system(&attachment));
                            }
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
                    let payload = tool_response_payload(&result);
                    let call_id = result.call_id.unwrap_or_else(|| {
                        pending_calls
                            .get(idx)
                            .and_then(|call| call.id.clone())
                            .unwrap_or_else(|| format!("tool_call_{idx}"))
                    });
                    messages.push(ChatMessage::from(ToolResponse::new(call_id, payload)));
                }

                // Attached knowledge goes last, after the tool responses:
                // appending keeps the entire preceding prefix cacheable, which
                // is what makes a retrieval path that fires often affordable.
                for content in attached.drain(..) {
                    messages.push(ChatMessage::system(&content));
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

    /// The prompt this handle will actually send, after the factory's
    /// enrichment (workspace header, rules files, skills catalog) — not the
    /// agent card's `system_prompt` the session config stores.
    fn get_system_prompt(&self) -> Option<String> {
        Some(if self.session_context.is_empty() {
            self.system_prompt.clone()
        } else {
            format!("{}\n\n{}", self.system_prompt, self.session_context)
        })
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

    async fn set_temperature(&mut self, temperature: f64) -> ChatResult<()> {
        self.temperature = Some(temperature);
        Ok(())
    }

    fn get_temperature(&self) -> Option<f64> {
        self.temperature
    }

    async fn set_max_tokens(&mut self, max_tokens: Option<u32>) -> ChatResult<()> {
        self.max_tokens = max_tokens;
        Ok(())
    }

    fn get_max_tokens(&self) -> Option<u32> {
        self.max_tokens
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

    async fn set_autocompact_threshold(&mut self, threshold: Option<f32>) -> ChatResult<()> {
        self.autocompact_threshold = threshold;
        Ok(())
    }

    fn get_autocompact_threshold(&self) -> Option<f32> {
        self.autocompact_threshold
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
    use crucible_core::traits::chat::ChatToolResult;

    fn failed(name: &str, error: &str) -> ChatToolResult {
        ChatToolResult {
            name: name.to_string(),
            result: String::new(),
            error: Some(error.to_string()),
            call_id: Some("c1".to_string()),
            terminate: false,
        }
    }

    /// A tool that failed must tell the model why.
    ///
    /// Every failure path — permission denial, timeout, containment refusal,
    /// unknown tool — sets `result` empty and puts the text in `error`, and
    /// `error` was never read when the message list was built. The model saw
    /// `""`, so it either repeated the identical call or invented a result,
    /// while the TUI showed the real message through a separate channel.
    #[test]
    fn a_failed_tool_call_reaches_the_model_with_its_error() {
        let denied = failed(
            "read_file",
            "Path '/etc/passwd' is outside this session's allowed roots",
        );
        let payload = tool_response_payload(&denied);

        assert!(
            payload.contains("outside this session's allowed roots"),
            "the model must see why the call failed, got {payload:?}"
        );
        assert!(!payload.is_empty());
    }

    /// A successful result is passed through untouched.
    #[test]
    fn a_successful_tool_result_is_unchanged() {
        let ok = ChatToolResult {
            name: "read_file".to_string(),
            result: "{\"result\":\"hello\"}".to_string(),
            error: None,
            call_id: Some("c1".to_string()),
            terminate: false,
        };
        assert_eq!(tool_response_payload(&ok), "{\"result\":\"hello\"}");
    }

    /// Both present: the content is the answer, the error is context on top of
    /// it — a partial failure must not silently drop what did come back.
    #[test]
    fn a_result_carrying_both_content_and_an_error_keeps_both() {
        let both = ChatToolResult {
            name: "bash".to_string(),
            result: "partial output".to_string(),
            error: Some("exit code 1".to_string()),
            call_id: Some("c1".to_string()),
            terminate: false,
        };
        let payload = tool_response_payload(&both);
        assert!(payload.contains("partial output"));
        assert!(payload.contains("exit code 1"));
    }

    /// The session's paths must not sit inside the cached prefix.
    ///
    /// One breakpoint covering both halves means a cache entry keyed on the
    /// whole system prompt, so a different workspace missed on the persona and
    /// the project rules too. Two breakpoints let the stable half survive a
    /// change of project.
    #[test]
    fn the_stable_prompt_and_the_session_context_get_separate_breakpoints() {
        let mut messages = vec![ChatMessage::user("hello")];

        apply_prompt_caching("PERSONA", "Workspace: /repo", &mut messages);

        assert_eq!(messages.len(), 3, "two system messages plus the user turn");
        let texts: Vec<String> = messages
            .iter()
            .map(|m| m.content.joined_texts().unwrap_or_default())
            .collect();
        assert_eq!(texts[0], "PERSONA", "the cacheable half leads");
        assert_eq!(texts[1], "Workspace: /repo", "session context follows it");

        for (idx, label) in [(0usize, "stable"), (1, "session context")] {
            assert!(
                messages[idx]
                    .options
                    .as_ref()
                    .and_then(|o| o.cache_control.as_ref())
                    .is_some(),
                "the {label} half needs its own cache breakpoint"
            );
        }
    }

    /// A session with no kiln and no workspace header still gets exactly one
    /// system message — an empty second block would be a wasted breakpoint.
    #[test]
    fn an_empty_session_context_adds_no_second_system_message() {
        let mut messages = vec![ChatMessage::user("hello")];

        apply_prompt_caching("PERSONA", "", &mut messages);

        assert_eq!(messages.len(), 2);
        assert_eq!(
            messages[0].content.joined_texts().unwrap_or_default(),
            "PERSONA"
        );
    }

    use crate::provider::ChatClient;
    use crucible_core::config::{BackendType, LlmProviderConfig};

    use futures::StreamExt;
    use test_case::test_case;

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
    /// resolve relative paths against the daemon's current working
    /// directory, which is fine for tests that don't exercise diff
    /// synthesis (the tool name is a non-edit tool like "bash" or
    /// "read", so synthesis returns an empty Vec).
    fn emitter(max_depth: usize) -> ToolCallEmitter {
        ToolCallEmitter::new(max_depth)
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
        let mut e = ToolCallEmitter::new(10);

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
            ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
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
            ..Default::default()
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
                ..Default::default()
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

    /// The settings must reach the outgoing request, not merely the handle.
    ///
    /// A getter returning what a setter stored proves nothing here: that is
    /// exactly what `SessionAgent.temperature` did for months while no request
    /// ever carried it. `ChatOptions` is what genai puts on the wire.
    #[test]
    fn generation_settings_reach_the_outgoing_chat_options() {
        let mut handle = test_handle_with_tools(Vec::new());

        let untouched = handle.build_chat_options();
        assert_eq!(
            (untouched.temperature, untouched.max_tokens),
            (None, None),
            "unset means the provider's default, not a value chosen here"
        );

        handle.temperature = Some(0.15);
        handle.max_tokens = Some(1_024);

        let options = handle.build_chat_options();
        assert_eq!(options.temperature, Some(0.15));
        assert_eq!(options.max_tokens, Some(1_024));
        // The capture flags the stream loop depends on must survive.
        assert_eq!(options.capture_tool_calls, Some(true));
        assert_eq!(options.capture_usage, Some(true));
    }

    // ─── Progressive tool disclosure: visible_tools() deferral ──────────

    fn test_handle_with_tools(tools: Vec<LlmToolDefinition>) -> GenaiAgentHandle {
        let config = LlmProviderConfig::builder(BackendType::OpenAI)
            .model("gpt-4o-mini")
            .build();
        let chat_client = ChatClient::new(&config);
        let client = chat_client.inner().clone();
        let model = chat_client
            .model_iden("gpt-4o-mini")
            .unwrap_or_else(|| ModelIden::new(genai::adapter::AdapterKind::OpenAI, "gpt-4o-mini"));
        GenaiAgentHandle::new(client, model, "system", tools, None)
    }

    /// Regression: a handle must accept a mode the Lua registry declares.
    ///
    /// `with_workspace` built `mode_state` from `default_internal_modes()`
    /// unconditionally, so `set_mode_str` rejected anything outside
    /// normal/plan/auto with `InvalidMode`. The resume path applies the
    /// persisted mode best-effort and only `warn!`s, so after a handle
    /// eviction the session silently ran the handle's default while
    /// `session.get_mode` kept reporting the user's mode — reported one mode,
    /// enforced another.
    #[tokio::test]
    async fn set_mode_str_accepts_a_lua_declared_mode() {
        let registry = crucible_lua::ModeRegistry::new();
        registry.set(crucible_lua::ModeDefinition {
            name: "review".to_string(),
            description: Some("Read-only review".to_string()),
            tools: crucible_lua::ToolSelector::Patterns(vec!["read_*".to_string()]),
            permissions: crucible_lua::ModePermissions::default(),
        });

        let mut handle = test_handle_with_tools(Vec::new()).with_modes(registry);

        handle
            .set_mode_str("review")
            .await
            .expect("a declared mode must be accepted by the handle");
        assert_eq!(handle.get_mode_id(), "review");
    }

    /// The fallback still restricts: with no registry, only the built-ins.
    #[tokio::test]
    async fn set_mode_str_still_rejects_an_undeclared_mode() {
        let mut handle = test_handle_with_tools(Vec::new());
        assert!(
            handle.set_mode_str("nonsense").await.is_err(),
            "an undeclared mode must still be rejected"
        );
    }

    /// A declared mode's `tools` selector must filter the advertised set.
    /// Before the fix `visible_tools()` resolved against the handle's default
    /// mode, so `review` advertised everything.
    #[tokio::test]
    async fn a_declared_modes_tool_selector_filters_the_advertised_set() {
        let registry = crucible_lua::ModeRegistry::new();
        registry.set(crucible_lua::ModeDefinition {
            name: "review".to_string(),
            description: None,
            tools: crucible_lua::ToolSelector::Patterns(vec!["read_*".to_string()]),
            permissions: crucible_lua::ModePermissions::default(),
        });

        let mut handle =
            test_handle_with_tools(vec![tool_def("read_file"), tool_def("write_file")])
                .with_modes(registry);
        handle.set_mode_str("review").await.unwrap();

        let names: Vec<String> = handle
            .visible_tools()
            .tools
            .iter()
            .map(|t| t.function.name.clone())
            .collect();
        assert_eq!(names, vec!["read_file"], "review must not advertise writes");
    }

    /// An EMPTY registry means nothing was declared — not that this session's
    /// mode vanished — and must leave the tool set alone.
    ///
    /// The fail-closed branch for a vanished mode was first keyed on
    /// "a registry is attached", but one always is on the daemon path. Before a
    /// session VM has run the Lua defaults — or if that file fails to parse, or
    /// a runtimepath entry shadows it — the registry is attached and empty, and
    /// every agent silently lost `read_file`, `grep` and `bash`.
    #[tokio::test]
    async fn an_empty_mode_registry_does_not_strip_the_tool_set() {
        let registry = crucible_lua::ModeRegistry::new();
        assert!(registry.is_empty());

        let handle = test_handle_with_tools(vec![
            tool_def("read_file"),
            tool_def("write_file"),
            tool_def("bash"),
        ])
        .with_modes(registry);

        let names: Vec<String> = handle
            .visible_tools()
            .tools
            .iter()
            .map(|t| t.function.name.clone())
            .collect();
        assert!(
            names.contains(&"read_file".to_string())
                && names.contains(&"bash".to_string())
                && names.contains(&"write_file".to_string()),
            "nothing was declared, so nothing should be filtered; got {names:?}"
        );
    }

    /// A mode whose declaration has gone away must not become the most
    /// PERMISSIVE one.
    ///
    /// `SessionAgent.mode` is a persisted string and `set_mode`'s validation
    /// runs only on an explicit set, never on resume. So: select `review`
    /// (`tools = read_*`, deny by default), delete the declaration, resume.
    /// The registry misses — and the unknown-mode branch used to return
    /// `true` for every tool, so the most restrictive mode silently became
    /// the most permissive. Fail closed instead.
    #[tokio::test]
    async fn a_mode_whose_declaration_vanished_does_not_widen_the_tool_set() {
        let registry = crucible_lua::ModeRegistry::new();
        registry.set(crucible_lua::ModeDefinition {
            name: "review".to_string(),
            description: None,
            tools: crucible_lua::ToolSelector::Patterns(vec!["read_*".to_string()]),
            permissions: crucible_lua::ModePermissions::default(),
        });

        // `gh_create_pr` is the case the first version of this test missed:
        // `write_file` is a literal in `is_write_tool_name`, so the assertion
        // below passed on the name blocklist alone and proved nothing about
        // upstream tools, whose names we do not control.
        let mut handle = test_handle_with_tools(vec![
            tool_def("read_file"),
            tool_def("write_file"),
            tool_def("gh_create_pr"),
        ])
        .with_deferrable_tools(std::collections::HashSet::from(
            ["gh_create_pr".to_string()],
        ))
        .with_modes(registry.clone());
        handle.set_mode_str("review").await.unwrap();

        // The declaration goes away underneath the live session.
        registry.remove("review");

        let names: Vec<String> = handle
            .visible_tools()
            .tools
            .iter()
            .map(|t| t.function.name.clone())
            .collect();
        assert!(
            !names.contains(&"gh_create_pr".to_string()),
            "an unknown mode must drop upstream tools the way plan does — our \
             write-name blocklist matches none of their names; got {names:?}"
        );
        assert!(
            !names.contains(&"write_file".to_string()),
            "a vanished mode must not advertise MORE than it did while declared; got {names:?}"
        );
    }

    fn tool_def(name: &str) -> LlmToolDefinition {
        LlmToolDefinition::new(
            name,
            "a tool description that contributes some tokens to the schema estimate",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "a query parameter"}
                }
            }),
        )
    }

    fn visible_names(set: &VisibleToolSet) -> Vec<String> {
        set.tools.iter().map(|t| t.function.name.clone()).collect()
    }

    /// M9 regression: mode is captured at agent creation, so plan-mode
    /// exclusion of plugin tools must be per-request. The write-name
    /// blocklist can't classify a plugin tool — before the plugin-name set,
    /// a session switched to plan mid-run kept plugin tools advertised.
    #[tokio::test]
    async fn switching_to_plan_mid_run_hides_plugin_tools() {
        let tools = vec![tool_def("read_file"), tool_def("my_plugin_tool")];
        let plugin_names: std::collections::HashSet<String> = ["my_plugin_tool".to_string()].into();
        let mut handle = test_handle_with_tools(tools).with_plugin_tools(plugin_names);

        let (act_names, _) = handle.visible_tool_names_for_test();
        assert!(
            act_names.iter().any(|n| n == "my_plugin_tool"),
            "plugin tools are visible outside plan mode"
        );

        handle.set_mode_str("plan").await.unwrap();
        let (plan_names, _) = handle.visible_tool_names_for_test();
        assert!(
            !plan_names.iter().any(|n| n == "my_plugin_tool"),
            "a mid-run switch to plan must stop advertising plugin tools"
        );
        assert!(
            plan_names.iter().any(|n| n == "read_file"),
            "read-only built-ins stay visible in plan"
        );

        handle.set_mode_str("normal").await.unwrap();
        let (back_names, _) = handle.visible_tool_names_for_test();
        assert!(
            back_names.iter().any(|n| n == "my_plugin_tool"),
            "leaving plan restores plugin tools — they are attached, only filtered"
        );
    }

    #[test]
    fn visible_tools_under_budget_attaches_all_and_no_bridge() {
        let gateway: Vec<_> = (0..3).map(|i| tool_def(&format!("gh_tool_{i}"))).collect();
        let mut tools = vec![tool_def("read_file")];
        tools.extend(gateway.iter().cloned());
        let deferrable: std::collections::HashSet<String> =
            gateway.iter().map(|t| t.function.name.clone()).collect();
        // Default effective budget (128k) → threshold ~19200 tokens; a handful
        // of small schemas is far under, so nothing defers.
        let handle = test_handle_with_tools(tools).with_deferrable_tools(deferrable);

        let visible = handle.visible_tools();
        assert_eq!(visible.deferred_count, 0);
        let names = visible_names(&visible);
        assert!(names.iter().any(|n| n == "read_file"));
        assert!(names.iter().any(|n| n == "gh_tool_0"));
        assert!(!names.iter().any(|n| n == "invoke_tool"));
    }

    #[test]
    fn visible_tools_over_budget_defers_gateway_and_adds_bridge() {
        let gateway: Vec<_> = (0..6).map(|i| tool_def(&format!("gh_tool_{i}"))).collect();
        let mut tools = vec![tool_def("read_file"), tool_def("semantic_search")];
        tools.extend(gateway.iter().cloned());
        let deferrable: std::collections::HashSet<String> =
            gateway.iter().map(|t| t.function.name.clone()).collect();
        let mut handle = test_handle_with_tools(tools).with_deferrable_tools(deferrable);
        // Small budget → threshold ~150 tokens, which the tool schemas exceed.
        handle.context_budget = Some(1_000);

        let visible = handle.visible_tools();
        assert_eq!(visible.deferred_count, 6, "all gateway tools deferred");
        let names = visible_names(&visible);
        assert!(names.iter().any(|n| n == "read_file"), "core kept");
        assert!(names.iter().any(|n| n == "semantic_search"), "core kept");
        assert!(
            !names.iter().any(|n| n.starts_with("gh_tool_")),
            "gateway tools dropped: {names:?}"
        );
        assert!(names.iter().any(|n| n == "discover_tools"));
        assert!(names.iter().any(|n| n == "get_tool_schema"));
        assert!(names.iter().any(|n| n == "invoke_tool"));
    }

    #[test]
    fn visible_tools_never_defers_without_deferrable_set() {
        let tools: Vec<_> = (0..20).map(|i| tool_def(&format!("tool_{i}"))).collect();
        let mut handle = test_handle_with_tools(tools);
        handle.context_budget = Some(1); // threshold 0 — would defer if anything were deferrable

        let visible = handle.visible_tools();
        assert_eq!(visible.deferred_count, 0);
        assert_eq!(visible.tools.len(), 20);
        assert!(!visible_names(&visible).iter().any(|n| n == "invoke_tool"));
    }

    #[test]
    fn visible_tools_plan_mode_filters_writes_and_defers() {
        let gateway: Vec<_> = (0..6).map(|i| tool_def(&format!("gh_tool_{i}"))).collect();
        let mut tools = vec![tool_def("read_file"), tool_def("edit_file")];
        tools.extend(gateway.iter().cloned());
        let deferrable: std::collections::HashSet<String> =
            gateway.iter().map(|t| t.function.name.clone()).collect();
        let mut handle = test_handle_with_tools(tools).with_deferrable_tools(deferrable);
        handle.current_mode_id = "plan".to_string();
        handle.context_budget = Some(1_000);

        let visible = handle.visible_tools();
        let names = visible_names(&visible);
        assert!(
            !names.iter().any(|n| n == "edit_file"),
            "write tool filtered in plan mode: {names:?}"
        );
        assert!(names.iter().any(|n| n == "read_file"));
        assert_eq!(visible.deferred_count, 6);
        assert!(names.iter().any(|n| n == "invoke_tool"));
    }

    #[test]
    fn visible_tools_plan_mode_disables_gateway_under_budget() {
        // Fail-closed: even comfortably under budget, plan mode attaches no
        // gateway/deferrable tool defs and no bridge — upstream MCP tools are
        // disabled entirely in plan mode.
        let gateway: Vec<_> = (0..3).map(|i| tool_def(&format!("gh_tool_{i}"))).collect();
        let mut tools = vec![tool_def("read_file"), tool_def("semantic_search")];
        tools.extend(gateway.iter().cloned());
        let deferrable: std::collections::HashSet<String> =
            gateway.iter().map(|t| t.function.name.clone()).collect();
        let mut handle = test_handle_with_tools(tools).with_deferrable_tools(deferrable);
        handle.current_mode_id = "plan".to_string();
        // Default 128k budget → far under; no budget-driven deferral.

        let visible = handle.visible_tools();
        let names = visible_names(&visible);
        assert!(
            !names.iter().any(|n| n.starts_with("gh_tool_")),
            "no gateway defs attached in plan mode: {names:?}"
        );
        assert!(
            !names.iter().any(|n| n == "invoke_tool"),
            "no bridge: {names:?}"
        );
        assert_eq!(visible.deferred_count, 0);
        assert!(names.iter().any(|n| n == "read_file"));
        assert!(names.iter().any(|n| n == "semantic_search"));
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

    fn tool_call_event() -> TurnEvent {
        TurnEvent::ToolCall {
            id: "call_1".to_string(),
            name: "search".to_string(),
            args: serde_json::Value::Null,
            diffs: Vec::new(),
        }
    }

    #[test_case(vec![TurnEvent::TextDelta("Hello world".to_string())] ; "normal text")]
    #[test_case(vec![tool_call_event()] ; "tool call only")]
    #[test_case(vec![TurnEvent::Thinking("Let me think about this...".to_string())] ; "thinking only")]
    #[test_case(vec![TurnEvent::TextDelta("Hello".to_string()), tool_call_event()] ; "text plus tool call")]
    #[tokio::test]
    async fn terminated_stream_yields_no_error(content: Vec<TurnEvent>) {
        let mut script = content;
        script.push(TurnEvent::Done {
            stop_reason: StopReason::EndTurn,
        });
        let events = wrap_stream_with_guards(scripted_turn_stream(script))
            .collect::<Vec<_>>()
            .await;
        assert!(!has_any_error(&events), "got: {events:?}");
    }

    // Undo semantics moved to AgentManager (operates on the scheduler-
    // owned ConversationTree). See agent_manager::models::undo + the
    // integration test `agent_manager::tests::messaging::*`.

    // ─── ContextStrategy::Summarize ─────────────────────────────────────

    fn user(s: &str) -> ChatMessage {
        ChatMessage::user(s)
    }
    fn asst(s: &str) -> ChatMessage {
        ChatMessage::assistant(s)
    }
    fn sys(s: &str) -> ChatMessage {
        ChatMessage::system(s)
    }

    /// With Summarize and a budget that the message list exceeds, the
    /// older non-system non-keep messages are replaced by a single
    /// "[summary placeholder]" system message — distinct from Truncate,
    /// which silently drops them.
    /// `enforce_context_budget` with Summarize must return
    /// `NeedsSummarize` so the async caller can replace the placeholder
    /// with an LLM-generated recap. The drained payload should be
    /// non-empty and match what was removed.
    #[test]
    fn summarize_returns_needs_summarize_action() {
        let long = "x".repeat(40);
        let mut messages = vec![
            sys("system"),
            user(&long),
            asst(&long),
            user(&long),
            asst(&long),
            user("current"),
        ];
        let action = enforce_context_budget(
            &mut messages,
            Some(12),
            &ContextStrategy::Summarize,
            Some(1),
        );
        match action {
            BudgetAction::NeedsSummarize {
                placeholder_idx,
                drained,
            } => {
                assert!(
                    placeholder_idx < messages.len(),
                    "placeholder_idx must point at a real message"
                );
                assert!(!drained.is_empty(), "drained must carry the removed turns");
                assert!(
                    content_string(&messages[placeholder_idx]).contains("[summary placeholder]")
                );
            }
            other => panic!("expected NeedsSummarize, got {}", action_label(&other)),
        }
    }

    /// Truncate / SlidingWindow / under-budget Summarize must NOT
    /// return NeedsSummarize — the async caller would otherwise issue
    /// pointless backend calls.
    #[test]
    fn truncate_and_window_do_not_request_summarization() {
        let long = "x".repeat(40);
        let mut messages = vec![sys("system"), user(&long), asst(&long), user("current")];
        let action = enforce_context_budget(
            &mut messages.clone(),
            Some(12),
            &ContextStrategy::Truncate,
            None,
        );
        assert!(matches!(
            action,
            BudgetAction::Mutated | BudgetAction::NoChange
        ));
        let action = enforce_context_budget(
            &mut messages,
            Some(12),
            &ContextStrategy::SlidingWindow,
            Some(1),
        );
        assert!(matches!(
            action,
            BudgetAction::Mutated | BudgetAction::NoChange
        ));
    }

    fn action_label(action: &BudgetAction) -> &'static str {
        match action {
            BudgetAction::NoChange => "NoChange",
            BudgetAction::Mutated => "Mutated",
            BudgetAction::NeedsSummarize { .. } => "NeedsSummarize",
        }
    }

    /// `drained_transcript` formats messages with role prefixes; an
    /// LLM consuming this can attribute statements to the right party.
    #[test]
    fn drained_transcript_formats_with_role_prefixes() {
        let drained = vec![user("hello there"), asst("hi back")];
        let transcript = drained_transcript(&drained);
        assert!(transcript.contains("user: hello there"));
        assert!(transcript.contains("assistant: hi back"));
    }

    #[test]
    fn summarize_inserts_elision_placeholder() {
        // Use long messages so token estimates exceed the small budget;
        // estimate_message_tokens uses chars/4.
        let long = "x".repeat(40); // 10 tokens
        let mut messages = vec![
            sys("system"),
            user(&long),
            asst(&long),
            user(&long),
            asst(&long),
            user(&long),
            asst(&long),
            user("current question"),
        ];
        // Budget=12 means the 80-token total triggers drainage; window=1
        // keeps the last 2 messages.
        enforce_context_budget(
            &mut messages,
            Some(12),
            &ContextStrategy::Summarize,
            Some(1),
        );

        let placeholder_present = messages.iter().any(|m| {
            m.role == genai::chat::ChatRole::System
                && content_string(m).contains("[summary placeholder]")
        });
        assert!(
            placeholder_present,
            "expected [summary placeholder] system message after Summarize, got: {:#?}",
            messages.iter().map(content_string).collect::<Vec<_>>()
        );
        // Last message survives.
        assert_eq!(content_string(messages.last().unwrap()), "current question");
    }

    /// When the message list is already under budget, Summarize is a
    /// no-op — no placeholder is inserted.
    #[test]
    fn summarize_noop_when_under_budget() {
        let mut messages = vec![sys("S"), user("hi"), asst("hello")];
        let before_len = messages.len();
        enforce_context_budget(
            &mut messages,
            Some(10_000),
            &ContextStrategy::Summarize,
            None,
        );
        assert_eq!(messages.len(), before_len);
        assert!(!messages
            .iter()
            .any(|m| content_string(m).contains("[summary placeholder]")));
    }

    /// The placeholder cites the correct number of elided turns.
    #[test]
    fn summarize_placeholder_reports_dropped_count() {
        let long = "x".repeat(40);
        let mut messages = vec![sys("S")];
        // 8 long turns past the system prompt; window=1 keeps the last 2.
        for _ in 0..4 {
            messages.push(user(&long));
            messages.push(asst(&long));
        }
        messages.push(user("current"));

        enforce_context_budget(
            &mut messages,
            Some(12),
            &ContextStrategy::Summarize,
            Some(1),
        );

        let placeholder = messages
            .iter()
            .find(|m| content_string(m).contains("[summary placeholder]"))
            .expect("placeholder must be present");
        let body = content_string(placeholder);
        assert!(
            body.contains("earlier turn"),
            "body should mention turn count: {body}"
        );
    }

    fn content_string(msg: &ChatMessage) -> String {
        msg.content
            .parts()
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    // ─── ROUND-3 THROWAWAY: plan-mode plugin admission matrix ───────────
    /// Advertisement and dispatch must agree on plugin-tool admission for
    /// every combination of mode and declaration, not just the common ones.
    ///
    /// Two rounds of review found this pair disagreeing. First the grant was
    /// honoured at dispatch but not in the advertised set, so a tool the
    /// operator had named was never offered to the model — a permission that
    /// reports success while changing nothing. Then wiring the shared rule into
    /// advertisement dropped the unknown-mode case, reopening every plugin tool
    /// for a mode that had gone away.
    ///
    /// Both were combinations nobody had enumerated, so this enumerates them.
    mod admission_matrix {
        use super::*;
        use crucible_lua::{ModeDefinition, ModePermissions, ModeRegistry, ToolSelector};

        fn reg(entries: &[(&str, ToolSelector)]) -> ModeRegistry {
            let r = ModeRegistry::new();
            for (name, sel) in [("normal", ToolSelector::All), ("auto", ToolSelector::All)]
                .iter()
                .cloned()
                .chain(entries.iter().map(|(n, s)| (*n, s.clone())))
            {
                r.set(ModeDefinition {
                    name: name.to_string(),
                    description: None,
                    tools: sel,
                    permissions: ModePermissions::default(),
                });
            }
            r
        }

        fn plugins() -> std::collections::HashSet<String> {
            ["web_search".to_string()].into()
        }

        /// Names advertised to the model in `mode`. `vanish` removes the mode
        /// after selecting it, which is how a mode "goes away" mid-session.
        async fn advertised(modes: ModeRegistry, mode: &str, vanish: bool) -> Vec<String> {
            let mut h = test_handle_with_tools(vec![
                tool_def("read_file"),
                tool_def("write_file"),
                tool_def("web_search"),
            ])
            .with_plugin_tools(plugins())
            .with_modes(modes.clone());
            h.set_mode_str(mode).await.unwrap();
            if vanish {
                modes.remove(mode);
            }
            h.visible_tool_names_for_test().0
        }

        fn barred(modes: &ModeRegistry, mode: &str) -> bool {
            crate::tools::tool_modes::plugin_tool_barred(
                mode,
                "web_search",
                &plugins(),
                Some(modes),
            )
        }

        /// The operator's grant reaches BOTH paths, which is the whole point.
        #[tokio::test]
        async fn a_named_plugin_tool_is_advertised_and_dispatchable_in_plan() {
            let modes = reg(&[(
                "plan",
                ToolSelector::Patterns(vec!["read_*".into(), "web_search".into()]),
            )]);
            assert!(advertised(modes.clone(), "plan", false)
                .await
                .contains(&"web_search".to_string()));
            assert!(!barred(&modes, "plan"));
        }

        /// Neither a glob nor `*` is a grant — a plugin could otherwise pick a
        /// name that walks through a selector written before it existed.
        #[tokio::test]
        async fn a_glob_or_wildcard_admits_nothing_on_either_path() {
            for selector in [
                ToolSelector::Patterns(vec!["*_search".into()]),
                ToolSelector::All,
            ] {
                let modes = reg(&[("plan", selector)]);
                assert!(!advertised(modes.clone(), "plan", false)
                    .await
                    .contains(&"web_search".to_string()));
                assert!(barred(&modes, "plan"));
            }
        }

        /// A mode that vanished mid-session has no declaration, so it has
        /// granted nothing — the most permissive reading of a missing config is
        /// the wrong one.
        #[tokio::test]
        async fn a_vanished_mode_advertises_no_plugin_tool() {
            for mode in ["plan", "review"] {
                let modes = reg(&[(
                    mode,
                    ToolSelector::Patterns(vec!["read_*".into(), "web_search".into()]),
                )]);
                assert!(
                    !advertised(modes, mode, true)
                        .await
                        .contains(&"web_search".to_string()),
                    "{mode} went away, so nothing it once granted still stands"
                );
            }
        }

        /// Outside plan the rule does not apply, and a declared custom mode
        /// that names the tool simply gets it.
        #[tokio::test]
        async fn an_unrestricted_declared_mode_is_unaffected() {
            let modes = reg(&[(
                "review",
                ToolSelector::Patterns(vec!["read_*".into(), "web_search".into()]),
            )]);
            assert!(advertised(modes.clone(), "review", false)
                .await
                .contains(&"web_search".to_string()));
            assert!(!barred(&modes, "review"));
        }
    }
}
