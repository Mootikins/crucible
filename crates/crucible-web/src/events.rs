use crucible_daemon::SessionEvent;
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

    ToolResultDelta {
        id: String,
        delta: String,
    },

    ToolResultComplete {
        id: String,
    },

    ToolResultError {
        id: String,
        error: String,
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

    SubagentSpawned {
        id: String,
        prompt: String,
    },

    SubagentCompleted {
        id: String,
        summary: String,
    },

    SubagentFailed {
        id: String,
        error: String,
    },

    DelegationSpawned {
        id: String,
        prompt: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        target_agent: Option<String>,
    },

    DelegationCompleted {
        id: String,
        summary: String,
    },

    DelegationFailed {
        id: String,
        error: String,
    },

    ContextUsage {
        used: u64,
        total: u64,
    },

    PrecognitionResult {
        notes_count: usize,
        #[serde(default)]
        notes: Vec<PrecognitionNote>,
    },

    ModeChanged {
        mode: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecognitionNote {
    pub name: String,
    #[serde(default)]
    pub relevance: f64,
}

impl ChatEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            ChatEvent::Token { .. } => "token",
            ChatEvent::ToolCall { .. } => "tool_call",
            ChatEvent::ToolResult { .. } => "tool_result",
            ChatEvent::ToolResultDelta { .. } => "tool_result_delta",
            ChatEvent::ToolResultComplete { .. } => "tool_result_complete",
            ChatEvent::ToolResultError { .. } => "tool_result_error",
            ChatEvent::Thinking { .. } => "thinking",
            ChatEvent::MessageComplete { .. } => "message_complete",
            ChatEvent::Error { .. } => "error",
            ChatEvent::InteractionRequested { .. } => "interaction_requested",
            ChatEvent::SubagentSpawned { .. } => "subagent_spawned",
            ChatEvent::SubagentCompleted { .. } => "subagent_completed",
            ChatEvent::SubagentFailed { .. } => "subagent_failed",
            ChatEvent::DelegationSpawned { .. } => "delegation_spawned",
            ChatEvent::DelegationCompleted { .. } => "delegation_completed",
            ChatEvent::DelegationFailed { .. } => "delegation_failed",
            ChatEvent::ContextUsage { .. } => "context_usage",
            ChatEvent::PrecognitionResult { .. } => "precognition_result",
            ChatEvent::ModeChanged { .. } => "mode_changed",
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

            "tool_result_delta" => ChatEvent::ToolResultDelta {
                id: data["id"]
                    .as_str()
                    .or_else(|| data["call_id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                delta: data["delta"]
                    .as_str()
                    .or_else(|| data["content"].as_str())
                    .unwrap_or("")
                    .to_string(),
            },

            "tool_result_complete" => ChatEvent::ToolResultComplete {
                id: data["id"]
                    .as_str()
                    .or_else(|| data["call_id"].as_str())
                    .unwrap_or("")
                    .to_string(),
            },

            "tool_result_error" => ChatEvent::ToolResultError {
                id: data["id"]
                    .as_str()
                    .or_else(|| data["call_id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                error: data["error"]
                    .as_str()
                    .unwrap_or("Unknown error")
                    .to_string(),
            },

            "turn_complete" | "message_complete" => ChatEvent::MessageComplete {
                id: data["message_id"]
                    .as_str()
                    .or_else(|| data["id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                content: data["full_response"]
                    .as_str()
                    .or_else(|| data["content"].as_str())
                    .unwrap_or("")
                    .to_string(),
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

            "subagent_spawned" => ChatEvent::SubagentSpawned {
                id: data["id"].as_str().unwrap_or("").to_string(),
                prompt: data["prompt"]
                    .as_str()
                    .or_else(|| data["description"].as_str())
                    .unwrap_or("")
                    .to_string(),
            },

            "subagent_completed" => ChatEvent::SubagentCompleted {
                id: data["id"].as_str().unwrap_or("").to_string(),
                summary: data["summary"]
                    .as_str()
                    .or_else(|| data["result"].as_str())
                    .unwrap_or("")
                    .to_string(),
            },

            "subagent_failed" => ChatEvent::SubagentFailed {
                id: data["id"].as_str().unwrap_or("").to_string(),
                error: data["error"]
                    .as_str()
                    .unwrap_or("Unknown error")
                    .to_string(),
            },

            "delegation_spawned" => ChatEvent::DelegationSpawned {
                id: data["delegation_id"]
                    .as_str()
                    .or_else(|| data["id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                prompt: data["prompt"].as_str().unwrap_or("").to_string(),
                target_agent: data["target_agent"].as_str().map(String::from),
            },

            "delegation_completed" => ChatEvent::DelegationCompleted {
                id: data["delegation_id"]
                    .as_str()
                    .or_else(|| data["id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                summary: data["result_summary"]
                    .as_str()
                    .or_else(|| data["summary"].as_str())
                    .unwrap_or("")
                    .to_string(),
            },

            "delegation_failed" => ChatEvent::DelegationFailed {
                id: data["delegation_id"]
                    .as_str()
                    .or_else(|| data["id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                error: data["error"]
                    .as_str()
                    .unwrap_or("Unknown error")
                    .to_string(),
            },

            // Daemon emits "precognition_complete"; we normalize to "precognition_result" for frontend
            "precognition_complete" => {
                let notes = data
                    .get("notes")
                    .and_then(|n| {
                        n.as_array().map(|arr| {
                            arr.iter()
                                .filter_map(|note| {
                                    let name = note
                                        .get("title")
                                        .or_else(|| note.get("name"))
                                        .and_then(|v| v.as_str())?;
                                    let relevance = note
                                        .get("relevance")
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.0);
                                    Some(PrecognitionNote {
                                        name: name.to_string(),
                                        relevance,
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                    })
                    .unwrap_or_default();
                let notes_count = data["notes_count"]
                    .as_u64()
                    .map(|n| n as usize)
                    .unwrap_or(notes.len());

                ChatEvent::PrecognitionResult { notes_count, notes }
            }

            "context_usage" => ChatEvent::ContextUsage {
                used: data["used"].as_u64().unwrap_or(0),
                total: data["total"].as_u64().unwrap_or(0),
            },

            "mode_changed" => ChatEvent::ModeChanged {
                mode: data["mode"].as_str().unwrap_or("normal").to_string(),
            },

            _ => ChatEvent::SessionEvent {
                event_type: event.event_type.clone(),
                data: data.clone(),
            },
        }
    }
}
