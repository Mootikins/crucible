//! Pure translation between the ACP wire and Crucible `TurnEvent`s.
//!
//! Everything here is a free function over values the handle already holds, so
//! the wire-shape decisions that drive rendering — what a turn stopped for,
//! what text an agent is sent — are testable without spawning an agent process.

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
/// `MaxTokens` and `Refusal` now have variants of their own, and the internal
/// agent reports the same two from `genai_handle::turn_stop_reason`, so a
/// delegated turn and an internal turn describe a truncation or a refusal with
/// the same word.
///
/// `MaxTurnRequests` still collapses to `EndTurn`, and it is listed explicitly
/// rather than left to the wildcard. It names a cap on the AGENT's own request
/// count, which no internal turn has and no provider reports, so a variant for
/// it would have exactly one producer and no distinct handler. The collapse is
/// logged so the loss stays visible at the call site, and a future
/// `#[non_exhaustive]` variant lands in the wildcard as an unreviewed default
/// rather than being quietly folded into this set.
///
/// `produced_anything` is "the turn yielded text, thinking or a tool call". It
/// outranks the other reasons: a refusal or a token cap that produced nothing
/// is still a turn the user saw nothing from, and `Empty` is the variant that
/// says so.
pub(super) fn turn_stop_reason(
    acp: agent_client_protocol::schema::v1::StopReason,
    produced_anything: bool,
) -> crucible_core::turn::StopReason {
    use agent_client_protocol::schema::v1::StopReason as Acp;
    use crucible_core::turn::StopReason;

    match acp {
        Acp::Cancelled => StopReason::Cancelled,
        _ if !produced_anything => StopReason::Empty,
        Acp::MaxTokens => StopReason::MaxTokens,
        Acp::Refusal => StopReason::Refusal,
        reason @ Acp::MaxTurnRequests => {
            tracing::debug!(
                acp_stop_reason = ?reason,
                "ACP turn ended for a reason Crucible has no variant for; reporting EndTurn"
            );
            StopReason::EndTurn
        }
        _ => StopReason::EndTurn,
    }
}

/// Map one client chunk onto one `TurnEvent`.
///
/// The client decides everything before a chunk reaches the handle: every
/// `ToolEnd` follows a `ToolStart` for its id and carries that start's name.
/// So this map is total and keeps no state. A new `StreamingChunk` variant
/// with no `TurnEvent` fails to compile here, not at run time.
impl From<crate::acp::streaming::StreamingChunk> for crucible_core::turn::TurnEvent {
    fn from(chunk: crate::acp::streaming::StreamingChunk) -> Self {
        use crate::acp::streaming::StreamingChunk;
        use crucible_core::turn::TurnEvent;

        match chunk {
            StreamingChunk::Text(text) => {
                tracing::debug!(chunk_type = "text", len = text.len(), "ACP streaming chunk");
                TurnEvent::TextDelta(text)
            }
            StreamingChunk::Thinking(text) => {
                tracing::debug!(
                    chunk_type = "thinking",
                    len = text.len(),
                    "ACP streaming chunk"
                );
                TurnEvent::Thinking(text)
            }
            StreamingChunk::ContextWindow { used, limit } => {
                tracing::debug!(used, limit, "ACP agent reported its context window");
                TurnEvent::ContextWindow { used, limit }
            }
            StreamingChunk::ToolStart {
                name,
                id,
                arguments,
                diffs,
            } => {
                tracing::info!(
                    tool = %name,
                    tool_id = %id,
                    diff_count = diffs.len(),
                    "ACP tool call started"
                );
                TurnEvent::ToolCall {
                    id,
                    name,
                    args: arguments.unwrap_or(serde_json::Value::Null),
                    diffs,
                }
            }
            StreamingChunk::ToolEnd {
                id,
                name,
                result,
                error,
            } => {
                tracing::info!(
                    tool = %name,
                    tool_id = %id,
                    has_error = error.is_some(),
                    "ACP tool call completed"
                );
                TurnEvent::ToolResult {
                    id,
                    name,
                    result: serde_json::Value::String(result.unwrap_or_default()),
                    error,
                }
            }
            StreamingChunk::ToolDiffUpdate { call_id, diffs } => {
                tracing::debug!(tool_id = %call_id, diff_count = diffs.len(), "ACP late diff update");
                TurnEvent::ToolCallDiffUpdate { id: call_id, diffs }
            }
            StreamingChunk::ToolArgsUpdate { call_id, arguments } => {
                tracing::debug!(tool_id = %call_id, "ACP late args update");
                TurnEvent::ToolCallArgsUpdate {
                    id: call_id,
                    arguments,
                }
            }
        }
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
            turn_stop_reason(
                agent_client_protocol::schema::v1::StopReason::Cancelled,
                false
            ),
            StopReason::Cancelled
        );
        assert_eq!(
            turn_stop_reason(
                agent_client_protocol::schema::v1::StopReason::Cancelled,
                true
            ),
            StopReason::Cancelled
        );
    }

    #[test]
    fn a_turn_that_produced_nothing_is_empty() {
        use crucible_core::turn::StopReason;

        assert_eq!(
            turn_stop_reason(
                agent_client_protocol::schema::v1::StopReason::EndTurn,
                false
            ),
            StopReason::Empty
        );
        assert_eq!(
            turn_stop_reason(agent_client_protocol::schema::v1::StopReason::EndTurn, true),
            StopReason::EndTurn
        );
    }

    /// A delegated turn keeps the two reasons a plugin acts on.
    ///
    /// They used to collapse to `EndTurn`, so a plugin reading the payload saw
    /// a truncated answer described as a natural completion — and the internal
    /// agent said the same thing, so the two agreed on the wrong word.
    #[test]
    fn a_truncation_and_a_refusal_keep_their_own_names() {
        use crucible_core::turn::StopReason;

        assert_eq!(
            turn_stop_reason(
                agent_client_protocol::schema::v1::StopReason::MaxTokens,
                true
            ),
            StopReason::MaxTokens
        );
        assert_eq!(
            turn_stop_reason(agent_client_protocol::schema::v1::StopReason::Refusal, true),
            StopReason::Refusal
        );
    }

    /// The agent's own request cap has no Crucible variant, so it stays a
    /// completion. Nothing else reports it: no provider sends it, and no
    /// internal turn can reach it.
    #[test]
    fn the_agents_request_cap_is_still_a_completion() {
        use crucible_core::turn::StopReason;

        assert_eq!(
            turn_stop_reason(
                agent_client_protocol::schema::v1::StopReason::MaxTurnRequests,
                true
            ),
            StopReason::EndTurn
        );
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
            agent_client_protocol::schema::v1::StopReason::MaxTokens,
            agent_client_protocol::schema::v1::StopReason::MaxTurnRequests,
            agent_client_protocol::schema::v1::StopReason::Refusal,
        ] {
            assert_eq!(turn_stop_reason(acp, false), StopReason::Empty, "{acp:?}");
        }
    }

    // -- Chunk to event: one sample per variant --------------------------------

    /// Every `StreamingChunk` variant maps to exactly one `TurnEvent`.
    ///
    /// The match below has no wildcard, so a new chunk variant fails to
    /// compile here until someone writes down its event.
    #[test]
    fn every_streaming_chunk_maps_to_one_turn_event() {
        use crate::acp::streaming::StreamingChunk;
        use crucible_core::turn::TurnEvent;

        let samples = [
            StreamingChunk::Text("hi".into()),
            StreamingChunk::Thinking("hmm".into()),
            StreamingChunk::ContextWindow { used: 3, limit: 10 },
            StreamingChunk::ToolStart {
                name: "Read".into(),
                id: "t1".into(),
                arguments: None,
                diffs: vec![],
            },
            StreamingChunk::ToolEnd {
                id: "t1".into(),
                name: "Read".into(),
                result: Some("out".into()),
                error: None,
            },
            StreamingChunk::ToolDiffUpdate {
                call_id: "t1".into(),
                diffs: vec![],
            },
            StreamingChunk::ToolArgsUpdate {
                call_id: "t1".into(),
                arguments: serde_json::json!({"path": "a"}),
            },
        ];

        for chunk in samples {
            let expected_shape = match &chunk {
                StreamingChunk::Text(_) => "TextDelta",
                StreamingChunk::Thinking(_) => "Thinking",
                StreamingChunk::ContextWindow { .. } => "ContextWindow",
                StreamingChunk::ToolStart { .. } => "ToolCall",
                StreamingChunk::ToolEnd { .. } => "ToolResult",
                StreamingChunk::ToolDiffUpdate { .. } => "ToolCallDiffUpdate",
                StreamingChunk::ToolArgsUpdate { .. } => "ToolCallArgsUpdate",
            };
            let event = TurnEvent::from(chunk);
            let actual_shape = match &event {
                TurnEvent::TextDelta(text) => {
                    assert_eq!(text, "hi");
                    "TextDelta"
                }
                TurnEvent::Thinking(text) => {
                    assert_eq!(text, "hmm");
                    "Thinking"
                }
                TurnEvent::ContextWindow { used, limit } => {
                    assert_eq!((*used, *limit), (3, 10));
                    "ContextWindow"
                }
                TurnEvent::ToolCall { id, name, args, .. } => {
                    assert_eq!((id.as_str(), name.as_str()), ("t1", "Read"));
                    assert!(args.is_null(), "absent arguments become Null");
                    "ToolCall"
                }
                TurnEvent::ToolResult {
                    id,
                    name,
                    result,
                    error,
                } => {
                    assert_eq!((id.as_str(), name.as_str()), ("t1", "Read"));
                    assert_eq!(result, &serde_json::Value::String("out".into()));
                    assert!(error.is_none());
                    "ToolResult"
                }
                TurnEvent::ToolCallDiffUpdate { id, .. } => {
                    assert_eq!(id, "t1");
                    "ToolCallDiffUpdate"
                }
                TurnEvent::ToolCallArgsUpdate { id, arguments } => {
                    assert_eq!(id, "t1");
                    assert_eq!(arguments["path"], "a");
                    "ToolCallArgsUpdate"
                }
                other => panic!("no chunk maps to {other:?}"),
            };
            assert_eq!(actual_shape, expected_shape);
        }
    }
}
