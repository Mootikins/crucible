//! Agent turn driver.
//!
//! Drives an `Agent::turn()` stream, emits session events, dispatches
//! tool calls, and steers the agent's tool loop via the inbound
//! `mpsc<TurnEvent>` channel. The plan's "one channel topology, not
//! three" rule: `ToolResult` and `ContextAttach` both arrive on the
//! same inbound channel and drive matching adapter-side behaviour.
//!
//! A turn ENDS here. A `turn:complete` handler that wants more work asks
//! for a NEW turn; `send.rs` starts it. There is no continuation
//! inside a turn.

use super::super::*;
use crate::agent_manager::tool_tracking::ToolCallTracker;
use crucible_core::protocol::session_events::ContextLimitSource;
use crucible_core::traits::chat::{ChatToolCall, ChatToolResult};
use crucible_core::traits::llm::TokenUsage;
use crucible_core::turn::{Agent as TurnAgent, StopReason, TurnContext, TurnEvent};
use crucible_core::types::{CanonicalToolCall, ToolSource};
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
/// provides the inputs and holds no opinion, which is why there is no budget
/// check here: a plugin that wants a bound sets its own.
#[derive(Debug, Clone, Copy)]
pub(in crate::agent_manager) struct TurnFacts {
    /// Why the provider or the delegated agent ended the turn. `None` when
    /// the stream closed without a terminal `Done`.
    pub(in crate::agent_manager) stop_reason: Option<StopReason>,
    /// Did the turn run a tool the user can see?
    ///
    /// A turn that ends right after a tool result is a model still working,
    /// which removes a class of false positive no reply text can.
    pub(in crate::agent_manager) saw_tool_activity: bool,
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

/// The loop guard of one turn. Three failures in a row of one tool with the
/// same key block that tool for the rest of the turn. The key is the
/// arguments, and for an ACP call also its title and its locations.
#[derive(Default)]
struct LoopGuard {
    last_failure: Option<(String, String)>,
    failures: usize,
    blocked: HashSet<String>,
}

impl LoopGuard {
    /// The refusal of a call to a blocked tool.
    fn refusal(&self, tool: &str) -> Option<String> {
        (self.blocked.contains(tool))
            .then(|| format!("Tool '{tool}' is blocked for this stream after repeated failures."))
    }

    fn record(&mut self, tool: &str, key: &serde_json::Value, failed: bool) {
        if !failed {
            self.last_failure = None;
            self.failures = 0;
            return;
        }
        let key = (tool.to_string(), key.to_string());
        if self.last_failure.as_ref() == Some(&key) {
            self.failures += 1;
        } else {
            self.failures = 1;
            self.last_failure = Some(key);
        }
        if self.failures >= 3 {
            self.blocked.insert(tool.to_string());
        }
    }
}

impl AgentManager {
    #[allow(clippy::ptr_arg)]
    pub(super) async fn run_reactor_handlers(
        stream_ctx: &StreamContext,
        usage: Option<&TokenUsage>,
        accumulated_response: &mut String,
        facts: TurnFacts,
    ) {
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

        // A handler that wants more work asks for a NEW turn. The daemon
        // starts it after this turn releases its request slot, so the new
        // turn gets admission, precognition, persistence and undo like any
        // other turn. There is no continuation inside a turn.
        if let Some(content) = Self::dispatch_turn_complete_handlers(
            &stream_ctx.session_id,
            &stream_ctx.message_id,
            accumulated_response,
            stream_ctx.agent_stream_config.plugin_handlers.as_ref(),
            facts,
            stream_ctx.agent_stream_config.response_tail_chars,
        )
        .await
        {
            info!(
                session_id = %stream_ctx.session_id,
                content_len = content.len(),
                "A turn:complete handler asked for a new turn"
            );
            stream_ctx.slot.set_follow_up(content);
        }
    }

    pub(super) async fn execute_agent_stream(
        agent: Arc<Mutex<BoxedAgentHandle>>,
        content: String,
        stream_ctx: StreamContext,
        stream_config: AgentStreamConfig,
        accumulated_response: &mut String,
    ) -> StreamOutcome {
        let ttft_local = Instant::now();
        info!(target: "ttft", session_id = %stream_ctx.session_id, stage = "execute_stream_entry", elapsed_ms = 0, "ttft");
        let Some(content) =
            Self::apply_pre_llm_call_handlers(content, &stream_ctx, &stream_config).await
        else {
            return StreamOutcome::HandlerCancelled("cancelled by pre_llm_call handler".into());
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
        let Some(transformed_messages) = Self::apply_transform_context_handlers(
            flattened_messages.clone(),
            &stream_ctx,
            &stream_config,
        )
        .await
        else {
            return StreamOutcome::HandlerCancelled(
                "cancelled by transform_context handler".into(),
            );
        };
        let injected =
            crucible_core::turn::added_messages(&flattened_messages, &transformed_messages);

        let (inbound_tx, inbound_rx) = mpsc::channel::<TurnEvent>(32);
        let turn_ctx = TurnContext::new(content)
            .with_inbound(inbound_rx)
            .with_messages(transformed_messages)
            .with_injected(injected);

        info!(target: "ttft", session_id = %stream_ctx.session_id, stage = "before_turn_start", elapsed_ms = ttft_local.elapsed().as_millis() as u64, "ttft");
        // Hold the handle guard for the entire turn; Agent::turn returns
        // a stream that borrows `&mut *guard`.
        let mut guard = agent.lock().await;
        // ACP-style agents run their own tool loop server-side and emit
        // `ToolCall` events as observations (and drop the inbound channel).
        // The scheduler runs their handlers but does not dispatch them.
        // Capture the flag now, before `turn()` borrows the guard mutably.
        let agent_owns_tools = guard.capabilities().owns_history;
        let mut event_stream = match guard.turn(turn_ctx).await {
            Ok(s) => s,
            Err(e) => {
                error!(
                    session_id = %stream_ctx.session_id,
                    error = %e,
                    "Agent failed to start turn"
                );
                return StreamOutcome::Failed(format!("agent failed to start turn: {e}"));
            }
        };

        // Per-batch tool tracking
        let mut tracker = ToolCallTracker::new();
        let mut loop_guard = LoopGuard::default();
        // Did this turn produce any tool activity the user can see? Set for
        // every call, also for a call that the agent runs itself. Reading it
        // as "dispatched" made the empty-response guard fire on every
        // delegated turn that ran tools and narrated nothing.
        let mut saw_tool_activity = false;
        // The args, the canonical call and the open review bracket of each
        // call that an agent runs itself, from its `ToolCall` to its
        // `ToolResult`, by call id. The result handlers get the same
        // `{tool, args, ...}` payload as for a Crucible tool, and the result
        // renders the last canonical call. A bracket that no result closes
        // deregisters itself when the turn drops it.
        let mut agent_calls: HashMap<String, (serde_json::Value, CanonicalToolCall)> =
            HashMap::new();
        let mut agent_brackets: HashMap<String, crate::review::CaptureHandle> = HashMap::new();

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
                    // text as their final delta. Skip if it matches. An
                    // answer of only whitespace is not an answer, so a
                    // second "\n" after a first one is not a resend.
                    if !accumulated_response.trim().is_empty() && delta == *accumulated_response {
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
                    call,
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

                    saw_tool_activity = true;
                    stream_ctx
                        .add_tool_node(crucible_core::turn::NodeContent::ToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            args: args.clone(),
                        })
                        .await;

                    // An agent that runs its own tools already runs this
                    // call. It goes through the same announce and result
                    // functions below, with no dispatch: the inbound channel
                    // is closed for such an agent. A loop guard cannot stop
                    // a call that runs, so it ends the turn.
                    if agent_owns_tools {
                        if let Some(reason) = loop_guard.refusal(&name) {
                            return StreamOutcome::HandlerCancelled(reason);
                        }
                        let mut call = call.map_or_else(
                            || CanonicalToolCall::crucible_tool(&name, &args),
                            |call| *call,
                        );
                        super::tool_hooks::render_call(
                            stream_ctx.agent_stream_config.plugin_handlers.as_ref(),
                            &stream_ctx.session_id,
                            &mut call,
                            &args,
                            stream_ctx.origin,
                        )
                        .await;
                        // The bracket opens at the call and closes at its
                        // result, so the ledger names the call that wrote.
                        // A read opens none: a bracket that overlaps a write
                        // makes the write contested.
                        if !matches!(call.kind.as_str(), "file_read" | "search") {
                            if let Some(bracket) = stream_ctx.open_review_bracket(&name).await {
                                agent_brackets.insert(id.clone(), bracket);
                            }
                        }
                        // The card names the agent, `[acp:claude]`. `None`
                        // is an agent with no name, which is not ACP.
                        let source =
                            (stream_ctx.agent_stream_config.agent_name.as_ref()).map(|agent| {
                                Self::format_tool_source(&ToolSource::Acp {
                                    agent: agent.clone(),
                                })
                            });
                        stream_ctx
                            .announce_tool_call(&id, &args, (None, source), call.clone(), None)
                            .await;
                        agent_calls.insert(id, (args, call));
                        continue;
                    }

                    let tool_call = ChatToolCall {
                        name: name.clone(),
                        arguments: Some(args.clone()),
                        id: Some(id.clone()),
                    };

                    // Dispatch (honoring blocked list + failure tracking).
                    let mut attempt: Option<usize> = None;
                    let mut tool_result = if let Some(blocked_error) = loop_guard.refusal(&name) {
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

                        ChatToolResult::error(name.clone(), id.clone(), blocked_error)
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
                            call.map(|call| call.diffs).unwrap_or_default(),
                            &mut bracket,
                        )
                        .await;
                        if let Some(bracket) = bracket {
                            stream_ctx.close_review_bracket(bracket, &id).await;
                        }
                        call_result
                    };

                    // Repeat-failure tracking / annotation.
                    loop_guard.record(&name, &args, tool_result.error.is_some());
                    if let Some(error) = tool_result.error.as_mut() {
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
                    }

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
                    stream_ctx
                        .add_tool_node(crucible_core::turn::NodeContent::ToolResult {
                            id: tool_result.call_id.clone().unwrap_or_else(|| id.clone()),
                            name: tool_result.name.clone(),
                            result: serde_json::Value::String(tool_result.result.clone()),
                            error: tool_result.error.clone(),
                        })
                        .await;

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
                    // Only an agent that runs its own tools sends a result.
                    // The result is a notification: the agent's model read
                    // it already, so the hooks shape what subscribers, the
                    // transcript and the tree get, not what the model saw.
                    let (args, call) = agent_calls.remove(&id).unwrap_or_else(|| {
                        let args = serde_json::Value::Null;
                        let call = CanonicalToolCall::crucible_tool(&name, &args);
                        (args, call)
                    });
                    // The agent got only a reject option. The reason of the
                    // gate says why, where the agent can only say "rejected".
                    let denial = stream_ctx.slot.take_denial(&id);
                    let error = error.map(|e| denial.unwrap_or(e));
                    let text = match result {
                        serde_json::Value::String(text) => text,
                        other => other.to_string(),
                    };
                    // An agent can send no tool name and put the command
                    // only in the title (Gemini), so the title and the
                    // locations tell its calls apart for the loop guard.
                    let guard_key = serde_json::json!([
                        &args,
                        call.raw.as_ref().map(|raw| (&raw.title, &raw.locations)),
                    ]);
                    let (text, error) = stream_ctx
                        .finish_tool_result(&id, call, &args, text, error, false)
                        .await;
                    if let Some(bracket) = agent_brackets.remove(&id) {
                        stream_ctx.close_review_bracket(bracket, &id).await;
                    }
                    loop_guard.record(&name, &guard_key, error.is_some());
                    stream_ctx
                        .add_tool_node(crucible_core::turn::NodeContent::ToolResult {
                            id,
                            name,
                            result: serde_json::Value::String(text),
                            error,
                        })
                        .await;
                    // The agent never reads the inbound channel, so an
                    // attachment can never reach its model. Drain it, or it
                    // takes the session's budget and dedup keys for nothing.
                    let stranded = stream_ctx.context_attach.drain(&stream_ctx.session_id);
                    if !stranded.is_empty() {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            count = stranded.len(),
                            "cru.context.attach is not supported for an agent that runs \
                             its own tools; discarding attachments"
                        );
                    }
                }
                TurnEvent::ToolCallUpdate { id, mut call } => {
                    // An ACP agent sent the arguments or the diff of a call
                    // in a later frame. Pass the new canonical call through,
                    // with a new render, so subscribers can update the
                    // existing tool entry.
                    let args = (call.raw.as_ref())
                        .and_then(|raw| raw.raw_input.clone())
                        .unwrap_or_default();
                    super::tool_hooks::render_call(
                        stream_ctx.agent_stream_config.plugin_handlers.as_ref(),
                        &stream_ctx.session_id,
                        &mut call,
                        &args,
                        stream_ctx.origin,
                    )
                    .await;
                    if let Some((_, known)) = agent_calls.get_mut(&id) {
                        *known = (*call).clone();
                    }
                    if !emit_event(
                        &stream_ctx.event_tx,
                        SessionEventMessage::tool_call_update(
                            &stream_ctx.session_id,
                            &id,
                            *call,
                            None,
                        ),
                    ) {
                        warn!(
                            session_id = %stream_ctx.session_id,
                            call_id = %id,
                            "No subscribers for tool_call_update event"
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
                        // A tool-requested terminate is a deliberate end of
                        // turn, not a failure.
                        return StreamOutcome::Completed(None);
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
                    // `PromptResponse`, see `acp::turn_usage`) arrives
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
                TurnEvent::ContextAttach { .. } => {
                    // Inbound-only variant. Adapter should not echo
                    // it, but tolerate if it ever does.
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
            return StreamOutcome::Failed(error_reason);
        }

        if terminal_stop_reason.is_none() {
            warn!(
                session_id = %stream_ctx.session_id,
                reason = crate::provider::genai_handle::STREAM_UNEXPECTED_END_ERROR,
                "Stream ended without terminal done event"
            );
        }

        // Emit message_complete + run turn:complete handlers. A handler that
        // asks for more work stores a follow-up on the slot; `send.rs` starts
        // it as a new turn after this one releases the request slot.
        Self::run_reactor_handlers(
            &stream_ctx,
            last_usage.as_ref(),
            accumulated_response,
            TurnFacts {
                stop_reason: terminal_stop_reason,
                saw_tool_activity,
            },
        )
        .await;

        let duration_ms = stream_start.elapsed().as_millis() as u64;
        let response_summary: String = accumulated_response.chars().take(200).collect();

        if stream_config.model.is_empty() {
            warn!(
                session_id = %stream_ctx.session_id,
                "PostLlmCall model string is empty, possible upstream issue"
            );
        }

        // One payload goes to the subscribers and to the Lua handlers.
        let post_llm = SessionEventMessage::typed(
            &stream_ctx.session_id,
            TurnPayload::PostLlmCall {
                response_summary,
                model: stream_config.model.clone(),
                duration_ms,
            },
        );
        let post_llm_event = SessionEvent::Custom {
            name: post_llm.event.clone(),
            payload: post_llm.data.clone(),
        };
        if !emit_event(&stream_ctx.event_tx, post_llm) {
            warn!(
                session_id = %stream_ctx.session_id,
                "No subscribers for post_llm_call event"
            );
        }

        // Observational event, run against the handler VM with the
        // state lock released (plugin Lua may run for seconds).
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

        StreamOutcome::Completed(terminal_stop_reason)
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
        let (response_tail, response_truncated) = response_tail(response, response_tail_chars);
        let event = SessionEvent::Custom {
            name: "turn:complete".to_string(),
            payload: serde_json::json!({
                "session_id": session_id,
                "message_id": message_id,
                "response_length": response.len(),
                "response_tail": response_tail,
                "response_truncated": response_truncated,
                "saw_tool_activity": facts.saw_tool_activity,
                "stop_reason": facts.stop_reason,
            }),
        };

        // The handler VM's registry — the one that runs Lua files.
        // Inject is last-writer-wins within a registry and across them, so a
        // plugin's inject overrides a session handler's. Within one registry
        // the later registration acts last, and `priority` no longer exists to
        // change that; cross-registry the later (plugin) pass acts last by the
        // same rule that lets plugin transforms see session transforms'
        // output.
        run_handlers(plugin_handlers, None, |registry, lua, pending_injection| {
            let event = &event;
            Box::pin(async move {
                let injection =
                    Self::run_turn_complete_handlers(session_id, &registry, &lua, event).await;
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
