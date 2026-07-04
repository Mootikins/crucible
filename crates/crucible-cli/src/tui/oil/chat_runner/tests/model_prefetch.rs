//! Regression tests for the startup model prefetch.
//!
//! `queue_model_prefetch` sends `FetchModels` down the UI channel, which the
//! reducer turns into `ModelListState::Loading`. But messages drained from
//! that channel never reach `process_action` — the only place the actual
//! daemon RPC was spawned. Without spawning the fetch here too, the app
//! wedges at Loading forever and `:model` shows "Loading models..."
//! indefinitely (both `handle_model_repl` and the autocomplete trigger
//! decline to re-fetch while Loading).

use tokio::sync::mpsc;

use crate::tui::oil::chat_app::ChatAppMsg;
use crate::tui::oil::chat_runner::OilChatRunner;
use crucible_oil::terminal::Terminal;

#[tokio::test]
async fn startup_prefetch_spawns_fetch_task_with_loading_transition() {
    let runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut background_tasks = Vec::new();

    runner.queue_model_prefetch(&tx, &mut background_tasks);

    assert!(
        matches!(rx.try_recv(), Ok(ChatAppMsg::FetchModels)),
        "prefetch must queue FetchModels so the reducer enters Loading"
    );
    assert_eq!(
        background_tasks.len(),
        1,
        "prefetch must spawn the daemon fetch task; the Loading transition \
         alone wedges the model picker forever"
    );

    OilChatRunner::abort_background_tasks(&mut background_tasks);
}
