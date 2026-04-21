//! Agent Event Bridge
//!
//! Holds the `SessionEvent` ring that the TUI polls. Production code
//! (chat runner) pushes events onto `bridge.ring` directly; the daemon
//! or agent runtime drives those pushes.

use std::sync::Arc;

use crucible_core::events::{EventRing, SessionEvent};

/// Bridge between agent output and the TUI's `SessionEvent` ring.
///
/// The bridge is a thin holder around `EventRing`. The chat runner
/// reaches into `bridge.ring` to push events produced as the agent
/// streams.
pub struct AgentEventBridge {
    pub(crate) ring: Arc<EventRing<SessionEvent>>,
}

impl AgentEventBridge {
    pub fn new(ring: Arc<EventRing<SessionEvent>>) -> Self {
        Self { ring }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bridge_creation() {
        let ring = Arc::new(EventRing::new(1024));
        let _bridge = AgentEventBridge::new(ring);
    }
}
