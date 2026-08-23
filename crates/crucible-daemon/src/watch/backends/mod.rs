//! The file watch backends.
//!
//! `WatchBackend` names a backend. `Backend` holds a live one. The two enums
//! replace a `FileWatcher` trait, a `WatcherFactory` trait and three factory
//! structs, each of which carried its own copy of the capability table. A new
//! backend is one variant in each enum plus one row in `capabilities`; the
//! `deny` lints below make the compiler list every match to update.

#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::match_wildcard_for_single_variants)]

mod editor_backend;
mod notify_backend;
mod polling_backend;
mod select;

pub use editor_backend::{EditorConfig, EditorWatcher};
pub use notify_backend::NotifyWatcher;
pub use polling_backend::PollingWatcher;
pub use select::{select_optimal_backend, WatcherRequirements, WatcherUseCase};

use crate::watch::error::Result;
use crate::watch::events::FileEvent;
use crate::watch::traits::{BackendCapabilities, WatchConfig, WatchHandle};
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use tokio::sync::mpsc;
#[cfg(test)]
use tracing::{debug, info, warn};

/// Remove the watch behind `handle` from a backend's table, keyed by handle id.
///
/// The three backends share this body. `backend` names the backend in the
/// log lines.
#[cfg(test)]
fn remove_watch<S>(watches: &mut HashMap<String, S>, handle: &WatchHandle, backend: &str) {
    debug!("Removing {} watch for: {}", backend, handle.path.display());
    if watches.remove(&handle.id).is_some() {
        info!("Removed {} watch: {}", backend, handle.path.display());
    } else {
        warn!("{} watch not found: {}", backend, handle.path.display());
    }
}

/// Rebuild one `WatchHandle` per entry of a backend's table.
///
/// `path_of` reads the watched path out of the backend's state type.
#[cfg(test)]
fn watch_handles<S>(
    watches: &HashMap<String, S>,
    path_of: impl Fn(&S) -> &Path,
) -> Vec<WatchHandle> {
    watches
        .iter()
        .map(|(id, state)| WatchHandle {
            id: id.clone(),
            path: path_of(state).to_path_buf(),
        })
        .collect()
}

/// The name of a file watch backend.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum WatchBackend {
    /// OS file system notifications through `notify`.
    Notify,
    /// A portable polling loop.
    Polling,
    /// A low-frequency loop for editor integrations.
    Editor,
}

impl WatchBackend {
    /// Every backend, in declaration order.
    pub const ALL: [WatchBackend; 3] = [
        WatchBackend::Notify,
        WatchBackend::Polling,
        WatchBackend::Editor,
    ];

    /// The backend name as event metadata and logs print it.
    pub const fn name(self) -> &'static str {
        match self {
            WatchBackend::Notify => "notify",
            WatchBackend::Polling => "polling",
            WatchBackend::Editor => "editor",
        }
    }

    /// What the backend can do. This is the one capability table.
    pub const fn capabilities(self) -> BackendCapabilities {
        match self {
            WatchBackend::Notify => BackendCapabilities {
                recursive: true,
                fine_grained_events: true,
                multiple_paths: true,
                hot_reconfig: false,
                platforms: &["linux", "macos", "windows"],
            },
            WatchBackend::Polling => BackendCapabilities {
                recursive: true,
                fine_grained_events: false,
                multiple_paths: true,
                hot_reconfig: true,
                platforms: &["all"],
            },
            WatchBackend::Editor => BackendCapabilities {
                recursive: false,
                fine_grained_events: true,
                multiple_paths: true,
                hot_reconfig: true,
                platforms: &["linux", "macos", "windows"],
            },
        }
    }

    /// The shortest latency the backend can promise, in milliseconds.
    ///
    /// Notify is event driven. Polling waits for its interval. The editor
    /// loop ticks every five seconds.
    pub const fn latency_floor_ms(self) -> u64 {
        match self {
            WatchBackend::Notify => 50,
            WatchBackend::Polling => 1000,
            WatchBackend::Editor => 5000,
        }
    }

    /// Whether the backend works on this platform.
    pub fn is_available(self) -> bool {
        let current = std::env::consts::OS;
        self.capabilities()
            .platforms
            .iter()
            .any(|platform| *platform == "all" || *platform == current)
    }

    /// Build a fresh instance of the backend.
    pub fn create(self) -> Backend {
        match self {
            WatchBackend::Notify => Backend::Notify(NotifyWatcher::new()),
            WatchBackend::Polling => Backend::Polling(PollingWatcher::new()),
            WatchBackend::Editor => Backend::Editor(EditorWatcher::new()),
        }
    }
}

/// A live file watch backend.
pub enum Backend {
    /// See [`NotifyWatcher`].
    Notify(NotifyWatcher),
    /// See [`PollingWatcher`].
    Polling(PollingWatcher),
    /// See [`EditorWatcher`].
    Editor(EditorWatcher),
}

impl Backend {
    /// Which backend this is.
    pub fn kind(&self) -> WatchBackend {
        match self {
            Backend::Notify(_) => WatchBackend::Notify,
            Backend::Polling(_) => WatchBackend::Polling,
            Backend::Editor(_) => WatchBackend::Editor,
        }
    }

    /// The backend name as event metadata and logs print it.
    pub fn backend_type(&self) -> &'static str {
        self.kind().name()
    }

    /// Set the channel the backend sends events on. Call this before `watch`.
    pub fn set_event_sender(&mut self, sender: mpsc::UnboundedSender<FileEvent>) {
        match self {
            Backend::Notify(w) => w.set_event_sender(sender),
            Backend::Polling(w) => w.set_event_sender(sender),
            Backend::Editor(w) => w.set_event_sender(sender),
        }
    }

    /// Start to watch `path` with `config`.
    pub async fn watch(&mut self, path: PathBuf, config: WatchConfig) -> Result<WatchHandle> {
        match self {
            Backend::Notify(w) => w.watch(path, config).await,
            Backend::Polling(w) => w.watch(path, config).await,
            Backend::Editor(w) => w.watch(path, config).await,
        }
    }

    /// Stop the watch behind `handle`.
    ///
    /// Production drops a whole backend to release its watches; see
    /// `WatchManager::remove_watch_group`.
    #[cfg(test)]
    pub async fn unwatch(&mut self, handle: WatchHandle) -> Result<()> {
        match self {
            Backend::Notify(w) => w.unwatch(handle).await,
            Backend::Polling(w) => w.unwatch(handle).await,
            Backend::Editor(w) => w.unwatch(handle).await,
        }
    }

    /// Every watch the backend holds.
    #[cfg(test)]
    pub fn active_watches(&self) -> Vec<WatchHandle> {
        match self {
            Backend::Notify(w) => w.active_watches(),
            Backend::Polling(w) => w.active_watches(),
            Backend::Editor(w) => w.active_watches(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn remove_watch_drops_only_the_handle_id() {
        let mut watches: HashMap<String, PathBuf> = HashMap::new();
        watches.insert("a".into(), PathBuf::from("/a"));
        watches.insert("b".into(), PathBuf::from("/b"));
        let handle = WatchHandle {
            id: "a".into(),
            path: PathBuf::from("/a"),
        };
        remove_watch(&mut watches, &handle, "test");
        remove_watch(&mut watches, &handle, "test");
        let handles = watch_handles(&watches, |p| p.as_path());
        assert_eq!(handles.len(), 1);
        assert_eq!(handles[0].id, "b");
        assert_eq!(handles[0].path, PathBuf::from("/b"));
    }

    #[test]
    fn all_lists_every_backend() {
        let from_iter: Vec<WatchBackend> = WatchBackend::iter().collect();
        assert_eq!(from_iter, WatchBackend::ALL.to_vec());
    }

    #[test]
    fn create_returns_the_named_backend() {
        for kind in WatchBackend::iter() {
            let backend = kind.create();
            assert_eq!(backend.kind(), kind);
            assert_eq!(backend.backend_type(), kind.name());
            assert!(backend.active_watches().is_empty());
        }
    }

    #[test]
    fn capability_table_rows() {
        let notify = WatchBackend::Notify.capabilities();
        assert!(notify.recursive);
        assert!(notify.fine_grained_events);
        assert!(!notify.hot_reconfig);

        let polling = WatchBackend::Polling.capabilities();
        assert!(polling.recursive);
        assert!(!polling.fine_grained_events);
        assert!(polling.hot_reconfig);

        let editor = WatchBackend::Editor.capabilities();
        assert!(!editor.recursive);
        assert!(editor.fine_grained_events);
        assert_eq!(editor.platforms, &["linux", "macos", "windows"]);
    }

    #[test]
    fn every_backend_is_available_here() {
        for kind in WatchBackend::iter() {
            assert!(kind.is_available(), "{kind:?}");
        }
    }
}
