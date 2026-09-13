//! The bounded log of plugin errors a VM carries.
//!
//! The log is app data on the VM whose plugins it describes, because the
//! errors happen there: a hook that raises, an emitter listener that raises.
//! `cru.errors.recent` (`prelude/mod.rs`) reads the same app data, so the
//! Lua side sees what the Rust side recorded. A VM without a log drops the
//! entry, because an error path that raises is worse than one that forgets.

use mlua::Lua;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
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

/// Bounded ring buffer of recent plugin errors. One per VM.
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

    /// Give `lua` an error log, and answer the shared handle.
    pub fn install(lua: &Lua, capacity: usize) -> Arc<Mutex<Self>> {
        let log = Arc::new(Mutex::new(Self::new(capacity)));
        lua.set_app_data(Arc::clone(&log));
        log
    }

    /// The log `lua` carries, when it has one.
    pub fn of(lua: &Lua) -> Option<Arc<Mutex<Self>>> {
        lua.app_data_ref::<Arc<Mutex<Self>>>()
            .map(|shared| Arc::clone(&*shared))
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

/// Record an error in the log `lua` carries. A VM without a log drops it.
pub fn record_plugin_error(
    lua: &Lua,
    plugin: &str,
    error: impl ToString,
    context: impl Into<String>,
) {
    let Some(log) = PluginErrorLog::of(lua) else {
        return;
    };
    let locked = log.lock();
    match locked {
        Ok(mut guard) => guard.push(PluginErrorEntry {
            plugin: plugin.to_string(),
            error: error.to_string(),
            context: context.into(),
            timestamp: std::time::Instant::now(),
        }),
        Err(_) => warn!("Failed to capture plugin error due to poisoned error log"),
    }
}
