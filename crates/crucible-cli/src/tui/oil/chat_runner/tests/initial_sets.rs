//! Regression tests for `cru chat --set` startup overrides.
//!
//! `initial_sets` daemon-bound overrides used to be sent down the UI
//! message channel, where only the reducer runs — the daemon RPC arm in
//! `process_action` was never reached, so `--set context_strategy=truncate`
//! (and every other daemon-scoped key) was silently inert, and
//! `--set model=X` updated the status bar without switching the model.

use crucible_oil::terminal::Terminal;
use tokio::sync::mpsc;

use crate::test_daemon::FakeDaemon;
use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::chat_runner::OilChatRunner;
use crate::tui::oil::commands::{SetEffect, SetRpcAction};

#[tokio::test]
async fn startup_set_overrides_reach_the_daemon_rpc() {
    let mut runner =
        OilChatRunner::with_terminal(Terminal::with_size(80, 24)).with_initial_sets(vec![
            SetEffect::DaemonRpc(SetRpcAction::Knob(
                crucible_core::types::KnobValue::ContextStrategy("truncate".into()),
            )),
            SetEffect::DaemonRpc(SetRpcAction::Knob(crucible_core::types::KnobValue::Model(
                "gpt-4o".into(),
            ))),
        ]);

    let daemon = FakeDaemon::answering_null("chat-1").await;
    let mut app = OilChatApp::default();
    let (msg_tx, _msg_rx) = mpsc::unbounded_channel();
    let mut background_tasks = Vec::new();

    runner
        .apply_initial_sets(
            &mut app,
            Some(&daemon.session),
            &msg_tx,
            &mut background_tasks,
        )
        .await
        .expect("apply_initial_sets should not fail");

    assert_eq!(
        daemon.methods(),
        ["session.knob.set", "session.knob.set"],
        "--set context_strategy and --set model must each reach the daemon RPC once, \
         not only the reducer"
    );
    assert_eq!(
        app.current_model(),
        "gpt-4o",
        "the reducer half must also run so the status bar reflects the override"
    );

    OilChatRunner::abort_background_tasks(&mut background_tasks);
}
