use crucible_rpc::SessionEvent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    Token {
        content: String,
    },

    ToolCall {
        id: String,
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        arguments: Option<serde_json::Value>,
    },

    ToolResult {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<String>,
    },

    Thinking {
        content: String,
    },

    MessageComplete {
        id: String,
        content: String,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        tool_calls: Vec<ToolCallSummary>,
    },

    Error {
        code: String,
        message: String,
    },

    InteractionRequested {
        id: String,
        #[serde(flatten)]
        request: serde_json::Value,
    },

    SessionEvent {
        event_type: String,
        data: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallSummary {
    pub id: String,
    pub title: String,
}

impl ChatEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            ChatEvent::Token { .. } => "token",
            ChatEvent::ToolCall { .. } => "tool_call",
            ChatEvent::ToolResult { .. } => "tool_result",
            ChatEvent::Thinking { .. } => "thinking",
            ChatEvent::MessageComplete { .. } => "message_complete",
            ChatEvent::Error { .. } => "error",
            ChatEvent::InteractionRequested { .. } => "interaction_requested",
            ChatEvent::SessionEvent { .. } => "session_event",
        }
    }

    pub fn from_daemon_event(event: &SessionEvent) -> Self {
        let data = &event.data;

        match event.event_type.as_str() {
            "text_delta" => ChatEvent::Token {
                content: data["content"].as_str().unwrap_or("").to_string(),
            },

            "thinking_delta" => ChatEvent::Thinking {
                content: data["content"].as_str().unwrap_or("").to_string(),
            },

            "tool_call_start" | "tool_call" => ChatEvent::ToolCall {
                id: data["id"].as_str().unwrap_or("").to_string(),
                title: data["name"]
                    .as_str()
                    .or_else(|| data["title"].as_str())
                    .unwrap_or("")
                    .to_string(),
                arguments: data.get("arguments").cloned(),
            },

            "tool_result" => ChatEvent::ToolResult {
                id: data["id"].as_str().unwrap_or("").to_string(),
                result: data["result"].as_str().map(String::from),
            },

            "turn_complete" | "message_complete" => ChatEvent::MessageComplete {
                id: data["message_id"]
                    .as_str()
                    .or_else(|| data["id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                content: data["content"].as_str().unwrap_or("").to_string(),
                tool_calls: Vec::new(),
            },

            "error" => ChatEvent::Error {
                code: data["code"].as_str().unwrap_or("unknown").to_string(),
                message: data["message"]
                    .as_str()
                    .unwrap_or("Unknown error")
                    .to_string(),
            },

            "interaction_requested" => ChatEvent::InteractionRequested {
                id: data["request_id"]
                    .as_str()
                    .or_else(|| data["id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                request: data.clone(),
            },

            _ => ChatEvent::SessionEvent {
                event_type: event.event_type.clone(),
                data: data.clone(),
            },
        }
    }
}
