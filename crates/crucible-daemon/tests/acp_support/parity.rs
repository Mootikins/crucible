//! Shared parity harness: compare an ACP agent's TurnEvent stream against the
//! internal agent's for equivalent behavior.

use crucible_core::turn::{StopReason, TurnEvent};
use futures::{Stream, StreamExt};

/// A rendering-relevant projection of a `TurnEvent`.
///
/// Tool-call ids and token counts differ run to run and between agents; they
/// are not what the user sees. Everything retained here changes the frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventShape {
    Text(String),
    Thinking(String),
    /// `args_is_object` distinguishes structured args from a bare string —
    /// the difference that drives B2.
    ToolCall {
        name: String,
        args_is_object: bool,
        diff_count: usize,
    },
    ToolResult {
        name: String,
        result_is_string: bool,
        is_error: bool,
    },
    ToolCallDiffUpdate {
        diff_count: usize,
    },
    ToolBatchEnd,
    Usage,
    Done(StopReason),
    Error,
}

pub fn shape(ev: &TurnEvent) -> Option<EventShape> {
    Some(match ev {
        TurnEvent::TextDelta(t) => EventShape::Text(t.clone()),
        TurnEvent::Thinking(t) => EventShape::Thinking(t.clone()),
        TurnEvent::ToolCall {
            name, args, diffs, ..
        } => EventShape::ToolCall {
            name: name.clone(),
            args_is_object: args.is_object(),
            diff_count: diffs.len(),
        },
        TurnEvent::ToolResult {
            name,
            result,
            error,
            ..
        } => EventShape::ToolResult {
            name: name.clone(),
            result_is_string: result.is_string(),
            is_error: error.is_some(),
        },
        TurnEvent::ToolCallDiffUpdate { diffs, .. } => EventShape::ToolCallDiffUpdate {
            diff_count: diffs.len(),
        },
        TurnEvent::ToolBatchEnd => EventShape::ToolBatchEnd,
        TurnEvent::Usage(_) => EventShape::Usage,
        TurnEvent::Done { stop_reason } => EventShape::Done(stop_reason.clone()),
        TurnEvent::Error(_) => EventShape::Error,
        // Inbound-only variants never appear in an outbound stream.
        _ => return None,
    })
}

/// Drain a turn stream into its rendering-relevant shape sequence.
pub async fn shapes<S>(stream: S) -> Vec<EventShape>
where
    S: Stream<Item = TurnEvent>,
{
    futures::pin_mut!(stream);
    let mut out = Vec::new();
    while let Some(ev) = stream.next().await {
        if let Some(s) = shape(&ev) {
            out.push(s);
        }
    }
    out
}

/// Coalesce adjacent text deltas. Chunk boundaries are a transport artifact;
/// the rendered paragraph is not.
pub fn coalesce(shapes: Vec<EventShape>) -> Vec<EventShape> {
    let mut out: Vec<EventShape> = Vec::new();
    for s in shapes {
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
    use crucible_core::turn::TurnError;

    fn tool_call(name: &str, args: serde_json::Value) -> TurnEvent {
        TurnEvent::ToolCall {
            id: "call-1".into(),
            name: name.into(),
            args,
            diffs: Vec::new(),
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
    fn coalesce_leaves_non_text_variants_alone() {
        let original = vec![
            EventShape::Text("a".into()),
            EventShape::ToolCall {
                name: "read_file".into(),
                args_is_object: true,
                diff_count: 0,
            },
            EventShape::ToolCall {
                name: "read_file".into(),
                args_is_object: true,
                diff_count: 0,
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
            tool_call("read_file", serde_json::json!({"path": "README.md"})),
            TurnEvent::ToolResult {
                id: "call-1".into(),
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
                    name: "read_file".into(),
                    args_is_object: true,
                    diff_count: 0,
                },
                EventShape::ToolResult {
                    name: "read_file".into(),
                    result_is_string: true,
                    is_error: false,
                },
                EventShape::ToolBatchEnd,
                EventShape::Done(StopReason::EndTurn),
            ]
        );
    }

    #[test]
    fn shape_distinguishes_structured_from_stringified_payloads() {
        // B2: a stringified tool result and a structured one must not
        // compare equal, which is what makes the divergence assertable.
        let structured = shape(&TurnEvent::ToolResult {
            id: "call-1".into(),
            name: "grep".into(),
            result: serde_json::json!({"matches": 3}),
            error: None,
        });
        let stringified = shape(&TurnEvent::ToolResult {
            id: "call-1".into(),
            name: "grep".into(),
            result: serde_json::Value::String(r#"{"matches": 3}"#.into()),
            error: None,
        });

        assert_ne!(structured, stringified);
        assert_eq!(
            structured,
            Some(EventShape::ToolResult {
                name: "grep".into(),
                result_is_string: false,
                is_error: false,
            })
        );
    }

    #[test]
    fn shape_returns_none_for_inbound_only_variants() {
        assert!(shape(&TurnEvent::DepthCapHit { max_depth: 3 }).is_none());
        assert!(shape(&TurnEvent::ContextAttach {
            content: "note".into()
        })
        .is_none());
        assert!(shape(&TurnEvent::HandlerInjection {
            content: "go on".into(),
            position: "after".into(),
        })
        .is_none());
        assert_eq!(
            shape(&TurnEvent::Error(TurnError::Internal("boom".into()))),
            Some(EventShape::Error)
        );
    }
}
