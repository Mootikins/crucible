//! One upsert table per turn for the tool calls an ACP agent reports.
//!
//! The ACP spec gives no order between `tool_call` and `tool_call_update`.
//! Many agents send only updates, and some send a completed update before the
//! call that names it. So the client keeps one entry per `toolCallId`. The
//! entry holds the merged fields of every frame (a [`RawToolCall`]) and their
//! classification (a [`CanonicalToolCall`]). A frame that changes the fields
//! makes a new classification. The entry decides what the turn stream sees:
//!
//! - An entry is announced at most once, at the first frame that gives it a
//!   title. A later change of the canonical call comes as `ToolCallUpdate`.
//! - A completion for an entry with no title is held until the title arrives,
//!   or until the turn ends. Then the entry is announced under its canonical
//!   name, and the held result follows it.
//! - At the end of the turn, an announced entry with no completion gets a
//!   failed result, so a card does not stay open forever.
//! - A `session/request_permission` joins the entry of its `toolCallId`. The
//!   request can come before the `tool_call` (codex-acp in Rust) or after it
//!   (codex-acp in TypeScript), so both orders join.

use agent_client_protocol::schema::v1::{
    ToolCall, ToolCallContent, ToolCallStatus, ToolCallUpdate,
};

use super::CrucibleAcpClient;
use crucible_core::text::sanitize_single_line;
use crucible_core::turn::TurnEvent;
use crucible_core::types::{classify_acp, AgentKeys, CanonicalToolCall, RawToolCall};

/// How many still-unnamed tool results one turn will hold.
///
/// A real turn holds a handful at most: one `tool_call_update{completed}`
/// that arrived before its `tool_call`.
const MAX_HELD_RESULTS: usize = 256;

/// How many bytes of them one turn will hold.
///
/// The count cap alone bounds nothing the agent controls: each held entry
/// keeps an uncut `rawOutput` payload until the turn ends, so 256 entries is
/// 256 times the largest frame. The byte cap makes the peak a fixed number,
/// and it also catches the one case the count cap cannot see: one enormous
/// result. 8 MiB is far above real held traffic, which is kilobytes.
const MAX_HELD_RESULT_BYTES: usize = 8 * 1024 * 1024;

/// The error a held completion gets when the caps refuse its payload.
const HELD_RESULT_DROPPED: &str = "tool result dropped: held results are over the cap";

/// A completion that arrived before its call had a name.
#[derive(Debug, Clone, PartialEq, Eq)]
struct HeldResult {
    result: Option<serde_json::Value>,
    error: Option<String>,
}

impl HeldResult {
    /// A string counts its text; a structured result counts its JSON.
    fn bytes(&self) -> usize {
        let result = match &self.result {
            None => 0,
            Some(serde_json::Value::String(text)) => text.len(),
            Some(other) => other.to_string().len(),
        };
        result + self.error.as_ref().map_or(0, String::len)
    }
}

/// The fields of one frame, with the one-line labels sanitized.
///
/// A title and a name are labels that say what the agent does, so they get
/// the single-line form: a newline or a bidi override in them makes the card
/// claim one action and perform another.
pub(super) fn frame_fields(mut raw: RawToolCall) -> RawToolCall {
    raw.title = raw.title.map(|t| sanitize_single_line(&t));
    raw.name = raw.name.map(|n| sanitize_single_line(&n));
    raw
}

#[derive(Debug)]
struct Entry {
    id: String,
    /// The merged fields of the frames.
    raw: RawToolCall,
    /// `raw`, classified.
    call: CanonicalToolCall,
    /// The agent profile that made the call.
    agent: Option<String>,
    /// A `session/update` frame named this id. An entry that only a
    /// permission request made gets no card.
    seen: bool,
    announced: bool,
    completions: u32,
    held: Option<HeldResult>,
}

impl Entry {
    fn new(id: String, agent: Option<String>) -> Self {
        Self {
            id,
            raw: RawToolCall::default(),
            call: classify_acp(RawToolCall::default(), &[]),
            agent,
            seen: false,
            announced: false,
            completions: 0,
            held: None,
        }
    }

    /// Classify `raw` as a call of the agent. Each ACP call is classified
    /// here, so each canonical call names its agent: the card, each update
    /// and each permission request.
    fn classify(&self, raw: RawToolCall, keys: &[AgentKeys]) -> CanonicalToolCall {
        CanonicalToolCall {
            agent: self.agent.clone(),
            ..classify_acp(raw, keys)
        }
    }

    /// The `ToolResult` event of this call, under its canonical name.
    fn result(&self, result: Option<serde_json::Value>, error: Option<String>) -> TurnEvent {
        TurnEvent::ToolResult {
            id: self.id.clone(),
            name: self.call.tool.clone(),
            result: result.unwrap_or_else(|| serde_json::Value::String(String::new())),
            error,
        }
    }

    /// Merge a frame and classify the call again. An announced entry reports
    /// a changed call.
    fn merge(&mut self, frame: RawToolCall, keys: &[AgentKeys], out: &mut Vec<TurnEvent>) {
        self.raw.merge(frame);
        let call = self.classify(self.raw.clone(), keys);
        if self.announced && call != self.call {
            out.push(TurnEvent::ToolCallUpdate {
                id: self.id.clone(),
                call: Box::new(call.clone()),
            });
        }
        self.call = call;
    }

    /// Emit `ToolCall`, and then the held completion when there is one.
    ///
    /// Returns the held bytes this released. The table must subtract them
    /// from its total, or the cap refuses later results against bytes that
    /// already left the table.
    fn announce(&mut self, out: &mut Vec<TurnEvent>) -> usize {
        self.announced = true;
        out.push(TurnEvent::ToolCall {
            id: self.id.clone(),
            name: self.call.tool.clone(),
            // The agent's `rawInput`, or `Null` when no frame sent one.
            args: (self.call.raw.as_ref())
                .and_then(|raw| raw.raw_input.clone())
                .unwrap_or(serde_json::Value::Null),
            call: Some(Box::new(self.call.clone())),
        });
        match self.held.take() {
            Some(held) => {
                let released = held.bytes();
                self.completions += 1;
                out.push(self.result(held.result, held.error));
                released
            }
            None => 0,
        }
    }
}

/// The tool calls of one turn, keyed by `toolCallId`, in first-seen order.
#[derive(Debug, Default)]
pub(super) struct ToolCallTable {
    entries: Vec<Entry>,
    held_bytes: usize,
    /// The agent profile that makes the calls.
    agent: Option<String>,
}

impl ToolCallTable {
    pub(super) fn for_agent(agent: &str) -> Self {
        Self {
            entries: Vec::new(),
            held_bytes: 0,
            agent: Some(agent.to_string()),
        }
    }

    fn entry_mut(&mut self, id: &str) -> &mut Entry {
        let index = match self.entries.iter().position(|entry| entry.id == id) {
            Some(index) => index,
            None => {
                let entry = Entry::new(id.to_string(), self.agent.clone());
                self.entries.push(entry);
                self.entries.len() - 1
            }
        };
        &mut self.entries[index]
    }

    fn held_count(&self) -> usize {
        self.entries.iter().filter(|e| e.held.is_some()).count()
    }

    /// Merge a `tool_call` frame. The chunks it returns go to the callback.
    pub(super) fn upsert_call(&mut self, call: ToolCall, keys: &[AgentKeys]) -> Vec<TurnEvent> {
        let mut out = Vec::new();
        let frame = frame_fields(RawToolCall::from(&call));
        let entry = self.entry_mut(&call.tool_call_id.to_string());
        entry.seen = true;
        entry.merge(frame, keys, &mut out);
        // The agent's own `tool_call` announces the call, also when no frame
        // gave a title before it.
        if !entry.announced {
            let released = entry.announce(&mut out);
            self.held_bytes -= released;
        }
        out
    }

    /// Merge a `tool_call_update` frame. The chunks it returns go to the
    /// callback.
    pub(super) fn upsert_update(
        &mut self,
        update: ToolCallUpdate,
        keys: &[AgentKeys],
    ) -> Vec<TurnEvent> {
        let mut out = Vec::new();
        let id = update.tool_call_id.to_string();
        let mut frame = frame_fields(RawToolCall::from(&update));
        let fields = update.fields;
        let completed = matches!(
            fields.status,
            Some(ToolCallStatus::Completed | ToolCallStatus::Failed)
        );
        let content = fields.content.as_deref().unwrap_or_default();
        let completion = completed.then(|| HeldResult {
            result: CrucibleAcpClient::extract_tool_result(fields.raw_output.as_ref(), content),
            error: CrucibleAcpClient::extract_tool_error(
                fields.status,
                fields.raw_output.as_ref(),
                content,
            ),
        });
        // The content of a completion is the result, not the call. A diff in
        // it still describes the call, so the content stays only with a diff.
        let has_diff = frame
            .content
            .iter()
            .any(|c| matches!(c, ToolCallContent::Diff(_)));
        if completed && !has_diff {
            frame.content.clear();
        }

        let held_count = self.held_count();
        let mut held_bytes = self.held_bytes;
        let entry = self.entry_mut(&id);
        entry.seen = true;
        entry.merge(frame, keys, &mut out);

        if !entry.announced && entry.raw.title.is_some() {
            held_bytes -= entry.announce(&mut out);
        }

        if let Some(completion) = completion {
            if entry.announced {
                entry.completions += 1;
                out.push(entry.result(completion.result, completion.error));
            } else {
                hold(entry, completion, held_count, &mut held_bytes);
            }
        }
        self.held_bytes = held_bytes;
        out
    }

    /// The canonical call that a `session/request_permission` asks about.
    ///
    /// The request joins the entry of its `toolCallId`. A field that the
    /// request sets wins, because the agent asks about what the request says.
    /// A field that it does not set comes from the earlier frames: the
    /// TypeScript codex adapter sends the diff only in the `tool_call`.
    ///
    /// A request that comes before any frame of its id stays in the entry,
    /// so the later frames merge into it. A request after a frame changes
    /// nothing in the entry, because the card shows what the frames said.
    pub(super) fn permission_call(
        &mut self,
        request: &ToolCallUpdate,
        keys: &[AgentKeys],
    ) -> CanonicalToolCall {
        let frame = frame_fields(RawToolCall::from(request));
        let entry = self.entry_mut(&request.tool_call_id.to_string());
        let mut joined = entry.raw.clone();
        joined.merge(frame);
        let mut call = entry.classify(joined.clone(), keys);
        // A call that nothing names has its kind as its name. The frames can
        // name it when the request does not (old codex replaces `rawInput`
        // in an MCP approval), and the more specific name wins.
        if call.tool == call.kind && entry.call.tool != entry.call.kind {
            call.tool = entry.call.tool.clone();
        }
        if !entry.seen {
            entry.raw = joined;
            entry.call = call.clone();
        }
        call
    }

    /// Close the turn. Every unannounced entry is announced under its
    /// canonical name, and every entry with no completion gets a failed
    /// result that names the stop reason. An entry that only a permission
    /// request made is not a call that the agent reported, so it gets no
    /// card.
    pub(super) fn flush(&mut self, stop_reason: &str) -> Vec<TurnEvent> {
        let mut out = Vec::new();
        for entry in self.entries.iter_mut().filter(|e| e.seen) {
            if !entry.announced {
                tracing::debug!(
                    tool_id = %entry.id,
                    "ACP never gave this tool call a title; announcing it under its canonical name"
                );
                entry.announce(&mut out);
            }
            if entry.completions == 0 {
                entry.completions += 1;
                out.push(entry.result(None, Some(format!("turn ended: {stop_reason}"))));
            }
        }
        self.held_bytes = 0;
        out
    }

    /// Warn with each result the turn still holds, and release them all.
    ///
    /// Not every turn reaches `flush`: the agent-error return and the
    /// overall-timeout return drop the table mid-turn. The log is then the
    /// only place a held payload survives, as on the refusal path in `hold`.
    /// Returns how many results it reported.
    fn log_dropped(&mut self) -> usize {
        let mut dropped = 0;
        for entry in &mut self.entries {
            if let Some(held) = entry.held.take() {
                dropped += 1;
                tracing::warn!(
                    tool_id = %entry.id,
                    result = ?held.result,
                    error = ?held.error,
                    "the turn ended before this held ACP tool result was \
                     announced; dropping it (lost from session.jsonl, \
                     recordings and Lua handlers)"
                );
            }
        }
        self.held_bytes = 0;
        dropped
    }

    /// Whether the turn announced at least one call. After `flush` this is
    /// true for every table with a call that the agent reported.
    pub(super) fn announced_any(&self) -> bool {
        self.entries.iter().any(|entry| entry.announced)
    }

    /// The number of calls the turn reported, for tests that inspect the table.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    /// The arguments of one call, for tests that inspect the table.
    #[cfg(test)]
    pub(super) fn args_of(&self, id: &str) -> Option<&serde_json::Value> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .and_then(|entry| entry.raw.raw_input.as_ref())
    }
}

/// The failure returns drop the table without a `flush`. This logs what
/// they lose. After a `flush` there is nothing held, so a normal turn
/// logs nothing here.
impl Drop for ToolCallTable {
    fn drop(&mut self) {
        self.log_dropped();
    }
}

/// Hold a completion on an unnamed entry, within the caps.
///
/// A later completion for the same entry replaces the earlier one, because
/// both renderers key a result on the call id and show the last one. A
/// refused payload is logged in full, because the log is the only place it
/// survives, and the entry keeps a short error so the drop reaches the
/// transcript when the entry is announced.
fn hold(entry: &mut Entry, completion: HeldResult, held_count: usize, held_bytes: &mut usize) {
    let replaced_bytes = entry.held.as_ref().map_or(0, HeldResult::bytes);
    let new_count = held_count + usize::from(entry.held.is_none());
    let new_bytes = *held_bytes - replaced_bytes + completion.bytes();

    if new_count > MAX_HELD_RESULTS || new_bytes > MAX_HELD_RESULT_BYTES {
        tracing::warn!(
            tool_id = %entry.id,
            result = ?completion.result,
            error = ?completion.error,
            replaced = ?entry.held,
            held = held_count,
            held_bytes = *held_bytes,
            entry_bytes = completion.bytes(),
            count_cap = MAX_HELD_RESULTS,
            byte_cap = MAX_HELD_RESULT_BYTES,
            "ACP held tool results hit their cap; dropping this payload \
             (lost from session.jsonl, recordings and Lua handlers)"
        );
        // The renderers show the last result per call id, so a kept partial
        // result would read as final. The marker replaces it, and its bytes
        // come back.
        *held_bytes -= replaced_bytes;
        *held_bytes += HELD_RESULT_DROPPED.len();
        entry.held = Some(HeldResult {
            result: None,
            error: Some(HELD_RESULT_DROPPED.to_string()),
        });
        return;
    }

    *held_bytes = new_bytes;
    entry.held = Some(completion);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn call(value: Value) -> ToolCall {
        serde_json::from_value(value).expect("tool_call deserializes")
    }

    fn update(value: Value) -> ToolCallUpdate {
        serde_json::from_value(value).expect("tool_call_update deserializes")
    }

    fn completed(id: &str, result: &str) -> ToolCallUpdate {
        update(json!({
            "toolCallId": id,
            "status": "completed",
            "rawOutput": result,
        }))
    }

    /// The tables here have no agent key table: the default matcher alone.
    fn upsert_call(table: &mut ToolCallTable, value: Value) -> Vec<TurnEvent> {
        table.upsert_call(call(value), &[])
    }

    fn upsert_update(table: &mut ToolCallTable, frame: ToolCallUpdate) -> Vec<TurnEvent> {
        table.upsert_update(frame, &[])
    }

    fn shapes(chunks: &[TurnEvent]) -> Vec<String> {
        chunks
            .iter()
            .map(|c| match c {
                TurnEvent::ToolCall { id, name, .. } => format!("start {id} {name}"),
                TurnEvent::ToolResult {
                    id,
                    name,
                    result,
                    error,
                } => format!("end {id} {name} {result} {error:?}"),
                TurnEvent::ToolCallUpdate { id, .. } => format!("update {id}"),
                other => format!("{other:?}"),
            })
            .collect()
    }

    /// The client makes the whole `TurnEvent`, so the stream passes it
    /// through. A call carries its `rawInput` as its arguments, or `Null` when
    /// no frame sent one. A result is the text of the completion.
    #[test]
    fn the_table_makes_the_turn_events_of_a_call() {
        let mut table = ToolCallTable::default();

        let bare = upsert_call(&mut table, json!({"toolCallId": "t1", "title": "Run"}));
        let [TurnEvent::ToolCall {
            name, args, call, ..
        }] = bare.as_slice()
        else {
            panic!("expected one ToolCall, got {bare:?}")
        };
        assert_eq!(name, "tool");
        assert_eq!(args, &Value::Null);
        assert_eq!(call.as_ref().map(|call| call.tool.as_str()), Some("tool"));

        let started = upsert_call(
            &mut table,
            json!({"toolCallId": "t2", "title": "Run", "rawInput": {"q": 1}}),
        );
        let [TurnEvent::ToolCall { args, .. }] = started.as_slice() else {
            panic!("expected one ToolCall, got {started:?}")
        };
        assert_eq!(args, &json!({"q": 1}));

        assert_eq!(
            upsert_update(&mut table, completed("t2", "out")),
            vec![TurnEvent::ToolResult {
                id: "t2".into(),
                name: "tool".into(),
                result: json!("out"),
                error: None,
            }]
        );
    }

    /// A structured `rawOutput` stays structured, announced or held.
    #[test]
    fn a_structured_raw_output_stays_structured() {
        let mut table = ToolCallTable::default();
        let output = json!({"exit_code": 0, "stdout": "ok"});
        let frame = |id: &str| {
            update(json!({"toolCallId": id, "status": "completed", "rawOutput": output}))
        };
        upsert_call(&mut table, json!({"toolCallId": "t1", "title": "Run"}));
        let held = upsert_update(&mut table, frame("t2"));
        assert!(held.is_empty(), "{held:?}");
        let announced = upsert_update(&mut table, frame("t1"));
        let flushed = table.flush("end_turn");
        for events in [&announced[..], &flushed[1..]] {
            let [TurnEvent::ToolResult { result, .. }] = events else {
                panic!("expected one ToolResult, got {events:?}")
            };
            assert_eq!(result, &output);
        }
    }

    #[test]
    fn update_before_call_is_named_by_the_call() {
        let mut table = ToolCallTable::default();

        let first = upsert_update(&mut table, completed("t1", "four"));
        assert!(
            first.is_empty(),
            "an unnamed completion must wait; got {first:?}"
        );

        let second = upsert_call(
            &mut table,
            json!({
                "toolCallId": "t1",
                "title": "Late call",
                "name": "late_call",
                "rawInput": {"q": "2+2"},
            }),
        );
        assert_eq!(
            shapes(&second),
            vec!["start t1 late_call", "end t1 late_call \"four\" None"]
        );
        assert!(table.flush("end_turn").is_empty());
    }

    #[test]
    fn update_with_title_for_unseen_id_announces_it() {
        let mut table = ToolCallTable::default();

        let chunks = upsert_update(
            &mut table,
            update(json!({
                "toolCallId": "t1",
                "title": "Late named tool",
                "name": "late_named_tool",
                "status": "completed",
                "rawOutput": "done",
            })),
        );
        assert_eq!(
            shapes(&chunks),
            vec![
                "start t1 late_named_tool",
                "end t1 late_named_tool \"done\" None"
            ]
        );
    }

    /// A call that no frame names is announced under the fallback name
    /// `tool`, the canonical name of an unclassified call.
    #[test]
    fn bare_update_for_unseen_id_is_announced_at_flush_under_the_fallback_name() {
        let mut table = ToolCallTable::default();

        assert!(upsert_update(&mut table, completed("t1", "orphaned output")).is_empty());

        assert_eq!(
            shapes(&table.flush("end_turn")),
            vec![
                "start t1 tool".to_string(),
                "end t1 tool \"orphaned output\" None".to_string(),
            ]
        );
    }

    #[test]
    fn repeat_completion_keeps_the_name() {
        let mut table = ToolCallTable::default();
        upsert_call(
            &mut table,
            json!({"toolCallId": "t1", "title": "Repeated", "name": "repeated_tool"}),
        );

        let first = upsert_update(&mut table, completed("t1", "PARTIAL"));
        let second = upsert_update(&mut table, completed("t1", "FINAL"));

        assert_eq!(
            shapes(&first),
            vec!["end t1 repeated_tool \"PARTIAL\" None"]
        );
        assert_eq!(shapes(&second), vec!["end t1 repeated_tool \"FINAL\" None"]);
        assert!(table.flush("end_turn").is_empty());
    }

    #[test]
    fn a_call_is_announced_once_even_when_the_agent_repeats_it() {
        let mut table = ToolCallTable::default();
        let frame = json!({"toolCallId": "t1", "title": "Read", "name": "read"});
        let first = upsert_call(&mut table, frame.clone());
        let second = upsert_call(&mut table, frame);

        assert_eq!(shapes(&first), vec!["start t1 read"]);
        assert!(second.is_empty(), "got {second:?}");
    }

    /// A frame that changes nothing emits nothing. A frame that changes the
    /// arguments or the diff emits one update with the new canonical call.
    #[test]
    fn a_late_frame_for_an_announced_entry_emits_only_a_changed_call() {
        let mut table = ToolCallTable::default();
        let diff = json!({"type": "diff", "path": "/x.rs", "oldText": "a\n", "newText": "b\n"});
        upsert_call(
            &mut table,
            json!({
                "toolCallId": "t1",
                "title": "edit_file",
                "rawInput": {"path": "/x.rs"},
                "content": [diff],
            }),
        );

        let same = upsert_call(
            &mut table,
            json!({
                "toolCallId": "t1",
                "title": "edit_file",
                "rawInput": {"path": "/x.rs"},
                "content": [diff],
            }),
        );
        assert!(
            same.is_empty(),
            "unchanged values must not re-emit; got {same:?}"
        );

        let changed = upsert_call(
            &mut table,
            json!({
                "toolCallId": "t1",
                "title": "edit_file",
                "rawInput": {"path": "/y.rs"},
                "content": [{"type": "diff", "path": "/y.rs", "oldText": "a\n", "newText": "c\n"}],
            }),
        );
        assert_eq!(shapes(&changed), vec!["update t1"]);
        let TurnEvent::ToolCallUpdate { call, .. } = &changed[0] else {
            unreachable!()
        };
        assert_eq!(call.paths, ["/y.rs"]);
        assert_eq!(call.diffs.len(), 1);
        assert_eq!(call.diffs[0].new_content, "c\n");
    }

    /// The text of a completion is the result. It must not replace the diff
    /// that the call carries, or the card loses its diff when the call ends.
    #[test]
    fn the_text_of_a_completion_does_not_replace_the_diff_of_the_call() {
        let mut table = ToolCallTable::default();
        upsert_call(
            &mut table,
            json!({
                "toolCallId": "t1",
                "title": "Edit",
                "kind": "edit",
                "content": [{"type": "diff", "path": "/x.rs", "oldText": "a\n", "newText": "b\n"}],
            }),
        );
        let end = upsert_update(
            &mut table,
            update(json!({
                "toolCallId": "t1",
                "status": "completed",
                "content": [{"type": "content", "content": {"type": "text", "text": "done"}}],
            })),
        );
        assert_eq!(
            shapes(&end),
            vec!["end t1 file_edit \"done\" None"],
            "a completion with only text changes no field of the call"
        );
    }

    #[test]
    fn an_announced_call_with_no_completion_is_closed_at_flush() {
        let mut table = ToolCallTable::default();
        upsert_call(
            &mut table,
            json!({"toolCallId": "t1", "title": "Slow", "name": "slow_tool"}),
        );

        assert_eq!(
            shapes(&table.flush("cancelled")),
            vec!["end t1 slow_tool \"\" Some(\"turn ended: cancelled\")"]
        );
    }

    #[test]
    fn an_update_with_a_diff_and_no_title_waits_for_flush() {
        let mut table = ToolCallTable::default();
        let chunks = upsert_update(
            &mut table,
            update(json!({
                "toolCallId": "t1",
                "content": [{"type": "diff", "path": "/y.rs", "oldText": "a\n", "newText": "b\n"}],
            })),
        );
        assert!(chunks.is_empty(), "got {chunks:?}");

        let flushed = table.flush("end_turn");
        match &flushed[0] {
            TurnEvent::ToolCall {
                call: Some(call), ..
            } => {
                assert_eq!(call.tool, "file_edit");
                assert_eq!(call.diffs.len(), 1);
            }
            other => panic!("expected ToolCall, got {other:?}"),
        }
    }

    /// The permission request of the TypeScript codex adapter has no diff.
    /// The diff comes from the `tool_call` before it.
    #[test]
    fn a_request_with_no_diff_gets_the_diff_of_the_earlier_tool_call() {
        let mut table = ToolCallTable::default();
        upsert_call(
            &mut table,
            json!({
                "toolCallId": "p1",
                "title": "Editing files",
                "kind": "edit",
                "content": [{"type": "diff", "path": "/a.rs", "oldText": "a\n", "newText": "b\n"}],
            }),
        );

        let asked = table.permission_call(
            &update(json!({
                "toolCallId": "p1",
                "kind": "edit",
                "title": "Edit files",
                "locations": [{"path": "/a.rs"}],
            })),
            &[],
        );
        assert_eq!(asked.kind, "file_edit");
        assert_eq!(asked.diffs.len(), 1, "the diff joins by toolCallId");
        assert_eq!(asked.diffs[0].new_content, "b\n");
        assert_eq!(
            asked.raw.as_ref().and_then(|r| r.title.as_deref()),
            Some("Edit files"),
            "a field that the request sets wins"
        );

        let other = table.permission_call(&update(json!({"toolCallId": "p2"})), &[]);
        assert!(other.diffs.is_empty(), "another id joins nothing");
    }

    /// The old codex adapter names its MCP call only in the `tool_call`. Its
    /// approval request replaces `rawInput` and names nothing, so the request
    /// alone is a call of the kind `mcp_tool`. The more specific name of the
    /// earlier frame wins, so the prompt and the rules read `search_notes`.
    #[test]
    fn a_request_that_names_nothing_keeps_the_name_of_its_frame() {
        let keys: Vec<AgentKeys> = serde_json::from_value(json!([
            {"title": "Tool: ", "kind": "mcp_tool",
             "args": ["/rawInput/arguments"], "tool": ["/rawInput/tool"]},
            {"title": "Approve MCP tool call", "kind": "mcp_tool"},
        ]))
        .expect("the codex key table");
        let mut table = ToolCallTable::default();
        table.upsert_call(
            call(json!({
                "toolCallId": "call_mcp1",
                "title": "Tool: crucible/search_notes",
                "status": "in_progress",
                "rawInput": {"server": "crucible", "tool": "search_notes",
                             "arguments": {"query": "rust"}},
            })),
            &keys,
        );

        let asked = table.permission_call(
            &update(json!({
                "toolCallId": "call_mcp1",
                "status": "pending",
                "title": "Approve MCP tool call",
                "rawInput": {"server_name": "crucible", "id": "mcp_tool_call_approval_call_mcp1"},
            })),
            &keys,
        );
        assert_eq!(asked.kind, "mcp_tool");
        assert_eq!(asked.tool, "search_notes");
    }

    /// The Rust codex adapter asks before it sends the `tool_call`. The
    /// request starts the entry, the call merges into it, and the card
    /// comes once, from the call.
    #[test]
    fn a_request_before_its_tool_call_joins_the_same_entry() {
        let mut table = ToolCallTable::default();
        let asked = table.permission_call(
            &update(json!({
                "toolCallId": "s1",
                "kind": "execute",
                "title": "cargo test",
                "rawInput": {"command": ["/bin/zsh", "-lc", "cargo test"]},
            })),
            &[],
        );
        assert_eq!(asked.command.as_deref(), Some("cargo test"));

        let started = upsert_call(
            &mut table,
            json!({"toolCallId": "s1", "title": "cargo test", "kind": "execute"}),
        );
        assert_eq!(table.len(), 1, "one entry for one toolCallId");
        let [TurnEvent::ToolCall {
            call: Some(call), ..
        }] = started.as_slice()
        else {
            panic!("expected one ToolCall, got {started:?}")
        };
        assert_eq!(
            call.command.as_deref(),
            Some("cargo test"),
            "the call keeps what the request said"
        );
    }

    /// A request that the user rejects has no `tool_call` after it. It is not
    /// a call that the agent reported, so the turn shows no card for it.
    #[test]
    fn a_request_alone_gets_no_card() {
        let mut table = ToolCallTable::default();
        table.permission_call(
            &update(json!({"toolCallId": "r1", "title": "rm -rf /"})),
            &[],
        );

        assert!(table.flush("end_turn").is_empty());
        assert!(!table.announced_any());
    }

    /// A request after the frames asks about the joined call, and the card
    /// keeps what the frames said.
    #[test]
    fn a_request_after_the_frames_does_not_change_the_card() {
        let mut table = ToolCallTable::default();
        upsert_call(
            &mut table,
            json!({"toolCallId": "m1", "title": "Tool: srv/search", "rawInput": {"q": "x"}}),
        );
        table.permission_call(
            &update(json!({"toolCallId": "m1", "title": "Approve MCP tool call", "rawInput": {}})),
            &[],
        );
        let changed = upsert_call(
            &mut table,
            json!({"toolCallId": "m1", "title": "Tool: srv/search", "rawInput": {"q": "x"}}),
        );
        assert!(changed.is_empty(), "got {changed:?}");
    }

    fn held_ids(table: &ToolCallTable) -> Vec<String> {
        table
            .entries
            .iter()
            .filter(|e| e.held.as_ref().is_some_and(|h| h.error.is_none()))
            .map(|e| e.id.clone())
            .collect()
    }

    /// The count cap is not a memory bound on its own: 256 entries of the
    /// agent's own frame size is 256 times that size.
    #[test]
    fn held_results_are_bounded_by_bytes_not_only_by_count() {
        let mut table = ToolCallTable::default();
        let chunk = "x".repeat(MAX_HELD_RESULT_BYTES / 4);

        for i in 0..MAX_HELD_RESULTS {
            upsert_update(&mut table, completed(&format!("t{i}"), &chunk));
        }

        assert_eq!(held_ids(&table), vec!["t0", "t1", "t2", "t3"]);
    }

    #[test]
    fn the_count_cap_still_bounds_a_flood_of_tiny_results() {
        let mut table = ToolCallTable::default();
        for i in 0..MAX_HELD_RESULTS * 2 {
            upsert_update(&mut table, completed(&format!("t{i}"), "ok"));
        }
        assert_eq!(held_ids(&table).len(), MAX_HELD_RESULTS);
    }

    #[test]
    fn one_oversized_result_is_refused_outright() {
        let mut table = ToolCallTable::default();
        upsert_update(
            &mut table,
            completed("huge", &"x".repeat(MAX_HELD_RESULT_BYTES + 1)),
        );
        assert!(held_ids(&table).is_empty());
    }

    /// A structured result counts its JSON, not zero.
    #[test]
    fn an_oversized_structured_result_is_refused() {
        let mut table = ToolCallTable::default();
        let big = json!({"stdout": "x".repeat(MAX_HELD_RESULT_BYTES)});
        upsert_update(
            &mut table,
            update(json!({"toolCallId": "huge", "status": "completed", "rawOutput": big})),
        );
        assert!(held_ids(&table).is_empty());
    }

    /// Refusal is per entry. One oversized frame says nothing about the next.
    #[test]
    fn an_oversized_result_does_not_poison_the_ones_after_it() {
        let mut table = ToolCallTable::default();
        upsert_update(
            &mut table,
            completed("huge", &"x".repeat(u16::MAX as usize)),
        );
        upsert_update(
            &mut table,
            completed("big", &"x".repeat(MAX_HELD_RESULT_BYTES)),
        );
        upsert_update(&mut table, completed("small", "ok"));
        assert_eq!(held_ids(&table), vec!["huge", "small"]);
    }

    /// A refused payload still reaches the transcript as a failed result.
    #[test]
    fn a_refused_hold_is_reported_as_an_error_at_flush() {
        let mut table = ToolCallTable::default();
        upsert_update(
            &mut table,
            completed("huge", &"x".repeat(MAX_HELD_RESULT_BYTES + 1)),
        );

        let flushed = table.flush("end_turn");
        assert_eq!(
            shapes(&flushed)[1],
            format!("end huge tool \"\" Some({HELD_RESULT_DROPPED:?})")
        );
    }

    /// A later completion for one unnamed entry replaces the earlier one and
    /// gives its bytes back, so the cap counts what is held, not what was.
    #[test]
    fn a_repeated_held_completion_replaces_the_earlier_one() {
        let mut table = ToolCallTable::default();
        let half = "x".repeat(MAX_HELD_RESULT_BYTES / 2);
        upsert_update(&mut table, completed("t1", &half));
        upsert_update(&mut table, completed("t1", &half));
        upsert_update(&mut table, completed("t2", &half));

        assert_eq!(held_ids(&table), vec!["t1", "t2"]);
        assert_eq!(
            shapes(&table.flush("end_turn")).len(),
            4,
            "two entries, each announced once with one result"
        );
    }

    /// An announcement releases the held bytes it emits. Without the refund
    /// the cap counts results that already left the table, and a later real
    /// result is refused against a stale total.
    #[test]
    fn an_announcement_gives_the_held_bytes_back() {
        let mut table = ToolCallTable::default();
        let big = "x".repeat(MAX_HELD_RESULT_BYTES / 2 + 1);

        upsert_update(&mut table, completed("t1", &big));
        upsert_call(
            &mut table,
            json!({"toolCallId": "t1", "title": "late_call"}),
        );
        upsert_update(&mut table, completed("t2", &big));

        assert_eq!(held_ids(&table), vec!["t2"]);
    }

    /// A refused replacement removes the earlier partial result. Both
    /// renderers show the last result per call id, so a kept partial would
    /// read as final. The marker replaces it, and the partial's bytes come
    /// back so the next entry is not refused against them.
    #[test]
    fn a_refused_replacement_drops_the_partial_and_marks_the_entry() {
        let mut table = ToolCallTable::default();
        let big = "x".repeat(MAX_HELD_RESULT_BYTES / 2 + 1);

        upsert_update(&mut table, completed("t1", &big));
        upsert_update(
            &mut table,
            completed("t1", &"x".repeat(MAX_HELD_RESULT_BYTES + 1)),
        );
        upsert_update(&mut table, completed("t2", &big));

        assert_eq!(held_ids(&table), vec!["t2"]);
        let flushed = table.flush("end_turn");
        assert_eq!(
            shapes(&flushed)[1],
            format!("end t1 tool \"\" Some({HELD_RESULT_DROPPED:?})")
        );
    }

    /// A turn that ends without a `flush` reports each held result once.
    /// `Drop` calls this, so the agent-error return and the timeout return
    /// leave a warning per lost payload instead of silence.
    #[test]
    fn a_table_dropped_mid_turn_reports_each_held_result_once() {
        let mut table = ToolCallTable::default();
        upsert_update(&mut table, completed("t1", "one"));
        upsert_update(&mut table, completed("t2", "two"));

        assert_eq!(table.log_dropped(), 2);
        assert_eq!(table.log_dropped(), 0, "a second pass reports nothing");
    }

    /// A flushed table holds nothing, so the `Drop` of a normal turn is
    /// silent.
    #[test]
    fn a_flushed_table_has_nothing_left_to_report() {
        let mut table = ToolCallTable::default();
        upsert_update(&mut table, completed("t1", "one"));
        table.flush("end_turn");

        assert_eq!(table.log_dropped(), 0);
    }

    #[test]
    fn flush_announces_entries_in_first_seen_order() {
        let mut table = ToolCallTable::default();
        upsert_update(&mut table, completed("b", "1"));
        upsert_update(&mut table, completed("a", "2"));

        let ids: Vec<String> = table
            .flush("end_turn")
            .into_iter()
            .filter_map(|c| match c {
                TurnEvent::ToolCall { id, .. } => Some(id),
                _ => None,
            })
            .collect();
        assert_eq!(ids, vec!["b", "a"]);
    }
}
