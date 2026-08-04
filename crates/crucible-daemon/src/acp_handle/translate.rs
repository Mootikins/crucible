//! Pure translation between the ACP wire and Crucible `TurnEvent`s.
//!
//! Everything here is a free function over values the handle already holds, so
//! the wire-shape decisions that drive rendering — what a turn stopped for,
//! which tool calls still need announcing, what text an agent is sent — are
//! testable without spawning an agent process.

/// Replay the tool calls the ACP client reported that the streaming callback
/// never announced, in wire order.
///
/// `tc.diffs` was populated by `record_tool_call` from the ACP frames'
/// `ToolCallContent::Diff` entries (initial + later `ToolCallUpdate` frames
/// merged via `upsert_tool_info`).
///
/// Dedup keys on the call id, and only a call that *has* an id can dedup:
/// `ToolCallInfo::id` is `Option<String>`, and `upsert_tool_info`
/// (`acp/client/tools.rs`) falls through to `push` for an id-less call without
/// merging, so one turn's `acp_tool_calls` can hold several. They all collapse
/// to `""` here and every one of them still has to reach the renderer — hence
/// this only *reads* the announced set and never adds to it. Deciding whether
/// a batch existed is the caller's bookkeeping for the same reason: a replayed
/// id-less call announces something while contributing no id.
///
/// No production `ToolCallInfo` reaches here without an id (every construction
/// site in `acp/client/streaming.rs` calls `.with_id`), so the multi-`None`
/// case is latent — but the rule belongs to this seam, not to the accident
/// that its only current caller cannot hit it.
///
/// The title is humanized here because the live path already is: the
/// `ToolStart` chunk carries `humanize_tool_title(&tool_call.title)`
/// (`acp/client/streaming.rs`) while `record_tool_call` stores the raw title,
/// so replaying it verbatim named the same tool `mcp__crucible__semantic_search`
/// when replayed and `Semantic Search` when announced live. The function is
/// idempotent on an already-clean title, so applying it twice is not a risk.
pub(super) fn replay_unannounced_tool_calls(
    acp_tool_calls: Vec<crucible_core::types::acp::ToolCallInfo>,
    announced_ids: &std::collections::HashSet<String>,
) -> Vec<crucible_core::turn::TurnEvent> {
    acp_tool_calls
        .into_iter()
        .filter_map(|tc| {
            let id = tc.id.clone().unwrap_or_default();
            if announced_ids.contains(&id) {
                return None;
            }
            Some(crucible_core::turn::TurnEvent::ToolCall {
                id,
                name: crate::acp::streaming::humanize_tool_title(&tc.title),
                args: tc.arguments.unwrap_or(serde_json::Value::Null),
                diffs: tc.diffs,
            })
        })
        .collect()
}

/// Map the ACP turn-ending reason onto Crucible's `StopReason`.
///
/// The reason is the only signal separating "the agent finished" from "the
/// agent was stopped", and ACP puts it on the wire: an agent that has seen
/// `session/cancel` MUST answer with `Cancelled`, so the response — not the
/// client's own `StreamingState.cancelled` — is where a cancelled delegated
/// turn becomes observable. (`StreamingState.cancelled` is set when a streaming
/// callback returns `false`, which for this handle means `chunk_rx` was
/// dropped; at that point the stream body that would yield this event is gone,
/// so nothing could ever read a stop reason derived from it.)
///
/// `MaxTokens`, `MaxTurnRequests` and `Refusal` all describe a turn the agent
/// chose to end and that the user sees as complete; Crucible has no variant for
/// them, so they collapse to `EndTurn`. That collapse is a real loss — a
/// budget-truncated answer is reported as a natural completion, which is the
/// same failure mode divergence B3 named — but it is *parity*: the internal
/// agent collapses identically, having no upstream reason to carry either.
/// They are listed explicitly rather than left to the wildcard so the loss is
/// visible at the call site, so it is logged, and so a future
/// `#[non_exhaustive]` variant lands in the wildcard as an unreviewed default
/// instead of being quietly folded into this set.
///
/// `MaxToolDepth` stays unreachable on this path: the daemon does not dispatch a
/// delegated agent's tools, so it never counts their depth
/// (`crucible-core/src/traits/chat.rs`).
///
/// `produced_anything` is "the turn yielded text, thinking or a tool call". It
/// outranks the budget reasons: a refusal or a token cap that produced nothing
/// is still a turn the user saw nothing from, and `Empty` is the variant that
/// says so.
///
/// **No consumer reads any of this yet.** `terminal_stop_reason`
/// (`agent_manager/messaging/stream.rs`) is only ever tested for `is_none()`,
/// nothing matches on the value, and the daemon-proxy path re-fabricates
/// `EndTurn` (`rpc_client/agent/convert.rs`). This lands for contract
/// consistency with `GenaiAgentHandle` — so the first consumer that does read a
/// stop reason is not silently wrong on delegated turns — not because a reader
/// exists today.
pub(super) fn turn_stop_reason(
    acp: agent_client_protocol::StopReason,
    produced_anything: bool,
) -> crucible_core::turn::StopReason {
    use agent_client_protocol::StopReason as Acp;
    use crucible_core::turn::StopReason;

    match acp {
        Acp::Cancelled => StopReason::Cancelled,
        _ if !produced_anything => StopReason::Empty,
        reason @ (Acp::MaxTokens | Acp::MaxTurnRequests | Acp::Refusal) => {
            tracing::debug!(
                acp_stop_reason = ?reason,
                "ACP turn ended for a reason Crucible has no variant for; reporting EndTurn"
            );
            StopReason::EndTurn
        }
        _ => StopReason::EndTurn,
    }
}

/// Build the prompt text sent to an ACP agent for one turn.
///
/// ACP agents own their conversation history, so we send only the new user
/// content — never the daemon's flattened history (that would duplicate what
/// the agent already holds). The exception is daemon-injected context:
/// Precognition and Lua `transform_context` handlers prepend System-role
/// blocks to `ctx.messages` (see `apply_transform_context_handlers`). Those
/// represent knowledge the external agent has no other way to see, so we
/// forward them ahead of the user content. Precognition only fires on the
/// first user message, so this does not bloat the agent's context every turn.
pub(super) fn acp_prompt_text(
    content: &str,
    messages: &[crucible_core::traits::context_ops::ContextMessage],
) -> String {
    use crucible_core::traits::llm::MessageRole;

    let injected: Vec<&str> = messages
        .iter()
        .filter(|m| m.role == MessageRole::System)
        .map(|m| m.content.as_str())
        .filter(|s| !s.is_empty())
        .collect();

    if injected.is_empty() {
        return content.to_string();
    }

    let mut out = injected.join("\n\n");
    out.push_str("\n\n");
    out.push_str(content);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- ACP prompt building: daemon-injected context push -------------------
    //
    // ACP agents own their conversation history, so `turn()` sends only the
    // new user content — never the flattened history. But daemon-side context
    // injection (Precognition, Lua `transform_context`) is prepended to
    // `ctx.messages` as System-role blocks the agent would otherwise never
    // see. `acp_prompt_text` is the seam that forwards that injected context.

    use crucible_core::traits::context_ops::ContextMessage;

    #[test]
    fn injected_system_context_is_prepended_to_user_content() {
        let precog = ContextMessage::system("KNOWLEDGE:\n- foo relates to bar")
            .with_tag(crate::agent_manager::precognition::PRECOGNITION_TAG);
        let user = ContextMessage::user("What is foo?");

        let prompt = acp_prompt_text("What is foo?", &[precog, user]);

        assert!(
            prompt.contains("KNOWLEDGE:\n- foo relates to bar"),
            "injected precognition context must reach the ACP prompt, got: {prompt:?}"
        );
        assert!(prompt.contains("What is foo?"));
        // Injected context precedes the user's question.
        assert!(
            prompt.find("KNOWLEDGE").unwrap() < prompt.find("What is foo?").unwrap(),
            "injected context must come before the user content"
        );
    }

    #[test]
    fn no_injected_context_leaves_user_content_unchanged() {
        let user = ContextMessage::user("just chatting");
        // History (User/Assistant) must NOT be resent — the ACP agent owns it.
        let prior = ContextMessage::assistant("earlier reply");
        let prompt = acp_prompt_text("just chatting", &[prior, user]);
        assert_eq!(prompt, "just chatting");
    }

    #[test]
    fn multiple_system_blocks_are_all_injected_in_order() {
        let a = ContextMessage::system("BLOCK_A");
        let b = ContextMessage::system("BLOCK_B");
        let prompt = acp_prompt_text("hi", &[a, b]);
        let ia = prompt.find("BLOCK_A").unwrap();
        let ib = prompt.find("BLOCK_B").unwrap();
        let iu = prompt.find("hi").unwrap();
        assert!(
            ia < ib && ib < iu,
            "blocks injected in order before content: {prompt:?}"
        );
    }

    #[test]
    fn empty_messages_falls_back_to_raw_content() {
        assert_eq!(acp_prompt_text("solo", &[]), "solo");
    }

    // -- Stop reasons (divergence B3) ----------------------------------------

    #[test]
    fn wire_cancellation_beats_an_empty_turn() {
        use crucible_core::turn::StopReason;

        // A turn cancelled before it produced anything is cancelled, not
        // empty: "the agent said nothing" and "the user stopped it" are
        // different things to report, and the cancellation is the cause.
        assert_eq!(
            turn_stop_reason(agent_client_protocol::StopReason::Cancelled, false),
            StopReason::Cancelled
        );
        assert_eq!(
            turn_stop_reason(agent_client_protocol::StopReason::Cancelled, true),
            StopReason::Cancelled
        );
    }

    #[test]
    fn a_turn_that_produced_nothing_is_empty() {
        use crucible_core::turn::StopReason;

        assert_eq!(
            turn_stop_reason(agent_client_protocol::StopReason::EndTurn, false),
            StopReason::Empty
        );
        assert_eq!(
            turn_stop_reason(agent_client_protocol::StopReason::EndTurn, true),
            StopReason::EndTurn
        );
    }

    #[test]
    fn budget_and_refusal_endings_are_completions_not_cancellations() {
        use crucible_core::turn::StopReason;

        // Crucible has no variant for these. Reporting them as `Cancelled`
        // would claim the user stopped the turn; reporting `Empty` would claim
        // the agent said nothing. Both are false — the agent ended a turn that
        // produced output, which is `EndTurn`.
        for acp in [
            agent_client_protocol::StopReason::MaxTokens,
            agent_client_protocol::StopReason::MaxTurnRequests,
            agent_client_protocol::StopReason::Refusal,
        ] {
            assert_eq!(turn_stop_reason(acp, true), StopReason::EndTurn, "{acp:?}");
        }
    }

    /// A budget or refusal ending that produced nothing is still empty.
    ///
    /// The `EndTurn` collapse above is only honest for a turn the user saw
    /// output from. A refusal the agent emitted no text for, or a token cap hit
    /// before the first delta, showed nothing at all — reporting `EndTurn`
    /// there would claim a completed answer where there is a blank screen.
    #[test]
    fn a_budget_or_refusal_ending_that_produced_nothing_is_still_empty() {
        use crucible_core::turn::StopReason;

        for acp in [
            agent_client_protocol::StopReason::MaxTokens,
            agent_client_protocol::StopReason::MaxTurnRequests,
            agent_client_protocol::StopReason::Refusal,
        ] {
            assert_eq!(turn_stop_reason(acp, false), StopReason::Empty, "{acp:?}");
        }
    }

    // -- Post-stream replay of tool calls the callback missed ----------------

    use crucible_core::turn::TurnEvent;
    use crucible_core::types::acp::ToolCallInfo;
    use std::collections::HashSet;

    fn replayed_names(events: &[TurnEvent]) -> Vec<&str> {
        events
            .iter()
            .map(|e| match e {
                TurnEvent::ToolCall { name, .. } => name.as_str(),
                other => panic!("replay yielded a non-ToolCall event: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn replay_skips_calls_the_stream_already_announced() {
        let announced: HashSet<String> = ["seen".to_string()].into_iter().collect();
        let calls = vec![
            ToolCallInfo::new("Already Rendered").with_id("seen"),
            ToolCallInfo::new("Missed By The Callback").with_id("unseen"),
        ];

        let events = replay_unannounced_tool_calls(calls, &announced);

        assert_eq!(replayed_names(&events), vec!["Missed By The Callback"]);
    }

    /// A replayed call is named exactly as the live path would have named it.
    ///
    /// `record_tool_call` stores the raw ACP `title`, but the live `ToolStart`
    /// carries `humanize_tool_title(&title)` (`acp/client/streaming.rs`). Left
    /// alone, the same tool reads `Semantic Search` when the callback announced
    /// it and `mcp__crucible__semantic_search` when the replay did — a name
    /// that changes with the timing of a frame.
    #[test]
    fn replayed_names_are_humanized_like_the_live_ones() {
        let calls = vec![ToolCallInfo::new("mcp__crucible__semantic_search").with_id("late")];

        let events = replay_unannounced_tool_calls(calls, &HashSet::new());

        assert_eq!(replayed_names(&events), vec!["Semantic Search"]);
    }

    /// Every id-less call replays — dedup keys on an id these calls do not have.
    ///
    /// `upsert_tool_info` (`acp/client/tools.rs`) pushes an id-less
    /// `ToolCallInfo` without merging, so `acp_tool_calls` can hold several,
    /// and they all collapse to `""` at the replay. Treating that shared empty
    /// key as a dedup key renders the first and silently swallows the rest.
    #[test]
    fn replay_keeps_every_id_less_call() {
        let calls = vec![
            ToolCallInfo::new("First Untitled"),
            ToolCallInfo::new("Second Untitled"),
            ToolCallInfo::new("Third Untitled"),
        ];

        let events = replay_unannounced_tool_calls(calls, &HashSet::new());

        assert_eq!(
            replayed_names(&events),
            vec!["First Untitled", "Second Untitled", "Third Untitled"],
            "id-less tool calls share the empty key and must not dedup against \
             each other — dropping them loses the call in every renderer"
        );
    }

    /// A replayed id-less call leaves the announced set empty.
    ///
    /// This is what forces the batch-end guard to be its own flag rather than
    /// `!announced_ids.is_empty()`: a batch plainly existed — the caller is
    /// about to yield a `ToolCall` for it — while the set that would have
    /// answered the question is still empty.
    #[test]
    fn an_id_less_replay_leaves_no_id_for_the_batch_end_guard_to_read() {
        let announced: HashSet<String> = HashSet::new();
        let calls = vec![ToolCallInfo::new("Untitled")];

        let events = replay_unannounced_tool_calls(calls, &announced);

        assert_eq!(replayed_names(&events), vec!["Untitled"]);
        assert!(
            announced.is_empty(),
            "the replay reports what it yielded, it does not record it"
        );
    }
}
