//! The TUI draws the same rows of each golden transcript as the web client
//! and `cru acp`.
//!
//! The core test `each_golden_transcript_has_its_client_rows` writes the rows
//! of each file in `assets/fixtures/golden/transcript/`. The web test and the
//! ACP test compare with the same rows. See the core test for the rules.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crucible_core::transcript::{ItemBody, Transcript};

use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::containers::ChatNode;

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fixtures/golden/transcript")
}

/// The first 32 letters and digits of a text, as the core test counts them.
fn words(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .take(32)
        .collect()
}

/// The rows that the container list draws, in order.
fn drawn_rows(app: &OilChatApp, transcript: &Transcript) -> Vec<String> {
    let list = app.container_list();
    // The notes of a turn draw as a system line under it. The web client
    // draws them inside the prompt, so they are not a row.
    let notes: HashSet<usize> = transcript
        .items
        .iter()
        .filter(|item| matches!(item.body, ItemBody::UserTurn { .. }))
        .filter_map(|item| list.item_node(&format!("{}-precognition", item.id)))
        .collect();
    let mut rows = Vec::new();
    for (index, node) in list.nodes().iter().enumerate() {
        match node {
            ChatNode::UserMessage { text } => rows.push(format!("user:{text}")),
            ChatNode::AssistantResponse { text, thinking, .. } => {
                if !text.trim().is_empty() || !thinking.is_empty() {
                    rows.push(format!("segment:{}", words(text)));
                }
            }
            ChatNode::ToolGroup { tools } => rows.extend(
                tools
                    .iter()
                    .filter_map(|tool| tool.call_id.as_ref())
                    .map(|id| format!("tool:{id}")),
            ),
            ChatNode::SystemMessage { .. } if !notes.contains(&index) => {
                rows.push("notice".to_string())
            }
            // The finish row of a slow tool, a delegation and a shell run
            // are not rows of the transcript.
            ChatNode::SystemMessage { .. }
            | ChatNode::BackgroundToolFinished { .. }
            | ChatNode::SubagentTask { .. }
            | ChatNode::ShellExecution { .. } => {}
        }
    }
    rows
}

#[test]
fn the_tui_draws_the_rows_of_each_golden_transcript() {
    let mut names: Vec<_> = std::fs::read_dir(golden_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    names.sort();
    assert!(!names.is_empty());
    for path in names {
        let transcript: Transcript =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let rows_path = golden_dir().join("rows").join(path.file_name().unwrap());
        let expected: Vec<String> =
            serde_json::from_str(&std::fs::read_to_string(&rows_path).unwrap()).unwrap();

        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::TranscriptLoaded(transcript.clone()));

        assert_eq!(
            drawn_rows(&app, &transcript),
            expected,
            "{}",
            path.display()
        );
    }
}
