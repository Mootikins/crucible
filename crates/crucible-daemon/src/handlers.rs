//! Simple event logging for the daemon
//!
//! Basic event logging without over-engineering. Uses direct flume channels.

use anyhow::Result;
use tracing::{error, info};

/// Simple event logger - no complex handler system
pub struct EventLogger;

impl EventLogger {
    pub fn new() -> Self {
        Self
    }

    /// Log a filesystem event
    pub fn log_filesystem_event(&self, event: &str) -> Result<()> {
        info!("Filesystem event: {}", event);
        Ok(())
    }

    /// Log a database event
    pub fn log_database_event(&self, event: &str) -> Result<()> {
        info!("Database event: {}", event);
        Ok(())
    }

    /// Log a sync event
    pub fn log_sync_event(&self, event: &str) -> Result<()> {
        info!("Sync event: {}", event);
        Ok(())
    }

    /// Log an error event
    pub fn log_error_event(&self, event: &str) -> Result<()> {
        error!("Error event: {}", event);
        Ok(())
    }

    /// Log a health event
    pub fn log_health_event(&self, event: &str) -> Result<()> {
        info!("Health event: {}", event);
        Ok(())
    }
}

impl Default for EventLogger {
    fn default() -> Self {
        Self::new()
    }
}
