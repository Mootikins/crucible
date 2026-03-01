//! Utility components for performance and scalability.

mod debouncer;
mod monitor;
mod queue;

pub use debouncer::Debouncer;
pub use monitor::{PerformanceMonitor, PerformanceStats};
pub use queue::{EventQueue, QueueStats};

use crate::watch::FileEvent;

/// Utility functions for file event processing.
pub struct EventUtils;

impl EventUtils {
    /// Create a deduplication key for an event.
    pub fn deduplication_key(event: &FileEvent) -> String {
        match &event.kind {
            crate::watch::events::FileEventKind::Created => {
                format!("create:{}", event.path.display())
            }
            crate::watch::events::FileEventKind::Modified => {
                format!("modify:{}", event.path.display())
            }
            crate::watch::events::FileEventKind::Deleted => {
                format!("delete:{}", event.path.display())
            }
            crate::watch::events::FileEventKind::Moved { from, to } => {
                format!("move:{}->{}", from.display(), to.display())
            }
            crate::watch::events::FileEventKind::Batch(_) => {
                format!("batch:{}", event.path.display())
            }
            crate::watch::events::FileEventKind::Unknown(_) => {
                format!("unknown:{}", event.path.display())
            }
        }
    }
}
