//! US-910: `:diff` opens, and the text of its first file appears.
//!
//! The diffset reaches the app through the message channel. Its reducer
//! answers with `FetchDiffFile`. The drain loop must give that follow-up to
//! `process_action`, because only `process_action` starts the daemon read.
//! Before the fix, the drain gave the follow-up to the reducer only, and the
//! view showed "loading" for ever.
//!
//! The test runtime runs one thread, and no step yields, so no read reaches
//! a daemon. The test counts the reads that the runner starts, and then
//! sends the answer of the daemon itself.

use crate::tui::oil::app::Action;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::chat_runner::{
    DrainMessagesOutcome, EventLoopParams, OilChatRunner, StageCtx,
};
use crate::tui::oil::event::Event;
use crate::tui::oil::tests::vt100_runtime::Vt100TestRuntime;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_core::diff::{DiffFileEntry, DiffFileText, Diffset, DiffsetSource, FileStatus};
use crucible_core::session::PhysicalRoot;
use crucible_oil::terminal::Terminal;
use tokio::sync::mpsc;

fn diffset() -> Diffset {
    let root = PhysicalRoot::from_top_level("/repo");
    let source = DiffsetSource::Branch {
        root: root.clone(),
        base: "main".into(),
        head: None,
    };
    Diffset {
        id: source.id(),
        source,
        files: vec![DiffFileEntry {
            root,
            path: "src/lib.rs".into(),
            status: FileStatus::Modified,
            added: 1,
            removed: 1,
            binary: false,
            too_large: false,
        }],
        unreadable_roots: Vec::new(),
    }
}

fn screen(app: &mut OilChatApp) -> String {
    let mut vt = Vt100TestRuntime::new(80, 24);
    vt.render_frame(app);
    vt.screen_contents()
}

/// Drain the channel once. Answer the count of reads that the drain started.
async fn drain(runner: &mut OilChatRunner, params: &mut EventLoopParams<'_>) -> usize {
    let before = params.background_tasks.len();
    let mut deadline = None;
    let outcome = runner
        .drain_pending_messages(params, &mut deadline)
        .await
        .expect("the drain does not fail");
    assert_eq!(outcome, DrainMessagesOutcome::Processed);
    params.background_tasks.len() - before
}

/// Run `:diff` through the runner. Answer the reads that the drained
/// diffset started, and the app after the text of the first file arrived.
async fn open_the_diff(is_replay: bool) -> (usize, String) {
    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    runner.is_replay = is_replay;
    let mut app = OilChatApp::default();
    let daemon = crate::test_daemon::FakeDaemon::answering_null("chat-1").await;
    let (msg_tx, msg_rx) = mpsc::unbounded_channel();
    let mut background_tasks = Vec::new();

    // The user types `:diff`. The runner starts the read of the diffset.
    for c in ":diff".chars() {
        app.update(Event::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::NONE,
        )));
    }
    let action = app.update(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )));
    assert!(
        matches!(action, Action::Send(ChatAppMsg::OpenDiff(None))),
        "`:diff` asks for the branch diff, got {action:?}"
    );
    runner
        .process_action(
            StageCtx {
                app: &mut app,
                session: Some(&daemon.session),
                msg_tx: &msg_tx,
                background_tasks: &mut background_tasks,
            },
            action,
        )
        .await
        .expect("the action does not fail");

    let mut params = EventLoopParams {
        app: &mut app,
        session: Some(&daemon.session),
        msg_tx,
        msg_rx,
        background_tasks: &mut background_tasks,
    };

    // The daemon answers with the diffset. The drain must start the read
    // of the first file.
    params
        .msg_tx
        .send(ChatAppMsg::DiffLoaded(Box::new(diffset())))
        .unwrap();
    let reads = drain(&mut runner, &mut params).await;
    let loading = screen(params.app);
    assert!(loading.contains("file 1/1"), "the view is open:\n{loading}");

    // The daemon answers with the text of the first file.
    params
        .msg_tx
        .send(ChatAppMsg::DiffFileLoaded {
            id: diffset().id,
            index: 0,
            text: DiffFileText {
                base_text: Some("fn kept() {}\nfn old() {}\n".into()),
                current_text: Some("fn kept() {}\nfn new() {}\n".into()),
            },
        })
        .unwrap();
    drain(&mut runner, &mut params).await;
    let shown = screen(params.app);

    OilChatRunner::abort_background_tasks(params.background_tasks);
    (reads, shown)
}

#[tokio::test]
async fn the_diff_opens_and_the_first_file_text_appears() {
    let (reads, shown) = open_the_diff(false).await;
    assert_eq!(
        reads, 1,
        "the drained diffset starts the read of its first file"
    );
    assert!(shown.contains("-fn old() {}"), "the removed line:\n{shown}");
    assert!(shown.contains("+fn new() {}"), "the added line:\n{shown}");
    assert!(
        !shown.contains("loading"),
        "the text replaced the wait:\n{shown}"
    );
}

#[tokio::test]
async fn a_replay_starts_no_file_read() {
    let (reads, _) = open_the_diff(true).await;
    assert_eq!(reads, 0);
}
