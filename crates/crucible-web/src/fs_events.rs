//! Filesystem-change events forwarded to the browser over SSE.
//!
//! The daemon's file watcher broadcasts `file_changed` / `file_deleted` /
//! `file_moved` [`SessionEvent`]s on session id `"system"`
//! (`file_watch_bridge.rs`). [`FsEvent::from_daemon_event`] projects those into
//! the browser wire shape consumed by the web file-tree explorer's reconciler.
//!
//! Only `file_*` watcher events drive this channel — `note_*` DB events are NOT
//! forwarded (they never reach the broadcast bus; see plan §4). In Phase 1 only
//! kiln directories are watched, so project events never actually arrive, but
//! the mapping handles all three defensively so lighting up project watching
//! later needs no web change.

use crucible_daemon::SessionEvent;
use serde::Serialize;

/// A filesystem change delivered to the browser.
///
/// Serializes with an internal `type` tag whose values are `changed` /
/// `deleted` / `moved` (e.g. `{"type":"changed","path":"/abs","kind":"modified"}`).
/// The SSE `event:` name is separate — see [`FsEvent::event_name`].
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FsEvent {
    Changed { path: String, kind: String },
    Deleted { path: String },
    Moved { from: String, to: String },
}

impl FsEvent {
    /// SSE `event:` field name (distinct from the serde `type` tag).
    pub fn event_name(&self) -> &'static str {
        match self {
            FsEvent::Changed { .. } => "fs_changed",
            FsEvent::Deleted { .. } => "fs_deleted",
            FsEvent::Moved { .. } => "fs_moved",
        }
    }

    /// Project a daemon watcher event into an [`FsEvent`], or `None` for any
    /// event type that is not a filesystem change.
    pub fn from_daemon_event(ev: &SessionEvent) -> Option<Self> {
        let d = &ev.data;
        match ev.event_type.as_str() {
            "file_changed" => Some(FsEvent::Changed {
                path: d["path"].as_str()?.to_string(),
                kind: d["kind"].as_str().unwrap_or("modified").to_string(),
            }),
            "file_deleted" => Some(FsEvent::Deleted {
                path: d["path"].as_str()?.to_string(),
            }),
            "file_moved" => Some(FsEvent::Moved {
                from: d["from"].as_str()?.to_string(),
                to: d["to"].as_str()?.to_string(),
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(event_type: &str, data: serde_json::Value) -> SessionEvent {
        SessionEvent {
            session_id: "system".to_string(),
            event_type: event_type.to_string(),
            data,
        }
    }

    #[test]
    fn file_changed_maps_to_changed_with_kind() {
        let ev = event(
            "file_changed",
            serde_json::json!({ "path": "/proj/a.rs", "kind": "modified" }),
        );
        let fe = FsEvent::from_daemon_event(&ev).unwrap();
        assert_eq!(fe.event_name(), "fs_changed");
        assert_eq!(
            serde_json::to_value(&fe).unwrap(),
            serde_json::json!({ "type": "changed", "path": "/proj/a.rs", "kind": "modified" })
        );
    }

    #[test]
    fn file_changed_defaults_kind_to_modified() {
        let ev = event("file_changed", serde_json::json!({ "path": "/proj/a.rs" }));
        match FsEvent::from_daemon_event(&ev).unwrap() {
            FsEvent::Changed { kind, .. } => assert_eq!(kind, "modified"),
            other => panic!("expected Changed, got {other:?}"),
        }
    }

    #[test]
    fn file_deleted_maps_to_deleted() {
        let ev = event(
            "file_deleted",
            serde_json::json!({ "path": "/proj/gone.rs" }),
        );
        let fe = FsEvent::from_daemon_event(&ev).unwrap();
        assert_eq!(fe.event_name(), "fs_deleted");
        assert_eq!(
            serde_json::to_value(&fe).unwrap(),
            serde_json::json!({ "type": "deleted", "path": "/proj/gone.rs" })
        );
    }

    #[test]
    fn file_moved_maps_to_moved() {
        let ev = event(
            "file_moved",
            serde_json::json!({ "from": "/proj/a.rs", "to": "/proj/b.rs" }),
        );
        let fe = FsEvent::from_daemon_event(&ev).unwrap();
        assert_eq!(fe.event_name(), "fs_moved");
        assert_eq!(
            serde_json::to_value(&fe).unwrap(),
            serde_json::json!({ "type": "moved", "from": "/proj/a.rs", "to": "/proj/b.rs" })
        );
    }

    #[test]
    fn non_file_events_are_ignored() {
        let ev = event("note_created", serde_json::json!({ "path": "/proj/a.md" }));
        assert!(FsEvent::from_daemon_event(&ev).is_none());
        let ev = event("token", serde_json::json!({ "content": "hi" }));
        assert!(FsEvent::from_daemon_event(&ev).is_none());
    }

    #[test]
    fn malformed_payload_missing_path_is_ignored() {
        let ev = event("file_changed", serde_json::json!({ "kind": "modified" }));
        assert!(FsEvent::from_daemon_event(&ev).is_none());
    }
}
