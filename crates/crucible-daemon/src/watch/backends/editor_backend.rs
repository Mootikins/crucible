//! Editor integration backend for low-frequency inode watching.

use crate::watch::{
    error::{Error, Result},
    events::FileEvent,
    traits::{WatchConfig, WatchHandle},
};

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, info};

/// Configuration for editor integration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EditorConfig {
    /// Editor type (vscode, vim, emacs, etc.)
    pub editor_type: String,
    /// Editor-specific configuration
    pub editor_config: HashMap<String, serde_json::Value>,
    /// Low-frequency polling interval
    #[serde(with = "duration_serde")]
    pub poll_interval: Duration,
    /// Inode change detection
    pub detect_inode_changes: bool,
    /// Editor API integration
    pub use_editor_api: bool,
}

mod duration_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}

/// State information for an editor watch.
#[derive(Debug, Clone)]
struct EditorWatchState {
    /// Path being watched
    watched_path: PathBuf,
}

/// Editor integration backend for low-frequency file watching.
pub struct EditorWatcher {
    /// Event sender
    event_sender: Option<mpsc::UnboundedSender<FileEvent>>,
    /// Active watches
    watches: HashMap<String, EditorWatchState>,
    /// Background monitoring task
    monitor_task: Option<JoinHandle<()>>,
    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl Default for EditorWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorWatcher {
    /// Create a new editor watcher.
    pub fn new() -> Self {
        Self {
            event_sender: None,
            watches: HashMap::new(),
            monitor_task: None,
            shutdown_tx: None,
        }
    }

    /// Initialize the watcher with event sender.
    async fn initialize(&mut self, event_sender: mpsc::UnboundedSender<FileEvent>) -> Result<()> {
        self.event_sender = Some(event_sender);
        self.start_monitoring_task().await?;
        info!("Editor watcher initialized");
        Ok(())
    }

    /// Start the background monitoring task.
    async fn start_monitoring_task(&mut self) -> Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        let _event_sender = self
            .event_sender
            .clone()
            .ok_or_else(|| Error::Internal("Event sender not initialized".to_string()))?;

        let task = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(5)); // 5-second default
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        // Editor-specific monitoring logic would go here
                        // For now, this is a placeholder
                        debug!("Editor watcher tick");
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Editor monitor task shutting down");
                        break;
                    }
                }
            }
        });

        self.monitor_task = Some(task);
        self.shutdown_tx = Some(shutdown_tx);

        Ok(())
    }
}

impl EditorWatcher {
    /// Set the channel the watcher sends events on. Call this before `watch`.
    pub fn set_event_sender(&mut self, sender: mpsc::UnboundedSender<FileEvent>) {
        self.event_sender = Some(sender);
    }

    /// Start to watch `path` with `config`.
    pub async fn watch(&mut self, path: PathBuf, config: WatchConfig) -> Result<WatchHandle> {
        debug!("Adding editor watch for: {}", path.display());

        // Initialize if not already done
        if self.monitor_task.is_none() {
            let sender = self.event_sender.clone().ok_or_else(|| {
                Error::Internal("Event sender not set before calling watch".to_string())
            })?;
            self.initialize(sender).await?;
        }

        let watch_id = config.id.clone();
        let watch_handle = WatchHandle {
            id: watch_id.clone(),
            path: path.clone(),
        };

        let watch_state = EditorWatchState {
            watched_path: path.clone(),
        };

        self.watches.insert(watch_id.clone(), watch_state);
        info!("Added editor watch: {} -> {}", watch_id, path.display());

        Ok(watch_handle)
    }

    /// Stop the watch behind `handle`.
    pub async fn unwatch(&mut self, handle: WatchHandle) -> Result<()> {
        super::remove_watch(&mut self.watches, &handle, "editor");
        Ok(())
    }

    /// Every watch the backend holds.
    pub fn active_watches(&self) -> Vec<WatchHandle> {
        super::watch_handles(&self.watches, |state| &state.watched_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_watches_empty() {
        let watcher = EditorWatcher::new();
        assert!(watcher.active_watches().is_empty());
    }

    #[test]
    fn editor_config_serde_roundtrip() {
        let config = EditorConfig {
            editor_type: "vscode".to_string(),
            editor_config: HashMap::from([(
                "workspace".to_string(),
                serde_json::json!("/home/user/project"),
            )]),
            poll_interval: Duration::from_secs(3),
            detect_inode_changes: true,
            use_editor_api: false,
        };

        let json = serde_json::to_string(&config).unwrap();
        let roundtripped: EditorConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(roundtripped.editor_type, config.editor_type);
        assert_eq!(roundtripped.editor_config, config.editor_config);
        assert_eq!(roundtripped.poll_interval, config.poll_interval);
        assert_eq!(
            roundtripped.detect_inode_changes,
            config.detect_inode_changes
        );
        assert_eq!(roundtripped.use_editor_api, config.use_editor_api);
    }

    #[test]
    fn duration_serializes_as_millis() {
        let config = EditorConfig {
            editor_type: "vim".to_string(),
            editor_config: HashMap::new(),
            poll_interval: Duration::from_millis(5000),
            detect_inode_changes: false,
            use_editor_api: false,
        };

        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["poll_interval"], serde_json::json!(5000));
    }
}
