use super::PluginManager;
use std::collections::VecDeque;
use std::sync::MutexGuard;
use tracing::warn;

/// A single captured error entry from plugin execution.
#[derive(Debug, Clone)]
pub struct PluginErrorEntry {
    /// Plugin that generated this error.
    pub plugin: String,
    /// Error message string.
    pub error: String,
    /// Context where the error occurred (e.g. "emitter:emit('on_message')" or "handler:my_handler").
    pub context: String,
    /// When the error was captured.
    pub timestamp: std::time::Instant,
}

/// Bounded ring buffer of recent plugin errors. Stored per-PluginManager for test isolation.
#[derive(Debug)]
pub struct PluginErrorLog {
    entries: VecDeque<PluginErrorEntry>,
    capacity: usize,
}

impl PluginErrorLog {
    /// Create a new error log with given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Push a new error entry. Evicts oldest if over capacity.
    pub fn push(&mut self, entry: PluginErrorEntry) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Return the `n` most recent entries. If n > len, returns all.
    pub fn recent(&self, n: usize) -> Vec<&PluginErrorEntry> {
        let start = self.entries.len().saturating_sub(n);
        self.entries.iter().skip(start).collect()
    }

    /// Number of entries currently stored.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl PluginManager {
    /// Access the error log for this plugin manager.
    pub fn error_log(&self) -> MutexGuard<'_, PluginErrorLog> {
        self.error_log.lock().expect("error_log: poisoned")
    }

    pub(super) fn capture_plugin_error(
        &self,
        plugin: &str,
        error: impl ToString,
        context: impl Into<String>,
    ) {
        match self.error_log.lock() {
            Ok(mut log) => log.push(PluginErrorEntry {
                plugin: plugin.to_string(),
                error: error.to_string(),
                context: context.into(),
                timestamp: std::time::Instant::now(),
            }),
            Err(_) => warn!("Failed to capture plugin error due to poisoned error log"),
        }
    }
}
