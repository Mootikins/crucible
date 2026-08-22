//! Tool call representation
//!
//! Represents a tool call made by an agent.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// A tool call made by an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    /// Tool name.
    pub name: String,
    /// Tool arguments as JSON.
    pub args: JsonValue,
    /// Optional call ID for correlation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
}

impl ToolCall {
    /// Create a new tool call.
    pub fn new(name: impl Into<String>, args: JsonValue) -> Self {
        Self {
            name: name.into(),
            args,
            call_id: None,
        }
    }

    /// Set the call ID.
    pub fn with_call_id(mut self, id: impl Into<String>) -> Self {
        self.call_id = Some(id.into());
        self
    }
}

impl Default for ToolCall {
    fn default() -> Self {
        Self {
            name: String::new(),
            args: JsonValue::Null,
            call_id: None,
        }
    }
}

/// The provider wire carries the arguments as a JSON string. When the string
/// does not parse, the event keeps the raw text, so nothing is lost.
impl From<crate::traits::llm::ToolCall> for ToolCall {
    fn from(call: crate::traits::llm::ToolCall) -> Self {
        let args = serde_json::from_str(&call.function.arguments)
            .unwrap_or(JsonValue::String(call.function.arguments));
        Self {
            name: call.function.name,
            args,
            call_id: (!call.id.is_empty()).then_some(call.id),
        }
    }
}

/// A missing call id becomes an empty id, because the provider wire requires
/// the field.
impl From<ToolCall> for crate::traits::llm::ToolCall {
    fn from(call: ToolCall) -> Self {
        Self::new(
            call.call_id.unwrap_or_default(),
            call.name,
            call.args.to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::llm::ToolCall as LlmToolCall;

    #[test]
    fn llm_call_becomes_event_call_with_parsed_args() {
        let llm = LlmToolCall::new("call-1", "read_note", r#"{"path":"a.md"}"#.to_string());
        let event = ToolCall::from(llm);
        assert_eq!(event.name, "read_note");
        assert_eq!(event.args, serde_json::json!({"path": "a.md"}));
        assert_eq!(event.call_id.as_deref(), Some("call-1"));
    }

    #[test]
    fn unparseable_llm_args_stay_as_text() {
        let llm = LlmToolCall::new("call-1", "bash", "not json".to_string());
        let event = ToolCall::from(llm);
        assert_eq!(event.args, JsonValue::String("not json".to_string()));
    }

    #[test]
    fn empty_llm_id_becomes_no_call_id() {
        let llm = LlmToolCall::new("", "bash", "{}".to_string());
        assert_eq!(ToolCall::from(llm).call_id, None);
    }

    #[test]
    fn event_call_round_trips_through_llm_call() {
        let event =
            ToolCall::new("read_note", serde_json::json!({"path": "a.md"})).with_call_id("call-1");
        let llm = LlmToolCall::from(event.clone());
        assert_eq!(llm.id, "call-1");
        assert_eq!(llm.r#type, "function");
        assert_eq!(llm.function.name, "read_note");
        assert_eq!(ToolCall::from(llm), event);
    }

    #[test]
    fn event_call_without_id_gets_empty_llm_id() {
        let event = ToolCall::new("bash", JsonValue::Null);
        assert_eq!(LlmToolCall::from(event).id, "");
    }
}
