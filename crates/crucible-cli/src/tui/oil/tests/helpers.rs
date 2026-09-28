use crate::tui::oil::app::ViewContext;
use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::Node;
use crucible_oil::ansi::strip_ansi;
use crucible_oil::focus::FocusContext;

use super::vt100_runtime::Vt100TestRuntime;

/// Resolve `assets/fixtures/<name>` from the crate manifest, not the cwd.
///
/// The one place a test may name a fixture. A relative `../../assets/...`
/// silently depends on which directory the runner happens to start in.
///
/// **Panics if the fixture is absent, by design.** Every fixture under
/// `assets/fixtures` is committed, so a missing one is a broken checkout, not a
/// condition to tiptoe around. The replay tests used to open with
/// `if !path.exists() { eprintln!("Skipping…"); return; }` — which reports
/// success while asserting nothing. Deleting all four recordings left nine such
/// tests passing in 0.02s apiece. Resolving through here makes that
/// unrepresentable: there is no way to name a fixture and receive a path that
/// does not exist.
pub fn fixture_path(name: &str) -> std::path::PathBuf {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("assets/fixtures")
        .join(name);

    assert!(
        path.exists(),
        "fixture {} is missing. It is committed to the repo, so this is a \
         broken checkout — not a test to skip.",
        path.display()
    );

    path
}

/// Read `assets/fixtures/<name>`, panicking with the resolved path on failure.
pub fn read_fixture(name: &str) -> String {
    let path = fixture_path(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()))
}

/// Every distinct `tool` a `tool_call` names in
/// `assets/fixtures/malformed-acp-recording.jsonl`.
///
/// That recording is the repo's richest capture of a real Claude Code session
/// that ran tools (the current `acp-demo.jsonl` re-recording carries only two
/// calls, and the five `crucible-daemon/tests/fixtures/acp/recorded/*/
/// basic-chat.jsonl` wire dumps contain no tool call at all). Any test about
/// "the titles ACP agents send" must read them from here rather than spell
/// them inline: a hand-written ACP title is whatever its author imagined an
/// agent sends, which is how divergence A4's first fix came to be verified
/// only against titles a mock had manufactured for it.
///
/// The recording is malformed in other ways (see
/// `replay_malformed_acp_recording_80x24`) — 13 calls announced twice,
/// results keyed to no call. None of that touches the `title` strings, which
/// are the agent's own and the only thing read here.
pub fn recorded_claude_code_tool_titles() -> Vec<String> {
    let text = read_fixture("malformed-acp-recording.jsonl");
    let mut titles: Vec<String> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|value| value.get("event").and_then(|e| e.as_str()) == Some("tool_call"))
        .filter_map(|value| Some(value.get("data")?.get("tool")?.as_str()?.to_string()))
        .collect();
    titles.sort();
    titles.dedup();
    assert!(
        !titles.is_empty(),
        "malformed-acp-recording.jsonl names no tools, so nothing keyed off it \
         is grounded in a real recording any more"
    );
    titles
}

/// Assert `title` is one a real agent actually sent, then hand it back.
///
/// Callers use the return value so the title under test cannot drift away from
/// the recording it claims to come from: re-record `acp-demo.jsonl` with
/// different titles and the caller fails here rather than quietly testing a
/// spelling no agent produces.
pub fn recorded_claude_code_title(title: &str) -> &str {
    let recorded = recorded_claude_code_tool_titles();
    assert!(
        recorded.iter().any(|t| t == title),
        "`{title}` is no longer in malformed-acp-recording.jsonl, so this test \
         is no longer grounded in a real agent's output. Recorded titles: {recorded:?}"
    );
    title
}

pub fn view_with_default_ctx(app: &OilChatApp) -> Node {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    app.view(&ctx)
}

/// Render app through the real terminal path (Terminal<Vec<u8>> → vt100)
/// and return stripped screen contents. This is the canonical test render
/// function — it exercises the same code path as production.
pub fn vt_render(app: &mut OilChatApp) -> String {
    vt_render_sized(app, 80, 24)
}

/// Like vt_render but with custom terminal dimensions.
pub fn vt_render_sized(app: &mut OilChatApp, width: u16, height: u16) -> String {
    let mut vt = Vt100TestRuntime::new(width, height);
    vt.render_frame(app);
    strip_ansi(&vt.screen_contents())
}

/// Assert no triple-blank-line run *between content* (always a spacing bug).
///
/// The leading run is skipped: the frame reserves the tallest completion
/// popup, and those rows are blank until something draws over them (US-505).
/// A guard that counted them would report the reserve as a spacing bug on
/// every short transcript.
pub fn assert_no_triple_blanks(screen: &str, context: &str) {
    let lines: Vec<&str> = screen.lines().collect();
    let first_content = lines
        .iter()
        .position(|line| !line.trim().is_empty())
        .unwrap_or(lines.len());
    for (i, window) in lines[first_content..].windows(3).enumerate() {
        assert!(
            !window.iter().all(|line| line.trim().is_empty()),
            "{}: triple blank at lines {}-{}.\nScreen:\n{}",
            context,
            first_content + i,
            first_content + i + 2,
            screen
        );
    }
}

// `EventFeed` lives with the fixtures, because the `fullscreen_demo` example
// includes that file and feeds its fake sessions through the same path.
pub use crate::tui::oil::fullscreen::fixtures::EventFeed;

/// The wire events of a turn, for a test. Each method gives the messages
/// that the event makes on the live path. Send them to the app with
/// [`SendMsgs::send_msgs`].
impl EventFeed {
    /// The submit of `content` in this TUI, then the echo of the daemon.
    pub fn user(&mut self, content: &str) -> Vec<crate::tui::oil::chat_app::ChatAppMsg> {
        let mut msgs = vec![crate::tui::oil::chat_app::ChatAppMsg::UserMessage(
            content.to_string(),
        )];
        msgs.extend(self.msgs("user_message", serde_json::json!({ "content": content })));
        msgs
    }

    pub fn text(&mut self, content: &str) -> Vec<crate::tui::oil::chat_app::ChatAppMsg> {
        self.msgs("text_delta", serde_json::json!({ "content": content }))
    }

    pub fn thinking(&mut self, content: &str) -> Vec<crate::tui::oil::chat_app::ChatAppMsg> {
        self.msgs("thinking", serde_json::json!({ "content": content }))
    }

    /// The end of the answer. The streamed deltas hold its text.
    pub fn complete(&mut self) -> Vec<crate::tui::oil::chat_app::ChatAppMsg> {
        self.msgs(
            "message_complete",
            serde_json::json!({ "full_response": "" }),
        )
    }

    /// A tool call with JSON `args` and no display.
    pub fn tool_call(
        &mut self,
        tool: &str,
        call_id: &str,
        args: &str,
    ) -> Vec<crate::tui::oil::chat_app::ChatAppMsg> {
        self.tool(ToolCallEvent {
            tool,
            call_id,
            args,
            ..ToolCallEvent::default()
        })
    }

    pub fn tool(&mut self, call: ToolCallEvent<'_>) -> Vec<crate::tui::oil::chat_app::ChatAppMsg> {
        let args: serde_json::Value = serde_json::from_str(call.args)
            .unwrap_or_else(|_| serde_json::Value::String(call.args.to_string()));
        let mut data = serde_json::json!({
            "call_id": call.call_id,
            "tool": call.tool,
            "args": args,
        });
        if call.render.is_some() || !call.diffs.is_empty() {
            data["display"] = serde_json::json!({
                "kind": "other",
                "tool": call.tool,
                "render": call.render,
                "diffs": call.diffs,
            });
        }
        if let Some(source) = call.source {
            data["source"] = source.into();
        }
        if let Some(auto_approved) = call.auto_approved {
            data["auto_approved"] = auto_approved.into();
        }
        self.msgs("tool_call", data)
    }

    /// The result of the call `call_id`, with the whole `output`.
    pub fn tool_result(
        &mut self,
        tool: &str,
        call_id: &str,
        output: &str,
    ) -> Vec<crate::tui::oil::chat_app::ChatAppMsg> {
        self.msgs(
            "tool_result",
            serde_json::json!({
                "call_id": call_id, "tool": tool, "result": { "result": output },
            }),
        )
    }

    /// The call `call_id` failed with `error`.
    pub fn tool_error(
        &mut self,
        tool: &str,
        call_id: &str,
        error: &str,
    ) -> Vec<crate::tui::oil::chat_app::ChatAppMsg> {
        self.msgs(
            "tool_result",
            serde_json::json!({
                "call_id": call_id, "tool": tool, "result": { "error": error },
            }),
        )
    }
}

/// The fields of a `tool_call` event. The display goes on the wire only
/// when `render` or `diffs` has a value.
#[derive(Default)]
pub struct ToolCallEvent<'a> {
    pub tool: &'a str,
    pub call_id: &'a str,
    /// JSON text. Other text goes on the wire as a JSON string.
    pub args: &'a str,
    pub render: Option<crucible_core::types::ToolRender>,
    pub diffs: Vec<crucible_core::types::acp::FileDiff>,
    pub source: Option<&'a str>,
    pub auto_approved: Option<&'a str>,
}

/// Send each message to the app, through `on_message`.
pub trait SendMsgs {
    fn send_msgs(&mut self, msgs: Vec<crate::tui::oil::chat_app::ChatAppMsg>);
}

impl SendMsgs for OilChatApp {
    fn send_msgs(&mut self, msgs: Vec<crate::tui::oil::chat_app::ChatAppMsg>) {
        for msg in msgs {
            self.on_message(msg);
        }
    }
}

/// The app messages of `events`, through one [`EventFeed`].
pub fn event_msgs(
    events: impl IntoIterator<Item = crucible_core::protocol::SessionEventMessage>,
) -> Vec<crate::tui::oil::chat_app::ChatAppMsg> {
    let mut feed = EventFeed::default();
    events.into_iter().flat_map(|e| feed.event(e)).collect()
}

/// The app messages of a recorded fixture, through [`event_msgs`].
pub fn fixture_msgs(name: &str) -> Vec<crate::tui::oil::chat_app::ChatAppMsg> {
    let events = read_fixture(name)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|line| {
            let name = line.get("event")?.as_str()?.to_string();
            let mut event = crucible_core::protocol::SessionEventMessage::new(
                "fixture",
                name,
                line.get("data").cloned().unwrap_or_default(),
            );
            event.seq = line.get("seq").and_then(serde_json::Value::as_u64);
            Some(event)
        })
        .collect::<Vec<_>>();
    event_msgs(events)
}
