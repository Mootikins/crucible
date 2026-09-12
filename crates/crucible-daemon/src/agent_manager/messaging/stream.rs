//! Agent turn driver.
//!
//! Drives an `Agent::turn()` stream, emits session events, dispatches
//! tool calls, and steers the agent's continuation via the inbound
//! `mpsc<TurnEvent>` channel. The plan's "one channel topology, not
//! three" rule: `ToolResult` and `HandlerInjection` both arrive on the
//! same inbound channel and drive matching adapter-side behaviour.
//!
//! Two tool-loop re-entry points share the inbound channel:
//!
//! 1. **Tool continuation** — runtime sends `ToolResult` after
//!    dispatching the agent's `ToolCall`.
//! 2. **Handler injection** — runtime's `turn:complete` handler returns
//!    injected content; runtime re-enters `execute_agent_stream`
//!    recursively at one greater `continuation_depth`. (The inbound channel
//!    handles this within a single adapter turn, but handler
//!    injection happens after `Done`, so we re-enter for a fresh
//!    message_id + user_message visibility.)

use super::super::*;
use crate::agent_manager::tool_tracking::ToolCallTracker;
use crucible_core::protocol::session_events::ContextLimitSource;
use crucible_core::traits::chat::{ChatToolCall, ChatToolResult};
use crucible_core::traits::llm::TokenUsage;
use crucible_core::turn::{Agent as TurnAgent, StopReason, TurnContext, TurnEvent};
use crucible_core::types::ToolSource;
use crucible_lua::StageId;
use futures::StreamExt;
use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;

use crate::agent_manager::vm_pass::{run_handlers, PluginHandlers};
use tokio::sync::mpsc;

/// What the finished turn knows about itself.
///
/// The host reads none of these. They ride the `turn:complete` payload so a
/// plugin can decide whether the model stopped before the work was done — from
/// a plan file, a subsession, a tool result or the reply text. The host
/// provides the inputs and holds no opinion, which is why there is no depth
/// cap and no budget check here: a plugin that wants a bound sets its own.
#[derive(Debug, Clone, Copy)]
pub(in crate::agent_manager) struct TurnFacts {
    /// Why the provider or the delegated agent ended the turn. `None` when
    /// the stream closed without a terminal `Done`.
    pub(in crate::agent_manager) stop_reason: Option<StopReason>,
    /// How many re-prompts precede this turn. 0 is the user's own message.
    ///
    /// A FACT, not a cap. A 500-step plan needs 500 re-prompts, so any depth
    /// the host enforced would be wrong for some plan. What stops a runaway
    /// re-prompt is the user's cancel, which reaches every recursion depth
    /// through the `tokio::select!` in `send.rs`.
    pub(in crate::agent_manager) continuation_depth: u32,
    /// Did the turn run a tool the user can see?
    ///
    /// A turn that ends right after a tool result is a model still working,
    /// which removes a class of false positive no reply text can.
    pub(in crate::agent_manager) saw_tool_activity: bool,
}

impl TurnFacts {
    fn is_continuation(&self) -> bool {
        self.continuation_depth > 0
    }
}

/// The last `limit` characters of a reply, and whether anything was cut.
///
/// A tail, not a head. The signal a plugin reads sits at the END of a reply —
/// what the model says it will do next comes after the work, not before it.
/// (`post_llm_call` sends a 200-character HEAD for display. The two are not
/// interchangeable.)
///
/// `limit` of 0 means the whole reply. The cut lands on a character boundary,
/// so a reply that ends in a multi-byte glyph still crosses to Lua.
fn response_tail(response: &str, limit: usize) -> (String, bool) {
    if limit == 0 {
        return (response.to_string(), false);
    }
    match response.char_indices().rev().nth(limit - 1) {
        // Fewer characters than the limit, or exactly the limit: nothing cut.
        None => (response.to_string(), false),
        Some((0, _)) => (response.to_string(), false),
        Some((start, _)) => (response[start..].to_string(), true),
    }
}

impl AgentManager {
    #[allow(clippy::ptr_arg)]
    pub(super) async fn run_reactor_handlers(
        stream_ctx: &StreamContext,
        usage: Option<&TokenUsage>,
        accumulated_response: &mut String,
        facts: TurnFacts,
    ) -> Option<String> {
        // Scheduler-owned conversation tree: commit the assistant
        // response text as an Agent node. Today this is shadow state;
        // later phases flip the handle to read from the tree.
        if !accumulated_response.is_empty() {
            let mut tree = stream_ctx.conversation_tree.lock().await;
            let parent = tree.current();
            let _agent = tree.add_child_and_advance(
                parent,
                crucible_core::turn::NodeContent::Agent {
                    text: accumulated_response.clone(),
                },
            );
        }

        debug!(
            session_id = %stream_ctx.session_id,
            message_id = %stream_ctx.message_id,
            response_len = accumulated_response.len(),
            "Sending message_complete event"
        );
        if let Some(u) = usage {
            stream_ctx.slot.record_usage(u);
            if crate::agent_manager::autocompact::should_autocompact(
                u.prompt_tokens,
                stream_ctx.agent_stream_config.context_budget,
                stream_ctx.agent_stream_config.autocompact_threshold,
            ) {
                match stream_ctx
                    .session_manager
                    .request_compaction(&stream_ctx.session_id)
                    .await
                {
                    Ok(_) => info!(
                        session_id = %stream_ctx.session_id,
                        prompt_tokens = u.prompt_tokens,
                        budget = ?stream_ctx.agent_stream_config.context_budget,
                        "Auto-compaction triggered"
                    ),
                    Err(e) => debug!(
                        session_id = %stream_ctx.session_id,
                        error = %e,
                        "Auto-compaction request skipped (not Active or already compacting)"
                    ),
                }
            }
        }
        if !emit_event(
            &stream_ctx.event_tx,
            SessionEventMessage::message_complete(
                &stream_ctx.session_id,
                &stream_ctx.message_id,
                accumulated_response.clone(),
                usage,
                facts.stop_reason,
            ),
        ) {
            warn!(
                session_id = %stream_ctx.session_id,
                "No subscribers for message_complete event"
            );
        }

        let injection = Self::dispatch_turn_complete_handlers(
            &stream_ctx.session_id,
            &stream_ctx.message_id,
            accumulated_response,
            stream_ctx.agent_stream_config.plugin_handlers.as_ref(),
            facts,
            stream_ctx.agent_stream_config.response_tail_chars,
        )
        .await;

        if let Some(injected_content) = &injection {
            info!(
                session_id = %stream_ctx.session_id,
                content_len = injected_content.len(),
                "Processing handler injection"
            );

            if !emit_event(
                &stream_ctx.event_tx,
                SessionEventMessage::new(
                    &stream_ctx.session_id,
                    "injection_pending",
                    serde_json::json!({
                        "content": injected_content,
                        "is_continuation": true,
                    }),
                ),
            ) {
                warn!(
                    session_id = %stream_ctx.session_id,
                    "No subscribers for injection_pending event"
                );
            }
        }

        injection
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn execute_agent_stream(
        agent: Arc<Mutex<BoxedAgentHandle>>,
        content: String,
        stream_ctx: StreamContext,
        stream_config: AgentStreamConfig,
        accumulated_response: &mut String,
        continuation_depth: u32,
    ) -> StreamOutcome {
        let ttft_local = Instant::now();
        info!(target: "ttft", session_id = %stream_ctx.session_id, stage = "execute_stream_entry", elapsed_ms = 0, "ttft");
        let Some(content) =
            Self::apply_pre_llm_call_handlers(content, &stream_ctx, &stream_config).await
        else {
            return StreamOutcome::Failed("cancelled by pre_llm_call handler".into());
        };
        info!(target: "ttft", session_id = %stream_ctx.session_id, stage = "pre_llm_done", elapsed_ms = ttft_local.elapsed().as_millis() as u64, "ttft");

        let stream_start = Instant::now();

        // Flatten the scheduler-owned tree path to messages so the
        // agent sees the full conversation and doesn't need to hold
        // its own history.
        let flattened_messages = {
            let tree = stream_ctx.conversation_tree.lock().await;
            tree.flatten_current_path_to_context()
        };

        // Two-stage context seam (Pi-style): transform_context fires
        // here, on the rich Vec<ContextMessage>, before linearization.
        // pre_llm_call already fired above on the string `content`.
        // Kiln/Precognition handlers should attach here, not at
        // pre_llm_call — they get structured messages instead of having
        // to parse the prompt string.
        let Some(flattened_messages) =
            Self::apply_transform_context_handlers(flattened_messages, &stream_ctx, &stream_config)
                .await
        else {
            return StreamOutcome::Failed("cancelled by transform_context handler".into());
        };

        let (inbound_tx, inbound_rx) = mpsc::channel::<TurnEvent>(32);
        let mut turn_ctx = TurnContext::new(content)
            .with_inbound(inbound_rx)
            .with_messages(flattened_messages);
        if continuation_depth > 0 {
            turn_ctx = turn_ctx.continuation();
        }

        info!(target: "ttft", session_id = %stream_ctx.session_id, stage = "before_turn_start", elapsed_ms = ttft_local.elapsed().as_millis() as u64, "ttft");
        // Hold the handle guard for the entire turn; Agent::turn returns
        // a stream that borrows `&mut *guard`.
        let mut guard = agent.lock().await;
        // ACP-style agents run their own tool loop server-side and emit
        // `ToolCall` events as observations (and drop the inbound channel).
        // The scheduler must pass those through, not dispatch them. Capture
        // the flag now, before `turn()` borrows the guard mutably.
        let agent_owns_tools = guard.capabilities().owns_history;
        let mut event_stream = match guard.turn(turn_ctx).await {
            Ok(s) => s,
            Err(e) => {
                error!(
                    session_id = %stream_ctx.session_id,
                    error = %e,
                    "Agent failed to start turn"
                );
                if !emit_event(
                    &stream_ctx.event_tx,
                    SessionEventMessage::ended(&stream_ctx.session_id, format!("error: {e}")),
                ) {
                    warn!(
                        session_id = %stream_ctx.session_id,
                        "No subscribers for turn-start error event"
                    );
                }
                return StreamOutcome::Failed(format!("agent failed to start turn: {e}"));
            }
        };

        // Per-batch tool tracking
        let mut tracker = ToolCallTracker::new();
        let mut blocked_tools: HashSet<String> = HashSet::new();
        let mut last_failure_key: Option<(String, String)> = None;
        let mut consecutive_failure_count = 0usize;
        // Did this turn produce any tool activity the user can see? Set on
        // both forks — the dispatch path below and the `agent_owns_tools`
        // pass-through, which `continue`s before ever reaching the dispatch
        // site. Reading it as "dispatched" made the empty-response guard fire
        // on every delegated turn that ran tools and narrated nothing.
        let mut saw_tool_activity = false;
        // Args of tool calls an ACP-style agent announced but executed itself,
        // kept so the pass-through `tool_result` below can hand handlers the
        // same `{tool, args, ...}` payload the dispatched path gets. Keyed by
        // call id; entries are removed when the matching result arrives.
        let mut acp_tool_args: HashMap<String, serde_json::Value> = HashMap::new();

        // Conjunctive early-stop signals collected per batch. The loop
        // ends after the batch only when every result in this vec is
        // true (and the vec is non-empty) — one tool can't unilaterally
        // cut another tool's work short.
        let mut batch_terminate_signals: Vec<bool> = Vec::new();

        // Terminal state
        let mut last_usage: Option<TokenUsage> = None;
        let mut terminal_stop_reason: Option<StopReason> = None;

        // Text→tool segment tracking. A tool call after streamed text is a
        // segment boundary: we emit a `segment_complete` carrying the text
        // accumulated since the last boundary so viewers converge on a
        // canonical bubble (id + content) for the pre-tool narration, live
        // and on reload. `message_complete` still carries the whole
        // accumulated response, so segments are strictly additive.
        // `last_segment_end` is a byte offset into `accumulated_response`;
        // it always lands on a valid UTF-8 boundary because it's only ever
        // set to `accumulated_response.len()`.
        let mut segment_index: usize = 0;
        let mut last_segment_end: usize = accumulated_response.len();

        let mut ttft_first_token_logged = false;
        let mut ttft_first_event_logged = false;
        while let Some(event) = event_stream.next().await {
            if !ttft_first_event_logged {
                info!(target: "ttft", session_id = %stream_ctx.session_id, stage = "first_turn_event", elapsed_ms = ttft_local.elapsed().as_millis() as u64, kind = ?std::mem::discriminant(&event), "ttft");
                ttft_first_event_logged = true;
            }
            match event {
                TurnEvent::TextDelta(delta) => {
                    if delta.is_empty() {
                        continue;
                    }

                    // Dedup: some providers send the whole accumulated
                    // text as their final delta. Skip if it matches.
                    if !accumulated_response.is_empty() && delta == *accumulated_response {
                        debug!(
                            session_id = %stream_ctx.session_id,
                            delta_len = delta.len(),
                            "Skipping duplicate full-text delta (matches accumulated response)"
                        );
                        continue;
                    }

                    if !ttft_first_token_logged {
                        info!(target: "ttft", session_id = %stream_ctx.session_id, stage = "first_text_delta", elapsed_ms = ttft_local.elapsed().as_millis() as u64, "ttft");
                        ttft_first_token_logged = true;
                    }
                    accumulated_response.push_str(&delta);
                    debug!(
                        session_id = %stream_ctx.session_id,
                        delta_len = delta.len(),
                        "Sending text_delta event"
                    );
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::text_delta(&stream_ctx.session_id, &delta),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            "No subscribers for text_delta event"
                        );
                    }
                }
                TurnEvent::Thinking(reasoning) => {
                    debug!(session_id = %stream_ctx.session_id, "Sending thinking event");
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::thinking(&stream_ctx.session_id, &reasoning),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            "No subscribers for thinking event"
                        );
                    }
                }
                TurnEvent::ToolCall {
                    id,
                    name,
                    args,
                    diffs,
                } => {
                    // Text → tool boundary: if text streamed since the last
                    // boundary, freeze it into a canonical segment before the
                    // tool_call event. This fires for both internal and
                    // ACP-style agents (the web reducer freezes on every
                    // tool_call), so parallel tool calls in one batch emit at
                    // most one segment — subsequent calls see no new text.
                    if accumulated_response.len() > last_segment_end {
                        let segment_text = accumulated_response[last_segment_end..].to_string();
                        if !emit_event(
                            &stream_ctx.event_tx,
                            SessionEventMessage::segment_complete(
                                &stream_ctx.session_id,
                                &stream_ctx.message_id,
                                segment_index,
                                segment_text,
                            ),
                        ) {
                            warn!(
                                session_id = %stream_ctx.session_id,
                                "No subscribers for segment_complete event"
                            );
                        }
                        segment_index += 1;
                        last_segment_end = accumulated_response.len();
                    }

                    // ACP-style agents already executed this tool in their own
                    // server-side loop; the event is an observation. Pass it
                    // through to subscribers + the tree and move on. We must NOT
                    // dispatch it (the dispatch path feeds `inbound_tx`, which
                    // the ACP turn dropped — the send fails and breaks the loop,
                    // truncating the turn right after the first tool call), nor
                    // count it toward the tool-depth cap. The agent streams its
                    // own ToolResult + follow-up text, handled below.
                    if agent_owns_tools {
                        saw_tool_activity = true;
                        acp_tool_args.insert(id.clone(), args.clone());
                        {
                            let mut tree = stream_ctx.conversation_tree.lock().await;
                            let parent = tree.current();
                            tree.add_child(
                                parent,
                                crucible_core::turn::NodeContent::ToolCall {
                                    id: id.clone(),
                                    name: name.clone(),
                                    args: args.clone(),
                                },
                            );
                        }
                        if !emit_event(
                            &stream_ctx.event_tx,
                            SessionEventMessage::tool_call_with_metadata(
                                &stream_ctx.session_id,
                                &id,
                                &name,
                                args.clone(),
                                None,
                                // Name the agent so the card badges
                                // `[acp:claude]` — which agent ran the tool is
                                // the whole point of the provenance badge.
                                // An ACP session cannot exist without a name
                                // (`session/create` rejects it), so `None`
                                // here means a non-ACP owns-history agent and
                                // there is no provenance to claim.
                                stream_ctx
                                    .agent_stream_config
                                    .agent_name
                                    .as_ref()
                                    .map(|agent| {
                                        Self::format_tool_source(&ToolSource::Acp {
                                            agent: agent.clone(),
                                        })
                                    }),
                                None,
                                diffs,
                                // The ACP agent ran its own gate in its own
                                // process; we granted nothing here.
                                None,
                            ),
                        ) {
                            warn!(
                                session_id = %stream_ctx.session_id,
                                tool = %name,
                                "No subscribers for pass-through tool_call event"
                            );
                        }
                        continue;
                    }

                    saw_tool_activity = true;

                    // Commit to scheduler-owned conversation tree
                    // (shadow state until handle.history retires).
                    {
                        let mut tree = stream_ctx.conversation_tree.lock().await;
                        let parent = tree.current();
                        tree.add_child(
                            parent,
                            crucible_core::turn::NodeContent::ToolCall {
                                id: id.clone(),
                                name: name.clone(),
                                args: args.clone(),
                            },
                        );
                    }

                    let tool_call = ChatToolCall {
                        name: name.clone(),
                        arguments: Some(args.clone()),
                        id: Some(id.clone()),
                    };

                    // Dispatch (honoring blocked list + failure tracking).
                    let mut attempt: Option<usize> = None;
                    let mut result = if blocked_tools.contains(&name) {
                        let blocked_error = format!(
                            "Tool '{}' is blocked for this stream after repeated failures.",
                            name
                        );

                        if !emit_event(
                            &stream_ctx.event_tx,
                            SessionEventMessage::tool_result(
                                &stream_ctx.session_id,
                                &id,
                                &name,
                                serde_json::json!({ "error": blocked_error }),
                            ),
                        ) {
                            warn!(
                                session_id = %stream_ctx.session_id,
                                tool = %name,
                                "No subscribers for blocked tool_result event"
                            );
                        }

                        Some(ChatToolResult::error(
                            name.clone(),
                            id.clone(),
                            blocked_error,
                        ))
                    } else {
                        attempt = Some(tracker.record_call(&name, &args));
                        // The review bracket is CLOSED here and OPENED inside
                        // `handle_tool_call_in_stream`, below its review gate.
                        // Splitting the two is deliberate: opening late keeps
                        // the gate's unbounded wait for a human out of the
                        // agent's interval, while keeping the handle in this
                        // frame keeps the close-edge count at one. Every early
                        // return in the callee unwinds to the line below, and a
                        // cancelled turn drops this frame and fires the
                        // handle's own `Drop` guard.
                        let mut bracket = None;
                        let call_result = Self::handle_tool_call_in_stream(
                            &stream_ctx,
                            &tool_call,
                            diffs.clone(),
                            &mut bracket,
                        )
                        .await;
                        if let Some(bracket) = bracket {
                            stream_ctx.close_review_bracket(bracket, &id).await;
                        }
                        call_result
                    };

                    // Repeat-failure tracking / annotation.
                    let args_key =
                        serde_json::to_string(&args).unwrap_or_else(|_| "null".to_string());

                    if let Some(tool_result) = result.as_mut() {
                        if let Some(error) = tool_result.error.as_mut() {
                            let failure_key = (name.clone(), args_key.clone());
                            if last_failure_key.as_ref() == Some(&failure_key) {
                                consecutive_failure_count += 1;
                            } else {
                                consecutive_failure_count = 1;
                                last_failure_key = Some(failure_key);
                            }

                            if attempt.is_some_and(|a| a >= 3)
                                && tracker.is_repeat_failure(&name, &args, 3)
                            {
                                let attempt_val = attempt.unwrap_or_default();
                                let annotation = format!(
                                    "Attempt {}. This tool has failed {} times with identical arguments. Try a different approach.",
                                    attempt_val, attempt_val
                                );
                                if !error.contains(&annotation) {
                                    if !error.is_empty() {
                                        error.push(' ');
                                    }
                                    error.push_str(&annotation);
                                }
                            }

                            if consecutive_failure_count >= 3 {
                                blocked_tools.insert(name.clone());
                            }
                        } else {
                            last_failure_key = None;
                            consecutive_failure_count = 0;
                        }
                    }

                    let tool_result = result.unwrap_or_else(|| {
                        ChatToolResult::error(
                            name.clone(),
                            id.clone(),
                            "tool dispatcher returned no result",
                        )
                    });

                    // A delegation is attributed by the child's own ledger,
                    // not by a parent interval (see `needs_review_bracket`),
                    // so the parent records a reference to it here.
                    if name == "delegate_session" {
                        stream_ctx
                            .link_delegation_child(&id, &tool_result.result)
                            .await;
                    }

                    // Commit ToolResult to scheduler-owned tree before
                    // feeding it back to the adapter.
                    {
                        let mut tree = stream_ctx.conversation_tree.lock().await;
                        let parent = tree.current();
                        let result_value = serde_json::Value::String(tool_result.result.clone());
                        tree.add_child(
                            parent,
                            crucible_core::turn::NodeContent::ToolResult {
                                id: tool_result.call_id.clone().unwrap_or_else(|| id.clone()),
                                name: tool_result.name.clone(),
                                result: result_value,
                                error: tool_result.error.clone(),
                            },
                        );
                    }

                    // Record this result's terminate flag for the
                    // conjunctive batch-terminate check at ToolBatchEnd.
                    batch_terminate_signals.push(tool_result.terminate);

                    // Hand over anything a `tool_result` handler retrieved via
                    // `cru.context.attach` BEFORE the result itself.
                    //
                    // Ordering is load-bearing: the adapter collects results
                    // with `while collected.len() < pending_calls.len()`, so it
                    // stops reading the instant the final result arrives.
                    // Anything sent after that sits unread in the channel until
                    // the next batch — or forever, if the model answers instead
                    // of calling another tool. The handler has already run by
                    // this point, so the attachment is queued and ready.
                    //
                    // Context only — deliberately NOT committed to the
                    // conversation tree above, because history is append-only
                    // and scheduler-owned.
                    let mut adapter_gone = false;
                    for content in stream_ctx.context_attach.drain(&stream_ctx.session_id) {
                        if inbound_tx
                            .send(TurnEvent::ContextAttach { content })
                            .await
                            .is_err()
                        {
                            adapter_gone = true;
                            break;
                        }
                    }
                    if adapter_gone {
                        break;
                    }

                    // Feed back to the adapter so it can continue the turn.
                    let reply = TurnEvent::ToolResult {
                        id: tool_result.call_id.clone().unwrap_or_else(|| id.clone()),
                        name: tool_result.name,
                        result: serde_json::Value::String(tool_result.result),
                        error: tool_result.error,
                    };
                    if inbound_tx.send(reply).await.is_err() {
                        // Adapter dropped; end turn.
                        break;
                    }
                }
                TurnEvent::ToolResult {
                    id,
                    name,
                    result,
                    error,
                } => {
                    // The agent observed an external tool result
                    // (ACP-style). Pass through to subscribers — but run the
                    // `tool_result` handlers first, so a redactor scrubs the
                    // transcript no matter who executed the tool. Caveat: the
                    // external agent already consumed this result in its own
                    // process, so unlike the dispatched path these patches
                    // shape what subscribers and the session log see, NOT what
                    // that agent's model saw. Same boundary as isolation
                    // claims on external agents.
                    let args = acp_tool_args.remove(&id).unwrap_or(serde_json::Value::Null);
                    let original = result.clone();
                    let result_text = match &result {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    let (patched_text, error) = super::tool_hooks::apply_tool_result_handlers(
                        &stream_ctx,
                        &name,
                        &args,
                        result_text.clone(),
                        error,
                    )
                    .await;
                    // Same boundary as the patch caveat above: an external
                    // agent owns its own tool loop and never reads our inbound
                    // channel, so an attachment can never reach its model.
                    // Drain and say so — leaving it queued would burn the
                    // session's budget and dedup keys on content nobody will
                    // ever see, and grow the buffer for the session's lifetime.
                    let stranded = stream_ctx.context_attach.drain(&stream_ctx.session_id);
                    if !stranded.is_empty() {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            count = stranded.len(),
                            "cru.context.attach is not supported for external (ACP) agents; \
                             discarding attachments — the agent runs its own tool loop and \
                             never reads the runtime's inbound channel"
                        );
                    }

                    // Structured results survive verbatim when no handler
                    // touched them; a handler's patch is a string by contract.
                    let result = if patched_text == result_text {
                        original
                    } else {
                        serde_json::Value::String(patched_text)
                    };

                    let event_result = if let Some(err) = error {
                        serde_json::json!({ "error": err })
                    } else {
                        serde_json::json!({ "result": result })
                    };
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::tool_result(
                            &stream_ctx.session_id,
                            &id,
                            &name,
                            event_result,
                        ),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            tool = %name,
                            "No subscribers for pass-through tool_result event"
                        );
                    }
                }
                TurnEvent::ToolCallArgsUpdate { id, arguments } => {
                    // ACP late-args path: the agent announced the call
                    // without `rawInput` and supplied it in a follow-up
                    // frame. Pass through so subscribers can fill in the
                    // existing tool entry's arguments.
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::tool_call_args_update(
                            &stream_ctx.session_id,
                            &id,
                            arguments,
                        ),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            call_id = %id,
                            "No subscribers for tool_call_args_update event"
                        );
                    }
                }
                TurnEvent::ToolCallDiffUpdate { id, diffs } => {
                    // ACP late-diff path: the agent attached file-diff
                    // content via a `tool_call_update` after the matching
                    // `tool_call` was already announced. Pass through to
                    // subscribers so the TUI can merge into the existing
                    // tool entry. Does not advance tool depth or trigger
                    // dispatch.
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::tool_call_diff_update(
                            &stream_ctx.session_id,
                            &id,
                            diffs,
                        ),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            call_id = %id,
                            "No subscribers for tool_call_diff_update event"
                        );
                    }
                }
                TurnEvent::ToolBatchEnd => {
                    // Conjunctive early-stop: if every result in this
                    // batch set terminate=true, end the turn now instead
                    // of looping back to the model. Empty batches are
                    // ignored.
                    let should_terminate = !batch_terminate_signals.is_empty()
                        && batch_terminate_signals.iter().all(|t| *t);
                    batch_terminate_signals.clear();
                    if should_terminate {
                        if !emit_event(
                            &stream_ctx.event_tx,
                            SessionEventMessage::ended(
                                &stream_ctx.session_id,
                                "tool requested terminate".to_string(),
                            ),
                        ) {
                            warn!(
                                session_id = %stream_ctx.session_id,
                                "No subscribers for terminate ended event"
                            );
                        }
                        // A tool-requested terminate is a deliberate end of
                        // turn, not a failure.
                        return StreamOutcome::Completed;
                    }
                }
                TurnEvent::Usage(usage) => {
                    last_usage = Some(usage);
                }
                TurnEvent::ContextWindow { used, limit } => {
                    // A3. Only a delegated agent gets here: the internal
                    // agent's window is resolved once per session from its
                    // provider API, but an ACP session has no endpoint and no
                    // model to query, so the number can only come from the
                    // agent. Re-emitting the *setup* event rather than a new
                    // one is the point — every subscriber already assigns
                    // `context_limit_resolved` to its context total, so the
                    // statusline lights up with no client change at all.
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::context_limit_resolved(
                            &stream_ctx.session_id,
                            limit as usize,
                            ContextLimitSource::Agent,
                        ),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            "No subscribers for context_limit_resolved event"
                        );
                    }

                    // `used` is the other operand of that indicator, and it
                    // reaches subscribers on `message_complete` — there is no
                    // standalone event for it. Seeding `last_usage` is a
                    // floor, not the primary path: `usage_update` arrives
                    // mid-turn and `TurnEvent::Usage` (parsed from the final
                    // `PromptResponse`, see `acp/client/usage.rs`) arrives
                    // after it, so whenever the agent reports both, the later
                    // and strictly better value wins. The `is_none` guard only
                    // stops a trailing window frame from clobbering it.
                    //
                    // The floor is what keeps the fix honest. Emitting a limit
                    // with no occupancy would make the statusline draw
                    // "0% ctx" — a confident wrong answer, and worse than the
                    // "— ctx" it draws today. Nothing in ACP requires an agent
                    // that reports its window to also report per-turn usage;
                    // both fields are optional and independently gated
                    // upstream.
                    //
                    // `used` counts the context *before* the response is
                    // appended, so it reads as prompt tokens: opencode's 28224
                    // is exactly inputTokens 24496 + cachedReadTokens 3728,
                    // while its totalTokens 28278 also counts the 54 output
                    // tokens.
                    if last_usage.is_none() {
                        let used = u32::try_from(used).unwrap_or(u32::MAX);
                        last_usage = Some(TokenUsage {
                            prompt_tokens: used,
                            completion_tokens: 0,
                            total_tokens: used,
                            cache_read_tokens: None,
                            cache_creation_tokens: None,
                        });
                    }
                }
                TurnEvent::HandlerInjection { .. } | TurnEvent::ContextAttach { .. } => {
                    // Inbound-only variants. Adapter should not echo
                    // them, but tolerate if it ever does.
                }
                TurnEvent::Done { stop_reason } => {
                    terminal_stop_reason = Some(stop_reason);
                    break;
                }
                TurnEvent::Error(e) => {
                    error!(
                        session_id = %stream_ctx.session_id,
                        error = %e,
                        "Agent turn error"
                    );
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::ended(&stream_ctx.session_id, format!("error: {e}")),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            "No subscribers for error event"
                        );
                    }
                    return StreamOutcome::Failed(format!("agent turn error: {e}"));
                }
            }
        }

        // Close the inbound channel so the adapter wakes up if still
        // waiting (it should have terminated by now).
        drop(inbound_tx);

        // Empty response handling.
        if accumulated_response.trim().is_empty() && !saw_tool_activity {
            let error_reason = format!(
                "error: {}",
                crate::provider::genai_handle::EMPTY_RESPONSE_ERROR
            );
            error!(
                session_id = %stream_ctx.session_id,
                "LLM stream completed with no content and no tool calls"
            );
            if !emit_event(
                &stream_ctx.event_tx,
                SessionEventMessage::ended(&stream_ctx.session_id, error_reason.clone()),
            ) {
                warn!(
                    session_id = %stream_ctx.session_id,
                    "No subscribers for empty-response ended event"
                );
            }
            return StreamOutcome::Failed(error_reason);
        }

        if terminal_stop_reason.is_none() {
            warn!(
                session_id = %stream_ctx.session_id,
                reason = crate::provider::genai_handle::STREAM_UNEXPECTED_END_ERROR,
                "Stream ended without terminal done event"
            );
        }

        // Emit message_complete + run turn:complete handlers. Handler
        // injection short-circuits back into a fresh execute_agent_stream
        // with a new message_id so subscribers see a clean user message
        // boundary.
        let injection = Self::run_reactor_handlers(
            &stream_ctx,
            last_usage.as_ref(),
            accumulated_response,
            TurnFacts {
                stop_reason: terminal_stop_reason,
                continuation_depth,
                saw_tool_activity,
            },
        )
        .await;

        let mut continuation_outcome = StreamOutcome::Completed;
        if let Some(injected_content) = injection {
            drop(event_stream);
            // Release the handle lock before recursing so the inner
            // invocation can re-acquire it.
            drop(guard);

            accumulated_response.clear();
            // This literal names every field on purpose, although
            // `StreamContext` derives `Clone`. A new field then fails to
            // compile here, so its author must decide whether the retry
            // carries it or resets it. Do not replace it with `.clone()`.
            let continuation_ctx = StreamContext {
                session_id: stream_ctx.session_id.clone(),
                message_id: format!("msg-{}", uuid::Uuid::new_v4()),
                event_tx: stream_ctx.event_tx.clone(),
                slot: stream_ctx.slot.clone(),
                workspace_path: stream_ctx.workspace_path.clone(),
                session_dir: stream_ctx.session_dir.clone(),
                whitelists_dir: stream_ctx.whitelists_dir.clone(),
                agent_stream_config: stream_ctx.agent_stream_config.clone(),
                tool_dispatcher: stream_ctx.tool_dispatcher.clone(),
                permission_override: stream_ctx.permission_override,
                conversation_tree: stream_ctx.conversation_tree.clone(),
                session_manager: stream_ctx.session_manager.clone(),
                // Don't re-inject Precognition on a validation retry —
                // the original turn already prepended it.
                precognition_message: None,
                // Same reasoning for attachments: the original turn already
                // put the file contents in front of the agent.
                attachment_message: None,
                session_mode: stream_ctx.session_mode.clone(),
                is_interactive: stream_ctx.is_interactive,
                permission_engine: stream_ctx.permission_engine.clone(),
                // Carried across the retry: dedup and budget are per session,
                // so a validation retry must not get a fresh allowance to
                // re-attach what the original turn already attached.
                context_attach: stream_ctx.context_attach.clone(),
            };

            continuation_outcome = Box::pin(Self::execute_agent_stream(
                agent,
                injected_content,
                continuation_ctx,
                stream_config.clone(),
                accumulated_response,
                // The host counts; the plugin decides. No cap: the payload
                // carries this number so a plugin that wants a bound sets its
                // own, and the user's cancel reaches every depth.
                continuation_depth + 1,
            ))
            .await;
        }

        let duration_ms = stream_start.elapsed().as_millis() as u64;
        let response_summary: String = accumulated_response.chars().take(200).collect();

        if stream_config.model.is_empty() {
            warn!(
                session_id = %stream_ctx.session_id,
                "PostLlmCall model string is empty, possible upstream issue"
            );
        }

        if !emit_event(
            &stream_ctx.event_tx,
            SessionEventMessage::new(
                &stream_ctx.session_id,
                "post_llm_call",
                serde_json::json!({
                    "response_summary": &response_summary,
                    "model": &stream_config.model,
                    "duration_ms": duration_ms,
                    "token_count": Option::<u64>::None,
                }),
            ),
        ) {
            warn!(
                session_id = %stream_ctx.session_id,
                "No subscribers for post_llm_call event"
            );
        }

        // Observational event, run against the handler VM with the
        // state lock released (plugin Lua may run for seconds).
        let post_llm_event = SessionEvent::Custom {
            name: "post_llm_call".to_string(),
            payload: serde_json::json!({
                "response_summary": &response_summary,
                "model": &stream_config.model,
                "duration_ms": duration_ms,
            }),
        };
        run_handlers(
            stream_ctx.agent_stream_config.plugin_handlers.as_ref(),
            (),
            |registry, lua, ()| {
                let post_llm_event = &post_llm_event;
                let session_id = stream_ctx.session_id.as_str();
                Box::pin(async move {
                    Self::run_post_llm_call_handlers(session_id, &registry, &lua, post_llm_event)
                        .await;
                    ControlFlow::<(), ()>::Continue(())
                })
            },
        )
        .await;

        continuation_outcome
    }

    /// One registry's `post_llm_call` pass — fire-and-forget, fail-open.
    async fn run_post_llm_call_handlers(
        session_id: &str,
        registry: &crucible_lua::LuaScriptHandlerRegistry,
        lua: &mlua::Lua,
        event: &SessionEvent,
    ) {
        for handler in registry.runtime_handlers_for(
            StageId::PostLlmCall.as_str(),
            None,
            crucible_lua::Firing::InSession(session_id),
        ) {
            if let Err(error) = registry
                .execute_runtime_handler(lua, handler.id, event, Some(session_id))
                .await
            {
                warn!(
                    session_id = %session_id,
                    error = %error,
                    "post_llm_call handler error (fail-open)"
                );
            }
        }
    }

    /// Hand every `turn:complete` handler what the turn knows about itself,
    /// and return the last inject.
    ///
    /// The payload carries the reply TAIL rather than only its length. The
    /// text is already in memory, and a plugin that wants it otherwise has to
    /// read `session.jsonl` — a whole file per turn, growing with the session.
    ///
    /// It carries NO phrase list, NO regex and NO "the model means to
    /// continue" flag. A shipped pattern becomes an API, and Crucible would
    /// then own the accuracy of a guess about another vendor's prose. Policy
    /// over model output belongs in Lua.
    pub(in crate::agent_manager) async fn dispatch_turn_complete_handlers(
        session_id: &str,
        message_id: &str,
        response: &str,
        plugin_handlers: Option<&PluginHandlers>,
        facts: TurnFacts,
        response_tail_chars: usize,
    ) -> Option<String> {
        let is_continuation = facts.is_continuation();
        let (response_tail, response_truncated) = response_tail(response, response_tail_chars);
        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({
                "session_id": session_id,
                "message_id": message_id,
                "response_length": response.len(),
                "response_tail": response_tail,
                "response_truncated": response_truncated,
                "is_continuation": is_continuation,
                "continuation_depth": facts.continuation_depth,
                "saw_tool_activity": facts.saw_tool_activity,
                "stop_reason": facts.stop_reason,
            }),
        };

        // The handler VM's registry — the one that runs Lua files.
        // Inject is last-writer-wins within a registry and across them, so a
        // plugin's inject overrides a session handler's; a session that must
        // win can use priority within its own registry, but cross-registry
        // the later (plugin) pass acts last by the same rule that lets
        // plugin transforms see session transforms' output.
        run_handlers(plugin_handlers, None, |registry, lua, pending_injection| {
            let event = &event;
            Box::pin(async move {
                let injection = Self::run_turn_complete_handlers(
                    session_id,
                    &registry,
                    &lua,
                    event,
                    is_continuation,
                )
                .await;
                ControlFlow::Continue(injection.or(pending_injection))
            })
        })
        .await
    }

    /// One registry's `turn:complete` pass; returns its last inject, if any.
    async fn run_turn_complete_handlers(
        session_id: &str,
        registry: &crucible_lua::LuaScriptHandlerRegistry,
        lua: &mlua::Lua,
        event: &SessionEvent,
        is_continuation: bool,
    ) -> Option<String> {
        use crucible_lua::ScriptHandlerResult;

        let handlers = registry.runtime_handlers_for(
            StageId::TurnComplete.as_str(),
            None,
            crucible_lua::Firing::InSession(session_id),
        );
        if handlers.is_empty() {
            return None;
        }

        debug!(
            session_id = %session_id,
            handler_count = handlers.len(),
            is_continuation = is_continuation,
            "Dispatching turn:complete handlers"
        );

        let mut pending_injection: Option<String> = None;
        for handler in handlers {
            match registry
                .execute_runtime_handler(lua, handler.id, event, Some(session_id))
                .await
            {
                Ok(result) => {
                    debug!(
                        session_id = %session_id,
                        handler = handler.id,
                        result = ?result,
                        "Handler executed"
                    );

                    if let ScriptHandlerResult::Inject { content } = result {
                        debug!(
                            session_id = %session_id,
                            handler = handler.id,
                            content_len = content.len(),
                            "Handler returned inject"
                        );
                        pending_injection = Some(content);
                    }
                }
                Err(e) => {
                    error!(
                        session_id = %session_id,
                        handler = handler.id,
                        error = %e,
                        "Handler failed"
                    );
                }
            }
        }
        pending_injection
    }
}
