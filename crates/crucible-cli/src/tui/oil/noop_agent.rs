//! `AgentHandle` that no-ops everything — used for pure-display replay.
//!
//! Only the methods that are required by the trait (no default impl) or
//! that need to deviate from the defaults for replay-safety are overridden
//! here. Everything else — `set_*` / `get_*` accessors, `clear_history`,
//! `cancel`, and friends — uses the `AgentHandle` default impls, which are
//! already local / side-effect-free.

use async_trait::async_trait;
use futures::stream::{self, BoxStream, StreamExt};

use crucible_core::interaction::InteractionEvent;
use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatResult};
use crucible_core::turn::{
    Agent, AgentCapabilities, AgentError, NotSupported, StopReason, TurnContext, TurnEvent,
};
use tokio::sync::mpsc;

/// No-op [`AgentHandle`] used for pure-display replay.
///
/// Owns a synthetic session id (for [`AgentHandle::session_id`]) and a
/// dropped-sender interaction receiver, so consumers that wait on
/// interactions see `None` immediately rather than hanging forever.
pub struct NoopAgentHandle {
    session_id: String,
    interaction_rx: Option<mpsc::UnboundedReceiver<InteractionEvent>>,
}

impl NoopAgentHandle {
    pub fn new(session_id: String) -> Self {
        // Dropped-sender channel: the first `.recv().await` on the receiver
        // yields `None`, so consumers never block.
        let (_tx, rx) = mpsc::unbounded_channel::<InteractionEvent>();
        Self {
            session_id,
            interaction_rx: Some(rx),
        }
    }
}

#[async_trait]
impl Agent for NoopAgentHandle {
    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities::default()
    }

    async fn turn<'a>(
        &'a mut self,
        _ctx: TurnContext,
    ) -> Result<BoxStream<'a, TurnEvent>, AgentError> {
        Ok(stream::iter(vec![TurnEvent::Done {
            stop_reason: StopReason::Empty,
        }])
        .boxed())
    }

    async fn cancel(&self) -> Result<(), AgentError> {
        Ok(())
    }

    async fn switch_model(&mut self, _model_id: &str) -> Result<(), NotSupported> {
        Err(NotSupported::new("switch_model"))
    }
}

#[async_trait]
impl AgentHandle for NoopAgentHandle {
    fn send_message_stream(
        &mut self,
        _message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        stream::empty().boxed()
    }

    async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
        Ok(())
    }

    fn take_interaction_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<InteractionEvent>> {
        self.interaction_rx.take()
    }

    fn session_id(&self) -> Option<&str> {
        Some(&self.session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn noop_agent_session_id_returns_constructor_arg() {
        let agent = NoopAgentHandle::new("replay-session-42".into());
        assert_eq!(agent.session_id(), Some("replay-session-42"));
    }

    #[tokio::test]
    async fn noop_agent_send_message_stream_is_empty() {
        let mut agent = NoopAgentHandle::new("replay-test".into());
        let mut s = agent.send_message_stream("hi".into());
        assert!(s.next().await.is_none());
    }

    #[tokio::test]
    async fn noop_agent_take_interaction_receiver_yields_none_on_recv() {
        let mut agent = NoopAgentHandle::new("replay-test".into());
        let mut rx = agent
            .take_interaction_receiver()
            .expect("receiver should be available on first take");
        // Sender was dropped immediately in `new`, so recv() resolves to None.
        assert!(rx.recv().await.is_none());
        // Subsequent takes return None.
        assert!(agent.take_interaction_receiver().is_none());
    }

    /// Smoke-test core replay-safety behaviors: no panics, no blocking,
    /// and the session id round-trips through the constructor.
    #[tokio::test]
    async fn noop_agent_is_benign_for_replay() {
        let mut agent = NoopAgentHandle::new("replay-all".into());

        // Streams are empty, so replay never waits on the agent.
        {
            let mut s = agent.send_message_stream("hi".into());
            assert!(s.next().await.is_none());
        }

        // set_mode_str must succeed — it has no default impl.
        assert!(agent.set_mode_str("plan").await.is_ok());

        // Session id matches constructor arg.
        assert_eq!(agent.session_id(), Some("replay-all"));

        // take_interaction_receiver: first call Some, second None.
        assert!(agent.take_interaction_receiver().is_some());
        assert!(agent.take_interaction_receiver().is_none());
    }
}
