use crate::events::LogEvent;
use crucible_core::parser::BlockHash;
use crucible_core::storage::note_store::NoteRecord;

const MAX_USER_MESSAGES: usize = 20;
const SESSION_TAG: &str = "session";
const CHAT_TAG: &str = "chat";

pub struct SessionContent {
    pub session_id: String,
    pub title: String,
    pub user_messages: Vec<String>,
    pub last_assistant: Option<String>,
    pub model: Option<String>,
}

impl SessionContent {
    pub fn content_for_embedding(&self) -> String {
        self.user_messages.join("\n\n")
    }

    pub fn content_for_display(&self) -> String {
        let mut parts: Vec<&str> = self.user_messages.iter().map(|s| s.as_str()).collect();
        if let Some(ref assistant) = self.last_assistant {
            parts.push(assistant.as_str());
        }
        parts.join("\n\n")
    }

    pub fn to_note_record(&self, embedding: Option<Vec<f32>>) -> NoteRecord {
        let path = format!("sessions/{}", self.session_id);
        let content_hash =
            BlockHash::new(*blake3::hash(self.content_for_display().as_bytes()).as_bytes());

        NoteRecord {
            path,
            content_hash,
            embedding,
            title: self.title.clone(),
            tags: vec![SESSION_TAG.to_string(), CHAT_TAG.to_string()],
            links_to: Vec::new(),
            properties: Default::default(),
            updated_at: chrono::Utc::now(),
        }
    }
}

pub fn extract_session_content(session_id: &str, events: &[LogEvent]) -> Option<SessionContent> {
    let mut user_messages = Vec::new();
    let mut last_assistant: Option<String> = None;
    let mut last_model: Option<String> = None;

    for event in events {
        match event {
            LogEvent::User { content, .. } => {
                if user_messages.len() < MAX_USER_MESSAGES {
                    let trimmed = content.trim();
                    if !trimmed.is_empty() {
                        user_messages.push(trimmed.to_string());
                    }
                }
            }
            LogEvent::Assistant { content, model, .. } => {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    last_assistant = Some(trimmed.to_string());
                }
                if let Some(m) = model {
                    last_model = Some(m.clone());
                }
            }
            _ => {}
        }
    }

    if user_messages.is_empty() {
        return None;
    }

    let title = derive_title(&user_messages);

    Some(SessionContent {
        session_id: session_id.to_string(),
        title,
        user_messages,
        last_assistant,
        model: last_model,
    })
}

fn derive_title(user_messages: &[String]) -> String {
    let first = &user_messages[0];
    if first.len() <= 80 {
        first.clone()
    } else {
        let truncated: String = first.chars().take(77).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn user_event(content: &str) -> LogEvent {
        LogEvent::User {
            ts: Utc::now(),
            content: content.to_string(),
        }
    }

    fn assistant_event(content: &str) -> LogEvent {
        LogEvent::Assistant {
            ts: Utc::now(),
            content: content.to_string(),
            model: Some("test-model".to_string()),
            tokens: None,
        }
    }

    fn system_event(content: &str) -> LogEvent {
        LogEvent::System {
            ts: Utc::now(),
            content: content.to_string(),
        }
    }

    fn tool_call_event() -> LogEvent {
        LogEvent::ToolCall {
            ts: Utc::now(),
            id: "call-1".to_string(),
            name: "read_file".to_string(),
            args: json!({"path": "test.rs"}),
        }
    }

    #[test]
    fn extracts_user_and_last_assistant() {
        let events = vec![
            user_event("Hello"),
            assistant_event("Hi there"),
            user_event("How are you?"),
            assistant_event("I'm doing well!"),
        ];

        let content = extract_session_content("test-session", &events).unwrap();
        assert_eq!(content.user_messages, vec!["Hello", "How are you?"]);
        assert_eq!(content.last_assistant.as_deref(), Some("I'm doing well!"));
        assert_eq!(content.model.as_deref(), Some("test-model"));
    }

    #[test]
    fn skips_system_and_tool_events() {
        let events = vec![
            system_event("You are helpful"),
            user_event("Hello"),
            tool_call_event(),
            assistant_event("Response"),
        ];

        let content = extract_session_content("test-session", &events).unwrap();
        assert_eq!(content.user_messages.len(), 1);
        assert_eq!(content.user_messages[0], "Hello");
    }

    #[test]
    fn returns_none_for_empty_user_messages() {
        let events = vec![
            system_event("System prompt"),
            assistant_event("Unprompted response"),
        ];

        assert!(extract_session_content("test-session", &events).is_none());
    }

    #[test]
    fn returns_none_for_no_events() {
        assert!(extract_session_content("test-session", &[]).is_none());
    }

    #[test]
    fn skips_whitespace_only_user_messages() {
        let events = vec![
            user_event("  "),
            user_event(""),
            user_event("Actual message"),
        ];

        let content = extract_session_content("test-session", &events).unwrap();
        assert_eq!(content.user_messages, vec!["Actual message"]);
    }

    #[test]
    fn caps_user_messages_at_limit() {
        let events: Vec<LogEvent> = (0..25)
            .map(|i| user_event(&format!("Message {}", i)))
            .collect();

        let content = extract_session_content("test-session", &events).unwrap();
        assert_eq!(content.user_messages.len(), MAX_USER_MESSAGES);
    }

    #[test]
    fn title_from_first_user_message() {
        let events = vec![
            user_event("What is the meaning of life?"),
            assistant_event("42"),
        ];

        let content = extract_session_content("test-session", &events).unwrap();
        assert_eq!(content.title, "What is the meaning of life?");
    }

    #[test]
    fn title_truncates_long_messages() {
        let long_msg = "x".repeat(100);
        let events = vec![user_event(&long_msg)];

        let content = extract_session_content("test-session", &events).unwrap();
        assert!(content.title.len() <= 80);
        assert!(content.title.ends_with("..."));
    }

    #[test]
    fn content_for_embedding_joins_user_messages() {
        let events = vec![
            user_event("First question"),
            assistant_event("First answer"),
            user_event("Second question"),
        ];

        let content = extract_session_content("test-session", &events).unwrap();
        assert_eq!(
            content.content_for_embedding(),
            "First question\n\nSecond question"
        );
    }

    #[test]
    fn content_for_display_includes_assistant() {
        let events = vec![
            user_event("Hello"),
            assistant_event("Hi"),
            user_event("Bye"),
            assistant_event("Goodbye"),
        ];

        let content = extract_session_content("test-session", &events).unwrap();
        let display = content.content_for_display();
        assert!(display.contains("Hello"));
        assert!(display.contains("Bye"));
        assert!(display.contains("Goodbye"));
    }

    #[test]
    fn to_note_record_creates_valid_record() {
        let events = vec![user_event("Test question"), assistant_event("Test answer")];

        let content = extract_session_content("sess-123", &events).unwrap();
        let record = content.to_note_record(Some(vec![0.1, 0.2, 0.3]));

        assert_eq!(record.path, "sessions/sess-123");
        assert_eq!(record.title, "Test question");
        assert_eq!(record.tags, vec!["session", "chat"]);
        assert!(record.embedding.is_some());
        assert_eq!(record.embedding.unwrap().len(), 3);
    }

    #[test]
    fn to_note_record_without_embedding() {
        let events = vec![user_event("Test")];
        let content = extract_session_content("sess-456", &events).unwrap();
        let record = content.to_note_record(None);

        assert!(record.embedding.is_none());
        assert_eq!(record.path, "sessions/sess-456");
    }
}
