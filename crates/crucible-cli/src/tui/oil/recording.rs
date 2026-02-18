//! JSONL recording format for TUI demo capture and replay.
//!
//! This module provides types and utilities for recording and replaying TUI interactions
//! as JSONL (JSON Lines) files. Each line is a complete JSON object representing either
//! the recording header or a timestamped event.
//!
//! # Format
//!
//! ```text
//! {"type":"header","version":1,"terminal_width":120,"terminal_height":40,"timestamp":"2026-02-18T10:30:00Z","command":"cru chat"}
//! {"ts_ms":0,"event":{"type":"user_message","content":"Hello"}}
//! {"ts_ms":150,"event":{"type":"text_delta","delta":"Hi "}}
//! {"ts_ms":200,"event":{"type":"text_delta","delta":"there"}}
//! ```

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

use super::ChatAppMsg;

/// A timestamped event in a recording.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampedEvent {
    /// Milliseconds since recording start.
    pub ts_ms: u64,
    /// The event that occurred.
    pub event: DemoEvent,
}

/// Recording header metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingHeader {
    /// Always "header" to identify this as a header line.
    #[serde(rename = "type")]
    pub record_type: String,
    /// Recording format version (currently 1).
    pub version: u32,
    /// Terminal width in columns.
    pub terminal_width: u16,
    /// Terminal height in rows.
    pub terminal_height: u16,
    /// ISO 8601 timestamp of recording start.
    pub timestamp: String,
    /// Command that was executed (e.g., "cru chat").
    pub command: String,
}

/// A serializable event from the TUI that can be recorded and replayed.
///
/// This is a slim subset of `ChatAppMsg` containing only replay-relevant events.
/// Events like `FetchModels`, `McpStatusLoaded`, etc. are not recorded because they
/// are not deterministic or are handled separately during replay.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DemoEvent {
    /// Text delta from LLM response.
    TextDelta { delta: String },
    /// Thinking delta from LLM (extended thinking).
    ThinkingDelta { delta: String },
    /// Tool call initiated by LLM.
    ToolCall {
        name: String,
        args: String,
        call_id: Option<String>,
    },
    /// Partial result from tool execution.
    ToolResultDelta {
        name: String,
        delta: String,
        call_id: Option<String>,
    },
    /// Tool execution completed successfully.
    ToolResultComplete {
        name: String,
        call_id: Option<String>,
    },
    /// Tool execution failed with error.
    ToolResultError {
        name: String,
        error: String,
        call_id: Option<String>,
    },
    /// LLM response stream completed.
    StreamComplete,
    /// User sent a message.
    UserMessage { content: String },
    /// Enriched message (with context injection).
    EnrichedMessage { original: String, enriched: String },
    /// Subagent was spawned.
    SubagentSpawned { id: String, prompt: String },
    /// Subagent completed successfully.
    SubagentCompleted { id: String, summary: String },
    /// Subagent failed with error.
    SubagentFailed { id: String, error: String },
    /// Delegation to external agent was spawned.
    DelegationSpawned {
        id: String,
        prompt: String,
        target_agent: Option<String>,
    },
    /// Delegation completed successfully.
    DelegationCompleted { id: String, summary: String },
    /// Delegation failed with error.
    DelegationFailed { id: String, error: String },
    /// Keyboard key was pressed.
    KeyPress { key: String, modifiers: String },
    /// Error occurred.
    Error { message: String },
    /// Status message.
    Status { message: String },
}

/// Writes a recording to a JSONL file.
pub struct RecordingWriter {
    writer: BufWriter<File>,
    start: std::time::Instant,
}

impl RecordingWriter {
    /// Create a new recording file at the given path.
    pub fn create(path: &Path) -> std::io::Result<Self> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        Ok(RecordingWriter {
            writer,
            start: std::time::Instant::now(),
        })
    }

    /// Write the recording header.
    pub fn write_header(&mut self, width: u16, height: u16, command: &str) -> std::io::Result<()> {
        let header = RecordingHeader {
            record_type: "header".to_string(),
            version: 1,
            terminal_width: width,
            terminal_height: height,
            timestamp: chrono::Utc::now().to_rfc3339(),
            command: command.to_string(),
        };

        let line = serde_json::to_string(&header)?;
        self.writer.write_all(line.as_bytes())?;
        self.writer.write_all(b"\n")?;
        Ok(())
    }

    /// Write a timestamped event.
    pub fn write_event(&mut self, event: &TimestampedEvent) -> std::io::Result<()> {
        let line = serde_json::to_string(event)?;
        self.writer.write_all(line.as_bytes())?;
        self.writer.write_all(b"\n")?;
        Ok(())
    }

    /// Get elapsed milliseconds since recording started.
    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

impl Drop for RecordingWriter {
    fn drop(&mut self) {
        let _ = self.writer.flush();
    }
}

/// Reads a recording from a JSONL file.
pub struct RecordingReader {
    reader: BufReader<File>,
    header_read: bool,
}

impl RecordingReader {
    /// Open a recording file for reading.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        Ok(RecordingReader {
            reader,
            header_read: false,
        })
    }

    /// Read the recording header (must be called first).
    pub fn read_header(&mut self) -> std::io::Result<RecordingHeader> {
        let mut line = String::new();
        use std::io::BufRead;
        self.reader.read_line(&mut line)?;

        let header: RecordingHeader = serde_json::from_str(&line)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        self.header_read = true;
        Ok(header)
    }

    /// Iterate over timestamped events in the recording.
    pub fn events(&mut self) -> RecordingEventIterator<'_> {
        RecordingEventIterator {
            reader: &mut self.reader,
        }
    }
}

/// Iterator over events in a recording.
pub struct RecordingEventIterator<'a> {
    reader: &'a mut BufReader<File>,
}

impl<'a> Iterator for RecordingEventIterator<'a> {
    type Item = std::io::Result<TimestampedEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        use std::io::BufRead;
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => None, // EOF
            Ok(_) => {
                if line.trim().is_empty() {
                    self.next() // Skip empty lines
                } else {
                    match serde_json::from_str::<TimestampedEvent>(&line) {
                        Ok(event) => Some(Ok(event)),
                        Err(e) => {
                            Some(Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
                        }
                    }
                }
            }
            Err(e) => Some(Err(e)),
        }
    }
}

/// Convert a `ChatAppMsg` to a `DemoEvent` if it's replay-relevant.
///
/// Returns `None` for events that are not deterministic or are handled separately
/// during replay (e.g., `FetchModels`, `McpStatusLoaded`, interaction modals).
pub fn from_chat_app_msg(msg: &ChatAppMsg, ts_ms: u64) -> Option<TimestampedEvent> {
    let event = match msg {
        ChatAppMsg::TextDelta(delta) => DemoEvent::TextDelta {
            delta: delta.clone(),
        },
        ChatAppMsg::ThinkingDelta(delta) => DemoEvent::ThinkingDelta {
            delta: delta.clone(),
        },
        ChatAppMsg::ToolCall {
            name,
            args,
            call_id,
        } => DemoEvent::ToolCall {
            name: name.clone(),
            args: args.clone(),
            call_id: call_id.clone(),
        },
        ChatAppMsg::ToolResultDelta {
            name,
            delta,
            call_id,
        } => DemoEvent::ToolResultDelta {
            name: name.clone(),
            delta: delta.clone(),
            call_id: call_id.clone(),
        },
        ChatAppMsg::ToolResultComplete { name, call_id } => DemoEvent::ToolResultComplete {
            name: name.clone(),
            call_id: call_id.clone(),
        },
        ChatAppMsg::ToolResultError {
            name,
            error,
            call_id,
        } => DemoEvent::ToolResultError {
            name: name.clone(),
            error: error.clone(),
            call_id: call_id.clone(),
        },
        ChatAppMsg::StreamComplete => DemoEvent::StreamComplete,
        ChatAppMsg::UserMessage(content) => DemoEvent::UserMessage {
            content: content.clone(),
        },
        ChatAppMsg::EnrichedMessage { original, enriched } => DemoEvent::EnrichedMessage {
            original: original.clone(),
            enriched: enriched.clone(),
        },
        ChatAppMsg::SubagentSpawned { id, prompt } => DemoEvent::SubagentSpawned {
            id: id.clone(),
            prompt: prompt.clone(),
        },
        ChatAppMsg::SubagentCompleted { id, summary } => DemoEvent::SubagentCompleted {
            id: id.clone(),
            summary: summary.clone(),
        },
        ChatAppMsg::SubagentFailed { id, error } => DemoEvent::SubagentFailed {
            id: id.clone(),
            error: error.clone(),
        },
        ChatAppMsg::DelegationSpawned {
            id,
            prompt,
            target_agent,
        } => DemoEvent::DelegationSpawned {
            id: id.clone(),
            prompt: prompt.clone(),
            target_agent: target_agent.clone(),
        },
        ChatAppMsg::DelegationCompleted { id, summary } => DemoEvent::DelegationCompleted {
            id: id.clone(),
            summary: summary.clone(),
        },
        ChatAppMsg::DelegationFailed { id, error } => DemoEvent::DelegationFailed {
            id: id.clone(),
            error: error.clone(),
        },
        ChatAppMsg::Error(message) => DemoEvent::Error {
            message: message.clone(),
        },
        ChatAppMsg::Status(message) => DemoEvent::Status {
            message: message.clone(),
        },
        // Not replay-relevant: filtered out
        ChatAppMsg::FetchModels => return None,
        ChatAppMsg::McpStatusLoaded(_) => return None,
        ChatAppMsg::PluginStatusLoaded(_) => return None,
        ChatAppMsg::ModelsLoaded(_) => return None,
        ChatAppMsg::ModelsFetchFailed(_) => return None,
        ChatAppMsg::SetThinkingBudget(_) => return None,
        ChatAppMsg::SetTemperature(_) => return None,
        ChatAppMsg::SetMaxTokens(_) => return None,
        ChatAppMsg::ClearHistory => return None,
        ChatAppMsg::QueueMessage(_) => return None,
        ChatAppMsg::ToggleMessages => return None,
        ChatAppMsg::LoadHistory(_) => return None,
        ChatAppMsg::PrecognitionResult { .. } => return None,
        ChatAppMsg::ModeChanged(_) => return None,
        ChatAppMsg::SwitchModel(_) => return None,
        ChatAppMsg::StreamCancelled => return None,
        ChatAppMsg::CloseInteraction { .. } => return None,
        ChatAppMsg::OpenInteraction { .. } => return None,
        ChatAppMsg::ContextUsage { .. } => return None,
        ChatAppMsg::ReloadPlugin(_) => return None,
        ChatAppMsg::ExecuteSlashCommand(_) => return None,
        ChatAppMsg::ExportSession(_) => return None,
    };

    Some(TimestampedEvent { ts_ms, event })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_demo_event_serde_text_delta() {
        let event = DemoEvent::TextDelta {
            delta: "Hello".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"text_delta\""));
        assert!(json.contains("\"delta\":\"Hello\""));

        let deserialized: DemoEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_demo_event_serde_tool_call() {
        let event = DemoEvent::ToolCall {
            name: "search".to_string(),
            args: r#"{"query":"test"}"#.to_string(),
            call_id: Some("call_123".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"tool_call\""));
        assert!(json.contains("\"name\":\"search\""));

        let deserialized: DemoEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_demo_event_serde_all_variants() {
        let variants = vec![
            DemoEvent::TextDelta {
                delta: "text".to_string(),
            },
            DemoEvent::ThinkingDelta {
                delta: "thinking".to_string(),
            },
            DemoEvent::ToolCall {
                name: "tool".to_string(),
                args: "{}".to_string(),
                call_id: None,
            },
            DemoEvent::ToolResultDelta {
                name: "tool".to_string(),
                delta: "result".to_string(),
                call_id: Some("id".to_string()),
            },
            DemoEvent::ToolResultComplete {
                name: "tool".to_string(),
                call_id: None,
            },
            DemoEvent::ToolResultError {
                name: "tool".to_string(),
                error: "failed".to_string(),
                call_id: None,
            },
            DemoEvent::StreamComplete,
            DemoEvent::UserMessage {
                content: "hello".to_string(),
            },
            DemoEvent::EnrichedMessage {
                original: "hello".to_string(),
                enriched: "hello with context".to_string(),
            },
            DemoEvent::SubagentSpawned {
                id: "sub1".to_string(),
                prompt: "do something".to_string(),
            },
            DemoEvent::SubagentCompleted {
                id: "sub1".to_string(),
                summary: "done".to_string(),
            },
            DemoEvent::SubagentFailed {
                id: "sub1".to_string(),
                error: "error".to_string(),
            },
            DemoEvent::DelegationSpawned {
                id: "del1".to_string(),
                prompt: "delegate".to_string(),
                target_agent: Some("claude".to_string()),
            },
            DemoEvent::DelegationCompleted {
                id: "del1".to_string(),
                summary: "delegated".to_string(),
            },
            DemoEvent::DelegationFailed {
                id: "del1".to_string(),
                error: "delegation failed".to_string(),
            },
            DemoEvent::KeyPress {
                key: "Enter".to_string(),
                modifiers: "".to_string(),
            },
            DemoEvent::Error {
                message: "error".to_string(),
            },
            DemoEvent::Status {
                message: "status".to_string(),
            },
        ];

        for event in variants {
            let json = serde_json::to_string(&event).unwrap();
            let deserialized: DemoEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event, deserialized, "Failed for: {:?}", event);
        }
    }

    #[test]
    fn test_timestamped_event_serde() {
        let event = TimestampedEvent {
            ts_ms: 1234,
            event: DemoEvent::TextDelta {
                delta: "test".to_string(),
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"ts_ms\":1234"));

        let deserialized: TimestampedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event.ts_ms, deserialized.ts_ms);
    }

    #[test]
    fn test_recording_header_serde() {
        let header = RecordingHeader {
            record_type: "header".to_string(),
            version: 1,
            terminal_width: 120,
            terminal_height: 40,
            timestamp: "2026-02-18T10:30:00Z".to_string(),
            command: "cru chat".to_string(),
        };

        let json = serde_json::to_string(&header).unwrap();
        assert!(json.contains("\"type\":\"header\""));
        assert!(json.contains("\"version\":1"));

        let deserialized: RecordingHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(header.version, deserialized.version);
        assert_eq!(header.terminal_width, deserialized.terminal_width);
    }

    #[test]
    fn test_writer_reader_roundtrip() -> std::io::Result<()> {
        let temp_file = NamedTempFile::new()?;
        let path = temp_file.path().to_path_buf();

        // Write
        {
            let mut writer = RecordingWriter::create(&path)?;
            writer.write_header(120, 40, "cru chat")?;
            writer.write_event(&TimestampedEvent {
                ts_ms: 0,
                event: DemoEvent::UserMessage {
                    content: "hello".to_string(),
                },
            })?;
            writer.write_event(&TimestampedEvent {
                ts_ms: 100,
                event: DemoEvent::TextDelta {
                    delta: "hi".to_string(),
                },
            })?;
        }

        // Read
        let mut reader = RecordingReader::open(&path)?;
        let header = reader.read_header()?;
        assert_eq!(header.version, 1);
        assert_eq!(header.terminal_width, 120);
        assert_eq!(header.command, "cru chat");

        let events: Vec<_> = reader.events().collect::<Result<Vec<_>, _>>()?;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].ts_ms, 0);
        assert_eq!(events[1].ts_ms, 100);

        Ok(())
    }

    #[test]
    fn test_from_chat_app_msg_text_delta() {
        let msg = ChatAppMsg::TextDelta("hello".to_string());
        let event = from_chat_app_msg(&msg, 100).unwrap();
        assert_eq!(event.ts_ms, 100);
        match event.event {
            DemoEvent::TextDelta { delta } => assert_eq!(delta, "hello"),
            _ => panic!("Expected TextDelta"),
        }
    }

    #[test]
    fn test_from_chat_app_msg_filters_non_replay() {
        let non_replay_msgs = vec![
            ChatAppMsg::FetchModels,
            ChatAppMsg::ModelsLoaded(vec![]),
            ChatAppMsg::SetThinkingBudget(1000),
            ChatAppMsg::ClearHistory,
        ];

        for msg in non_replay_msgs {
            assert!(
                from_chat_app_msg(&msg, 0).is_none(),
                "Expected None for: {:?}",
                msg
            );
        }
    }

    #[test]
    fn test_from_chat_app_msg_user_message() {
        let msg = ChatAppMsg::UserMessage("test message".to_string());
        let event = from_chat_app_msg(&msg, 50).unwrap();
        match event.event {
            DemoEvent::UserMessage { content } => assert_eq!(content, "test message"),
            _ => panic!("Expected UserMessage"),
        }
    }

    #[test]
    fn test_from_chat_app_msg_enriched_message() {
        let msg = ChatAppMsg::EnrichedMessage {
            original: "hello".to_string(),
            enriched: "hello with context".to_string(),
        };
        let event = from_chat_app_msg(&msg, 75).unwrap();
        match event.event {
            DemoEvent::EnrichedMessage { original, enriched } => {
                assert_eq!(original, "hello");
                assert_eq!(enriched, "hello with context");
            }
            _ => panic!("Expected EnrichedMessage"),
        }
    }

    #[test]
    fn test_from_chat_app_msg_tool_call() {
        let msg = ChatAppMsg::ToolCall {
            name: "search".to_string(),
            args: r#"{"q":"test"}"#.to_string(),
            call_id: Some("call_1".to_string()),
        };
        let event = from_chat_app_msg(&msg, 200).unwrap();
        match event.event {
            DemoEvent::ToolCall {
                name,
                args: _,
                call_id,
            } => {
                assert_eq!(name, "search");
                assert_eq!(call_id, Some("call_1".to_string()));
            }
            _ => panic!("Expected ToolCall"),
        }
    }
}
