//! Bridge from a kiln's watcher to the kiln's index owner and to the daemon
//! event bus.
//!
//! The watcher reports each change once, as a `SessionEvent`. The bridge
//! queues it for the index owner (`kiln_manager/index.rs`), then converts it
//! into the `SessionEventMessage` the daemon broadcasts, so every subscribed
//! client and each Lua `FileChanged` handler sees it.
//!
//! The conversion itself lives in [`crate::event_map`], not here. This file
//! used to hold its own three-arm `match`, and `server/file_event_hooks.rs`
//! held a second one for the same three events in the other direction; the two
//! could disagree with nothing to catch it, and neither could be extended
//! without editing both. One table now answers both.

use async_trait::async_trait;
use crucible_core::events::{
    EmitOutcome, EmitResult, EventEmitter, InternalSessionEvent, SessionEvent,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::debug;

use crate::event_emitter::emit_event;
use crate::kiln_manager::{ChangeOrigin, IndexJob, IndexQueue};
use crate::protocol::SessionEventMessage;
use crate::watch::WATCH_RESCAN_EVENT;

/// Bridges the watcher of one kiln to the kiln's index owner and to the
/// daemon's event bus.
///
/// Each change goes first to the index owner's queue, which drops nothing,
/// and then to the bus, which drops events for a slow receiver. The index
/// must not depend on the bus. The echo of a daemon write goes to neither:
/// the index owner already indexed and announced that change.
pub struct DaemonEventBridge {
    event_tx: broadcast::Sender<SessionEventMessage>,
    /// The index owner's queue. `None` for a bridge that only broadcasts.
    index: Option<Arc<IndexQueue>>,
    /// The kiln this bridge's watcher watches, for a rescan.
    kiln: PathBuf,
}

impl DaemonEventBridge {
    /// A bridge for the watcher of `kiln`.
    pub(crate) fn new(
        event_tx: broadcast::Sender<SessionEventMessage>,
        index: Option<Arc<IndexQueue>>,
        kiln: &Path,
    ) -> Self {
        Self {
            event_tx,
            index,
            kiln: kiln.to_path_buf(),
        }
    }

    /// Queue `event` for the index owner, and decide whether to broadcast
    /// it. Returns false for the echo of a daemon change.
    async fn index(&self, event: &InternalSessionEvent) -> bool {
        let Some(index) = self.index.as_ref() else {
            return true;
        };
        let job = match event {
            InternalSessionEvent::FileChanged { path, kind } => {
                if index.is_echo_of_write(path).await {
                    return false;
                }
                IndexJob::Changed {
                    path: path.clone(),
                    kind: *kind,
                    origin: ChangeOrigin::Watcher,
                }
            }
            InternalSessionEvent::FileDeleted { path } => {
                if index.is_echo_of_removal(path) {
                    return false;
                }
                IndexJob::Deleted {
                    path: path.clone(),
                    origin: ChangeOrigin::Watcher,
                }
            }
            InternalSessionEvent::FileMoved { from, to } => {
                if index.is_echo_of_move(from, to) {
                    return false;
                }
                IndexJob::Moved {
                    from: from.clone(),
                    to: to.clone(),
                    origin: ChangeOrigin::Watcher,
                }
            }
            _ => return true,
        };
        index.push(job);
        true
    }
}

#[async_trait]
impl EventEmitter for DaemonEventBridge {
    type Event = SessionEvent;

    async fn emit(&self, event: Self::Event) -> EmitResult<EmitOutcome<Self::Event>> {
        match &event {
            SessionEvent::Internal(inner) => {
                if !self.index(inner).await {
                    debug!("Dropped the watcher's echo of a daemon change");
                    return Ok(EmitOutcome::new(event));
                }
                // A moved folder is for the index only. The bus has carried
                // file events, and a handler of `FileMoved` expects a file.
                let folder_move = matches!(
                    inner.as_ref(),
                    InternalSessionEvent::FileMoved { to, .. } if to.is_dir()
                );
                // Only events with a row in the table are broadcast. The
                // watcher emits pipeline signals this bus has no name for.
                if let Some(msg) =
                    crate::event_map::message_for(inner.as_ref()).filter(|_| !folder_move)
                {
                    debug!(event_type = %msg.event, "Broadcasting file event via daemon bus");
                    if !emit_event(&self.event_tx, msg) {
                        tracing::debug!("Failed to emit file watch event (no subscribers)");
                    }
                }
            }
            SessionEvent::Custom { name, .. } if name == WATCH_RESCAN_EVENT => {
                if let Some(index) = self.index.as_ref() {
                    index.push(IndexJob::Rescan {
                        kiln: self.kiln.clone(),
                    });
                }
            }
            _ => {}
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

/// Create the bridge for the watcher of `kiln`, for
/// `WatchManager::with_emitter`.
pub(crate) fn create_event_bridge(
    event_tx: broadcast::Sender<SessionEventMessage>,
    index: Option<Arc<IndexQueue>>,
    kiln: &Path,
) -> Arc<dyn EventEmitter<Event = SessionEvent>> {
    Arc::new(DaemonEventBridge::new(event_tx, index, kiln))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::events::{FileChangeKind, InternalSessionEvent};
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_bridge_broadcasts_file_changed() {
        let (tx, mut rx) = broadcast::channel(16);
        let bridge = DaemonEventBridge::new(tx, None, Path::new("/tmp"));

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
        let bridge = DaemonEventBridge::new(tx, None, Path::new("/tmp"));

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
        let bridge = DaemonEventBridge::new(tx, None, Path::new("/tmp"));

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
        let bridge = DaemonEventBridge::new(tx, None, Path::new("/tmp"));

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
        let bridge = DaemonEventBridge::new(tx, None, Path::new("/tmp"));
        assert!(bridge.is_available());
    }
}
