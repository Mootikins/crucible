//! Per-agent `TurnEvent` shape projection.
//!
//! Projects **one** agent's outbound `TurnEvent` stream down to the parts that
//! drive rendering, so a test can assert that agent's own contract — e.g. "an
//! ACP turn containing one edit yields `ToolCall` → `ToolResult` →
//! `ToolBatchEnd` → `Done(EndTurn)`, and the diff update names the file the
//! call named". Values that legitimately vary run to run (raw tool-call ids,
//! token counts, chunk boundaries) are normalized away; everything retained
//! here changes the frame.
//!
//! This is deliberately **not** a cross-agent comparator. The two agents
//! diverge at `TurnEvent` *by design*: the internal agent yields `ToolCall` +
//! `ToolBatchEnd` and lets the daemon dispatch the tool, receiving the result
//! back **inbound**, while an `owns_history` ACP agent runs its own tool loop
//! and yields `ToolCall` + `ToolResult` **outbound**. So
//! `assert_eq!(turn_events(acp), turn_events(internal))` is structurally
//! impossible rather than merely noisy, and must never be written.
//!
//! Cross-agent equality belongs one layer down, at `SessionEventMessage` —
//! where both paths converge on
//! `tool_result(session_id, id, name, {"result"|"error": …})` — and at the
//! rendered frame, which has exactly one renderer for both sources.

use std::collections::HashMap;

use crucible_core::turn::{StopReason, TurnError, TurnEvent};
use crucible_core::types::acp::FileDiff;
use futures::{Stream, StreamExt};

/// A rendering-relevant projection of an outbound `TurnEvent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventShape {
    Text(String),
    Thinking(String),
    ToolCall {
        /// First-seen ordinal of the raw tool-call id (`call#0`, `call#1`, …).
        call: String,
        name: String,
        /// Paths of the attached diffs, in order. The renderer draws the path
        /// as well as the hunks, so two turns editing different files are not
        /// the same frame.
        diff_paths: Vec<String>,
    },
    ToolResult {
        call: String,
        name: String,
        is_error: bool,
    },
    ToolCallDiffUpdate {
        call: String,
        diff_paths: Vec<String>,
    },
    ToolCallArgsUpdate {
        call: String,
    },
    ToolBatchEnd,
    Usage,
    /// Both numbers are retained, unlike `Usage`: they are not incidental
    /// token counts but the two operands of the statusline's context
    /// indicator (`context_label`, `components/status_items.rs`), which draws
    /// `used/limit` as a percentage. Erasing them would let a turn that
    /// reported the wrong window project identically to a correct one.
    ContextWindow {
        used: u64,
        limit: u64,
    },
    Done(StopReason),
    /// The `TurnError` discriminant only. The message can carry absolute
    /// paths, but its first line *is* rendered
    /// (`crucible-cli/src/tui/oil/components/tool_render.rs:115-165`), so
    /// collapsing every error to a single shape is lossy.
    Error(TurnErrorKind),
}

/// Discriminant of a [`TurnError`], with the message stripped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnErrorKind {
    Connection,
    Communication,
    AgentUnavailable,
    Internal,
    InvalidInput,
}

impl From<&TurnError> for TurnErrorKind {
    fn from(err: &TurnError) -> Self {
        match err {
            TurnError::Connection(_) => Self::Connection,
            TurnError::Communication(_) => Self::Communication,
            TurnError::AgentUnavailable(_) => Self::AgentUnavailable,
            TurnError::Internal(_) => Self::Internal,
            TurnError::InvalidInput(_) => Self::InvalidInput,
        }
    }
}

/// Projects events one at a time, numbering tool-call ids as it goes.
///
/// Correlation is user-visible: `ContainerList::update_tool_by_call_id`
/// (`crucible-cli/src/tui/oil/containers.rs:369-386`) matches a late diff
/// against a live tool card by `call_id` and, on a miss, logs a warning and
/// drops the diff. Erasing ids would make a mis-correlated stream project
/// identically to a correct one; raw ids would make the projection
/// non-deterministic. First-seen ordinals give both.
#[derive(Debug, Default)]
pub struct ShapeProjector {
    ordinals: HashMap<String, String>,
}

impl ShapeProjector {
    pub fn new() -> Self {
        Self::default()
    }

    /// `call#N` for `id`, assigning the next ordinal the first time it is seen.
    fn ordinal(&mut self, id: &str) -> String {
        let next = self.ordinals.len();
        self.ordinals
            .entry(id.to_string())
            .or_insert_with(|| format!("call#{next}"))
            .clone()
    }

    /// Project one event, or `None` if it is inbound-only.
    pub fn project(&mut self, ev: &TurnEvent) -> Option<EventShape> {
        Some(match ev {
            TurnEvent::TextDelta(t) => EventShape::Text(t.clone()),
            TurnEvent::Thinking(t) => EventShape::Thinking(t.clone()),
            TurnEvent::ToolCall {
                id, name, diffs, ..
            } => EventShape::ToolCall {
                call: self.ordinal(id),
                name: name.clone(),
                diff_paths: diff_paths(diffs),
            },
            TurnEvent::ToolResult {
                id, name, error, ..
            } => EventShape::ToolResult {
                call: self.ordinal(id),
                name: name.clone(),
                is_error: error.is_some(),
            },
            TurnEvent::ToolCallDiffUpdate { id, diffs } => EventShape::ToolCallDiffUpdate {
                call: self.ordinal(id),
                diff_paths: diff_paths(diffs),
            },
            TurnEvent::ToolCallArgsUpdate { id, .. } => EventShape::ToolCallArgsUpdate {
                call: self.ordinal(id),
            },
            TurnEvent::ToolBatchEnd => EventShape::ToolBatchEnd,
            TurnEvent::Usage(_) => EventShape::Usage,
            TurnEvent::ContextWindow { used, limit } => EventShape::ContextWindow {
                used: *used,
                limit: *limit,
            },
            TurnEvent::Done { stop_reason } => EventShape::Done(stop_reason.clone()),
            TurnEvent::Error(e) => EventShape::Error(e.into()),

            // Inbound only (runtime → agent), so never part of an agent's
            // outbound contract. Spelled out rather than caught by `_` so that
            // a future *outbound* variant fails to compile here instead of
            // being silently dropped from every parity assertion — the same
            // intent as `_terminal_variant_check` in
            // `crucible-daemon/src/rpc_client/agent/native_agent.rs:112-128`.
            TurnEvent::HandlerInjection { .. }
            | TurnEvent::ContextAttach { .. }
            | TurnEvent::DepthCapHit { .. } => return None,
        })
    }
}

/// The wire-level kind of a [`StreamingChunk`], for tests that assert the
/// *order* chunks arrived in rather than their payloads.
///
/// Four call sites were carrying byte-identical copies of this match, so every
/// new chunk variant broke all four the same way.
// `acp_support` is `#[path]`-included by several test binaries; only
// `acp_integration` calls this one.
#[allow(dead_code)]
pub fn chunk_kind(chunk: &crucible_daemon::acp::StreamingChunk) -> &'static str {
    use crucible_daemon::acp::StreamingChunk;
    match chunk {
        StreamingChunk::Text(_) => "text",
        StreamingChunk::Thinking(_) => "thinking",
        StreamingChunk::ToolStart { .. } => "tool_start",
        StreamingChunk::ToolEnd { .. } => "tool_end",
        StreamingChunk::ToolDiffUpdate { .. } => "tool_diff_update",
        StreamingChunk::ToolArgsUpdate { .. } => "tool_args_update",
        StreamingChunk::ContextWindow { .. } => "context_window",
    }
}

fn diff_paths(diffs: &[FileDiff]) -> Vec<String> {
    diffs.iter().map(|d| d.path.clone()).collect()
}

/// Drain a turn stream into its rendering-relevant shape sequence.
pub async fn shapes<S>(stream: S) -> Vec<EventShape>
where
    S: Stream<Item = TurnEvent>,
{
    futures::pin_mut!(stream);
    let mut projector = ShapeProjector::new();
    let mut out = Vec::new();
    while let Some(ev) = stream.next().await {
        if let Some(s) = projector.project(&ev) {
            out.push(s);
        }
    }
    out
}

/// Coalesce adjacent text (and adjacent thinking) deltas, dropping empty ones.
///
/// Chunk boundaries are a transport artifact; the rendered paragraph is not.
/// An empty delta is likewise invisible in the frame, but would otherwise
/// survive as a bare `Text("")` whenever it is not adjacent to another delta.
pub fn coalesce(shapes: Vec<EventShape>) -> Vec<EventShape> {
    let mut out: Vec<EventShape> = Vec::new();
    for s in shapes {
        if matches!(&s, EventShape::Text(t) | EventShape::Thinking(t) if t.is_empty()) {
            continue;
        }
        match (out.last_mut(), &s) {
            (Some(EventShape::Text(prev)), EventShape::Text(next)) => prev.push_str(next),
            (Some(EventShape::Thinking(prev)), EventShape::Thinking(next)) => prev.push_str(next),
            _ => out.push(s),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_call(id: &str, name: &str, diffs: Vec<FileDiff>) -> TurnEvent {
        TurnEvent::ToolCall {
            id: id.into(),
            name: name.into(),
            args: serde_json::json!({"path": "README.md"}),
            diffs,
        }
    }

    fn diff(path: &str) -> FileDiff {
        FileDiff {
            path: path.into(),
            old_content: Some("old line\n".into()),
            new_content: "new line\n".into(),
        }
    }

    #[test]
    fn coalesce_merges_adjacent_text_and_thinking() {
        let merged = coalesce(vec![
            EventShape::Thinking("let me ".into()),
            EventShape::Thinking("check".into()),
            EventShape::Text("Hello".into()),
            EventShape::Text(", world".into()),
            EventShape::Text("!".into()),
        ]);

        assert_eq!(
            merged,
            vec![
                EventShape::Thinking("let me check".into()),
                EventShape::Text("Hello, world!".into()),
            ]
        );
    }

    #[test]
    fn coalesce_keeps_thinking_that_arrives_after_text_separate() {
        // C1: an ACP agent legitimately interleaves thoughts *after* text.
        // Those thoughts must survive as their own block, in order — not be
        // absorbed into the surrounding text and not be reordered ahead of it.
        let merged = coalesce(vec![
            EventShape::Text("Let me ".into()),
            EventShape::Text("check. ".into()),
            EventShape::Thinking("reading ".into()),
            EventShape::Thinking("the config".into()),
            EventShape::Text("Found it.".into()),
        ]);

        assert_eq!(
            merged,
            vec![
                EventShape::Text("Let me check. ".into()),
                EventShape::Thinking("reading the config".into()),
                EventShape::Text("Found it.".into()),
            ]
        );
    }

    #[test]
    fn coalesce_drops_empty_text_and_thinking() {
        // A lone `TextDelta("")` renders nothing; keeping it as `Text("")`
        // would make two identical-looking turns compare unequal.
        let merged = coalesce(vec![
            EventShape::Text(String::new()),
            EventShape::ToolBatchEnd,
            EventShape::Thinking(String::new()),
            EventShape::Text("done".into()),
        ]);

        assert_eq!(
            merged,
            vec![EventShape::ToolBatchEnd, EventShape::Text("done".into())]
        );
    }

    #[test]
    fn coalesce_leaves_non_text_variants_alone() {
        let original = vec![
            EventShape::Text("a".into()),
            EventShape::ToolCall {
                call: "call#0".into(),
                name: "read_file".into(),
                diff_paths: Vec::new(),
            },
            EventShape::ToolCall {
                call: "call#1".into(),
                name: "read_file".into(),
                diff_paths: Vec::new(),
            },
            EventShape::Text("b".into()),
            EventShape::ToolBatchEnd,
            EventShape::ToolBatchEnd,
            EventShape::Done(StopReason::EndTurn),
        ];

        // Text separated by a tool call is *not* merged, and repeated
        // non-text shapes survive as distinct entries.
        assert_eq!(coalesce(original.clone()), original);
    }

    #[tokio::test]
    async fn shapes_projects_a_stream_and_drops_inbound_only_variants() {
        let events = vec![
            TurnEvent::Thinking("hmm".into()),
            TurnEvent::TextDelta("Reading".into()),
            TurnEvent::TextDelta(" now".into()),
            // Inbound-only: must not appear in the projection.
            TurnEvent::DepthCapHit { max_depth: 4 },
            tool_call("toolu_01ABC", "read_file", Vec::new()),
            TurnEvent::ToolResult {
                id: "toolu_01ABC".into(),
                name: "read_file".into(),
                result: serde_json::Value::String("# Crucible".into()),
                error: None,
            },
            TurnEvent::ToolBatchEnd,
            TurnEvent::Done {
                stop_reason: StopReason::EndTurn,
            },
        ];

        let projected = coalesce(shapes(futures::stream::iter(events)).await);

        assert_eq!(
            projected,
            vec![
                EventShape::Thinking("hmm".into()),
                EventShape::Text("Reading now".into()),
                EventShape::ToolCall {
                    call: "call#0".into(),
                    name: "read_file".into(),
                    diff_paths: Vec::new(),
                },
                EventShape::ToolResult {
                    call: "call#0".into(),
                    name: "read_file".into(),
                    is_error: false,
                },
                EventShape::ToolBatchEnd,
                EventShape::Done(StopReason::EndTurn),
            ]
        );
    }

    #[tokio::test]
    async fn tool_call_ids_become_first_seen_ordinals() {
        // Raw ids are agent-specific and vary run to run, so they cannot be
        // asserted; their *correlation* can.
        let events = vec![
            tool_call("toolu_zzz", "read_file", Vec::new()),
            tool_call("toolu_aaa", "edit_file", Vec::new()),
            TurnEvent::ToolResult {
                id: "toolu_aaa".into(),
                name: "edit_file".into(),
                result: serde_json::Value::String("ok".into()),
                error: None,
            },
        ];

        let calls: Vec<String> = shapes(futures::stream::iter(events))
            .await
            .into_iter()
            .map(|s| match s {
                EventShape::ToolCall { call, .. } | EventShape::ToolResult { call, .. } => call,
                other => panic!("unexpected shape {other:?}"),
            })
            .collect();

        assert_eq!(calls, vec!["call#0", "call#1", "call#1"]);
    }

    #[tokio::test]
    async fn a_diff_update_naming_the_wrong_call_id_is_distinguishable() {
        // C2: `update_tool_by_call_id` (containers.rs:369-386) drops a diff
        // whose call_id matches no live card, so a mis-correlated stream
        // renders differently from a correct one and must project differently.
        let correlated = shapes(futures::stream::iter(vec![
            tool_call("toolu_01", "edit_file", Vec::new()),
            TurnEvent::ToolCallDiffUpdate {
                id: "toolu_01".into(),
                diffs: vec![diff("src/main.rs")],
            },
        ]))
        .await;

        let mismatched = shapes(futures::stream::iter(vec![
            tool_call("toolu_01", "edit_file", Vec::new()),
            TurnEvent::ToolCallDiffUpdate {
                id: "toolu_99".into(),
                diffs: vec![diff("src/main.rs")],
            },
        ]))
        .await;

        assert_ne!(
            correlated, mismatched,
            "a diff update pointing at an unknown tool call projected identically \
             to a correlated one, so the harness cannot see a dropped diff"
        );
        assert_eq!(
            correlated.last(),
            Some(&EventShape::ToolCallDiffUpdate {
                call: "call#0".into(),
                diff_paths: vec!["src/main.rs".into()],
            })
        );
        assert_eq!(
            mismatched.last(),
            Some(&EventShape::ToolCallDiffUpdate {
                call: "call#1".into(),
                diff_paths: vec!["src/main.rs".into()],
            })
        );
    }

    #[tokio::test]
    async fn diffs_of_different_files_do_not_compare_equal() {
        let one = shapes(futures::stream::iter(vec![tool_call(
            "toolu_01",
            "edit_file",
            vec![diff("src/main.rs")],
        )]))
        .await;
        let other = shapes(futures::stream::iter(vec![tool_call(
            "toolu_01",
            "edit_file",
            vec![diff("src/lib.rs")],
        )]))
        .await;

        assert_ne!(
            one, other,
            "two turns editing different files compared equal"
        );
        assert_eq!(
            one,
            vec![EventShape::ToolCall {
                call: "call#0".into(),
                name: "edit_file".into(),
                diff_paths: vec!["src/main.rs".into()],
            }]
        );
    }

    #[test]
    fn error_shape_keeps_the_discriminant_but_not_the_message() {
        let mut p = ShapeProjector::new();
        let internal = p.project(&TurnEvent::Error(TurnError::Internal(
            "/home/user/secret exploded".into(),
        )));
        let connection = p.project(&TurnEvent::Error(TurnError::Connection("refused".into())));

        assert_eq!(internal, Some(EventShape::Error(TurnErrorKind::Internal)));
        assert_ne!(internal, connection);
        assert_eq!(
            internal,
            p.project(&TurnEvent::Error(TurnError::Internal(
                "something else entirely".into()
            ))),
            "the error message must not affect the shape"
        );
    }

    #[test]
    fn inbound_only_variants_project_to_none() {
        let mut p = ShapeProjector::new();
        assert!(p
            .project(&TurnEvent::DepthCapHit { max_depth: 3 })
            .is_none());
        assert!(p
            .project(&TurnEvent::ContextAttach {
                content: "note".into()
            })
            .is_none());
        assert!(p
            .project(&TurnEvent::HandlerInjection {
                content: "go on".into(),
                position: "after".into(),
            })
            .is_none());
    }
}
