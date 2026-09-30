//! `/resume` in the runner (US-912): the list of sessions and the switch.
//!
//! The switch itself crosses to the daemon and to a new run of the TUI, so
//! the PTY test `tests/tui_e2e_tests/session_store.rs` proves it. These tests
//! prove the guards that stop a switch before any daemon call.

use crate::tui::oil::app::Action;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::chat_runner::actions::resumable_sessions;
use crate::tui::oil::chat_runner::OilChatRunner;
use crucible_oil::terminal::Terminal;

/// Send one message through `process_action`. Answer whether the loop quits.
async fn process(runner: &mut OilChatRunner, app: &mut OilChatApp, msg: ChatAppMsg) -> bool {
    let daemon = crate::test_daemon::FakeDaemon::answering_null("chat-open").await;
    runner
        .process_action_for_test(Action::Send(msg), app, Some(&daemon.session))
        .await
        .expect("process_action does not fail")
}

#[tokio::test]
async fn a_resume_of_the_open_session_stays_in_it() {
    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    let mut app = OilChatApp::default();

    let quit = process(
        &mut runner,
        &mut app,
        ChatAppMsg::ResumeSession("chat-open".into()),
    )
    .await;

    assert!(!quit, "the console already shows this session");
    assert_eq!(runner.next_session, None);
    assert_eq!(
        app.notification_messages(),
        ["This console already shows session chat-open"],
        "the user learns why nothing moved, and no daemon call was made"
    );
}

#[tokio::test]
async fn a_resume_while_a_turn_runs_is_refused() {
    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    let mut app = OilChatApp::default();
    app.container_list_mut().mark_turn_active();

    let quit = process(
        &mut runner,
        &mut app,
        ChatAppMsg::ResumeSession("chat-other".into()),
    )
    .await;

    assert!(!quit, "a running turn keeps the console");
    assert_eq!(runner.next_session, None);
    assert_eq!(
        app.notification_messages(),
        ["Cannot resume another session while a turn runs"]
    );
}

#[tokio::test]
async fn a_replay_never_resumes() {
    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    runner.is_replay = true;
    let mut app = OilChatApp::default();

    let quit = process(
        &mut runner,
        &mut app,
        ChatAppMsg::ResumeSession("chat-other".into()),
    )
    .await;

    assert!(!quit);
    assert_eq!(runner.next_session, None);
    assert!(
        app.notification_messages().is_empty(),
        "a replay reaches no daemon, so it has nothing to report: {:?}",
        app.notification_messages()
    );
}

/// Answer the count of reads that one `FetchSessions` starts.
async fn reads_for_a_fetch(is_replay: bool) -> usize {
    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    runner.is_replay = is_replay;
    let mut app = OilChatApp::default();
    let daemon = crate::test_daemon::FakeDaemon::answering_null("chat-open").await;
    let (msg_tx, _msg_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut background_tasks = Vec::new();
    runner
        .process_action(
            crate::tui::oil::chat_runner::StageCtx {
                app: &mut app,
                session: Some(&daemon.session),
                msg_tx: &msg_tx,
                background_tasks: &mut background_tasks,
            },
            Action::Send(ChatAppMsg::FetchSessions),
        )
        .await
        .expect("process_action does not fail");
    let reads = background_tasks.len();
    OilChatRunner::abort_background_tasks(&mut background_tasks);
    reads
}

#[tokio::test]
async fn a_session_fetch_starts_one_read() {
    assert_eq!(reads_for_a_fetch(false).await, 1);
}

#[tokio::test]
async fn a_replay_reads_no_session_list() {
    assert_eq!(reads_for_a_fetch(true).await, 0);
}

#[test]
fn the_picker_list_drops_the_open_session_and_puts_the_newest_first() {
    let listed = serde_json::json!({ "sessions": [
        { "session_id": "chat-old", "title": "Old work",
          "started_at": "2026-09-20T10:00:00Z", "last_activity": "2026-09-20T11:00:00Z" },
        { "session_id": "chat-open", "title": "This one",
          "started_at": "2026-09-25T10:00:00Z", "last_activity": null },
        { "session_id": "chat-new", "title": null,
          "started_at": "2026-09-24T09:00:00Z", "last_activity": "2026-09-24T18:00:00Z" },
        { "session_id": "chat-blank", "title": "",
          "started_at": "2026-09-21T09:00:00Z", "last_activity": null },
    ]});

    let sessions = resumable_sessions("chat-open", &listed, 10);

    let ids: Vec<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["chat-new", "chat-blank", "chat-old"]);
    assert_eq!(sessions[0].title, None);
    assert_eq!(sessions[1].title, None, "an empty title is no title");
    assert_eq!(sessions[2].title.as_deref(), Some("Old work"));
    assert!(sessions.iter().all(|s| !s.when.is_empty()));
}

#[test]
fn the_picker_list_is_capped() {
    let listed = serde_json::json!({ "sessions": (0..30).map(|i| serde_json::json!({
        "session_id": format!("chat-{i:02}"),
        "started_at": format!("2026-09-01T00:{i:02}:00Z"),
    })).collect::<Vec<_>>() });

    let sessions = resumable_sessions("chat-open", &listed, 20);

    assert_eq!(sessions.len(), 20);
    assert_eq!(sessions[0].id, "chat-29", "the newest comes first");
}
