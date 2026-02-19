//! Round-trip integration tests for demo recording/replay infrastructure.
//!
//! Tests verify that:
//! 1. RecordingWriter can write events to JSONL
//! 2. RecordingReader can read them back correctly
//! 3. ReplayAgentHandle can replay events as ChatChunks
//! 4. Tool calls and results are properly reconstructed

use crucible_cli::tui::oil::recording::{DemoEvent, RecordingWriter, TimestampedEvent};
use crucible_cli::tui::oil::replay_agent::ReplayAgentHandle;
use crucible_core::traits::chat::AgentHandle;
use futures::StreamExt;
use tempfile::NamedTempFile;

#[test]
fn test_recording_writer_reader_roundtrip() -> anyhow::Result<()> {
    // Create a temporary file
    let temp_file = NamedTempFile::new()?;
    let path = temp_file.path().to_path_buf();

    // Write a recording
    {
        let mut writer = RecordingWriter::create(&path)?;
        writer.write_header(80, 24, "test")?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 0,
            event: DemoEvent::UserMessage {
                content: "hello".into(),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 100,
            event: DemoEvent::TextDelta {
                delta: "Hi ".into(),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 200,
            event: DemoEvent::TextDelta {
                delta: "there!".into(),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 500,
            event: DemoEvent::StreamComplete,
        })?;
    } // Writer dropped, file flushed

    // Read it back
    let mut reader = crucible_cli::tui::oil::recording::RecordingReader::open(&path)?;
    let header = reader.read_header()?;

    assert_eq!(header.version, 1, "header version should be 1");
    assert_eq!(header.terminal_width, 80, "terminal width should be 80");
    assert_eq!(header.terminal_height, 24, "terminal height should be 24");
    assert_eq!(header.command, "test", "command should be 'test'");

    let events: Vec<_> = reader.events().collect::<Result<Vec<_>, _>>()?;

    assert_eq!(events.len(), 4, "should have 4 events");

    // Verify first event
    match &events[0].event {
        DemoEvent::UserMessage { content } => {
            assert_eq!(content, "hello", "first event should be UserMessage with 'hello'");
        }
        _ => panic!("first event should be UserMessage"),
    }

    // Verify second event
    match &events[1].event {
        DemoEvent::TextDelta { delta } => {
            assert_eq!(delta, "Hi ", "second event should be TextDelta with 'Hi '");
        }
        _ => panic!("second event should be TextDelta"),
    }

    // Verify third event
    match &events[2].event {
        DemoEvent::TextDelta { delta } => {
            assert_eq!(delta, "there!", "third event should be TextDelta with 'there!'");
        }
        _ => panic!("third event should be TextDelta"),
    }

    // Verify last event
    match &events[3].event {
        DemoEvent::StreamComplete => {
            // Expected
        }
        _ => panic!("last event should be StreamComplete"),
    }

    Ok(())
}

#[tokio::test]
async fn test_replay_agent_produces_correct_chunks() -> anyhow::Result<()> {
    // Create a temporary file
    let temp_file = NamedTempFile::new()?;
    let path = temp_file.path().to_path_buf();

    // Write a recording
    {
        let mut writer = RecordingWriter::create(&path)?;
        writer.write_header(80, 24, "test")?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 0,
            event: DemoEvent::UserMessage {
                content: "test".into(),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 0,
            event: DemoEvent::TextDelta {
                delta: "Hello".into(),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 0,
            event: DemoEvent::TextDelta {
                delta: " world".into(),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 0,
            event: DemoEvent::StreamComplete,
        })?;
    } // Writer dropped, file flushed

    // Create replay agent with high speed (no delays)
    let mut agent = ReplayAgentHandle::from_file(&path, 100.0)?;

    // Send message and collect chunks
    let chunks: Vec<_> = agent
        .send_message_stream("ignored".to_string())
        .collect::<Vec<_>>()
        .await;

    assert_eq!(chunks.len(), 3, "should have 3 chunks (2 TextDelta + 1 StreamComplete)");

    // Verify first chunk
    let chunk0 = chunks[0].as_ref().expect("first chunk should be Ok");
    assert_eq!(chunk0.delta, "Hello", "first chunk delta should be 'Hello'");
    assert!(!chunk0.done, "first chunk should not be done");

    // Verify second chunk
    let chunk1 = chunks[1].as_ref().expect("second chunk should be Ok");
    assert_eq!(chunk1.delta, " world", "second chunk delta should be ' world'");
    assert!(!chunk1.done, "second chunk should not be done");

    // Verify third chunk (StreamComplete)
    let chunk2 = chunks[2].as_ref().expect("third chunk should be Ok");
    assert_eq!(chunk2.delta, "", "third chunk delta should be empty");
    assert!(chunk2.done, "third chunk should be done");

    Ok(())
}

#[tokio::test]
async fn test_replay_agent_handles_tool_calls() -> anyhow::Result<()> {
    // Create a temporary file
    let temp_file = NamedTempFile::new()?;
    let path = temp_file.path().to_path_buf();

    // Write a recording with tool calls
    {
        let mut writer = RecordingWriter::create(&path)?;
        writer.write_header(80, 24, "test")?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 0,
            event: DemoEvent::ToolCall {
                name: "search".into(),
                args: "{}".into(),
                call_id: Some("call-1".into()),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 0,
            event: DemoEvent::ToolResultDelta {
                name: "search".into(),
                delta: "result".into(),
                call_id: Some("call-1".into()),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 0,
            event: DemoEvent::StreamComplete,
        })?;
    } // Writer dropped, file flushed

    // Create replay agent
    let mut agent = ReplayAgentHandle::from_file(&path, 100.0)?;

    // Send message and collect chunks
    let chunks: Vec<_> = agent
        .send_message_stream("ignored".to_string())
        .collect::<Vec<_>>()
        .await;

    assert_eq!(chunks.len(), 3, "should have 3 chunks (ToolCall + ToolResultDelta + StreamComplete)");

    // Verify first chunk (ToolCall)
    let chunk0 = chunks[0].as_ref().expect("first chunk should be Ok");
    assert!(
        chunk0.tool_calls.is_some(),
        "first chunk should have tool_calls"
    );
    let tool_calls = chunk0.tool_calls.as_ref().unwrap();
    assert_eq!(tool_calls.len(), 1, "should have 1 tool call");
    assert_eq!(tool_calls[0].name, "search", "tool call name should be 'search'");
    assert_eq!(
        tool_calls[0].id,
        Some("call-1".to_string()),
        "tool call id should be 'call-1'"
    );

    // Verify second chunk (ToolResultDelta)
    let chunk1 = chunks[1].as_ref().expect("second chunk should be Ok");
    assert!(
        chunk1.tool_results.is_some(),
        "second chunk should have tool_results"
    );
    let tool_results = chunk1.tool_results.as_ref().unwrap();
    assert_eq!(tool_results.len(), 1, "should have 1 tool result");
    assert_eq!(tool_results[0].name, "search", "tool result name should be 'search'");
    assert_eq!(
        tool_results[0].result, "result",
        "tool result should be 'result'"
    );
    assert_eq!(
        tool_results[0].call_id,
        Some("call-1".to_string()),
        "tool result call_id should be 'call-1'"
    );

    // Verify third chunk (StreamComplete)
    let chunk2 = chunks[2].as_ref().expect("third chunk should be Ok");
    assert!(chunk2.done, "third chunk should be done");

    Ok(())
}

#[tokio::test]
async fn test_replay_agent_empty_recording() -> anyhow::Result<()> {
    // Create a temporary file
    let temp_file = NamedTempFile::new()?;
    let path = temp_file.path().to_path_buf();

    // Write a minimal recording (header + StreamComplete only)
    {
        let mut writer = RecordingWriter::create(&path)?;
        writer.write_header(80, 24, "test")?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 0,
            event: DemoEvent::StreamComplete,
        })?;
    } // Writer dropped, file flushed

    // Create replay agent
    let mut agent = ReplayAgentHandle::from_file(&path, 100.0)?;

    // Send message and collect chunks
    let chunks: Vec<_> = agent
        .send_message_stream("ignored".to_string())
        .collect::<Vec<_>>()
        .await;

    assert_eq!(chunks.len(), 1, "should have exactly 1 chunk");

    // Verify the chunk
    let chunk = chunks[0].as_ref().expect("chunk should be Ok");
    assert!(chunk.done, "chunk should be done");
    assert_eq!(chunk.delta, "", "chunk delta should be empty");

    Ok(())
}

#[test]
fn test_replay_user_messages_single_turn() -> anyhow::Result<()> {
    // Create a temporary file
    let temp_file = NamedTempFile::new()?;
    let path = temp_file.path().to_path_buf();

    // Write a recording with a single user message
    {
        let mut writer = RecordingWriter::create(&path)?;
        writer.write_header(80, 24, "test")?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 0,
            event: DemoEvent::UserMessage {
                content: "hello".into(),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 100,
            event: DemoEvent::TextDelta {
                delta: "Hi!".into(),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 200,
            event: DemoEvent::StreamComplete,
        })?;
    } // Writer dropped, file flushed

    // Create replay agent and call user_messages()
    let agent = ReplayAgentHandle::from_file(&path, 100.0)?;
    let messages = agent.user_messages();

    assert_eq!(messages.len(), 1, "should have 1 user message");
    assert_eq!(messages[0], "hello", "user message should be 'hello'");

    Ok(())
}

#[test]
fn test_replay_user_messages_multi_turn() -> anyhow::Result<()> {
    // Create a temporary file
    let temp_file = NamedTempFile::new()?;
    let path = temp_file.path().to_path_buf();

    // Write a recording with multiple user messages
    {
        let mut writer = RecordingWriter::create(&path)?;
        writer.write_header(80, 24, "test")?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 0,
            event: DemoEvent::UserMessage {
                content: "first".into(),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 100,
            event: DemoEvent::TextDelta {
                delta: "Response 1".into(),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 200,
            event: DemoEvent::StreamComplete,
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 300,
            event: DemoEvent::UserMessage {
                content: "second".into(),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 400,
            event: DemoEvent::TextDelta {
                delta: "Response 2".into(),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 500,
            event: DemoEvent::StreamComplete,
        })?;
    } // Writer dropped, file flushed

    // Create replay agent and call user_messages()
    let agent = ReplayAgentHandle::from_file(&path, 100.0)?;
    let messages = agent.user_messages();

    assert_eq!(messages.len(), 2, "should have 2 user messages");
    assert_eq!(messages[0], "first", "first user message should be 'first'");
    assert_eq!(messages[1], "second", "second user message should be 'second'");

    Ok(())
}

#[test]
fn test_replay_user_messages_empty_fixture() -> anyhow::Result<()> {
    // Create a temporary file
    let temp_file = NamedTempFile::new()?;
    let path = temp_file.path().to_path_buf();

    // Write a minimal recording with only header (no events)
    {
        let mut writer = RecordingWriter::create(&path)?;
        writer.write_header(80, 24, "test")?;
    } // Writer dropped, file flushed

    // Create replay agent and call user_messages()
    let agent = ReplayAgentHandle::from_file(&path, 100.0)?;
    let messages = agent.user_messages();

    assert_eq!(messages.len(), 0, "should have 0 user messages");
    assert!(messages.is_empty(), "messages should be empty");

    Ok(())
}

#[test]
fn test_replay_user_messages_skips_non_user_events() -> anyhow::Result<()> {
    // Create a temporary file
    let temp_file = NamedTempFile::new()?;
    let path = temp_file.path().to_path_buf();

    // Write a recording with mixed events, only UserMessage should be extracted
    {
        let mut writer = RecordingWriter::create(&path)?;
        writer.write_header(80, 24, "test")?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 0,
            event: DemoEvent::TextDelta {
                delta: "This is not a user message".into(),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 100,
            event: DemoEvent::ToolCall {
                name: "search".into(),
                args: "{}".into(),
                call_id: Some("call-1".into()),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 200,
            event: DemoEvent::UserMessage {
                content: "only_this".into(),
            },
        })?;

        writer.write_event(&TimestampedEvent {
            ts_ms: 300,
            event: DemoEvent::StreamComplete,
        })?;
    } // Writer dropped, file flushed

    // Create replay agent and call user_messages()
    let agent = ReplayAgentHandle::from_file(&path, 100.0)?;
    let messages = agent.user_messages();

    assert_eq!(messages.len(), 1, "should have 1 user message");
    assert_eq!(
        messages[0], "only_this",
        "should only extract UserMessage, not TextDelta or ToolCall"
    );

    Ok(())
}
