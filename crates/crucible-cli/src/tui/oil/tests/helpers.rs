#![allow(dead_code)] // helpers used by disabled test modules awaiting reconstruction

use crate::tui::oil::app::{App, ViewContext};
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
