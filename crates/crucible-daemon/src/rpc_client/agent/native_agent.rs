//! Native `Agent` impl for `DaemonAgentHandle`.
//!
//! The daemon-proxy is the simplest of the three handles to implement
//! natively: the daemon runs the tool loop and event assembly internally,
//! so this side only observes `SessionEvent`s and translates them to
//! `TurnEvent`s. No inbound channel is consumed — the proxy never needs
//! to reply to the agent with tool results.

use std::sync::Arc;

use async_stream::stream;
use async_trait::async_trait;
use crucible_core::turn::{
    Agent, AgentCapabilities, AgentError, NotSupported, TurnContext, TurnError, TurnEvent,
};
use futures::stream::BoxStream;

use super::convert::session_event_to_turn_events;
use super::DaemonAgentHandle;

impl DaemonAgentHandle {
    /// Static capability set for the daemon proxy.
    ///
    /// The proxy is authoritatively whatever the daemon says it is; the
    /// flags here are the union of what any backend might support. UIs
    /// use capabilities as a pre-filter hint; individual setter RPCs
    /// return `NotSupported` if the concrete backend does not honour a
    /// feature.
    pub fn capabilities_static() -> AgentCapabilities {
        AgentCapabilities {
            streaming: true,
            tool_calls: true,
            thinking: true,
            model_switching: true,
            usage_reporting: true,
            cancellation: true,
            temperature_control: true,
            max_tokens_control: true,
            owns_history: false,
            modes: true,
        }
    }
}

#[async_trait]
impl Agent for DaemonAgentHandle {
    fn capabilities(&self) -> AgentCapabilities {
        Self::capabilities_static()
    }

    async fn turn<'a>(
        &'a mut self,
        ctx: TurnContext,
    ) -> Result<BoxStream<'a, TurnEvent>, AgentError> {
        let client = Arc::clone(&self.client);
        let session_id = self.session_id.clone();
        let streaming_rx = Arc::clone(&self.streaming_rx);
        let content = ctx.content;

        let body = stream! {
            tracing::debug!(session_id = %session_id, "Sending message to daemon (Agent::turn)");
            if let Err(e) = client.session_send_message(&session_id, &content, true).await {
                tracing::error!(error = %e, "Failed to send message to daemon");
                yield TurnEvent::Error(TurnError::Communication(format!(
                    "Failed to send message: {e}"
                )));
                return;
            }

            let mut rx = streaming_rx.lock().await;
            while let Some(event) = rx.recv().await {
                let events = session_event_to_turn_events(&event);
                let terminal = events.iter().any(|e| {
                    matches!(e, TurnEvent::Done { .. } | TurnEvent::Error(_))
                });
                for turn_event in events {
                    yield turn_event;
                }
                if terminal {
                    return;
                }
            }

            // Receiver closed before a terminal event arrived.
            yield TurnEvent::Error(TurnError::Connection(
                "Event channel closed".to_string(),
            ));
        };

        Ok(Box::pin(body))
    }

    async fn cancel(&self) -> Result<(), AgentError> {
        self.client
            .session_cancel(&self.session_id)
            .await
            .map(|_| ())
            .map_err(|e| AgentError::Communication(e.to_string()))
    }

    async fn switch_model(&mut self, model_id: &str) -> Result<(), NotSupported> {
        self.client
            .session_switch_model(&self.session_id, model_id)
            .await
            .map_err(|_| NotSupported::new("switch_model"))?;
        self.cached_model = Some(model_id.to_string());
        Ok(())
    }
}

// Force the terminal `Done` variant check at compile time — if a new
// terminal variant is added to `TurnEvent`, this match must be updated so
// the `turn()` loop above still exits cleanly.
#[cfg(test)]
#[allow(dead_code)]
fn _terminal_variant_check(event: TurnEvent) -> bool {
    match event {
        TurnEvent::Done { .. } | TurnEvent::Error(_) => true,
        TurnEvent::TextDelta(_)
        | TurnEvent::Thinking(_)
        | TurnEvent::ToolCall { .. }
        | TurnEvent::ToolResult { .. }
        | TurnEvent::ToolCallDiffUpdate { .. }
        | TurnEvent::ToolBatchEnd
        | TurnEvent::HandlerInjection { .. }
        | TurnEvent::DepthCapHit { .. }
        | TurnEvent::Usage(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Object-safety smoke test: `Box<dyn Agent>` must compile around the
    // concrete handle.
    #[allow(dead_code)]
    fn object_safe(_: &dyn Agent) {}

    #[test]
    fn capabilities_claim_full_superset() {
        let caps = DaemonAgentHandle::capabilities_static();
        assert!(caps.streaming);
        assert!(caps.tool_calls);
        assert!(caps.thinking);
        assert!(caps.model_switching);
        assert!(caps.usage_reporting);
        assert!(caps.cancellation);
        assert!(!caps.owns_history);
    }
}
