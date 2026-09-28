//! US-912: resume an earlier session from inside the TUI.
//!
//! `/resume` opens a picker of the sessions of this workspace; Enter on a row
//! asks the runner to switch. These tests read the rendered frames of both
//! screen modes. The runner half (the list read, the guards, the switch) is
//! in `chat_runner/tests/session_resume.rs`, and the PTY test in
//! `tests/tui_e2e_tests/session_store.rs` crosses to a real daemon.

use crossterm::event::KeyCode;

use crate::tui::oil::app::{Action, ViewContext};
use crate::tui::oil::chat_app::model_state::SessionChoice;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::fullscreen::FullscreenView;
use crate::tui::oil::tests::vt100_runtime::Vt100TestRuntime;
use crate::tui::oil::theme;
use crucible_oil::focus::FocusContext;

use super::support::StoryRuntime;

fn sessions() -> Vec<SessionChoice> {
    vec![
        SessionChoice {
            id: "chat-2026-09-25T1501-2ecnzu".into(),
            title: Some("Fix the link parser".into()),
            when: "2026-09-25 15:01".into(),
        },
        SessionChoice {
            id: "chat-2026-09-24T0930-k3m9qa".into(),
            title: None,
            when: "2026-09-24 09:30".into(),
        },
    ]
}

#[test]
fn the_resume_picker_lists_the_sessions_and_enter_resumes_one() {
    let mut story = StoryRuntime::new(80, 16);
    story.text("/resume");
    let action = story.enter();
    assert!(
        matches!(action, Action::Send(ChatAppMsg::FetchSessions)),
        "{action:?}"
    );
    story.capture("the picker waits for the daemon");

    story.send(ChatAppMsg::SessionsLoaded(sessions()));
    story.capture("the list arrived");
    let screen = story.screen();
    assert!(screen.contains("chat-2026-09-25T1501-2ecnzu"), "{screen}");
    assert!(screen.contains("Fix the link parser"), "{screen}");
    assert!(screen.contains("(untitled)"), "{screen}");

    story.key(KeyCode::Down);
    let action = story.enter();
    assert!(
        matches!(action, Action::Send(ChatAppMsg::ResumeSession(ref id)) if id == "chat-2026-09-24T0930-k3m9qa"),
        "{action:?}"
    );
    story.capture("the choice closed the picker");
    insta::assert_snapshot!("resume_picker_frame_sequence", story.sequence());
}

/// The default mode draws its frame through `FullscreenView`, not through
/// the inline path, so the picker needs its own proof there.
#[test]
fn the_resume_picker_draws_in_the_full_screen_mode() {
    const WIDTH: u16 = 80;
    const HEIGHT: u16 = 16;
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("an earlier question".into()));
    let _ = app.update(crate::tui::oil::event::Event::Paste("/resume".into()));
    let _ = app.update(crate::tui::oil::event::Event::Key(
        crossterm::event::KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE),
    ));
    app.on_message(ChatAppMsg::SessionsLoaded(sessions()));

    let focus = FocusContext::new();
    let ctx = ViewContext::with_terminal_size(&focus, theme::active(), (WIDTH, HEIGHT));
    let mut view = FullscreenView::new();
    let mut vt = Vt100TestRuntime::new(WIDTH, HEIGHT);
    vt.present_fullscreen(&view.frame(&mut app, &ctx));
    let screen = vt.screen_contents();

    assert!(screen.contains("chat-2026-09-25T1501-2ecnzu"), "{screen}");
    assert!(screen.contains("Fix the link parser"), "{screen}");
    assert!(
        screen.contains("/resume"),
        "the prompt shows the command:\n{screen}"
    );
    insta::assert_snapshot!("resume_picker_full_screen", screen);
}

/// A resumed session draws the transcript that the daemon folded. The
/// narration before a tool stays above the tool: a resume that rebuilt the
/// turn from its stored events put all the text below the tools.
#[test]
fn a_resumed_session_keeps_the_narration_above_its_tool() {
    let path = crate::tui::oil::tests::helpers::fixture_path("acp_parity_internal.jsonl");
    let events: Vec<crucible_core::protocol::SessionEventMessage> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|line| {
            let name = line.get("event")?.as_str()?.to_string();
            Some(crucible_core::protocol::SessionEventMessage::new(
                "s",
                name,
                line.get("data").cloned().unwrap_or_default(),
            ))
        })
        .collect();
    let snapshot = crucible_core::transcript::TranscriptFold::of_events(&events);

    let mut story = StoryRuntime::new(80, 30);
    story.send(ChatAppMsg::TranscriptLoaded(snapshot));
    let screen = story.screen();

    let at = |needle: &str| {
        screen
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} is on screen:\n{screen}"))
    };
    assert!(
        at("fix the greeting") < at("I'll fix the greeting."),
        "{screen}"
    );
    assert!(at("I'll fix the greeting.") < at("Done."), "{screen}");
    assert!(
        screen
            .lines()
            .any(|line| line.contains("Edit") || line.contains("edit")),
        "the tool card is drawn:\n{screen}"
    );
}

/// The message that the user sends shows at once. When the daemon then
/// upserts the turn of that message, the TUI keeps the one row.
#[test]
fn a_sent_message_shows_once_when_the_daemon_names_its_turn() {
    let mut story = StoryRuntime::new(80, 24);
    story.text("hello there");
    let _ = story.enter();
    story.event(
        "user_message",
        serde_json::json!({ "message_id": "m1", "content": "hello there" }),
    );
    story.event("text_delta", serde_json::json!({ "content": "hi" }));
    story.event(
        "message_complete",
        serde_json::json!({ "message_id": "m1", "full_response": "hi" }),
    );

    let screen = story.screen();
    assert_eq!(screen.matches("hello there").count(), 1, "{screen}");
    assert!(screen.contains("hi"), "{screen}");
}
