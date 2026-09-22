//! A `proposal_changed` event starts a read of the proposal list.
//!
//! The event reaches the app through the message channel. The reducer
//! answers with `FetchProposals`, and the drain loop gives that follow-up to
//! `process_action`, which starts the read. If the follow-up stops there,
//! the status line count never changes.

use std::sync::Arc;

use crucible_core::events::EventRing;
use crucible_core::proposal::ProposalId;
use crucible_oil::terminal::Terminal;
use tokio::sync::mpsc;

use crate::chat::bridge::AgentEventBridge;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::chat_runner::{EventLoopParams, OilChatRunner};
use crate::tui::oil::noop_agent::NoopAgentHandle;

/// Drain one `proposal_changed` message. Answer the count of reads that
/// the drain started.
async fn reads_after_a_change(is_replay: bool) -> usize {
    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    runner.is_replay = is_replay;
    let mut app = OilChatApp::default();
    let mut agent = NoopAgentHandle::new("chat-1".to_string());
    let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));
    let (msg_tx, msg_rx) = mpsc::unbounded_channel();
    let mut background_tasks = Vec::new();
    msg_tx
        .send(ChatAppMsg::ProposalChanged(ProposalId::generate()))
        .unwrap();

    let mut params = EventLoopParams {
        app: &mut app,
        agent: &mut agent,
        bridge: &bridge,
        msg_tx,
        msg_rx,
        interaction_rx: None,
        background_tasks: &mut background_tasks,
    };
    let mut deadline = None;
    runner
        .drain_pending_messages(&mut params, &mut deadline)
        .await
        .expect("the drain does not fail");

    // The test runtime runs one thread, and the drain did not yield, so the
    // read never reaches a daemon before this abort.
    let reads = background_tasks.len();
    OilChatRunner::abort_background_tasks(&mut background_tasks);
    reads
}

#[tokio::test]
async fn a_drained_proposal_change_starts_the_read() {
    assert_eq!(reads_after_a_change(false).await, 1);
}

#[tokio::test]
async fn a_replay_starts_no_read() {
    assert_eq!(reads_after_a_change(true).await, 0);
}
