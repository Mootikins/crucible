//! The kept rows of finished messages must give the same frame as a layout
//! of every message from source.
//!
//! Each test drives one app through a change and renders every frame twice.
//! `render_frame` is the production path: it reuses the rows of finished
//! messages. `OilChatApp::view` lays every message out again. Both frames go
//! to their own headless terminal, and the two terminals must receive the
//! same bytes. A kept row that a change should have replaced gives a
//! different byte stream, so each test fails on stale rows.

use super::fixture_replay_tests::parse_fixture;
use super::helpers::fixture_path;
use super::transcript_fixtures as fixtures;
use crate::tui::oil::app::ViewContext;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::chat_runner::render_frame;
use crate::tui::oil::theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_core::types::acp::FileDiff;
use crucible_oil::ansi::strip_ansi;
use crucible_oil::focus::FocusContext;
use crucible_oil::style::{AdaptiveColor, Color};
use crucible_oil::{FrameRenderer, TestRuntime};
use std::time::{Duration, Instant};

/// Two terminals of one size: one gets the cached frames, one gets the frames
/// that lay out every message from source.
struct Twin {
    cached: TestRuntime,
    fresh: TestRuntime,
    focus: FocusContext,
    frames: usize,
    /// The styled rows of the last frame.
    styled: String,
}

impl Twin {
    fn new(width: u16, height: u16) -> Self {
        Self {
            cached: TestRuntime::new(width, height),
            fresh: TestRuntime::new(width, height),
            focus: FocusContext::new(),
            frames: 0,
            styled: String::new(),
        }
    }

    fn resize(&mut self, width: u16, height: u16) {
        self.cached.resize(width, height);
        self.fresh.resize(width, height);
    }

    /// Render one frame on both terminals, and compare the bytes. Returns the
    /// plain text of the frame.
    fn frame(&mut self, app: &mut OilChatApp) -> String {
        self.frames += 1;
        // `render_frame` takes the redraw request; both terminals need it.
        if app.take_needs_full_redraw() {
            FrameRenderer::force_full_redraw(&mut self.cached);
            FrameRenderer::force_full_redraw(&mut self.fresh);
        }
        render_frame(app, &mut self.cached, &self.focus);

        let size = FrameRenderer::size(&self.fresh);
        let ctx = ViewContext::with_terminal_size(&self.focus, theme::active(), size);
        FrameRenderer::set_min_viewport_rows(&mut self.fresh, app.min_viewport_rows(&ctx));
        self.fresh.render(&app.view(&ctx));

        let cached = self.cached.viewport_content().to_string();
        let fresh = self.fresh.viewport_content().to_string();
        if let Some((row, (c, f))) = cached
            .split("\r\n")
            .zip(fresh.split("\r\n"))
            .enumerate()
            .find(|(_, (c, f))| c != f)
        {
            panic!(
                "frame {}: row {row} differs\ncached: {c:?}\nfresh:  {f:?}",
                self.frames
            );
        }
        assert_eq!(
            cached.split("\r\n").count(),
            fresh.split("\r\n").count(),
            "frame {}: the row counts differ",
            self.frames
        );
        assert!(
            self.cached.take_bytes() == self.fresh.take_bytes(),
            "frame {}: the terminal bytes differ",
            self.frames
        );
        let plain = strip_ansi(&cached);
        self.styled = cached;
        plain
    }
}

fn key(app: &mut OilChatApp, code: KeyCode, modifiers: KeyModifiers) {
    let _ = app.update(crate::tui::oil::event::Event::Key(KeyEvent::new(
        code, modifiers,
    )));
}

fn tool_call(name: &str, call_id: &str, diffs: Vec<FileDiff>) -> ChatAppMsg {
    ChatAppMsg::ToolCall {
        name: name.to_string(),
        args: r#"{"path":"src/lib.rs"}"#.to_string(),
        call_id: Some(call_id.to_string()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs,
        auto_approved: None,
    }
}

fn edit_diff() -> FileDiff {
    FileDiff {
        path: "src/lib.rs".to_string(),
        old_content: Some("fn old() {}\n".to_string()),
        new_content: "fn new() {}\n".to_string(),
    }
}

#[test]
fn streaming_a_new_answer_and_finishing_it_match_a_fresh_layout() {
    let mut app = fixtures::app_with_exchanges(3);
    let mut twin = Twin::new(100, 30);
    twin.frame(&mut app);

    app.on_message(ChatAppMsg::UserMessage(fixtures::user_text(3)));
    twin.frame(&mut app);
    app.on_message(ChatAppMsg::ThinkingDelta("Consider the rows.".into()));
    twin.frame(&mut app);
    for delta in fixtures::stream_deltas(3).into_iter().take(40) {
        app.on_message(ChatAppMsg::TextDelta(delta));
        twin.frame(&mut app);
    }
    app.on_message(ChatAppMsg::StreamComplete);
    let screen = twin.frame(&mut app);
    assert!(screen.contains("Thought"), "the finished thinking header");

    // A second frame of the same state reuses every row.
    twin.frame(&mut app);
}

#[test]
fn a_tool_card_that_runs_gets_output_and_finishes_matches_a_fresh_layout() {
    let mut app = fixtures::app_with_exchanges(2);
    let mut twin = Twin::new(100, 30);
    app.on_message(ChatAppMsg::UserMessage("edit it".into()));
    app.on_message(ChatAppMsg::TextDelta("I will edit the file.".into()));
    twin.frame(&mut app);

    app.on_message(tool_call("Read", "read-1", Vec::new()));
    twin.frame(&mut app);
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "Read".into(),
        delta: "line one\nline two\n".into(),
        call_id: Some("read-1".into()),
    });
    twin.frame(&mut app);
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "Read".into(),
        call_id: Some("read-1".into()),
    });
    twin.frame(&mut app);

    // The edit card is finished before its diff arrives; the late diff
    // expands the card.
    app.on_message(tool_call("Edit", "edit-1", Vec::new()));
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "Edit".into(),
        call_id: Some("edit-1".into()),
    });
    let before = twin.frame(&mut app);
    app.on_message(ChatAppMsg::ToolCallDiffUpdate {
        call_id: "edit-1".into(),
        diffs: vec![edit_diff()],
    });
    let after = twin.frame(&mut app);
    assert!(
        !before.contains("fn new()") && after.contains("fn new()"),
        "the late diff must show in the finished card"
    );

    app.on_message(ChatAppMsg::ToolResultError {
        name: "Read".into(),
        error: "gone".into(),
        call_id: Some("read-1".into()),
    });
    twin.frame(&mut app);
    app.on_message(ChatAppMsg::TextDelta("Done.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    twin.frame(&mut app);
}

#[test]
fn toggling_the_diff_body_of_a_finished_tool_card_matches_a_fresh_layout() {
    let mut app = fixtures::app_with_exchanges(1);
    let mut twin = Twin::new(100, 30);
    app.on_message(ChatAppMsg::UserMessage("edit it".into()));
    app.on_message(tool_call("Edit", "edit-1", vec![edit_diff()]));
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "Edit".into(),
        call_id: Some("edit-1".into()),
    });
    app.on_message(ChatAppMsg::StreamComplete);
    let shown = twin.frame(&mut app);
    assert!(shown.contains("fn new()"));

    app.set_show_diffs(false);
    let hidden = twin.frame(&mut app);
    assert!(!hidden.contains("fn new()"), "the diff body must go");

    app.set_show_diffs(true);
    let again = twin.frame(&mut app);
    assert!(again.contains("fn new()"), "the diff body must come back");
}

#[test]
fn toggling_show_thinking_matches_a_fresh_layout() {
    let mut app = OilChatApp::default();
    let mut twin = Twin::new(100, 30);
    app.on_message(ChatAppMsg::UserMessage("think".into()));
    app.on_message(ChatAppMsg::ThinkingDelta(
        "A long private line of reasoning.".into(),
    ));
    app.on_message(ChatAppMsg::TextDelta("The answer.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    let expanded = twin.frame(&mut app);
    assert!(expanded.contains("private line"));

    key(&mut app, KeyCode::Char('t'), KeyModifiers::CONTROL);
    let collapsed = twin.frame(&mut app);
    assert!(
        !collapsed.contains("private line"),
        "thinking must collapse"
    );

    key(&mut app, KeyCode::Char('t'), KeyModifiers::CONTROL);
    let again = twin.frame(&mut app);
    assert!(again.contains("private line"), "thinking must expand again");
}

#[test]
fn a_width_change_reprints_matching_a_fresh_layout() {
    let mut app = fixtures::app_with_exchanges(3);
    let mut twin = Twin::new(120, 30);
    twin.frame(&mut app);
    for width in [80, 47, 120, 121] {
        twin.resize(width, 30);
        twin.frame(&mut app);
        twin.frame(&mut app);
    }
}

#[test]
fn a_theme_change_matches_a_fresh_layout() {
    let mut app = fixtures::app_with_exchanges(2);
    let mut twin = Twin::new(100, 30);
    let dark = twin.frame(&mut app);
    let dark_styled = twin.styled.clone();

    let mut other = theme::ThemeConfig::default_dark();
    other.name = "other".to_string();
    other.colors.background = AdaptiveColor::from_single(Color::Magenta);
    other.colors.text_muted = AdaptiveColor::from_single(Color::Green);
    theme::set(other);
    app.on_message(ChatAppMsg::StyleChanged);
    let lit = twin.frame(&mut app);
    assert_eq!(dark, lit, "a theme changes colors, not text");
    assert_ne!(dark_styled, twin.styled, "the colors must change");
}

#[test]
fn a_cleared_transcript_does_not_reuse_rows_by_position() {
    let mut app = fixtures::app_with_exchanges(2);
    let mut twin = Twin::new(100, 30);
    twin.frame(&mut app);

    // No frame between the clear and the new nodes, so the new nodes take
    // the old positions before the cache sees the list shrink.
    app.on_message(ChatAppMsg::ClearHistory);
    app.on_message(ChatAppMsg::UserMessage("a new first question".into()));
    app.on_message(ChatAppMsg::TextDelta("a new first answer".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    let screen = twin.frame(&mut app);
    assert!(screen.contains("a new first answer"));
    assert!(!screen.contains("Answer 0"), "old rows must not come back");
}

#[test]
fn a_slow_tool_that_moves_to_the_background_matches_a_fresh_layout() {
    let mut app = OilChatApp::default();
    let mut twin = Twin::new(100, 30);
    let start = Instant::now();
    app.set_frame_time(start);
    app.on_message(ChatAppMsg::UserMessage("run it".into()));
    app.on_message(tool_call("Bash", "bash-1", Vec::new()));
    twin.frame(&mut app);
    for step in 1..=8 {
        app.set_frame_time(start + Duration::from_millis(150 * step));
        twin.frame(&mut app);
    }
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "Bash".into(),
        call_id: Some("bash-1".into()),
    });
    let screen = twin.frame(&mut app);
    assert!(screen.contains("finished after"), "the finish node");
}

#[test]
fn a_running_subagent_animates_and_matches_a_fresh_layout() {
    let mut app = OilChatApp::default();
    let mut twin = Twin::new(100, 30);
    let start = Instant::now();
    app.set_frame_time(start);
    app.on_message(ChatAppMsg::UserMessage("delegate it".into()));
    app.on_message(ChatAppMsg::SubagentSpawned {
        id: "agent-1".into(),
        prompt: "look around".into(),
    });
    let mut screens = Vec::new();
    for step in 0..4 {
        app.set_frame_time(start + Duration::from_millis(1100 * step));
        screens.push(twin.frame(&mut app));
    }
    screens.dedup();
    assert_eq!(screens.len(), 4, "the running agent changes on each frame");

    app.on_message(ChatAppMsg::SubagentCompleted {
        id: "agent-1".into(),
        summary: "found it".into(),
    });
    let screen = twin.frame(&mut app);
    assert!(screen.contains("found it"));
}

#[test]
fn a_transcript_taller_than_the_layout_cap_matches_a_fresh_layout() {
    // The layout gives taffy at most 500 rows of height; the transcript
    // must keep every row past it in both paths.
    let mut app = fixtures::app_with_exchanges(20);
    let mut twin = Twin::new(120, 40);
    let screen = twin.frame(&mut app);
    assert!(
        screen.lines().count() > 600,
        "{} rows",
        screen.lines().count()
    );
    app.on_message(ChatAppMsg::UserMessage(fixtures::user_text(20)));
    app.on_message(ChatAppMsg::TextDelta(fixtures::assistant_text(20)));
    twin.frame(&mut app);
}

/// Recorded sessions hold every kind of node: tools, delegations, errors,
/// permission requests, markdown of every shape.
#[test]
fn recorded_sessions_match_a_fresh_layout_on_every_frame() {
    for name in [
        "demo.jsonl",
        "acp-demo.jsonl",
        "delegation-demo.jsonl",
        "showcase-demo.jsonl",
        "permission_flow.jsonl",
        "reproduce-formatting.jsonl",
        "undo_flow.jsonl",
    ] {
        let mut app = OilChatApp::default();
        let mut twin = Twin::new(100, 30);
        for msg in parse_fixture(&fixture_path(name)) {
            app.on_message(msg);
            twin.frame(&mut app);
        }
    }
}

#[test]
fn finished_messages_are_laid_out_once() {
    let mut app = fixtures::app_with_exchanges(3);
    let mut twin = Twin::new(100, 30);
    twin.frame(&mut app);
    let first = app.transcript_layouts();
    assert_eq!(first, 6, "every message once");

    twin.frame(&mut app);
    assert_eq!(
        app.transcript_layouts(),
        first,
        "an idle frame lays out nothing"
    );

    app.on_message(ChatAppMsg::UserMessage(fixtures::user_text(3)));
    app.on_message(ChatAppMsg::TextDelta("partial".into()));
    twin.frame(&mut app);
    // The new question is finished and laid out once; the answer streams and
    // is never kept.
    twin.frame(&mut app);
    assert_eq!(app.transcript_layouts(), first + 1);

    twin.resize(80, 30);
    twin.frame(&mut app);
    assert_eq!(
        app.transcript_layouts(),
        first + 1 + 7,
        "a width change lays out every finished message"
    );
}
