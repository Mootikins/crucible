//! Tests that `process_message` gates fire-and-forget sends on `is_replay`.
//!
//! Replay mode delivers user messages as SessionEvent broadcasts from the
//! daemon's replay session — the TUI must not re-send them via RPC. This
//! module uses a counting mock agent to assert the gate.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use crucible_core::events::EventRing;
use crucible_core::traits::chat::{AgentHandle, ChatResult};
use crucible_oil::terminal::Terminal;

use crate::chat::bridge::AgentEventBridge;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::chat_runner::OilChatRunner;
use crate::tui::Action;

struct CountingAgent {
    sends: AtomicUsize,
}

impl CountingAgent {
    fn new() -> Self {
        Self {
            sends: AtomicUsize::new(0),
        }
    }
}

crucible_core::impl_noop_agent!(CountingAgent);

#[async_trait]
impl AgentHandle for CountingAgent {
    async fn send_message_fire_and_forget(&mut self, _message: String) -> ChatResult<()> {
        self.sends.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
        Ok(())
    }
}

#[tokio::test]
async fn replay_user_message_does_not_invoke_send() {
    let mut agent = CountingAgent::new();
    let mut app = OilChatApp::default();
    let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));

    let _ = OilChatRunner::process_message_for_test(
        &ChatAppMsg::UserMessage("hi".into()),
        &mut app,
        &mut agent,
        &bridge,
        /* is_replay */ true,
    )
    .await;

    assert_eq!(agent.sends.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn live_user_message_invokes_send_once() {
    let mut agent = CountingAgent::new();
    let mut app = OilChatApp::default();
    app.set_precognition(false);
    let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));

    let _ = OilChatRunner::process_message_for_test(
        &ChatAppMsg::UserMessage("hi".into()),
        &mut app,
        &mut agent,
        &bridge,
        /* is_replay */ false,
    )
    .await;

    assert_eq!(agent.sends.load(Ordering::Relaxed), 1);
}

/// Regression test for the first-message hang fix (5e21776a5).
///
/// Sequence: keypress submit → `submit_user_message` flips `turn_active`
/// → returns `Action::Send(UserMessage)`. The fix removed the
/// `!is_streaming()` gate at line 487 of `actions.rs`. Without the fix,
/// every first message of a fresh conversation was silently dropped
/// because `is_streaming()` was already true by the time the action
/// reached the dispatch branch.
///
/// This test drives `process_action` directly with a turn-active app —
/// the same shape `process_action` sees from the live keypress flow —
/// so a regression in the production code (not a mirrored helper) trips
/// the assertion.
#[tokio::test]
async fn first_message_sends_when_turn_active_already_flipped() {
    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    runner.is_replay = false;

    let mut agent = CountingAgent::new();
    let mut app = OilChatApp::default();
    // Mirror the post-`submit_user_message` state the real keypress flow
    // produces before the action reaches `process_action`.
    app.container_list_mut().mark_turn_active();
    assert!(app.is_streaming(), "precondition: turn must be active");

    let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));

    runner
        .process_action_for_test(
            Action::Send(ChatAppMsg::UserMessage("hello".into())),
            &mut app,
            &mut agent,
            &bridge,
        )
        .await
        .expect("process_action should not fail");

    assert_eq!(
        agent.sends.load(Ordering::Relaxed),
        1,
        "send must fire even when the turn is already active"
    );
}

/// Guard-rail for the Task 2.3b audit: the daemon-bound `ChatAppMsg`
/// variants must all carry an `is_replay` guard in `chat_runner.rs`.
///
/// If someone adds a new arm that calls into the daemon without the
/// `if !self.is_replay` guard, this test won't catch it (grep-based audit
/// is the real mechanism). But it DOES guarantee that the list of known
/// daemon-bound variants documented in the source stays in sync with the
/// swallow-arm at the end of the match.
#[test]
fn replay_swallow_arm_documented_variants() {
    let src = include_str!("../chat_runner/actions.rs");
    // The swallow arm that no-ops daemon-bound messages during replay.
    // If any of these disappear, the match arms above must also be
    // re-audited for their `is_replay` guards.
    for variant in [
        "ChatAppMsg::ReloadPlugin(_)",
        "ChatAppMsg::ExecuteSlashCommand(_)",
        "ChatAppMsg::ExportSession(_)",
        "ChatAppMsg::FetchModels",
    ] {
        assert!(
            src.contains(variant),
            "expected daemon-bound variant {} to still appear in chat_runner/actions.rs \
             swallow arm; audit the match arms and re-check their is_replay guards",
            variant
        );
    }
    // And the guards themselves — if someone drops the guard, this fails.
    for guard_line in [
        "ChatAppMsg::FetchModels if !self.is_replay",
        "ChatAppMsg::ReloadPlugin(ref name) if !self.is_replay",
        "ChatAppMsg::ExecuteSlashCommand(ref cmd) if !self.is_replay",
        "ChatAppMsg::ExportSession(ref export_path) if !self.is_replay",
    ] {
        assert!(
            src.contains(guard_line),
            "expected guarded arm `{}` in chat_runner/actions.rs; the Task 2.3b audit \
             requires every daemon-reaching match arm to guard on !self.is_replay",
            guard_line
        );
    }
}
