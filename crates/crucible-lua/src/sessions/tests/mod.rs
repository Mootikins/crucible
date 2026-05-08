use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex as StdMutex;

mod crud;
mod graph;
mod messages;
mod messaging;
mod namespace;
mod subscription;

/// Mock implementation of DaemonSessionApi for testing.
pub(super) struct MockDaemonApi {
    create_called: AtomicBool,
    /// Captures the most recent `set_output_validation` spec so tests
    /// can assert what string the Lua binding serialised. Wrapped in a
    /// `StdMutex` because `DaemonSessionApi` takes `&self` and tests
    /// inspect the field across the async call.
    last_validation_spec: StdMutex<Option<(String, String)>>,
    /// Most recent `undo(session_id, count)` call.
    last_undo_call: StdMutex<Option<(String, usize)>>,
    /// Number of turns the next `undo` call should report. Defaults to
    /// `min(count, 2)` if `None`.
    undo_turns_to_return: StdMutex<Option<usize>>,
    /// Override for `can_undo`. Defaults to `true`.
    can_undo_value: StdMutex<bool>,
    /// Override for `undo_depth`. Defaults to `2`.
    undo_depth_value: StdMutex<usize>,
}

impl MockDaemonApi {
    pub(super) fn new() -> Self {
        Self {
            create_called: AtomicBool::new(false),
            last_validation_spec: StdMutex::new(None),
            last_undo_call: StdMutex::new(None),
            undo_turns_to_return: StdMutex::new(None),
            can_undo_value: StdMutex::new(true),
            undo_depth_value: StdMutex::new(2),
        }
    }

    /// Snapshot of `(session_id, spec)` from the most recent
    /// `set_output_validation` call, or `None` if not yet invoked.
    pub(super) fn last_validation_spec(&self) -> Option<(String, String)> {
        self.last_validation_spec.lock().unwrap().clone()
    }

    /// Snapshot of the most recent `undo` call, or `None` if not invoked.
    pub(super) fn last_undo_call(&self) -> Option<(String, usize)> {
        self.last_undo_call.lock().unwrap().clone()
    }
}

impl DaemonSessionApi for MockDaemonApi {
    fn create_session(
        &self,
        session_type: String,
        kiln: Option<String>,
        workspace: Option<String>,
        _connected_kilns: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
        self.create_called.store(true, Ordering::SeqCst);
        let kiln = kiln.unwrap_or_else(|| "/default/crucible".to_string());
        let ws = workspace.unwrap_or_else(|| kiln.clone());
        Box::pin(async move {
            Ok(serde_json::json!({
                "id": format!("{}-2025-01-01T0000-abc123", session_type),
                "session_type": session_type,
                "state": "active",
                "kiln": kiln,
                "workspace": ws,
            }))
        })
    }

    fn get_session(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>, String>> + Send>> {
        Box::pin(async move {
            if session_id == "exists-123" {
                Ok(Some(serde_json::json!({
                    "id": "exists-123",
                    "session_type": "chat",
                    "state": "active",
                })))
            } else {
                Ok(None)
            }
        })
    }

    fn list_sessions(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
        Box::pin(async {
            Ok(vec![
                serde_json::json!({
                    "id": "chat-001",
                    "session_type": "chat",
                    "state": "active",
                }),
                serde_json::json!({
                    "id": "agent-002",
                    "session_type": "agent",
                    "state": "paused",
                }),
            ])
        })
    }

    fn configure_agent(
        &self,
        _session_id: String,
        _agent_config: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    fn send_message(
        &self,
        _session_id: String,
        _content: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        Box::pin(async { Ok("msg-response-001".to_string()) })
    }

    fn cancel(
        &self,
        _session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<bool, String>> + Send>> {
        Box::pin(async { Ok(true) })
    }

    fn pause(
        &self,
        _session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    fn resume(
        &self,
        _session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    fn end_session(
        &self,
        _session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    fn respond_to_permission(
        &self,
        _session_id: String,
        _request_id: String,
        _response: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    fn subscribe(
        &self,
        _session_id: String,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>,
                        String,
                    >,
                > + Send,
        >,
    > {
        Box::pin(async {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            // Send a couple of test events then drop the sender
            let _ = tx.send(serde_json::json!({
                "type": "text_delta",
                "session_id": "test-session",
                "data": { "content": "Hello" }
            }));
            let _ = tx.send(serde_json::json!({
                "type": "text_delta",
                "session_id": "test-session",
                "data": { "content": " World" }
            }));
            // tx is dropped here, so after reading 2 events, recv() returns None
            Ok(rx)
        })
    }

    fn unsubscribe(
        &self,
        _session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    fn load_messages(
        &self,
        _session_id: String,
        role_filter: Option<String>,
        limit: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
        Box::pin(async move {
            let mut msgs = vec![
                serde_json::json!({ "role": "system", "content": "You are helpful.", "timestamp": "2025-01-01T00:00:00Z" }),
                serde_json::json!({ "role": "user", "content": "Hello", "timestamp": "2025-01-01T00:00:01Z" }),
                serde_json::json!({ "role": "assistant", "content": "Hi there!", "timestamp": "2025-01-01T00:00:02Z" }),
            ];
            if let Some(role) = role_filter {
                msgs.retain(|m| m.get("role").and_then(|r| r.as_str()) == Some(role.as_str()));
            }
            if let Some(n) = limit {
                let start = msgs.len().saturating_sub(n);
                msgs = msgs.split_off(start);
            }
            Ok(msgs)
        })
    }

    fn inject_context(
        &self,
        _session_id: String,
        _role: String,
        _content: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        Box::pin(async { Ok(()) })
    }

    fn collect_subagents(
        &self,
        _job_ids: Vec<String>,
        _timeout_secs: Option<f64>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
        Box::pin(async { Ok(vec![]) })
    }

    fn fork_session(
        &self,
        _session_id: String,
        _up_to: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
        Box::pin(async {
            Ok(serde_json::json!({
                "id": "fork-123",
                "parent_id": "parent-123",
                "messages_copied": 3,
            }))
        })
    }

    fn cache_stats(
        &self,
        _session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
        Box::pin(async {
            Ok(serde_json::json!({
                "session_id": "test-session",
                "hits": 0,
                "misses": 0,
                "read_tokens": 0,
                "creation_tokens": 0,
                "prompt_tokens": 0,
                "completion_tokens": 0,
                "hit_rate": serde_json::Value::Null,
            }))
        })
    }

    fn set_output_validation(
        &self,
        session_id: String,
        spec: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        *self.last_validation_spec.lock().unwrap() = Some((session_id, spec));
        Box::pin(async { Ok(()) })
    }

    fn undo(
        &self,
        session_id: String,
        count: usize,
    ) -> Pin<Box<dyn Future<Output = Result<usize, String>> + Send>> {
        *self.last_undo_call.lock().unwrap() = Some((session_id, count));
        let override_count = *self.undo_turns_to_return.lock().unwrap();
        let result = override_count.unwrap_or_else(|| count.min(2));
        Box::pin(async move { Ok(result) })
    }

    fn can_undo(
        &self,
        _session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<bool, String>> + Send>> {
        let v = *self.can_undo_value.lock().unwrap();
        Box::pin(async move { Ok(v) })
    }

    fn undo_depth(
        &self,
        _session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<usize, String>> + Send>> {
        let v = *self.undo_depth_value.lock().unwrap();
        Box::pin(async move { Ok(v) })
    }

    fn undo_history(
        &self,
        _session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
        Box::pin(async {
            Ok(vec![
                serde_json::json!({ "turn_index": 0, "messages_removed": 2 }),
                serde_json::json!({ "turn_index": 1, "messages_removed": 3 }),
            ])
        })
    }

    fn send_and_collect(
        &self,
        _session_id: String,
        _content: String,
        _timeout_secs: Option<f64>,
        _max_tool_result_len: Option<usize>,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<tokio::sync::mpsc::UnboundedReceiver<ResponsePart>, String>>
                + Send,
        >,
    > {
        Box::pin(async {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let _ = tx.send(ResponsePart::Text {
                content: "Hello World".to_string(),
            });
            drop(tx);
            Ok(rx)
        })
    }
}
