//! Utility components for performance and scalability.

mod debouncer;
mod queue;

pub use debouncer::Debouncer;
pub use queue::EventQueue;

use crate::watch::{FileEvent, FileEventKind};

/// Create a deduplication key for an event.
pub fn deduplication_key(event: &FileEvent) -> String {
    match &event.kind {
        FileEventKind::Created => format!("create:{}", event.path.display()),
        FileEventKind::Modified => format!("modify:{}", event.path.display()),
        FileEventKind::Deleted => format!("delete:{}", event.path.display()),
        FileEventKind::Moved { from, to } => {
            format!("move:{}->{}", from.display(), to.display())
        }
        FileEventKind::Batch(_) => format!("batch:{}", event.path.display()),
        FileEventKind::Rescan => "rescan".to_string(),
        FileEventKind::Unknown(_) => format!("unknown:{}", event.path.display()),
    }
}
