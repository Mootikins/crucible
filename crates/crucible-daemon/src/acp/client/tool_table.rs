//! One upsert table per turn for the tool calls an ACP agent reports.
//!
//! The ACP spec gives no order between `tool_call` and `tool_call_update`.
//! Many agents send only updates, and some send a completed update before the
//! call that names it. So the client keeps one entry per `toolCallId` and
//! merges every frame into it. The entry decides what the turn stream sees:
//!
//! - An entry is announced at most once, at the first frame that gives it a
//!   name. The TUI cannot rename a card, so a second announcement would show
//!   two cards for one call.
//! - A completion for an entry with no name is held until the name arrives,
//!   or until the turn ends. Then the entry is announced under a fixed
//!   placeholder label, as Zed does, and the held result follows it.
//! - At the end of the turn, an announced entry with no completion gets a
//!   failed result, so a card does not stay open forever.

use agent_client_protocol::schema::v1::{ToolCall, ToolCallStatus, ToolCallUpdate};
use serde_json::Value;

use super::CrucibleAcpClient;
use crate::acp::streaming::{humanize_tool_title, StreamingChunk};
use crucible_core::text::sanitize_single_line;
use crucible_core::types::acp::FileDiff;
#[cfg(test)]
use crucible_core::types::acp::ToolCallInfo;

/// The name of a call that no frame named before the turn ended.
pub(super) const PLACEHOLDER_TOOL_NAME: &str = "Unnamed tool";

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
    result: Option<String>,
    error: Option<String>,
}

impl HeldResult {
    fn bytes(&self) -> usize {
        let len = |s: &Option<String>| s.as_ref().map_or(0, String::len);
        len(&self.result) + len(&self.error)
    }
}

#[derive(Debug)]
struct Entry {
    id: String,
    /// The sanitized wire title. `None` until a frame carries one.
    title: Option<String>,
    args: Option<Value>,
    diffs: Vec<FileDiff>,
    announced: bool,
    completions: u32,
    held: Option<HeldResult>,
}

impl Entry {
    fn new(id: String) -> Self {
        Self {
            id,
            title: None,
            args: None,
            diffs: Vec::new(),
            announced: false,
            completions: 0,
            held: None,
        }
    }

    fn name(&self) -> String {
        self.title
            .as_deref()
            .map(humanize_tool_title)
            .unwrap_or_else(|| PLACEHOLDER_TOOL_NAME.to_string())
    }

    /// Emit `ToolStart`, and then the held completion when there is one.
    fn announce(&mut self, out: &mut Vec<StreamingChunk>) {
        self.announced = true;
        out.push(StreamingChunk::ToolStart {
            name: self.name(),
            id: self.id.clone(),
            arguments: self.args.clone(),
            diffs: self.diffs.clone(),
        });
        if let Some(held) = self.held.take() {
            self.completions += 1;
            out.push(StreamingChunk::ToolEnd {
                id: self.id.clone(),
                name: self.name(),
                result: held.result,
                error: held.error,
            });
        }
    }

    /// Merge new arguments; an announced entry reports a change.
    fn merge_args(&mut self, args: Option<Value>, out: &mut Vec<StreamingChunk>) {
        let Some(args) = args else { return };
        if self.announced && self.args.as_ref() != Some(&args) {
            out.push(StreamingChunk::ToolArgsUpdate {
                call_id: self.id.clone(),
                arguments: args.clone(),
            });
        }
        self.args = Some(args);
    }

    /// Merge new diffs; an announced entry reports a change. An empty list
    /// says nothing about the diffs, so it does not clear the ones held.
    fn merge_diffs(&mut self, diffs: Vec<FileDiff>, out: &mut Vec<StreamingChunk>) {
        if diffs.is_empty() {
            return;
        }
        if self.announced && self.diffs != diffs {
            out.push(StreamingChunk::ToolDiffUpdate {
                call_id: self.id.clone(),
                diffs: diffs.clone(),
            });
        }
        self.diffs = diffs;
    }
}

/// The tool calls of one turn, keyed by `toolCallId`, in first-seen order.
#[derive(Debug, Default)]
pub(super) struct ToolCallTable {
    entries: Vec<Entry>,
    held_bytes: usize,
}

impl ToolCallTable {
    fn entry_mut(&mut self, id: &str) -> &mut Entry {
        let index = match self.entries.iter().position(|entry| entry.id == id) {
            Some(index) => index,
            None => {
                self.entries.push(Entry::new(id.to_string()));
                self.entries.len() - 1
            }
        };
        &mut self.entries[index]
    }

    fn held_count(&self) -> usize {
        self.entries.iter().filter(|e| e.held.is_some()).count()
    }

    /// Merge a `tool_call` frame. The chunks it returns go to the callback.
    pub(super) fn upsert_call(&mut self, call: ToolCall) -> Vec<StreamingChunk> {
        let mut out = Vec::new();
        // A title is a one-line label that names what the agent does, so it
        // gets the single-line form: a newline or a bidi override in it makes
        // the card claim one action and perform another.
        let title = sanitize_single_line(&call.title);
        let diffs = diffs_from_content(call.content.iter());
        let id = call.tool_call_id.to_string();
        let entry = self.entry_mut(&id);

        if entry.announced {
            entry.merge_args(call.raw_input, &mut out);
            entry.merge_diffs(diffs, &mut out);
            return out;
        }

        // The agent's own `tool_call` names the call. A title that an update
        // set before it loses, because the call is the primary source.
        entry.title = Some(title);
        if call.raw_input.is_some() {
            entry.args = call.raw_input;
        }
        if !diffs.is_empty() {
            entry.diffs = diffs;
        }
        entry.announce(&mut out);
        out
    }

    /// Merge a `tool_call_update` frame. The chunks it returns go to the
    /// callback.
    pub(super) fn upsert_update(&mut self, update: ToolCallUpdate) -> Vec<StreamingChunk> {
        let mut out = Vec::new();
        let id = update.tool_call_id.to_string();
        let fields = update.fields;
        let diffs = diffs_from_content(fields.content.iter().flatten());
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

        let held_count = self.held_count();
        let mut held_bytes = self.held_bytes;
        let entry = self.entry_mut(&id);

        if let Some(title) = fields.title.as_deref() {
            if entry.title.is_none() {
                entry.title = Some(sanitize_single_line(title));
            }
        }
        entry.merge_args(fields.raw_input, &mut out);
        entry.merge_diffs(diffs, &mut out);

        if !entry.announced && entry.title.is_some() {
            entry.announce(&mut out);
        }

        if let Some(completion) = completion {
            if entry.announced {
                entry.completions += 1;
                out.push(StreamingChunk::ToolEnd {
                    id: entry.id.clone(),
                    name: entry.name(),
                    result: completion.result,
                    error: completion.error,
                });
            } else {
                hold(entry, completion, held_count, &mut held_bytes);
            }
        }
        self.held_bytes = held_bytes;
        out
    }

    /// Close the turn. Every unannounced entry is announced under the
    /// placeholder label, and every entry with no completion gets a failed
    /// result that names the stop reason.
    pub(super) fn flush(&mut self, stop_reason: &str) -> Vec<StreamingChunk> {
        let mut out = Vec::new();
        for entry in &mut self.entries {
            if !entry.announced {
                tracing::debug!(
                    tool_id = %entry.id,
                    "ACP never named this tool call; announcing it under the placeholder"
                );
                entry.announce(&mut out);
            }
            if entry.completions == 0 {
                entry.completions += 1;
                out.push(StreamingChunk::ToolEnd {
                    id: entry.id.clone(),
                    name: entry.name(),
                    result: None,
                    error: Some(format!("turn ended: {stop_reason}")),
                });
            }
        }
        self.held_bytes = 0;
        out
    }

    /// Whether the turn announced at least one call. After `flush` this is
    /// true for every non-empty table.
    pub(super) fn announced_any(&self) -> bool {
        self.entries.iter().any(|entry| entry.announced)
    }

    /// The calls the turn reported, for tests that inspect the table.
    #[cfg(test)]
    pub(super) fn to_tool_call_infos(&self) -> Vec<ToolCallInfo> {
        self.entries
            .iter()
            .map(|entry| {
                let mut info = ToolCallInfo::new(
                    entry
                        .title
                        .clone()
                        .unwrap_or_else(|| PLACEHOLDER_TOOL_NAME.to_string()),
                )
                .with_id(entry.id.clone())
                .with_diffs(entry.diffs.clone());
                if let Some(args) = entry.args.clone() {
                    info = info.with_arguments(args);
                }
                info
            })
            .collect()
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
            held = held_count,
            held_bytes = *held_bytes,
            entry_bytes = completion.bytes(),
            count_cap = MAX_HELD_RESULTS,
            byte_cap = MAX_HELD_RESULT_BYTES,
            "ACP held tool results hit their cap; dropping this payload \
             (lost from session.jsonl, recordings and Lua handlers)"
        );
        if entry.held.is_none() {
            entry.held = Some(HeldResult {
                result: None,
                error: Some(HELD_RESULT_DROPPED.to_string()),
            });
            *held_bytes += HELD_RESULT_DROPPED.len();
        }
        return;
    }

    *held_bytes = new_bytes;
    entry.held = Some(completion);
}

/// The file diffs in a tool call's content, with oversize diffs dropped.
///
/// Both the `tool_call` frame and the `tool_call_update` frame carry diffs in
/// the same `ToolCallContent::Diff` shape, so both read them here.
fn diffs_from_content<'a>(
    content: impl Iterator<Item = &'a agent_client_protocol::schema::v1::ToolCallContent>,
) -> Vec<FileDiff> {
    use agent_client_protocol::schema::v1::ToolCallContent;
    content
        .filter_map(|c| match c {
            ToolCallContent::Diff(diff) => Some(FileDiff::from_contents(
                diff.path.to_string_lossy().to_string(),
                diff.old_text.clone(),
                diff.new_text.clone(),
            )),
            _ => None,
        })
        .filter(|d| {
            if d.is_oversize() {
                tracing::debug!(
                    path = %d.path,
                    "ACP-supplied diff exceeded MAX_DIFF_BYTES; dropping at edge"
                );
                false
            } else {
                true
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

    fn shapes(chunks: &[StreamingChunk]) -> Vec<String> {
        chunks
            .iter()
            .map(|c| match c {
                StreamingChunk::ToolStart { name, id, .. } => format!("start {id} {name}"),
                StreamingChunk::ToolEnd {
                    id,
                    name,
                    result,
                    error,
                } => format!("end {id} {name} {result:?} {error:?}"),
                StreamingChunk::ToolArgsUpdate { call_id, .. } => format!("args {call_id}"),
                StreamingChunk::ToolDiffUpdate { call_id, .. } => format!("diffs {call_id}"),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn update_before_call_is_named_by_the_call() {
        let mut table = ToolCallTable::default();

        let first = table.upsert_update(completed("t1", "four"));
        assert!(
            first.is_empty(),
            "an unnamed completion must wait; got {first:?}"
        );

        let second = table.upsert_call(call(json!({
            "toolCallId": "t1",
            "title": "late_call",
            "rawInput": {"q": "2+2"},
        })));
        assert_eq!(
            shapes(&second),
            vec!["start t1 Late Call", "end t1 Late Call Some(\"four\") None"]
        );
        assert!(table.flush("end_turn").is_empty());
    }

    #[test]
    fn update_with_title_for_unseen_id_announces_it() {
        let mut table = ToolCallTable::default();

        let chunks = table.upsert_update(update(json!({
            "toolCallId": "t1",
            "title": "late_named_tool",
            "status": "completed",
            "rawOutput": "done",
        })));
        assert_eq!(
            shapes(&chunks),
            vec![
                "start t1 Late Named Tool",
                "end t1 Late Named Tool Some(\"done\") None"
            ]
        );
    }

    #[test]
    fn bare_update_for_unseen_id_is_announced_at_flush_under_the_placeholder() {
        let mut table = ToolCallTable::default();

        assert!(table
            .upsert_update(completed("t1", "orphaned output"))
            .is_empty());

        assert_eq!(
            shapes(&table.flush("end_turn")),
            vec![
                format!("start t1 {PLACEHOLDER_TOOL_NAME}"),
                format!("end t1 {PLACEHOLDER_TOOL_NAME} Some(\"orphaned output\") None"),
            ]
        );
    }

    #[test]
    fn repeat_completion_keeps_the_name() {
        let mut table = ToolCallTable::default();
        table.upsert_call(call(json!({"toolCallId": "t1", "title": "repeated_tool"})));

        let first = table.upsert_update(completed("t1", "PARTIAL"));
        let second = table.upsert_update(completed("t1", "FINAL"));

        assert_eq!(
            shapes(&first),
            vec!["end t1 Repeated Tool Some(\"PARTIAL\") None"]
        );
        assert_eq!(
            shapes(&second),
            vec!["end t1 Repeated Tool Some(\"FINAL\") None"]
        );
        assert!(table.flush("end_turn").is_empty());
    }

    #[test]
    fn a_call_is_announced_once_even_when_the_agent_repeats_it() {
        let mut table = ToolCallTable::default();
        let first = table.upsert_call(call(json!({"toolCallId": "t1", "title": "read"})));
        let second = table.upsert_call(call(json!({"toolCallId": "t1", "title": "read"})));

        assert_eq!(shapes(&first), vec!["start t1 Read"]);
        assert!(second.is_empty(), "got {second:?}");
    }

    #[test]
    fn late_call_for_an_announced_entry_emits_only_changed_args_and_diffs() {
        let mut table = ToolCallTable::default();
        let diff = json!({"type": "diff", "path": "/x.rs", "oldText": "a\n", "newText": "b\n"});
        table.upsert_update(update(json!({
            "toolCallId": "t1",
            "title": "edit_file",
            "rawInput": {"path": "/x.rs"},
            "content": [diff],
        })));

        let same = table.upsert_call(call(json!({
            "toolCallId": "t1",
            "title": "edit_file",
            "rawInput": {"path": "/x.rs"},
            "content": [diff],
        })));
        assert!(
            same.is_empty(),
            "unchanged values must not re-emit; got {same:?}"
        );

        let changed = table.upsert_call(call(json!({
            "toolCallId": "t1",
            "title": "edit_file",
            "rawInput": {"path": "/y.rs"},
            "content": [{"type": "diff", "path": "/y.rs", "oldText": "a\n", "newText": "c\n"}],
        })));
        assert_eq!(shapes(&changed), vec!["args t1", "diffs t1"]);
    }

    #[test]
    fn an_announced_call_with_no_completion_is_closed_at_flush() {
        let mut table = ToolCallTable::default();
        table.upsert_call(call(json!({"toolCallId": "t1", "title": "slow_tool"})));

        assert_eq!(
            shapes(&table.flush("cancelled")),
            vec!["end t1 Slow Tool None Some(\"turn ended: cancelled\")"]
        );
    }

    #[test]
    fn an_update_with_a_diff_and_no_title_waits_for_flush() {
        let mut table = ToolCallTable::default();
        let chunks = table.upsert_update(update(json!({
            "toolCallId": "t1",
            "content": [{"type": "diff", "path": "/y.rs", "oldText": "a\n", "newText": "b\n"}],
        })));
        assert!(chunks.is_empty(), "got {chunks:?}");

        let flushed = table.flush("end_turn");
        match &flushed[0] {
            StreamingChunk::ToolStart { name, diffs, .. } => {
                assert_eq!(name, PLACEHOLDER_TOOL_NAME);
                assert_eq!(diffs.len(), 1);
            }
            other => panic!("expected ToolStart, got {other:?}"),
        }
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
            table.upsert_update(completed(&format!("t{i}"), &chunk));
        }

        assert_eq!(held_ids(&table), vec!["t0", "t1", "t2", "t3"]);
    }

    #[test]
    fn the_count_cap_still_bounds_a_flood_of_tiny_results() {
        let mut table = ToolCallTable::default();
        for i in 0..MAX_HELD_RESULTS * 2 {
            table.upsert_update(completed(&format!("t{i}"), "ok"));
        }
        assert_eq!(held_ids(&table).len(), MAX_HELD_RESULTS);
    }

    #[test]
    fn one_oversized_result_is_refused_outright() {
        let mut table = ToolCallTable::default();
        table.upsert_update(completed("huge", &"x".repeat(MAX_HELD_RESULT_BYTES + 1)));
        assert!(held_ids(&table).is_empty());
    }

    /// Refusal is per entry. One oversized frame says nothing about the next.
    #[test]
    fn an_oversized_result_does_not_poison_the_ones_after_it() {
        let mut table = ToolCallTable::default();
        table.upsert_update(completed("huge", &"x".repeat(u16::MAX as usize)));
        table.upsert_update(completed("big", &"x".repeat(MAX_HELD_RESULT_BYTES)));
        table.upsert_update(completed("small", "ok"));
        assert_eq!(held_ids(&table), vec!["huge", "small"]);
    }

    /// A refused payload still reaches the transcript as a failed result.
    #[test]
    fn a_refused_hold_is_reported_as_an_error_at_flush() {
        let mut table = ToolCallTable::default();
        table.upsert_update(completed("huge", &"x".repeat(MAX_HELD_RESULT_BYTES + 1)));

        let flushed = table.flush("end_turn");
        assert_eq!(
            shapes(&flushed)[1],
            format!("end huge {PLACEHOLDER_TOOL_NAME} None Some({HELD_RESULT_DROPPED:?})")
        );
    }

    /// A later completion for one unnamed entry replaces the earlier one and
    /// gives its bytes back, so the cap counts what is held, not what was.
    #[test]
    fn a_repeated_held_completion_replaces_the_earlier_one() {
        let mut table = ToolCallTable::default();
        let half = "x".repeat(MAX_HELD_RESULT_BYTES / 2);
        table.upsert_update(completed("t1", &half));
        table.upsert_update(completed("t1", &half));
        table.upsert_update(completed("t2", &half));

        assert_eq!(held_ids(&table), vec!["t1", "t2"]);
        assert_eq!(
            shapes(&table.flush("end_turn")).len(),
            4,
            "two entries, each announced once with one result"
        );
    }

    #[test]
    fn flush_announces_entries_in_first_seen_order() {
        let mut table = ToolCallTable::default();
        table.upsert_update(completed("b", "1"));
        table.upsert_update(completed("a", "2"));

        let ids: Vec<String> = table
            .flush("end_turn")
            .into_iter()
            .filter_map(|c| match c {
                StreamingChunk::ToolStart { id, .. } => Some(id),
                _ => None,
            })
            .collect();
        assert_eq!(ids, vec!["b", "a"]);
    }
}
