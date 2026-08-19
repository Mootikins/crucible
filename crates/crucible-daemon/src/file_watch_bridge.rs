//! Bridge between daemon event bus and the EventEmitter trait.
//!
//! Converts `SessionEvent` variants into the `SessionEventMessage` the daemon
//! broadcasts, so the `WatchManager` can push file changes to every subscribed
//! client.
//!
//! The conversion itself lives in [`crate::event_map`], not here. This file
//! used to hold its own three-arm `match`, and `server/file_event_hooks.rs`
//! held a second one for the same three events in the other direction; the two
//! could disagree with nothing to catch it, and neither could be extended
//! without editing both. One table now answers both.

use async_trait::async_trait;
use crucible_core::events::{EmitOutcome, EmitResult, EventEmitter, SessionEvent};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::debug;

use crate::event_emitter::emit_event;
use crate::protocol::SessionEventMessage;

/// Bridges `SessionEvent` emissions from the watch system into the daemon's
/// `broadcast::Sender<SessionEventMessage>` event bus.
///
/// An event with a row in [`crate::event_map`] is converted and broadcast; all
/// other variants pass through unchanged.
pub struct DaemonEventBridge {
    event_tx: broadcast::Sender<SessionEventMessage>,
}

impl DaemonEventBridge {
    /// Create a new bridge connected to the daemon event bus.
    pub fn new(event_tx: broadcast::Sender<SessionEventMessage>) -> Self {
        Self { event_tx }
    }
}

#[async_trait]
impl EventEmitter for DaemonEventBridge {
    type Event = SessionEvent;

    async fn emit(&self, event: Self::Event) -> EmitResult<EmitOutcome<Self::Event>> {
        // Only events with a row in the table are broadcast. Everything else
        // passes through untouched — the watcher emits pipeline signals this
        // bus has no name for.
        let msg = match &event {
            SessionEvent::Internal(inner) => crate::event_map::message_for(inner.as_ref()),
            _ => None,
        };

        if let Some(msg) = msg {
            debug!(event_type = %msg.event, "Broadcasting file event via daemon bus");
            if !emit_event(&self.event_tx, msg) {
                tracing::debug!("Failed to emit file watch event (no subscribers)");
            }
        }

        Ok(EmitOutcome::new(event))
    }

    async fn emit_recursive(
        &self,
        event: Self::Event,
    ) -> EmitResult<Vec<EmitOutcome<Self::Event>>> {
        self.emit(event).await.map(|outcome| vec![outcome])
    }

    fn is_available(&self) -> bool {
        true
    }
}

/// Create a shared event bridge for use with `WatchManager::with_emitter`.
pub fn create_event_bridge(
    event_tx: broadcast::Sender<SessionEventMessage>,
) -> Arc<dyn EventEmitter<Event = SessionEvent>> {
    Arc::new(DaemonEventBridge::new(event_tx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::events::{FileChangeKind, InternalSessionEvent};
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_bridge_broadcasts_file_changed() {
        let (tx, mut rx) = broadcast::channel(16);
        let bridge = DaemonEventBridge::new(tx);

        let event = SessionEvent::internal(InternalSessionEvent::FileChanged {
            path: PathBuf::from("/tmp/test.md"),
            kind: FileChangeKind::Modified,
        });

        let result = bridge.emit(event).await;
        assert!(result.is_ok());

        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.session_id, "system");
        assert_eq!(msg.event, "file_changed");
    }

    #[tokio::test]
    async fn test_bridge_broadcasts_file_deleted() {
        let (tx, mut rx) = broadcast::channel(16);
        let bridge = DaemonEventBridge::new(tx);

        let event = SessionEvent::internal(InternalSessionEvent::FileDeleted {
            path: PathBuf::from("/tmp/gone.md"),
        });

        let result = bridge.emit(event).await;
        assert!(result.is_ok());

        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.event, "file_deleted");
    }

    #[tokio::test]
    async fn test_bridge_broadcasts_file_moved() {
        let (tx, mut rx) = broadcast::channel(16);
        let bridge = DaemonEventBridge::new(tx);

        let event = SessionEvent::internal(InternalSessionEvent::FileMoved {
            from: PathBuf::from("/tmp/old.md"),
            to: PathBuf::from("/tmp/new.md"),
        });

        let result = bridge.emit(event).await;
        assert!(result.is_ok());

        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.event, "file_moved");
    }

    #[tokio::test]
    async fn test_bridge_ignores_non_file_events() {
        let (tx, mut rx) = broadcast::channel(16);
        let bridge = DaemonEventBridge::new(tx);

        let event = SessionEvent::Custom {
            name: "test".to_string(),
            payload: serde_json::Value::Null,
        };

        let result = bridge.emit(event).await;
        assert!(result.is_ok());

        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_bridge_is_available() {
        let (tx, _rx) = broadcast::channel(16);
        let bridge = DaemonEventBridge::new(tx);
        assert!(bridge.is_available());
    }
}
