//! Utility components for performance and scalability.

mod batcher;
mod debouncer;
mod filter;
mod monitor;
mod queue;

pub use debouncer::*;
pub use monitor::*;
pub use queue::*;

use crate::FileEvent;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Utility functions for file event processing.
pub struct EventUtils;

#[allow(dead_code)]
impl EventUtils {
    /// Calculate a hash for a file event.
    pub fn event_hash(event: &FileEvent) -> u64 {
        let mut hasher = DefaultHasher::new();

        // Hash event kind
        match &event.kind {
            crate::events::FileEventKind::Created => 0.hash(&mut hasher),
            crate::events::FileEventKind::Modified => 1.hash(&mut hasher),
            crate::events::FileEventKind::Deleted => 2.hash(&mut hasher),
            crate::events::FileEventKind::Moved { from, to } => {
                3.hash(&mut hasher);
                from.hash(&mut hasher);
                to.hash(&mut hasher);
            }
            crate::events::FileEventKind::Batch(events) => {
                4.hash(&mut hasher);
                events.len().hash(&mut hasher);
                for e in events {
                    Self::event_hash(e).hash(&mut hasher);
                }
            }
            crate::events::FileEventKind::Unknown(s) => {
                5.hash(&mut hasher);
                s.hash(&mut hasher);
            }
        }

        // Hash path
        event.path.hash(&mut hasher);

        // Hash timestamp (truncated to seconds for deduplication)
        event.timestamp.timestamp().hash(&mut hasher);

        hasher.finish()
    }

    /// Check if two events are duplicates.
    pub fn are_duplicates(event1: &FileEvent, event2: &FileEvent) -> bool {
        // Same kind and path
        if event1.kind != event2.kind || event1.path != event2.path {
            return false;
        }

        // Check if timestamps are close enough (within 1 second)
        let time_diff = (event1.timestamp - event2.timestamp).num_seconds().abs();
        time_diff <= 1
    }

    /// Create a deduplication key for an event.
    pub fn deduplication_key(event: &FileEvent) -> String {
        match &event.kind {
            crate::events::FileEventKind::Created => {
                format!("create:{}", event.path.display())
            }
            crate::events::FileEventKind::Modified => {
                format!("modify:{}", event.path.display())
            }
            crate::events::FileEventKind::Deleted => {
                format!("delete:{}", event.path.display())
            }
            crate::events::FileEventKind::Moved { from, to } => {
                format!("move:{}->{}", from.display(), to.display())
            }
            crate::events::FileEventKind::Batch(_) => {
                format!("batch:{}", event.path.display())
            }
            crate::events::FileEventKind::Unknown(_) => {
                format!("unknown:{}", event.path.display())
            }
        }
    }

    /// Estimate memory usage of an event.
    pub fn estimate_event_size(event: &FileEvent) -> usize {
        let base_size = std::mem::size_of::<FileEvent>();
        let path_size = event.path.as_os_str().len();
        let metadata_size = event
            .metadata
            .as_ref()
            .map(|m| {
                std::mem::size_of::<crate::events::EventMetadata>()
                    + m.size.map_or(0, |_| 8)
                    + m.permissions.map_or(0, |_| 4)
                    + m.mime_type.as_ref().map_or(0, |s| s.len())
                    + m.content_hash.as_ref().map_or(0, |s| s.len())
                    + m.backend.len()
                    + m.watch_id.len()
            })
            .unwrap_or(0);

        base_size + path_size + metadata_size
    }

    /// Check if an event is high priority.
    pub fn is_high_priority(event: &FileEvent) -> bool {
        match &event.kind {
            // Deletions and moves are typically high priority
            crate::events::FileEventKind::Deleted | crate::events::FileEventKind::Moved { .. } => {
                true
            }

            // Creation/modification of important file types
            crate::events::FileEventKind::Created | crate::events::FileEventKind::Modified => {
                if let Some(ext) = event.extension() {
                    matches!(
                        ext.as_str(),
                        "md" | "txt" | "json" | "yaml" | "toml"
                    )
                } else {
                    false
                }
            }

            _ => false,
        }
    }

    /// Group events by directory.
    pub fn group_by_directory(
        events: &[FileEvent],
    ) -> std::collections::HashMap<std::path::PathBuf, Vec<&FileEvent>> {
        let mut groups: std::collections::HashMap<std::path::PathBuf, Vec<&FileEvent>> =
            std::collections::HashMap::new();

        for event in events {
            let parent = event
                .parent()
                .unwrap_or_else(|| std::path::PathBuf::from("/"));
            groups.entry(parent).or_default().push(event);
        }

        groups
    }

    /// Filter events by time window.
    pub fn filter_by_time_window(
        events: &[FileEvent],
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Vec<&FileEvent> {
        events
            .iter()
            .filter(|event| event.timestamp >= start && event.timestamp <= end)
            .collect()
    }

    /// Create a summary of events.
    pub fn create_summary(events: &[FileEvent]) -> EventSummary {
        let mut summary = EventSummary::default();

        for event in events {
            summary.total_events += 1;
            summary.total_size += Self::estimate_event_size(event);

            match event.kind {
                crate::events::FileEventKind::Created => summary.created += 1,
                crate::events::FileEventKind::Modified => summary.modified += 1,
                crate::events::FileEventKind::Deleted => summary.deleted += 1,
                crate::events::FileEventKind::Moved { .. } => summary.moved += 1,
                crate::events::FileEventKind::Batch(ref batch) => {
                    summary.batches += 1;
                    summary.total_events += batch.len() as u64; // Count batched events
                }
                crate::events::FileEventKind::Unknown(_) => summary.unknown += 1,
            }

            if event.is_dir {
                summary.directories += 1;
            } else {
                summary.files += 1;
            }
        }

        summary
    }
}

/// Summary statistics for a collection of events.
#[derive(Debug, Clone, Default)]
pub struct EventSummary {
    /// Total number of events
    pub total_events: u64,
    /// Number of created items
    pub created: u64,
    /// Number of modified items
    pub modified: u64,
    /// Number of deleted items
    pub deleted: u64,
    /// Number of moved items
    pub moved: u64,
    /// Number of batch events
    pub batches: u64,
    /// Number of unknown events
    pub unknown: u64,
    /// Number of files
    pub files: u64,
    /// Number of directories
    pub directories: u64,
    /// Estimated total size in bytes
    pub total_size: usize,
}

impl EventSummary {
    /// Get the percentage of events that are files.
    #[allow(dead_code)]
    pub fn file_percentage(&self) -> f64 {
        if self.total_events == 0 {
            0.0
        } else {
            (self.files as f64 / self.total_events as f64) * 100.0
        }
    }

    /// Get the percentage of events that are directories.
    #[allow(dead_code)]
    pub fn directory_percentage(&self) -> f64 {
        if self.total_events == 0 {
            0.0
        } else {
            (self.directories as f64 / self.total_events as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileEvent, FileEventKind};
    use chrono::Utc;
    use std::path::PathBuf;

    #[test]
    fn test_event_hash() {
        let event1 = FileEvent::new(FileEventKind::Created, PathBuf::from("test.txt"));
        let event2 = FileEvent::new(FileEventKind::Created, PathBuf::from("test.txt"));
        let event3 = FileEvent::new(FileEventKind::Modified, PathBuf::from("test.txt"));

        assert_eq!(
            EventUtils::event_hash(&event1),
            EventUtils::event_hash(&event2)
        );
        assert_ne!(
            EventUtils::event_hash(&event1),
            EventUtils::event_hash(&event3)
        );
    }

    #[test]
    fn test_duplicate_detection() {
        let mut event1 = FileEvent::new(FileEventKind::Created, PathBuf::from("test.txt"));
        let mut event2 = FileEvent::new(FileEventKind::Created, PathBuf::from("test.txt"));

        // Set timestamps close together
        let now = Utc::now();
        event1.timestamp = now;
        event2.timestamp = now + chrono::Duration::milliseconds(500);

        assert!(EventUtils::are_duplicates(&event1, &event2));
    }

    #[test]
    fn test_deduplication_key() {
        let event = FileEvent::new(FileEventKind::Created, PathBuf::from("test.txt"));
        let key = EventUtils::deduplication_key(&event);
        assert_eq!(key, "create:test.txt");
    }

    #[test]
    fn test_high_priority_detection() {
        let md_event = FileEvent::new(FileEventKind::Modified, PathBuf::from("test.md"));
        let txt_event = FileEvent::new(FileEventKind::Modified, PathBuf::from("test.txt"));
        let exe_event = FileEvent::new(FileEventKind::Modified, PathBuf::from("test.exe"));
        let delete_event = FileEvent::new(FileEventKind::Deleted, PathBuf::from("test.txt"));

        assert!(EventUtils::is_high_priority(&md_event));
        assert!(EventUtils::is_high_priority(&txt_event));
        assert!(!EventUtils::is_high_priority(&exe_event));
        assert!(EventUtils::is_high_priority(&delete_event));
    }

    #[test]
    fn test_event_summary() {
        let events = vec![
            FileEvent::new(FileEventKind::Created, PathBuf::from("test1.md")),
            FileEvent::new(FileEventKind::Modified, PathBuf::from("test2.txt")),
            FileEvent::new(FileEventKind::Deleted, PathBuf::from("test3.md")),
        ];

        let summary = EventUtils::create_summary(&events);
        assert_eq!(summary.total_events, 3);
        assert_eq!(summary.created, 1);
        assert_eq!(summary.modified, 1);
        assert_eq!(summary.deleted, 1);
        assert_eq!(summary.files, 3);
        assert_eq!(summary.file_percentage(), 100.0);
    }

    #[test]
    fn test_group_by_directory() {
        let events = vec![
            FileEvent::new(FileEventKind::Created, PathBuf::from("dir1/test1.md")),
            FileEvent::new(FileEventKind::Modified, PathBuf::from("dir1/test2.txt")),
            FileEvent::new(FileEventKind::Created, PathBuf::from("dir2/test3.md")),
        ];

        let groups = EventUtils::group_by_directory(&events);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups.get(&PathBuf::from("dir1")).unwrap().len(), 2);
        assert_eq!(groups.get(&PathBuf::from("dir2")).unwrap().len(), 1);
    }
}
