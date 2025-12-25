//! Integration tests for session types

use chrono::Utc;
use crucible_rig::{
    MessageRole, SessionEntry, SessionIndex, SessionMessage, SessionMetadata, SessionState, Task,
    TaskStatus,
};

#[test]
fn test_complete_session_workflow() {
    // Create a session state
    let mut state = SessionState {
        metadata: SessionMetadata {
            workspace: "my-project".to_string(),
            started: Utc::now(),
            ended: None,
            continued_from: None,
        },
        messages: vec![],
        tasks: vec![],
    };

    // Add messages
    state.messages.push(SessionMessage {
        role: MessageRole::User,
        content: "Create a new feature".to_string(),
        timestamp: Utc::now(),
        tool_name: None,
        tool_args: None,
        tool_result: None,
    });

    state.messages.push(SessionMessage {
        role: MessageRole::Assistant,
        content: "I'll help you create that feature.".to_string(),
        timestamp: Utc::now(),
        tool_name: None,
        tool_args: None,
        tool_result: None,
    });

    // Add tasks
    state.tasks.push(Task {
        content: "Design the API".to_string(),
        status: TaskStatus::Completed,
    });

    state.tasks.push(Task {
        content: "Write implementation".to_string(),
        status: TaskStatus::InProgress,
    });

    state.tasks.push(Task {
        content: "Add tests".to_string(),
        status: TaskStatus::Pending,
    });

    // Verify structure
    assert_eq!(state.messages.len(), 2);
    assert_eq!(state.tasks.len(), 3);
    assert_eq!(state.metadata.workspace, "my-project");

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&state).expect("serialization failed");
    assert!(json.contains("\"workspace\": \"my-project\""));

    // Deserialize back
    let deserialized: SessionState =
        serde_json::from_str(&json).expect("deserialization failed");
    assert_eq!(deserialized.messages.len(), state.messages.len());
    assert_eq!(deserialized.tasks.len(), state.tasks.len());
}

#[test]
fn test_session_index_management() {
    let mut index = SessionIndex::new();

    // Add sessions
    let session1 = SessionEntry {
        id: "sess-001".to_string(),
        workspace: "project-a".to_string(),
        md_path: "/sessions/sess-001.md".to_string(),
        started: Utc::now(),
        ended: None,
        continued_as: None,
    };

    let session2 = SessionEntry {
        id: "sess-002".to_string(),
        workspace: "project-a".to_string(),
        md_path: "/sessions/sess-002.md".to_string(),
        started: Utc::now(),
        ended: Some(Utc::now()),
        continued_as: None,
    };

    index.add_entry(session1);
    index.add_entry(session2);

    assert_eq!(index.sessions.len(), 2);

    // Find latest open session
    let latest = index.find_latest_open("project-a");
    assert!(latest.is_some());
    assert_eq!(latest.unwrap().id, "sess-001");

    // Serialize index
    let json = serde_json::to_string_pretty(&index).expect("serialization failed");
    let deserialized: SessionIndex =
        serde_json::from_str(&json).expect("deserialization failed");
    assert_eq!(deserialized.sessions.len(), 2);
}

#[test]
fn test_tool_call_message() {
    let tool_msg = SessionMessage {
        role: MessageRole::Tool,
        content: "Searching the web".to_string(),
        timestamp: Utc::now(),
        tool_name: Some("web_search".to_string()),
        tool_args: Some(serde_json::json!({
            "query": "Rust async programming",
            "max_results": 5
        })),
        tool_result: Some(serde_json::json!({
            "results": [
                {"title": "The Rust Book", "url": "https://doc.rust-lang.org"},
                {"title": "Tokio Docs", "url": "https://tokio.rs"}
            ]
        })),
    };

    // Serialize
    let json = serde_json::to_string(&tool_msg).expect("serialization failed");

    // Deserialize
    let deserialized: SessionMessage =
        serde_json::from_str(&json).expect("deserialization failed");

    assert_eq!(deserialized.role, MessageRole::Tool);
    assert_eq!(
        deserialized.tool_name,
        Some("web_search".to_string())
    );
    assert!(deserialized.tool_args.is_some());
    assert!(deserialized.tool_result.is_some());
}

#[test]
fn test_task_lifecycle() {
    let mut tasks = vec![
        Task {
            content: "Task 1".to_string(),
            status: TaskStatus::Pending,
        },
        Task {
            content: "Task 2".to_string(),
            status: TaskStatus::Pending,
        },
    ];

    // Start first task
    tasks[0].status = TaskStatus::InProgress;
    assert_eq!(tasks[0].status, TaskStatus::InProgress);

    // Complete first task
    tasks[0].status = TaskStatus::Completed;
    assert_eq!(tasks[0].status, TaskStatus::Completed);

    // Start second task
    tasks[1].status = TaskStatus::InProgress;
    assert_eq!(tasks[1].status, TaskStatus::InProgress);

    // Verify serialization preserves status
    let json = serde_json::to_string(&tasks).expect("serialization failed");
    let deserialized: Vec<Task> = serde_json::from_str(&json).expect("deserialization failed");

    assert_eq!(deserialized[0].status, TaskStatus::Completed);
    assert_eq!(deserialized[1].status, TaskStatus::InProgress);
}
