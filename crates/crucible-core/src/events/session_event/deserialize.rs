//! Manual `Deserialize` impl for `SessionEvent`.
//!
//! The `SessionEvent` enum dispatches unknown tag values to
//! `InternalSessionEvent` so that internal daemon events remain deserializable
//! from the same JSON stream. A helper enum mirrors the known variants to avoid
//! infinite recursion through the `Deserialize` impl.

use serde::Deserialize;
use serde_json::Value as JsonValue;

use super::{InternalSessionEvent, SessionEvent};

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
        const KNOWN_VARIANTS: &[&str] = &["message_received", "interaction_requested", "custom"];

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
    InteractionRequested {
        request_id: String,
        request: crate::interaction::InteractionRequest,
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
            SessionEventHelper::InteractionRequested {
                request_id,
                request,
            } => SessionEvent::InteractionRequested {
                request_id,
                request,
            },
            SessionEventHelper::Custom { name, payload } => SessionEvent::Custom { name, payload },
        }
    }
}
