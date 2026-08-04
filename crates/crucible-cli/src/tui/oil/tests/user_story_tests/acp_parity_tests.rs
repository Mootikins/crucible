//! US-307: delegated (ACP) turns must present like internal ones.
//!
//! A tool call executed inside an external agent's own loop still shows up
//! as a card in our transcript. Without a provenance badge the card is
//! indistinguishable from a tool Crucible ran itself, which hides the fact
//! that another process — under another permission gate — touched the
//! workspace. See `docs/Meta/Plans/2026-08-03-acp-presentation-parity.md`.

use super::support::StoryRuntime;
use crate::tui::oil::chat_app::ChatAppMsg;
use crate::tui::oil::chat_runner::session_event_to_chat_msgs;

fn acp_tool_call(name: &str, source: &str) -> ChatAppMsg {
    ChatAppMsg::ToolCall {
        name: name.into(),
        args: r#"{"path":"README.md"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: Some(source.into()),
        lua_primary_arg: None,
        diffs: Vec::new(),
        auto_approved: None,
    }
}

#[test]
fn acp_tool_call_renders_a_provenance_badge() {
    let mut story = StoryRuntime::new(80, 24);
    story.send(ChatAppMsg::UserMessage("read the readme".into()));
    story.send(acp_tool_call("read_file", "Acp:claude"));

    let frame = story.fresh_screen();
    assert!(
        frame.contains("acp:claude"),
        "delegated tool card showed no provenance badge:\n{frame}"
    );
}

#[test]
fn acp_tool_call_badge_survives_the_daemon_event_mapping() {
    // Pins the wire contract: the daemon's `tool_call` event carries the
    // agent under `source`, and the TUI's event mapping keeps it.
    let data = serde_json::json!({
        "tool": "read_file",
        "args": {"path": "README.md"},
        "call_id": "c1",
        "source": "Acp:claude",
    });
    let msgs = session_event_to_chat_msgs("tool_call", &data);

    let mut story = StoryRuntime::new(80, 24);
    story.send(ChatAppMsg::UserMessage("read the readme".into()));
    for msg in msgs {
        story.send(msg);
    }

    let frame = story.fresh_screen();
    assert!(
        frame.contains("acp:claude"),
        "the daemon's `source` field did not reach the rendered badge:\n{frame}"
    );
}

#[test]
fn internal_tool_call_still_renders_no_badge() {
    // Core/Crucible provenance is implicit; badging it would be noise. This
    // guards the ACP arm from widening into "badge everything".
    let mut story = StoryRuntime::new(80, 24);
    story.send(ChatAppMsg::UserMessage("read the readme".into()));
    story.send(acp_tool_call("read_file", "Core"));

    let frame = story.fresh_screen();
    assert!(
        !frame.contains("acp") && !frame.contains("core"),
        "an internal tool call should carry no provenance badge:\n{frame}"
    );
}
