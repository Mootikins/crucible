// Tracing layer for TUI log forwarding
//
// This module provides a custom tracing-subscriber layer that forwards
// log events to the TUI via an mpsc channel.
//
// Design: Non-blocking sends (try_send) to avoid blocking worker threads
// if the UI can't keep up. Dropped logs are acceptable - they're still
// written to the file log.

use crate::tui::events::LogEntry;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{field::Visit, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// Custom tracing layer that sends log events to the TUI
///
/// This layer extracts log events from the tracing infrastructure and
/// forwards them to the TUI via an async channel.
///
/// # Non-blocking Behavior
/// Uses `try_send()` to avoid blocking worker threads. If the channel is full,
/// the log entry is dropped (but still written to file by other layers).
pub struct TuiLayer {
    sender: mpsc::Sender<LogEntry>,
}

impl TuiLayer {
    /// Create a new TUI layer
    ///
    /// # Arguments
    /// - `sender`: Channel sender for log entries
    ///
    /// # Returns
    /// A new TuiLayer instance
    pub fn new(sender: mpsc::Sender<LogEntry>) -> Self {
        Self { sender }
    }
}

impl<S> Layer<S> for TuiLayer
where
    S: Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: Context<'_, S>,
    ) {
        // Extract event metadata
        let metadata = event.metadata();

        // Create visitor to collect fields
        let mut visitor = LogFieldVisitor::default();
        event.record(&mut visitor);

        // Build log entry
        let entry = LogEntry {
            timestamp: chrono::Utc::now(),
            level: *metadata.level(),
            target: metadata.target().to_string(),
            message: visitor.message.unwrap_or_else(|| "".to_string()),
            fields: visitor.fields,
        };

        // Non-blocking send (drop if channel full)
        let _ = self.sender.try_send(entry);
    }
}

/// Visitor for extracting fields from tracing events
#[derive(Default)]
struct LogFieldVisitor {
    message: Option<String>,
    fields: HashMap<String, String>,
}

impl Visit for LogFieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        // Special handling for the "message" field
        if field.name() == "message" {
            self.message = Some(format!("{:?}", value).trim_matches('"').to_string());
        } else {
            self.fields
                .insert(field.name().to_string(), format!("{:?}", value));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields.insert(field.name().to_string(), value.to_string());
        }
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }
}

/// Setup logging with both TUI layer and file layer
///
/// This function configures the tracing subscriber with:
/// - TUI layer: Sends logs to the UI
/// - File layer: Writes logs to ~/.crucible/daemon.log
///
/// # Arguments
/// - `log_tx`: Channel sender for TUI logs
/// - `log_file_path`: Path to log file
///
/// # Returns
/// Ok(()) on success, Err on setup failure
pub fn setup_logging(
    log_tx: mpsc::Sender<LogEntry>,
    log_file_path: impl AsRef<std::path::Path>,
) -> anyhow::Result<()> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    // Create TUI layer
    let tui_layer = TuiLayer::new(log_tx);

    // Create file layer
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path)?;

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::sync::Arc::new(log_file))
        .with_ansi(false); // No ANSI codes in file

    // Initialize subscriber with both layers
    tracing_subscriber::registry()
        .with(tui_layer)
        .with(file_layer)
        .init();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tui_layer_forwards_logs() {
        let (tx, mut rx) = mpsc::channel(10);
        let layer = TuiLayer::new(tx);

        // Create a subscriber with our layer
        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(layer);

        // Set as global default
        let _guard = tracing::subscriber::set_default(subscriber);

        // Emit a log event
        tracing::info!("test message");

        // Verify it was sent to channel
        let entry = rx.recv().await.expect("Should receive log entry");
        assert_eq!(entry.message, "test message");
        assert_eq!(entry.level, tracing::Level::INFO);
    }

    #[tokio::test]
    async fn test_tui_layer_extracts_fields() {
        let (tx, mut rx) = mpsc::channel(10);
        let layer = TuiLayer::new(tx);

        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        // Emit log with structured fields
        tracing::info!(file = "test.md", duration_ms = 42, "indexed");

        let entry = rx.recv().await.expect("Should receive log entry");
        assert_eq!(entry.message, "indexed");
        assert_eq!(entry.fields.get("file"), Some(&"test.md".to_string()));
        assert_eq!(entry.fields.get("duration_ms"), Some(&"42".to_string()));
    }

    #[tokio::test]
    async fn test_non_blocking_send() {
        // Create channel with capacity 1
        let (tx, _rx) = mpsc::channel(1);
        let layer = TuiLayer::new(tx.clone());

        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(layer);
        let _guard = tracing::subscriber::set_default(subscriber);

        // Fill the channel
        tx.try_send(LogEntry::new(
            tracing::Level::INFO,
            "test",
            "first",
        ))
        .expect("First send should succeed");

        // This should not block even though channel is full
        tracing::info!("second message");
        // Test passes if we get here without blocking
    }
}
