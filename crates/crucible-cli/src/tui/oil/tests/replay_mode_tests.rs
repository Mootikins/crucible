//! Tests that `process_message` gates `send_message_stream` on `is_replay`.
//!
//! Replay mode delivers user messages as SessionEvent broadcasts from the
//! daemon's replay session — the TUI must not re-send them via RPC. This
//! module uses a counting mock agent to assert the gate.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use crucible_core::events::EventRing;
use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatResult};
use futures::stream::BoxStream;

use crate::chat::bridge::AgentEventBridge;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::chat_runner::OilChatRunner;

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

#[async_trait]
impl AgentHandle for CountingAgent {
    fn send_message_stream(
        &mut self,
        _message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        self.sends.fetch_add(1, Ordering::Relaxed);
        Box::pin(futures::stream::empty())
    }

    async fn set_mode_str(
        &mut self,
        _mode_id: &str,
    ) -> ChatResult<()> {
        Ok(())
    }

    fn is_connected(&self) -> bool {
        true
    }
}

#[test]
fn replay_user_message_does_not_invoke_send_message_stream() {
    let mut agent = CountingAgent::new();
    let mut app = OilChatApp::default();
    let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));
    let mut active_stream = None;

    let _ = OilChatRunner::process_message_for_test(
        &ChatAppMsg::UserMessage("hi".into()),
        &mut app,
        &mut agent,
        &bridge,
        &mut active_stream,
        /* is_replay */ true,
    );

    assert_eq!(agent.sends.load(Ordering::Relaxed), 0);
    assert!(active_stream.is_none());
}

#[test]
fn live_user_message_invokes_send_message_stream_once() {
    let mut agent = CountingAgent::new();
    let mut app = OilChatApp::default();
    app.set_precognition(false);
    let bridge = AgentEventBridge::new(Arc::new(EventRing::new(16)));
    let mut active_stream = None;

    let _ = OilChatRunner::process_message_for_test(
        &ChatAppMsg::UserMessage("hi".into()),
        &mut app,
        &mut agent,
        &bridge,
        &mut active_stream,
        /* is_replay */ false,
    );

    assert_eq!(agent.sends.load(Ordering::Relaxed), 1);
    assert!(active_stream.is_some());
}
