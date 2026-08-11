//! Feeds observed worktree writes to the review ledger's backstop.
//!
//! Deliberately does no work beyond classification. It runs on the watch
//! event path, which is shared with indexing, so anything expensive here
//! delays every other handler; the composed diff is recomputed on demand by
//! whoever reacts to [`ExternalChangeTracker::subscribe`].

use std::sync::Arc;

use async_trait::async_trait;
use tracing::trace;

use crate::watch::{
    error::Result,
    events::{FileEvent, FileEventKind},
    external_changes::{ExternalChangeTracker, Ownership},
    traits::EventHandler,
};

/// Records worktree writes that no capture bracket owns.
pub struct ExternalChangeHandler {
    tracker: Arc<ExternalChangeTracker>,
}

impl ExternalChangeHandler {
    pub fn new(tracker: Arc<ExternalChangeTracker>) -> Self {
        Self { tracker }
    }

    /// Classify every path an event touches.
    ///
    /// Written iteratively rather than recursively because a batch event can
    /// nest, and a recursive `async fn` needs a boxed future per level for a
    /// job that is a `HashSet` insert.
    fn observe_all(&self, event: &FileEvent) {
        let mut pending = vec![event];
        while let Some(event) = pending.pop() {
            match &event.kind {
                FileEventKind::Batch(inner) => pending.extend(inner.iter()),
                // Both ends matter: the source stops existing and the
                // destination starts, and each is its own hunk in the
                // composed diff.
                FileEventKind::Moved { from, to } => {
                    self.observe(from);
                    self.observe(to);
                }
                FileEventKind::Created | FileEventKind::Modified | FileEventKind::Deleted => {
                    self.observe(&event.path)
                }
                // A backend that could not classify the change still saw one.
                // Treating it as nothing would be the silent-loss failure this
                // handler exists to prevent, so observe it and let the
                // composed diff decide whether anything actually moved.
                FileEventKind::Unknown(_) => self.observe(&event.path),
            }
        }
    }

    fn observe(&self, path: &std::path::Path) {
        let ownership = self.tracker.observe(path);
        if ownership != Ownership::Untracked {
            trace!(path = %path.display(), ?ownership, "review backstop observed write");
        }
    }
}

#[async_trait]
impl EventHandler for ExternalChangeHandler {
    async fn handle(&self, event: FileEvent) -> Result<()> {
        self.observe_all(&event);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "external-change"
    }

    fn priority(&self) -> u32 {
        // Below indexing (200). Nothing downstream reads this handler's
        // output synchronously, so it should not delay the pipeline that
        // does.
        150
    }

    fn can_handle(&self, event: &FileEvent) -> bool {
        // A directory event carries no content change of its own; the files
        // inside it generate their own events. A batch event's own path is
        // empty and never a directory, so batches still get through.
        !event.is_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn fixture() -> (Arc<ExternalChangeTracker>, ExternalChangeHandler, PathBuf) {
        let root = PathBuf::from("/repo");
        let tracker = Arc::new(ExternalChangeTracker::default());
        tracker.track("s1", std::slice::from_ref(&root));
        let handler = ExternalChangeHandler::new(Arc::clone(&tracker));
        (tracker, handler, root)
    }

    #[tokio::test]
    async fn a_modification_with_no_bracket_open_is_recorded() {
        let (tracker, handler, root) = fixture();

        handler
            .handle(FileEvent::new(
                FileEventKind::Modified,
                PathBuf::from("/repo/src/main.rs"),
            ))
            .await
            .unwrap();

        assert_eq!(
            tracker.external_paths(&root),
            vec![PathBuf::from("/repo/src/main.rs")]
        );
    }

    #[tokio::test]
    async fn a_modification_inside_a_capture_window_is_not_recorded() {
        let (tracker, handler, root) = fixture();
        let _window = tracker.capture("s1");

        handler
            .handle(FileEvent::new(
                FileEventKind::Modified,
                PathBuf::from("/repo/src/main.rs"),
            ))
            .await
            .unwrap();

        assert!(tracker.external_paths(&root).is_empty());
    }

    #[tokio::test]
    async fn batched_events_are_recorded_member_by_member() {
        let (tracker, handler, root) = fixture();
        let batch = FileEvent::new(
            FileEventKind::Batch(vec![
                FileEvent::new(FileEventKind::Modified, PathBuf::from("/repo/a.rs")),
                FileEvent::new(FileEventKind::Created, PathBuf::from("/repo/b.rs")),
                // The batch wrapper is filtered as a whole by the backend, so
                // members are the only place noise can be caught.
                FileEvent::new(FileEventKind::Modified, PathBuf::from("/repo/target/x")),
            ]),
            PathBuf::new(),
        );

        handler.handle(batch).await.unwrap();

        let mut recorded = tracker.external_paths(&root);
        recorded.sort();
        assert_eq!(
            recorded,
            vec![PathBuf::from("/repo/a.rs"), PathBuf::from("/repo/b.rs")]
        );
    }

    #[tokio::test]
    async fn a_move_records_both_ends() {
        let (tracker, handler, root) = fixture();

        handler
            .handle(FileEvent::new(
                FileEventKind::Moved {
                    from: PathBuf::from("/repo/old.rs"),
                    to: PathBuf::from("/repo/new.rs"),
                },
                PathBuf::from("/repo/new.rs"),
            ))
            .await
            .unwrap();

        let mut recorded = tracker.external_paths(&root);
        recorded.sort();
        assert_eq!(
            recorded,
            vec![PathBuf::from("/repo/new.rs"), PathBuf::from("/repo/old.rs")]
        );
    }

    #[test]
    fn directory_events_are_skipped() {
        let (_tracker, handler, _root) = fixture();
        let mut event = FileEvent::new(FileEventKind::Modified, PathBuf::from("/repo/src"));
        event.is_dir = true;
        assert!(!handler.can_handle(&event));

        let file = FileEvent::new(FileEventKind::Modified, PathBuf::from("/repo/src/main.rs"));
        assert!(handler.can_handle(&file));
    }

    #[tokio::test]
    async fn events_outside_tracked_roots_are_ignored() {
        let (tracker, handler, root) = fixture();

        handler
            .handle(FileEvent::new(
                FileEventKind::Modified,
                Path::new("/elsewhere/main.rs").to_path_buf(),
            ))
            .await
            .unwrap();

        assert!(tracker.external_paths(&root).is_empty());
    }
}
