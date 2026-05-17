use serde_json::json;
use std::path::PathBuf;

use super::{test_path, TEST_TIMESTAMP_MS};
use crate::events::{InternalSessionEvent, SessionEvent, SessionEventConfig, ToolCall};

#[test]
fn roundtrip_message_received() {
    let original = SessionEvent::MessageReceived {
        content: "Help me implement the task harness".into(),
        participant_id: "user".into(),
    };

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, timestamp) = SessionEvent::from_markdown_block(&md).unwrap();

    assert_eq!(timestamp, TEST_TIMESTAMP_MS);
    match parsed {
        SessionEvent::MessageReceived {
            content,
            participant_id,
        } => {
            assert_eq!(content, "Help me implement the task harness");
            assert_eq!(participant_id, "user");
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_message_received_multiline() {
    let original = SessionEvent::MessageReceived {
        content: "Line 1\nLine 2\nLine 3".into(),
        participant_id: "assistant".into(),
    };

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::MessageReceived {
            content,
            participant_id,
        } => {
            assert_eq!(content, "Line 1\nLine 2\nLine 3");
            assert_eq!(participant_id, "assistant");
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_agent_thinking() {
    let original = SessionEvent::AgentThinking {
        thought: "Analyzing the codebase...".into(),
    };

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::AgentThinking { thought } => {
            assert_eq!(thought, "Analyzing the codebase...");
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_tool_called() {
    let path = test_path("test.txt");
    let path_str = path.to_string_lossy();
    let original = SessionEvent::ToolCalled {
        name: "read_file".into(),
        args: json!({"path": path_str}),
        description: None,
        source: None,
    };

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::ToolCalled { name, args, .. } => {
            assert_eq!(name, "read_file");
            assert_eq!(args["path"], path_str.as_ref());
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_tool_completed_inline() {
    let original = SessionEvent::ToolCompleted {
        name: "read_file".into(),
        result: "File contents here".into(),
        error: None,
        terminate: false,
    };

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::ToolCompleted {
            name,
            result,
            error,
            ..
        } => {
            assert_eq!(name, "read_file");
            assert_eq!(result, "File contents here");
            assert!(error.is_none());
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_tool_completed_code_block() {
    let long_result = "Line 1\nLine 2\nLine 3\nMore content here that spans multiple lines";
    let original = SessionEvent::ToolCompleted {
        name: "search".into(),
        result: long_result.into(),
        error: None,
        terminate: false,
    };

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::ToolCompleted {
            name,
            result,
            error,
            ..
        } => {
            assert_eq!(name, "search");
            assert_eq!(result, long_result);
            assert!(error.is_none());
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_tool_completed_with_error() {
    let original = SessionEvent::ToolCompleted {
        name: "read_file".into(),
        result: "".into(),
        error: Some("File not found".into()),
        terminate: false,
    };

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::ToolCompleted {
            name,
            result: _,
            error,
            ..
        } => {
            assert_eq!(name, "read_file");
            assert_eq!(error, Some("File not found".to_string()));
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_session_started() {
    let original = SessionEvent::SessionStarted {
        config: SessionEventConfig::new("2025-12-14T1530-task")
            .with_folder("/kiln/Sessions/2025-12-14T1530-task"),
    };

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::SessionStarted { config } => {
            assert_eq!(config.session_id, "2025-12-14T1530-task");
            assert_eq!(
                config.folder,
                Some(PathBuf::from("/kiln/Sessions/2025-12-14T1530-task"))
            );
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_session_compacted() {
    let original = SessionEvent::internal(InternalSessionEvent::SessionCompacted {
        summary: "Discussed task harness implementation.".into(),
        new_file: PathBuf::from("/kiln/Sessions/test/001-context.md"),
    });

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::Internal(inner) => match *inner {
            InternalSessionEvent::SessionCompacted { summary, new_file } => {
                assert_eq!(summary, "Discussed task harness implementation.");
                assert_eq!(
                    new_file,
                    PathBuf::from("/kiln/Sessions/test/001-context.md")
                );
            }
            _ => panic!("Wrong internal event type"),
        },
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_session_ended() {
    let original = SessionEvent::SessionEnded {
        reason: "User closed session".into(),
    };

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::SessionEnded { reason } => {
            assert_eq!(reason, "User closed session");
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_subagent_spawned() {
    let original = SessionEvent::internal(InternalSessionEvent::SubagentSpawned {
        id: "sub_abc123".into(),
        prompt: "Find all files related to task harness".into(),
    });

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::Internal(inner) => match *inner {
            InternalSessionEvent::SubagentSpawned { id, prompt } => {
                assert_eq!(id, "sub_abc123");
                assert_eq!(prompt, "Find all files related to task harness");
            }
            _ => panic!("Wrong internal event type"),
        },
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_subagent_completed() {
    let original = SessionEvent::internal(InternalSessionEvent::SubagentCompleted {
        id: "sub_abc123".into(),
        result: "Found 5 files.".into(),
    });

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::Internal(inner) => match *inner {
            InternalSessionEvent::SubagentCompleted { id, result } => {
                assert_eq!(id, "sub_abc123");
                assert_eq!(result, "Found 5 files.");
            }
            _ => panic!("Wrong internal event type"),
        },
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_subagent_failed() {
    let original = SessionEvent::internal(InternalSessionEvent::SubagentFailed {
        id: "sub_abc123".into(),
        error: "Timeout exceeded".into(),
    });

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::Internal(inner) => match *inner {
            InternalSessionEvent::SubagentFailed { id, error } => {
                assert_eq!(id, "sub_abc123");
                assert_eq!(error, "Timeout exceeded");
            }
            _ => panic!("Wrong internal event type"),
        },
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_bash_task_spawned() {
    let original = SessionEvent::internal(InternalSessionEvent::BashTaskSpawned {
        id: "task-20250123-1830-abc123".into(),
        command: "cargo build --release".into(),
    });

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::Internal(inner) => match *inner {
            InternalSessionEvent::BashTaskSpawned { id, command } => {
                assert_eq!(id, "task-20250123-1830-abc123");
                assert_eq!(command, "cargo build --release");
            }
            _ => panic!("Wrong internal event type"),
        },
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_bash_task_completed() {
    let original = SessionEvent::internal(InternalSessionEvent::BashTaskCompleted {
        id: "task-20250123-1830-abc123".into(),
        output: "Build succeeded\n".into(),
        exit_code: 0,
    });

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::Internal(inner) => match *inner {
            InternalSessionEvent::BashTaskCompleted {
                id,
                output,
                exit_code,
            } => {
                assert_eq!(id, "task-20250123-1830-abc123");
                assert_eq!(output, "Build succeeded\n");
                assert_eq!(exit_code, 0);
            }
            _ => panic!("Wrong internal event type"),
        },
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_bash_task_failed() {
    let original = SessionEvent::internal(InternalSessionEvent::BashTaskFailed {
        id: "task-20250123-1830-abc123".into(),
        error: "Command not found".into(),
        exit_code: Some(127),
    });

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::Internal(inner) => match *inner {
            InternalSessionEvent::BashTaskFailed {
                id,
                error,
                exit_code,
            } => {
                assert_eq!(id, "task-20250123-1830-abc123");
                assert_eq!(error, "Command not found");
                assert_eq!(exit_code, Some(127));
            }
            _ => panic!("Wrong internal event type"),
        },
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_bash_task_failed_no_exit_code() {
    let original = SessionEvent::internal(InternalSessionEvent::BashTaskFailed {
        id: "task-20250123-1830-abc123".into(),
        error: "Timeout".into(),
        exit_code: None,
    });

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::Internal(inner) => match *inner {
            InternalSessionEvent::BashTaskFailed {
                id,
                error,
                exit_code,
            } => {
                assert_eq!(id, "task-20250123-1830-abc123");
                assert_eq!(error, "Timeout");
                assert_eq!(exit_code, None);
            }
            _ => panic!("Wrong internal event type"),
        },
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_background_task_completed() {
    let original = SessionEvent::internal(InternalSessionEvent::BackgroundTaskCompleted {
        id: "task-20250123-1830-abc123".into(),
        kind: "bash".into(),
        summary: "Build completed successfully".into(),
    });

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::Internal(inner) => match *inner {
            InternalSessionEvent::BackgroundTaskCompleted { id, kind, summary } => {
                assert_eq!(id, "task-20250123-1830-abc123");
                assert_eq!(kind, "bash");
                assert_eq!(summary, "Build completed successfully");
            }
            _ => panic!("Wrong internal event type"),
        },
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_custom_event() {
    let original = SessionEvent::Custom {
        name: "my_custom_event".into(),
        payload: json!({"key": "value", "count": 42}),
    };

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::Custom { name, payload } => {
            assert_eq!(name, "my_custom_event");
            assert_eq!(payload["key"], "value");
            assert_eq!(payload["count"], 42);
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_agent_responded_with_content_only() {
    let original = SessionEvent::AgentResponded {
        content: "Just text".into(),
        tool_calls: vec![],
    };

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::AgentResponded {
            content,
            tool_calls,
        } => {
            assert_eq!(content, "Just text");
            assert!(tool_calls.is_empty());
        }
        _ => panic!("Wrong event type"),
    }
}

#[test]
fn roundtrip_agent_responded_with_tool_calls() {
    let path = test_path("test.txt");
    let path_str = path.to_string_lossy();
    let original = SessionEvent::AgentResponded {
        content: "I'll help you with that.".into(),
        tool_calls: vec![ToolCall::new("read_file", json!({"path": path_str}))],
    };

    let md = original.to_markdown_block(Some(TEST_TIMESTAMP_MS));
    let (parsed, _) = SessionEvent::from_markdown_block(&md).unwrap();

    match parsed {
        SessionEvent::AgentResponded {
            content,
            tool_calls,
        } => {
            assert_eq!(content, "I'll help you with that.");
            assert_eq!(tool_calls.len(), 1);
            assert_eq!(tool_calls[0].name, "read_file");
            assert_eq!(tool_calls[0].args["path"], path_str.as_ref());
        }
        _ => panic!("Wrong event type"),
    }
}
