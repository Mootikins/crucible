use super::*;
use std::sync::Mutex as StdMutex;

mod completion;
mod crud;
mod delegate;
mod graph;
mod handles;
mod messages;
mod messaging;
mod namespace;
mod subscription;
mod ui;

/// Mock implementation of DaemonSessionApi for testing.
pub(super) struct MockDaemonApi {
    /// Whole params object from the most recent `create_session`, so tests can
    /// assert what the Lua binding put on the wire (aliases, implied flags).
    last_create_params: StdMutex<Option<serde_json::Value>>,
    /// Most recent `undo(session_id, count)` call.
    last_undo_call: StdMutex<Option<(String, usize)>>,
    /// Number of turns the next `undo` call should report. Defaults to
    /// `min(count, 2)` if `None`.
    undo_turns_to_return: StdMutex<Option<usize>>,
    /// Override for `can_undo`. Defaults to `true`.
    can_undo_value: StdMutex<bool>,
    /// Override for `undo_depth`. Defaults to `2`.
    undo_depth_value: StdMutex<usize>,
    /// Every `complete` call, as `(session_id, params)`.
    completions: StdMutex<Vec<(String, serde_json::Value)>>,
    /// Session ids from every `send_message` call, in order — what the
    /// handle-method tests assert the handle passed through.
    send_calls: StdMutex<Vec<String>>,
    /// Session ids from every `end_session` call, in order.
    end_calls: StdMutex<Vec<String>>,
    /// `(session_id, hunk_id)` from every `review_list_hunks` call.
    review_list_calls: StdMutex<Vec<String>>,
    /// Every `set_mode` call, as `(session_id, mode_id)`.
    mode_calls: StdMutex<Vec<(String, String)>>,
    /// Every `set_title` call, as `(session_id, title)`.
    title_calls: StdMutex<Vec<(String, String)>>,
    /// What the next `set_mode` refuses with, if anything.
    mode_refusal: StdMutex<Option<String>>,
    /// Every `request_interaction` call, as `(session_id, request, timeout)`.
    interaction_calls: StdMutex<Vec<(String, serde_json::Value, u64)>>,
    /// What the next `request_interaction` resolves to. Defaults to
    /// `{"kind":"cancelled"}` — the no-answer case, which is what a mock with
    /// no client attached honestly is.
    interaction_answer: StdMutex<Option<serde_json::Value>>,
}

impl MockDaemonApi {
    pub(super) fn new() -> Self {
        Self {
            last_create_params: StdMutex::new(None),
            last_undo_call: StdMutex::new(None),
            undo_turns_to_return: StdMutex::new(None),
            can_undo_value: StdMutex::new(true),
            undo_depth_value: StdMutex::new(2),
            completions: StdMutex::new(Vec::new()),
            send_calls: StdMutex::new(Vec::new()),
            end_calls: StdMutex::new(Vec::new()),
            review_list_calls: StdMutex::new(Vec::new()),
            mode_calls: StdMutex::new(Vec::new()),
            title_calls: StdMutex::new(Vec::new()),
            mode_refusal: StdMutex::new(None),
            interaction_calls: StdMutex::new(Vec::new()),
            interaction_answer: StdMutex::new(None),
        }
    }

    /// Every `complete` call this mock saw.
    pub(super) fn completions(&self) -> Vec<(String, serde_json::Value)> {
        self.completions.lock().unwrap().clone()
    }

    /// Session ids from every `send_message` call, in order.
    pub(super) fn send_calls(&self) -> Vec<String> {
        self.send_calls.lock().unwrap().clone()
    }

    /// Session ids from every `end_session` call, in order.
    pub(super) fn end_calls(&self) -> Vec<String> {
        self.end_calls.lock().unwrap().clone()
    }

    /// Session ids from every `review_list_hunks` call, in order.
    pub(super) fn review_list_calls(&self) -> Vec<String> {
        self.review_list_calls.lock().unwrap().clone()
    }

    /// Every `set_mode` call this mock saw, in order.
    pub(super) fn mode_calls(&self) -> Vec<(String, String)> {
        self.mode_calls.lock().unwrap().clone()
    }

    /// Every `set_title` call this mock saw, in order.
    pub(super) fn title_calls(&self) -> Vec<(String, String)> {
        self.title_calls.lock().unwrap().clone()
    }

    /// Make every later `set_mode` answer this refusal, the way the daemon
    /// refuses a mode the session does not offer.
    pub(super) fn refuse_mode(&self, message: &str) {
        *self.mode_refusal.lock().unwrap() = Some(message.to_string());
    }

    /// Every `request_interaction` call this mock saw.
    pub(super) fn interaction_calls(&self) -> Vec<(String, serde_json::Value, u64)> {
        self.interaction_calls.lock().unwrap().clone()
    }

    /// Set what the next `request_interaction` answers with.
    pub(super) fn set_interaction_answer(&self, answer: serde_json::Value) {
        *self.interaction_answer.lock().unwrap() = Some(answer);
    }

    /// Params object from the most recent `create_session`, or `None`.
    pub(super) fn last_create_params(&self) -> Option<serde_json::Value> {
        self.last_create_params.lock().unwrap().clone()
    }

    /// Snapshot of the most recent `undo` call, or `None` if not invoked.
    pub(super) fn last_undo_call(&self) -> Option<(String, usize)> {
        self.last_undo_call.lock().unwrap().clone()
    }
}

impl DaemonSessionApi for MockDaemonApi {
    // The tests here do not call these. A required method with no body
    // makes a missing override a compile error, not a silent stub.
    fn context_usage(
        &self,
        _: String,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
        unimplemented!()
    }

    fn compact(&self, _: String) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        unimplemented!()
    }

    fn remove_messages(
        &self,
        _: String,
        _: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<usize, String>> + Send>> {
        unimplemented!()
    }

    fn review_list_hunks(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
        self.review_list_calls.lock().unwrap().push(session_id);
        Box::pin(async { Ok(vec![]) })
    }

    fn review_set_state(
        &self,
        _: String,
        _: String,
        _: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        unimplemented!()
    }

    fn review_comment(
        &self,
        _: String,
        _: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
        unimplemented!()
    }

    fn review_resolve_comment(
        &self,
        _: String,
        _: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        unimplemented!()
    }

    /// Answers with the prompt it was given, so a test can assert what
    /// crossed the boundary without a provider behind it.
    fn complete(
        &self,
        session_id: String,
        params: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        self.completions
            .lock()
            .unwrap()
            .push((session_id, params.clone()));
        let echo = params
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        Box::pin(async move { Ok(format!("answered: {echo}")) })
    }

    fn request_interaction(
        &self,
        session_id: String,
        request: serde_json::Value,
        timeout_secs: u64,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
        self.interaction_calls
            .lock()
            .unwrap()
            .push((session_id, request, timeout_secs));
        let answer = self
            .interaction_answer
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| serde_json::json!({ "kind": "cancelled" }));
        Box::pin(async move { Ok(answer) })
    }

    fn create_session(
        &self,
        params: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
        *self.last_create_params.lock().unwrap() = Some(params.clone());
        let field = |key: &str| params.get(key).and_then(|v| v.as_str()).map(str::to_string);
        let session_type = field("type").unwrap_or_else(|| "chat".to_string());
        // Mirrors the daemon's own fallback: an omitted or empty `kilns`
        // resolves to the home kiln server-side, never client-side. The members
        // are registry *names*, which is what makes `"default"` a plausible
        // value here and a path not one.
        let kilns: Vec<String> = params
            .get("kilns")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|k| k.as_str().map(str::to_string))
                    .collect()
            })
            .filter(|k: &Vec<String>| !k.is_empty())
            .unwrap_or_else(|| vec!["default".to_string()]);
        // No workspace given means no workspace — `null`, not the first kiln.
        // Deriving it from `kilns` is the sentinel the daemon dropped, and a
        // mock that keeps it teaches plugin authors the shape that went away
        // (and would now hand back a *name* where a path belongs).
        let ws = field("workspace");
        Box::pin(async move {
            Ok(serde_json::json!({
                "id": format!("{}-2025-01-01T0000-abc123", session_type),
                "session_type": session_type,
                "state": "active",
                "kilns": kilns,
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
                    "model": "claude-haiku-4-5-20251001",
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
        session_id: String,
        _content: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
        self.send_calls.lock().unwrap().push(session_id);
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
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        self.end_calls.lock().unwrap().push(session_id);
        Box::pin(async { Ok(()) })
    }

    fn set_mode(
        &self,
        session_id: String,
        mode_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        self.mode_calls.lock().unwrap().push((session_id, mode_id));
        let refusal = self.mode_refusal.lock().unwrap().clone();
        Box::pin(async move {
            match refusal {
                Some(message) => Err(message),
                None => Ok(()),
            }
        })
    }

    fn set_title(
        &self,
        session_id: String,
        title: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
        self.title_calls.lock().unwrap().push((session_id, title));
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
        include_tools: bool,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
        Box::pin(async move {
            let mut msgs = vec![
                serde_json::json!({ "role": "system", "content": "You are helpful.", "timestamp": "2025-01-01T00:00:00Z" }),
                serde_json::json!({ "role": "user", "content": "Hello", "timestamp": "2025-01-01T00:00:01Z" }),
                serde_json::json!({ "role": "assistant", "content": "Hi there!", "timestamp": "2025-01-01T00:00:02Z" }),
            ];
            // The tool row appears only when the binding passed the flag, so a
            // Lua test can tell from the row count whether `tools = true` arrived.
            if include_tools {
                msgs.push(serde_json::json!({
                    "role": "tool_call", "id": "c1", "name": "bash",
                    "args": { "command": "ls" }, "timestamp": "2025-01-01T00:00:03Z",
                }));
            }
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
        _interactive: bool,
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
