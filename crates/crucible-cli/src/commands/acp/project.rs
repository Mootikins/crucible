//! The folded transcript of a session, as ACP `session/update` messages.
//!
//! The daemon folds each session once (`crucible_core::transcript`) and sends
//! the ops of the fold with each live event. An ACP host draws its own
//! transcript from updates that only add: a text chunk, a thought chunk, a
//! tool call, a tool call update. [`HostProjection`] turns the items and ops
//! into those updates. It keeps what the host already has, so an item that
//! comes again (an upsert with the whole text) sends only the new part.

use std::collections::{HashMap, HashSet};

use agent_client_protocol::schema::v1::{
    ContentBlock, ContentChunk, SessionUpdate, TextContent, ToolCall, ToolCallContent,
    ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields,
};
use crucible_core::transcript::{
    ItemBody, TextField, ToolStatus, Transcript, TranscriptItem, TranscriptOp,
};

use super::translate::describe;

/// What one ACP host already has of one session's transcript.
#[derive(Debug, Default)]
pub struct HostProjection {
    /// The text and the reasoning of each segment that the host has, by
    /// item id.
    sent: HashMap<String, (String, String)>,
    /// The tool cards that the host has, by item id.
    announced: HashSet<String>,
    /// The tool cards that the host has with their end.
    finished: HashSet<String>,
}

impl HostProjection {
    /// The updates of a whole transcript, for `session/load`. A host keeps
    /// no transcript across restarts, so the user's prompts go too.
    pub fn snapshot(&mut self, transcript: &Transcript) -> Vec<SessionUpdate> {
        let mut updates = Vec::new();
        for item in &transcript.items {
            if let ItemBody::UserTurn {
                content, origin, ..
            } = &item.body
            {
                if content.is_empty() {
                    continue;
                }
                // ACP has no system chunk, so a plugin turn carries the
                // TUI's `↻` label.
                let text = match origin.as_ref().and_then(|o| o.plugin()) {
                    Some(plugin) => format!("↻ {plugin}\n{content}"),
                    None => content.clone(),
                };
                updates.push(SessionUpdate::UserMessageChunk(chunk(text)));
                continue;
            }
            self.item(item, &mut updates);
        }
        updates
    }

    /// The updates of the ops of one live event. The host drew the user's
    /// prompt when it sent it, so a user turn sends nothing here.
    pub fn ops(&mut self, ops: &[TranscriptOp]) -> Vec<SessionUpdate> {
        let mut updates = Vec::new();
        for op in ops {
            match op {
                TranscriptOp::Upsert { item, .. } => self.item(item, &mut updates),
                TranscriptOp::Append {
                    id, field, text, ..
                } => {
                    let sent = self.sent.entry(id.clone()).or_default();
                    match field {
                        TextField::Text => {
                            sent.0.push_str(text);
                            updates.push(SessionUpdate::AgentMessageChunk(chunk(text.clone())));
                        }
                        TextField::Thinking => {
                            sent.1.push_str(text);
                            updates.push(SessionUpdate::AgentThoughtChunk(chunk(text.clone())));
                        }
                    }
                }
            }
        }
        updates
    }

    fn item(&mut self, item: &TranscriptItem, updates: &mut Vec<SessionUpdate>) {
        match &item.body {
            ItemBody::AssistantSegment { text, thinking, .. } => {
                let sent = self.sent.entry(item.id.clone()).or_default();
                if let Some(new) = unsent(&sent.1, thinking) {
                    updates.push(SessionUpdate::AgentThoughtChunk(chunk(new.to_string())));
                    sent.1 = thinking.clone();
                }
                if let Some(new) = unsent(&sent.0, text) {
                    updates.push(SessionUpdate::AgentMessageChunk(chunk(new.to_string())));
                    sent.0 = text.clone();
                }
            }
            ItemBody::ToolCard {
                call_id,
                name,
                args,
                display,
                status,
                result,
                error,
                ..
            } => {
                let (title, kind) = describe(name, display.as_deref());
                if self.announced.insert(item.id.clone()) {
                    let mut call = ToolCall::new(call_id.clone(), title.clone())
                        .kind(kind)
                        .status(ToolCallStatus::InProgress);
                    if !args.is_null() {
                        call = call.raw_input(args.clone());
                    }
                    updates.push(SessionUpdate::ToolCall(call));
                } else if matches!(status, ToolStatus::Running) {
                    // A later frame of a call: new arguments or a new line.
                    let mut fields = ToolCallUpdateFields::new().title(title.clone());
                    if !args.is_null() {
                        fields = fields.raw_input(args.clone());
                    }
                    updates.push(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                        call_id.clone(),
                        fields,
                    )));
                }
                if matches!(status, ToolStatus::Running) || !self.finished.insert(item.id.clone()) {
                    return;
                }
                let (status, text) = match status {
                    ToolStatus::Complete => (
                        ToolCallStatus::Completed,
                        result.clone().unwrap_or_default(),
                    ),
                    ToolStatus::Failed => {
                        (ToolCallStatus::Failed, error.clone().unwrap_or_default())
                    }
                    ToolStatus::Incomplete => (
                        ToolCallStatus::Failed,
                        "the tool did not complete".to_string(),
                    ),
                    ToolStatus::Running => return,
                };
                let mut fields = ToolCallUpdateFields::new().status(status).content(vec![
                    ToolCallContent::from(ContentBlock::Text(TextContent::new(text))),
                ]);
                // The render of the finished call gives the title its summary.
                if let Some(summary) = display
                    .as_ref()
                    .and_then(|d| d.render.as_ref())
                    .and_then(|r| r.summary.clone())
                {
                    fields = fields.title(format!("{title} → {summary}"));
                }
                updates.push(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                    call_id.clone(),
                    fields,
                )));
            }
            // ACP has no update for these: the host draws the turns, the
            // answers and the tools.
            ItemBody::UserTurn { .. }
            | ItemBody::Delegation { .. }
            | ItemBody::InjectedContext { .. }
            | ItemBody::Notice { .. } => {}
        }
    }
}

/// The part of `now` after `sent`, when `now` continues it and adds text.
/// Text that does not continue what the host has cannot go: an update only
/// adds.
fn unsent<'a>(sent: &str, now: &'a str) -> Option<&'a str> {
    now.strip_prefix(sent).filter(|rest| !rest.is_empty())
}

fn chunk(text: String) -> ContentChunk {
    ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::ToolKind;
    use crucible_core::protocol::SessionEventMessage;
    use crucible_core::transcript::TranscriptFold;
    use serde_json::json;

    /// The updates that `events` send to a host, live: the daemon folds each
    /// event, and the projection turns its ops into updates.
    fn live(events: &[(&str, serde_json::Value)]) -> Vec<SessionUpdate> {
        let mut fold = TranscriptFold::new();
        let mut host = HostProjection::default();
        events
            .iter()
            .flat_map(|(name, data)| {
                let ops = fold.apply(&SessionEventMessage::new("s", *name, data.clone()));
                host.ops(&ops)
            })
            .collect()
    }

    fn text(update: &SessionUpdate) -> Option<(&'static str, String)> {
        let (kind, chunk) = match update {
            SessionUpdate::AgentMessageChunk(c) => ("answer", c),
            SessionUpdate::AgentThoughtChunk(c) => ("thought", c),
            SessionUpdate::UserMessageChunk(c) => ("user", c),
            _ => return None,
        };
        match &chunk.content {
            ContentBlock::Text(t) => Some((kind, t.text.clone())),
            _ => None,
        }
    }

    #[test]
    fn the_answer_and_its_reasoning_stream_as_chunks() {
        let updates = live(&[
            (
                "user_message",
                json!({ "message_id": "m1", "content": "q" }),
            ),
            ("thinking", json!({ "content": "look" })),
            ("text_delta", json!({ "content": "hel" })),
            ("text_delta", json!({ "content": "lo" })),
            (
                "message_complete",
                json!({ "message_id": "m1", "full_response": "hello" }),
            ),
        ]);
        let texts: Vec<_> = updates.iter().filter_map(text).collect();
        assert_eq!(
            texts,
            [
                ("thought", "look".to_string()),
                ("answer", "hel".to_string()),
                ("answer", "lo".to_string()),
            ],
            "the user's own prompt and the closing upsert send nothing new"
        );
    }

    #[test]
    fn a_tool_call_starts_in_progress_and_its_result_completes_it() {
        let updates = live(&[
            (
                "tool_call",
                json!({
                    "call_id": "c1", "tool": "read_file", "args": { "path": "a.rs" },
                    "display": { "kind": "file_read", "tool": "read_file", "render": { "line": "a.rs" } },
                }),
            ),
            (
                "tool_result",
                json!({
                    "call_id": "c1", "tool": "read_file",
                    "result": { "result": "fn a() {}", "render": { "line": "a.rs", "summary": "1 line" } },
                }),
            ),
        ]);
        let SessionUpdate::ToolCall(call) = &updates[0] else {
            panic!("expected a tool call: {updates:?}");
        };
        assert_eq!(call.status, ToolCallStatus::InProgress);
        assert_eq!(call.kind, ToolKind::Read);
        assert_eq!(call.title, "read file: a.rs");
        assert_eq!(call.raw_input, Some(json!({ "path": "a.rs" })));
        let SessionUpdate::ToolCallUpdate(done) = &updates[1] else {
            panic!("expected a tool call update: {updates:?}");
        };
        assert_eq!(done.fields.status, Some(ToolCallStatus::Completed));
        assert_eq!(
            done.fields.title.as_deref(),
            Some("read file: a.rs → 1 line")
        );
        assert_eq!(updates.len(), 2);
    }

    #[test]
    fn a_failed_tool_fails_and_a_call_with_no_canonical_form_is_other() {
        let updates = live(&[
            (
                "tool_call",
                json!({ "call_id": "c1", "tool": "bash", "args": {} }),
            ),
            (
                "tool_result",
                json!({ "call_id": "c1", "tool": "bash", "result": { "error": "Error: no" } }),
            ),
        ]);
        let SessionUpdate::ToolCall(call) = &updates[0] else {
            panic!("{updates:?}");
        };
        assert_eq!(call.kind, ToolKind::Other);
        let SessionUpdate::ToolCallUpdate(failed) = &updates[1] else {
            panic!("{updates:?}");
        };
        assert_eq!(failed.fields.status, Some(ToolCallStatus::Failed));
    }

    /// A host draws the same rows of each golden transcript as the TUI and the
    /// web client. The core test `each_golden_transcript_has_its_client_rows`
    /// writes the rows. ACP has no update for a notice, so a host draws none.
    #[test]
    fn a_load_draws_the_rows_of_each_golden_transcript() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/fixtures/golden/transcript");
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        names.sort();
        assert!(!names.is_empty());
        let words = |text: &str| -> String {
            text.chars()
                .filter(|c| c.is_alphanumeric())
                .take(32)
                .collect()
        };
        for path in names {
            let transcript: Transcript =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let rows_path = dir.join("rows").join(path.file_name().unwrap());
            let expected: Vec<String> =
                serde_json::from_str::<Vec<String>>(&std::fs::read_to_string(&rows_path).unwrap())
                    .unwrap()
                    .into_iter()
                    .filter(|row| row != "notice")
                    .collect();

            // A thought chunk opens a segment, and the answer chunk after it
            // gives the segment its words.
            let mut rows: Vec<String> = Vec::new();
            let mut open = false;
            for update in HostProjection::default().snapshot(&transcript) {
                match (&update, text(&update)) {
                    (_, Some(("user", content))) => rows.push(format!("user:{content}")),
                    (_, Some(("thought", _))) => {
                        rows.push("segment:".to_string());
                        open = true;
                    }
                    (_, Some(("answer", content))) => {
                        if open {
                            rows.pop();
                        }
                        rows.push(format!("segment:{}", words(&content)));
                        open = false;
                    }
                    (SessionUpdate::ToolCall(call), _) => {
                        rows.push(format!("tool:{}", call.tool_call_id.0));
                        open = false;
                    }
                    _ => {}
                }
            }
            assert_eq!(rows, expected, "{}", path.display());
        }
    }

    /// A load replays the prompts too, labels a plugin turn, and skips an
    /// empty prompt. A later upsert of the same segment sends nothing twice.
    #[test]
    fn a_snapshot_replays_the_prompts_and_the_answers_once() {
        let transcript = TranscriptFold::of_events(&[
            SessionEventMessage::new(
                "s",
                "user_message",
                json!({ "message_id": "m1", "content": "q" }),
            ),
            SessionEventMessage::new(
                "s",
                "message_complete",
                json!({ "message_id": "m1", "full_response": "a" }),
            ),
            SessionEventMessage::new(
                "s",
                "user_message",
                json!({
                    "message_id": "m2", "content": "go on", "origin": { "kind": "plugin", "name": "goal" }
                }),
            ),
            SessionEventMessage::new(
                "s",
                "user_message",
                json!({ "message_id": "m3", "content": "" }),
            ),
        ]);
        let mut host = HostProjection::default();
        let texts: Vec<_> = host.snapshot(&transcript).iter().filter_map(text).collect();
        assert_eq!(
            texts,
            [
                ("user", "q".to_string()),
                ("answer", "a".to_string()),
                ("user", "↻ goal\ngo on".to_string()),
            ]
        );
        // The same items again add nothing.
        assert!(host
            .snapshot(&transcript)
            .iter()
            .all(|u| !matches!(u, SessionUpdate::AgentMessageChunk(_))));
    }
}
