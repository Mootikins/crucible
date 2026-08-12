use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::protocol::session_events::{
    ReviewPayload, SettingsPayload, SetupPayload, TurnPayload, WorkflowPayload,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(u64),
    String(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<RequestId>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl Response {
    pub fn success(id: Option<RequestId>, result: impl Into<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result.into()),
            error: None,
        }
    }

    pub fn error(id: Option<RequestId>, code: i32, message: impl Into<String>) -> Self {
        Self::error_with_data(id, code, message, None)
    }

    pub fn error_with_data(
        id: Option<RequestId>,
        code: i32,
        message: impl Into<String>,
        data: Option<Value>,
    ) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
                data,
            }),
        }
    }
}

pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEventMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub session_id: String,
    pub event: String,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

impl SessionEventMessage {
    /// Build an event from an untyped `{event, data}` pair.
    ///
    /// Prefer [`typed`](Self::typed) or one of the named constructors: this one
    /// will happily emit a name no consumer handles, which is the bug class
    /// [`session_events`](crate::protocol::session_events) exists to close. It
    /// stays public because replay re-emits recorded names verbatim
    /// (`daemon/src/replay.rs`, `cli/src/tui/oil/local_replay.rs`) and a daemon
    /// must be able to relay an event a newer daemon minted.
    pub fn new(session_id: impl Into<String>, event: impl Into<String>, data: Value) -> Self {
        Self {
            msg_type: "event".to_string(),
            session_id: session_id.into(),
            event: event.into(),
            data,
            timestamp: None,
            seq: None,
        }
    }

    /// Build an event from a typed payload.
    ///
    /// The payload's serde representation *is* the `{event, data}` pair, so this
    /// cannot produce a name/shape mismatch — which is what [`new`](Self::new)
    /// could and did.
    pub fn typed(
        session_id: impl Into<String>,
        payload: impl Into<crate::protocol::session_events::SessionEventPayload>,
    ) -> Self {
        let (event, data) = payload.into().to_wire();
        Self {
            msg_type: "event".to_string(),
            session_id: session_id.into(),
            event,
            data,
            timestamp: None,
            seq: None,
        }
    }

    /// Decode this event's payload.
    ///
    /// `Err(UnknownEvent)` for a name this build does not know — a newer daemon,
    /// or a replayed recording. `self.event` and `self.data` are still right, so
    /// a consumer passes them through rather than dropping the event.
    pub fn payload(
        &self,
    ) -> Result<
        crate::protocol::session_events::SessionEventPayload,
        crate::protocol::session_events::EventDecodeError,
    > {
        crate::protocol::session_events::SessionEventPayload::from_wire(&self.event, &self.data)
    }

    pub fn with_timestamp(mut self) -> Self {
        self.timestamp = Some(Utc::now());
        self
    }

    pub fn text_delta(session_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::typed(
            session_id,
            TurnPayload::TextDelta {
                content: content.into(),
            },
        )
    }

    pub fn user_message(
        session_id: impl Into<String>,
        message_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::typed(
            session_id,
            TurnPayload::UserMessage {
                message_id: message_id.into(),
                content: content.into(),
            },
        )
    }

    pub fn thinking(session_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::typed(
            session_id,
            TurnPayload::Thinking {
                content: content.into(),
            },
        )
    }

    pub fn tool_call(
        session_id: impl Into<String>,
        call_id: impl Into<String>,
        tool: impl Into<String>,
        args: Value,
    ) -> Self {
        Self::tool_call_with_metadata(
            session_id,
            call_id,
            tool,
            args,
            None,
            None,
            None,
            Vec::new(),
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn tool_call_with_metadata(
        session_id: impl Into<String>,
        call_id: impl Into<String>,
        tool: impl Into<String>,
        args: Value,
        description: Option<String>,
        source: Option<String>,
        lua_primary_arg: Option<String>,
        diffs: Vec<crate::types::acp::FileDiff>,
        auto_approved: Option<String>,
    ) -> Self {
        let tool_name = tool.into();
        // One projection of "which argument matters", computed here rather than
        // in the payload so the TUI and the web render the same answer instead
        // of each keeping its own key-priority list. A Lua display hook's
        // `lua_primary_arg` overrides it — a plugin knows its own tool better
        // than a heuristic.
        let mut display = crate::types::ToolDisplay::of(&tool_name, &args);
        if let Some(pa) = lua_primary_arg.clone() {
            display.primary = Some(pa);
        }
        Self::typed(
            session_id,
            TurnPayload::ToolCall {
                call_id: call_id.into(),
                tool: tool_name,
                args,
                description,
                source,
                lua_primary_arg,
                display: Some(display),
                auto_approved,
                diffs,
            },
        )
    }

    /// Late file-diff content for a tool call that was already announced
    /// via a prior `tool_call` event. Produced when an ACP agent (e.g.
    /// Claude Code) defers diff content until a follow-up
    /// `ToolCallUpdate` frame. Subscribers should merge `diffs` into the
    /// existing tool entry keyed by `call_id`.
    pub fn tool_call_diff_update(
        session_id: impl Into<String>,
        call_id: impl Into<String>,
        diffs: Vec<crate::types::acp::FileDiff>,
    ) -> Self {
        Self::typed(
            session_id,
            TurnPayload::ToolCallDiffUpdate {
                call_id: call_id.into(),
                diffs,
            },
        )
    }

    /// Late arguments for a tool call that was already announced via a prior
    /// `tool_call` event. Produced when an ACP agent (e.g. Claude Code)
    /// announces the call without `rawInput` and only supplies it in a
    /// follow-up `ToolCallUpdate` frame. Subscribers should merge `args` into
    /// the existing tool entry keyed by `call_id`.
    pub fn tool_call_args_update(
        session_id: impl Into<String>,
        call_id: impl Into<String>,
        args: serde_json::Value,
    ) -> Self {
        Self::typed(
            session_id,
            TurnPayload::ToolCallArgsUpdate {
                call_id: call_id.into(),
                args,
            },
        )
    }

    /// The session's context window is now known.
    ///
    /// Usually a setup event (the daemon queries the provider API for the
    /// configured endpoint + model). A delegated agent has neither, so on an
    /// ACP session this instead rides the turn stream, carrying the window the
    /// agent reported in a `usage_update` frame. Same event, same payload,
    /// later — subscribers assign the limit and do not care which produced it.
    pub fn context_limit_resolved(
        session_id: impl Into<String>,
        limit: usize,
        source: crate::protocol::session_events::ContextLimitSource,
    ) -> Self {
        Self::typed(
            session_id,
            SetupPayload::ContextLimitResolved(
                crate::protocol::session_events::ContextLimitResolvedPayload { limit, source },
            ),
        )
    }

    pub fn tool_result(
        session_id: impl Into<String>,
        call_id: impl Into<String>,
        tool: impl Into<String>,
        result: Value,
    ) -> Self {
        Self::tool_result_with_terminate(session_id, call_id, tool, result, false)
    }

    /// Same as [`tool_result`] but with the terminate flag carried in the
    /// payload. Used by tool handlers that signal early-stop via the
    /// conjunctive batch-terminate path; UI surfaces it as a badge.
    pub fn tool_result_with_terminate(
        session_id: impl Into<String>,
        call_id: impl Into<String>,
        tool: impl Into<String>,
        result: Value,
        terminate: bool,
    ) -> Self {
        Self::typed(
            session_id,
            TurnPayload::ToolResult {
                call_id: call_id.into(),
                tool: tool.into(),
                result,
                terminate,
            },
        )
    }

    pub fn ended(session_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::typed(
            session_id,
            TurnPayload::Ended {
                reason: reason.into(),
            },
        )
    }

    pub fn model_switched(
        session_id: impl Into<String>,
        model_id: impl Into<String>,
        provider: impl Into<String>,
    ) -> Self {
        Self::typed(
            session_id,
            SettingsPayload::ModelSwitched {
                model_id: model_id.into(),
                provider: provider.into(),
            },
        )
    }

    /// Session mode changed (normal/plan/auto). `data.mode` is the field the
    /// web SSE mapper and TUI reducers read — keep the name stable.
    pub fn mode_changed(session_id: impl Into<String>, mode: impl Into<String>) -> Self {
        Self::typed(
            session_id,
            SettingsPayload::ModeChanged { mode: mode.into() },
        )
    }

    /// The session's composed diff moved: a hunk was accepted, rejected,
    /// reverted, commented on, or a comment was resolved.
    ///
    /// Deliberately carries only `data.reason` and no hunk identity. A hunk id
    /// is derived from its content *and* its range in `session_base`, so a
    /// change that re-aligns an ambiguous region moves the ids of hunks the
    /// user never touched — a client that patched a single row in place from
    /// this event would be showing a decision attached to different lines. The
    /// event says "re-list"; the listing is the truth.
    ///
    /// `reason` is advisory (`"accepted"`, `"rejected"`, `"commented"`,
    /// `"comment_resolved"`, `"external"`). Clients must not switch on it for
    /// correctness — an unrecognised reason still means re-list.
    pub fn review_changed(session_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::typed(
            session_id,
            ReviewPayload::ReviewChanged {
                reason: reason.into(),
            },
        )
    }

    pub fn message_complete(
        session_id: impl Into<String>,
        message_id: impl Into<String>,
        full_response: impl Into<String>,
        usage: Option<&crate::traits::llm::TokenUsage>,
    ) -> Self {
        Self::typed(
            session_id,
            TurnPayload::MessageComplete {
                message_id: message_id.into(),
                full_response: full_response.into(),
                prompt_tokens: usage.map(|u| u.prompt_tokens),
                completion_tokens: usage.map(|u| u.completion_tokens),
                total_tokens: usage.map(|u| u.total_tokens),
                cache_read_tokens: usage.and_then(|u| u.cache_read_tokens),
                cache_creation_tokens: usage.and_then(|u| u.cache_creation_tokens),
            },
        )
    }

    /// A text segment that streamed before a tool call, emitted at the
    /// text→tool boundary. `message_id` is the turn id (shared with
    /// `user_message` and `message_complete`); `index` is the 0-based
    /// segment position within the turn; `content` is the segment's text
    /// (the delta accumulated since the previous boundary). Lets viewers
    /// converge on canonical per-segment bubbles across live streaming and
    /// history reload. `message_complete` still carries the WHOLE turn's
    /// accumulated text — segments are additive, not a replacement.
    pub fn segment_complete(
        session_id: impl Into<String>,
        message_id: impl Into<String>,
        index: usize,
        content: impl Into<String>,
    ) -> Self {
        Self::typed(
            session_id,
            TurnPayload::SegmentComplete {
                message_id: message_id.into(),
                index,
                content: content.into(),
            },
        )
    }

    pub fn interaction_requested(
        session_id: impl Into<String>,
        request_id: impl Into<String>,
        request: &crate::interaction::InteractionRequest,
    ) -> Self {
        Self::typed(
            session_id,
            TurnPayload::InteractionRequested {
                request_id: request_id.into(),
                request: request.clone(),
            },
        )
    }

    // ---- workflow events (Phase 3a) ----

    pub fn workflow_step_started(
        session_id: impl Into<String>,
        step_id: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        Self::typed(
            session_id,
            WorkflowPayload::WorkflowStepStarted {
                step_id: step_id.into(),
                title: title.into(),
            },
        )
    }

    pub fn workflow_step_completed(
        session_id: impl Into<String>,
        step_id: impl Into<String>,
        output_name: Option<String>,
    ) -> Self {
        Self::typed(
            session_id,
            WorkflowPayload::WorkflowStepCompleted {
                step_id: step_id.into(),
                output_name,
            },
        )
    }

    pub fn workflow_gate_reached(
        session_id: impl Into<String>,
        gate_id: impl Into<String>,
        title: Option<String>,
        owner: impl Into<String>,
    ) -> Self {
        Self::typed(
            session_id,
            WorkflowPayload::WorkflowGateReached {
                gate_id: gate_id.into(),
                title,
                owner: owner.into(),
            },
        )
    }

    pub fn workflow_gate_approved(
        session_id: impl Into<String>,
        gate_id: impl Into<String>,
    ) -> Self {
        Self::typed(
            session_id,
            WorkflowPayload::WorkflowGateApproved {
                gate_id: gate_id.into(),
            },
        )
    }

    pub fn workflow_completed(session_id: impl Into<String>) -> Self {
        Self::typed(session_id, WorkflowPayload::WorkflowCompleted {})
    }

    pub fn workflow_assessed(
        session_id: impl Into<String>,
        runnable_passed: &[crate::workflow::AssessmentOutcome],
        runnable_failed: &[crate::workflow::AssessmentOutcome],
        manual_entries: &[String],
    ) -> Self {
        Self::typed(
            session_id,
            WorkflowPayload::WorkflowAssessed {
                runnable_passed: runnable_passed.to_vec(),
                runnable_failed: runnable_failed.to_vec(),
                manual_entries: manual_entries.to_vec(),
            },
        )
    }

    pub fn workflow_failed(
        session_id: impl Into<String>,
        reason: impl Into<String>,
        at_step: Option<String>,
    ) -> Self {
        Self::typed(
            session_id,
            WorkflowPayload::WorkflowFailed {
                reason: reason.into(),
                at_step,
            },
        )
    }

    pub fn workflow_cancelled(session_id: impl Into<String>) -> Self {
        Self::typed(session_id, WorkflowPayload::WorkflowCancelled {})
    }

    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        let mut json = serde_json::to_string(self)?;
        json.push('\n');
        Ok(json)
    }
}

#[cfg(test)]
mod tests;
