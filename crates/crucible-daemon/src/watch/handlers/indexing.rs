//! The watcher handler that reports kiln file changes.
//!
//! It turns each [`FileEvent`] into a `SessionEvent` (`FileChanged`,
//! `FileDeleted`, `FileMoved`, or the rescan signal) and gives it to the
//! emitter. For a kiln, the emitter is the kiln's bridge
//! (`file_watch_bridge.rs`): it queues the change for the index owner and
//! broadcasts it. Parsing and embedding belong to the `NotePipeline`, which
//! the index owner runs.
//!
//! This handler used to carry an `index_file`, a `remove_file_index` and a
//! move handler that checked a path and logged, and indexed nothing. They are
//! gone, because the name promised work that another owner does.

use crate::watch::{
    error::Result,
    events::{FileEvent, FileEventKind},
    traits::EventHandler,
};
use async_trait::async_trait;
use crucible_core::events::{EventEmitter, FileChangeKind, InternalSessionEvent, SessionEvent};
use std::sync::Arc;
use tracing::{debug, warn};

/// The `SessionEvent::Custom` name of the watcher's rescan signal.
///
/// `SessionEvent` has no variant for "events were lost", and the signal
/// never leaves the daemon: the kiln bridge turns it into an index rescan.
pub const WATCH_RESCAN_EVENT: &str = "watch_rescan";

pub struct IndexingHandler {
    emitter: Arc<dyn EventEmitter<Event = SessionEvent>>,
}

impl IndexingHandler {
    pub fn with_emitter(emitter: Arc<dyn EventEmitter<Event = SessionEvent>>) -> Result<Self> {
        Ok(Self { emitter })
    }

    /// Whether this handler reports `event`.
    ///
    /// The predicate for a file is the canonical one, not a list this handler
    /// owns. A moved directory has no extension, and it carries each note
    /// under it, so it counts. A rescan counts, because it stands for events
    /// of any kind.
    fn reports(event: &FileEvent) -> bool {
        match &event.kind {
            FileEventKind::Rescan | FileEventKind::Batch(_) => true,
            FileEventKind::Moved { from, to } => {
                to.is_dir()
                    || crucible_core::kiln::is_indexable_file(from)
                    || crucible_core::kiln::is_indexable_file(to)
            }
            FileEventKind::Unknown(_) => false,
            FileEventKind::Created | FileEventKind::Modified | FileEventKind::Deleted => {
                !event.is_dir && crucible_core::kiln::is_indexable_file(&event.path)
            }
        }
    }

    /// Give `event` to the emitter, as the `SessionEvent` it stands for.
    async fn report(&self, event: &FileEvent) {
        let session_event = match &event.kind {
            FileEventKind::Created => SessionEvent::internal(InternalSessionEvent::FileChanged {
                path: event.path.clone(),
                kind: FileChangeKind::Created,
            }),
            FileEventKind::Modified => SessionEvent::internal(InternalSessionEvent::FileChanged {
                path: event.path.clone(),
                kind: FileChangeKind::Modified,
            }),
            FileEventKind::Deleted => SessionEvent::internal(InternalSessionEvent::FileDeleted {
                path: event.path.clone(),
            }),
            FileEventKind::Moved { from, to } => {
                SessionEvent::internal(InternalSessionEvent::FileMoved {
                    from: from.clone(),
                    to: to.clone(),
                })
            }
            FileEventKind::Rescan => SessionEvent::Custom {
                name: WATCH_RESCAN_EVENT.to_string(),
                payload: serde_json::Value::Null,
            },
            FileEventKind::Batch(events) => {
                for inner in events.iter().filter(|inner| Self::reports(inner)) {
                    // Boxed: a batch can nest, and a recursive async call
                    // needs a future of known size.
                    Box::pin(self.report(inner)).await;
                }
                return;
            }
            FileEventKind::Unknown(_) => {
                debug!(
                    "Not reporting an unknown file event: {}",
                    event.path.display()
                );
                return;
            }
        };

        match self.emitter.emit(session_event).await {
            Ok(outcome) if outcome.has_errors() => warn!(
                "File event had {} handler errors for: {}",
                outcome.error_count(),
                event.path.display()
            ),
            Ok(_) => {}
            Err(e) => warn!(
                "Failed to emit the file event for {}: {}",
                event.path.display(),
                e
            ),
        }
    }
}

#[async_trait]
impl EventHandler for IndexingHandler {
    async fn handle(&self, event: FileEvent) -> Result<()> {
        // The registry asks `can_handle` first, and a direct caller may not.
        if Self::reports(&event) {
            self.report(&event).await;
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "indexing"
    }

    fn priority(&self) -> u32 {
        200 // High priority for indexing
    }

    fn can_handle(&self, event: &FileEvent) -> bool {
        Self::reports(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::events::{EmitOutcome, EmitResult};
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// An emitter that keeps what it gets.
    #[derive(Default)]
    struct Kept(Mutex<Vec<SessionEvent>>);

    #[async_trait]
    impl EventEmitter for Kept {
        type Event = SessionEvent;

        async fn emit(&self, event: SessionEvent) -> EmitResult<EmitOutcome<SessionEvent>> {
            self.0.lock().unwrap().push(event.clone());
            Ok(EmitOutcome::new(event))
        }

        async fn emit_recursive(
            &self,
            event: SessionEvent,
        ) -> EmitResult<Vec<EmitOutcome<SessionEvent>>> {
            self.emit(event).await.map(|outcome| vec![outcome])
        }

        fn is_available(&self) -> bool {
            true
        }
    }

    async fn reported(event: FileEvent) -> Vec<SessionEvent> {
        let kept = Arc::new(Kept::default());
        let handler = IndexingHandler::with_emitter(kept.clone()).unwrap();
        if handler.can_handle(&event) {
            handler.handle(event).await.unwrap();
        }
        let events = kept.0.lock().unwrap().clone();
        events
    }

    #[tokio::test]
    async fn a_batch_reports_each_indexable_child_once() {
        let batch = FileEvent::new(
            FileEventKind::Batch(vec![
                FileEvent::new(FileEventKind::Modified, PathBuf::from("/k/a.md")),
                FileEvent::new(FileEventKind::Modified, PathBuf::from("/k/image.png")),
                FileEvent::new(FileEventKind::Deleted, PathBuf::from("/k/b.md")),
            ]),
            PathBuf::new(),
        );
        let events = reported(batch).await;
        assert_eq!(
            events.len(),
            2,
            "one report per indexable child: {events:?}"
        );
    }

    #[tokio::test]
    async fn a_rescan_is_reported_as_the_rescan_signal() {
        let events = reported(FileEvent::new(FileEventKind::Rescan, PathBuf::new())).await;
        assert!(
            matches!(&events[..], [SessionEvent::Custom { name, .. }] if name == WATCH_RESCAN_EVENT),
            "got {events:?}"
        );
    }

    #[tokio::test]
    async fn a_moved_note_is_reported_as_a_move() {
        let events = reported(FileEvent::new(
            FileEventKind::Moved {
                from: PathBuf::from("/k/a.md"),
                to: PathBuf::from("/k/b.md"),
            },
            PathBuf::from("/k/a.md"),
        ))
        .await;
        assert!(
            matches!(
                &events[..],
                [SessionEvent::Internal(inner)]
                    if matches!(inner.as_ref(), InternalSessionEvent::FileMoved { .. })
            ),
            "got {events:?}"
        );
    }
}
