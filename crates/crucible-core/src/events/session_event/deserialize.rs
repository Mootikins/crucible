//! Manual `Deserialize` impl for `SessionEvent`.
//!
//! The wire-facing `SessionEvent` enum dispatches unknown tag values to
//! `InternalSessionEvent` so that internal daemon events remain deserializable
//! from the same JSON stream. A helper enum mirrors the known variants to avoid
//! infinite recursion through the `Deserialize` impl.

use serde::Deserialize;
use serde_json::Value as JsonValue;

use super::{InternalSessionEvent, SessionEvent, SessionEventConfig, ToolCall};

impl<'de> Deserialize<'de> for SessionEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        // Deserialize to a raw Value first
        let value = serde_json::Value::deserialize(deserializer)?;

        // Extract the type field
        let type_str = value
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| D::Error::missing_field("type"))?;

        // Known SessionEvent variants (non-Internal)
        const KNOWN_VARIANTS: &[&str] = &[
            "message_received",
            "agent_responded",
            "agent_thinking",
            "tool_called",
            "tool_completed",
            "session_started",
            "session_ended",
            "text_delta",
            "interaction_requested",
            "interaction_completed",
            "delegation_spawned",
            "delegation_completed",
            "delegation_failed",
            "custom",
        ];

        if KNOWN_VARIANTS.contains(&type_str) {
            // For known SessionEvent variants, use serde_json to deserialize
            serde_json::from_value::<SessionEventHelper>(value)
                .map_err(|e| D::Error::custom(format!("failed to deserialize SessionEvent: {}", e)))
                .map(|helper| helper.into())
        } else {
            // Try to deserialize as InternalSessionEvent
            let type_str_owned = type_str.to_string();
            serde_json::from_value::<InternalSessionEvent>(value)
                .map_err(|e| {
                    D::Error::custom(format!("unknown event type '{}': {}", type_str_owned, e))
                })
                .map(|inner| SessionEvent::Internal(Box::new(inner)))
        }
    }
}

/// Helper enum for deserializing known SessionEvent variants.
/// This avoids infinite recursion when deserializing the main SessionEvent enum.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub(super) enum SessionEventHelper {
    MessageReceived {
        content: String,
        participant_id: String,
    },
    AgentResponded {
        content: String,
        tool_calls: Vec<ToolCall>,
    },
    AgentThinking {
        thought: String,
    },
    ToolCalled {
        name: String,
        args: JsonValue,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        source: Option<String>,
    },
    ToolCompleted {
        name: String,
        result: String,
        #[serde(default)]
        error: Option<String>,
    },
    SessionStarted {
        config: SessionEventConfig,
    },
    SessionEnded {
        reason: String,
    },
    TextDelta {
        delta: String,
        seq: u64,
    },
    InteractionRequested {
        request_id: String,
        request: crate::interaction::InteractionRequest,
    },
    InteractionCompleted {
        request_id: String,
        response: crate::interaction::InteractionResponse,
    },
    DelegationSpawned {
        delegation_id: String,
        prompt: String,
        parent_session_id: String,
        #[serde(default)]
        target_agent: Option<String>,
    },
    DelegationCompleted {
        delegation_id: String,
        result_summary: String,
        parent_session_id: String,
    },
    DelegationFailed {
        delegation_id: String,
        error: String,
        parent_session_id: String,
    },
    Custom {
        name: String,
        payload: JsonValue,
    },
}

impl From<SessionEventHelper> for SessionEvent {
    fn from(helper: SessionEventHelper) -> Self {
        match helper {
            SessionEventHelper::MessageReceived {
                content,
                participant_id,
            } => SessionEvent::MessageReceived {
                content,
                participant_id,
            },
            SessionEventHelper::AgentResponded {
                content,
                tool_calls,
            } => SessionEvent::AgentResponded {
                content,
                tool_calls,
            },
            SessionEventHelper::AgentThinking { thought } => {
                SessionEvent::AgentThinking { thought }
            }
            SessionEventHelper::ToolCalled {
                name,
                args,
                description,
                source,
            } => SessionEvent::ToolCalled {
                name,
                args,
                description,
                source,
            },
            SessionEventHelper::ToolCompleted {
                name,
                result,
                error,
            } => SessionEvent::ToolCompleted {
                name,
                result,
                error,
            },
            SessionEventHelper::SessionStarted { config } => {
                SessionEvent::SessionStarted { config }
            }
            SessionEventHelper::SessionEnded { reason } => SessionEvent::SessionEnded { reason },
            SessionEventHelper::TextDelta { delta, seq } => SessionEvent::TextDelta { delta, seq },
            SessionEventHelper::InteractionRequested {
                request_id,
                request,
            } => SessionEvent::InteractionRequested {
                request_id,
                request,
            },
            SessionEventHelper::InteractionCompleted {
                request_id,
                response,
            } => SessionEvent::InteractionCompleted {
                request_id,
                response,
            },
            SessionEventHelper::DelegationSpawned {
                delegation_id,
                prompt,
                parent_session_id,
                target_agent,
            } => SessionEvent::DelegationSpawned {
                delegation_id,
                prompt,
                parent_session_id,
                target_agent,
            },
            SessionEventHelper::DelegationCompleted {
                delegation_id,
                result_summary,
                parent_session_id,
            } => SessionEvent::DelegationCompleted {
                delegation_id,
                result_summary,
                parent_session_id,
            },
            SessionEventHelper::DelegationFailed {
                delegation_id,
                error,
                parent_session_id,
            } => SessionEvent::DelegationFailed {
                delegation_id,
                error,
                parent_session_id,
            },
            SessionEventHelper::Custom { name, payload } => SessionEvent::Custom { name, payload },
        }
    }
}
