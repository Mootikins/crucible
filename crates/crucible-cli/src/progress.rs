//! Progress Tracking for Chat Mode
//!
//! Provides shared progress state for background file processing
//! that can be displayed in the chat interface.

use crossterm::{
    cursor::{MoveToColumn, MoveUp, RestorePosition, SavePosition},
    terminal::{Clear, ClearType},
    ExecutableCommand,
};
use std::io::stdout;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

/// Shared progress state for background file processing
#[derive(Debug, Clone)]
pub struct BackgroundProgress {
    inner: Arc<ProgressInner>,
}

#[derive(Debug)]
struct ProgressInner {
    total: AtomicUsize,
    completed: AtomicUsize,
    failed: AtomicUsize,
}

impl BackgroundProgress {
    /// Create a new progress tracker with the given total count
    pub fn new(total: usize) -> Self {
        Self {
            inner: Arc::new(ProgressInner {
                total: AtomicUsize::new(total),
                completed: AtomicUsize::new(0),
                failed: AtomicUsize::new(0),
            }),
        }
    }

    /// Increment the completed count
    pub fn inc_completed(&self) {
        self.inner.completed.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the failed count
    pub fn inc_failed(&self) {
        self.inner.failed.fetch_add(1, Ordering::Relaxed);
    }

    /// Get the total count
    pub fn total(&self) -> usize {
        self.inner.total.load(Ordering::Relaxed)
    }

    /// Get the completed count
    pub fn completed(&self) -> usize {
        self.inner.completed.load(Ordering::Relaxed)
    }

    /// Get the failed count
    pub fn failed(&self) -> usize {
        self.inner.failed.load(Ordering::Relaxed)
    }

    /// Check if processing is complete
    pub fn is_complete(&self) -> bool {
        self.completed() + self.failed() >= self.total()
    }

    /// Get a status string for display
    pub fn status_string(&self) -> Option<String> {
        let total = self.total();
        if total == 0 {
            return None;
        }

        let completed = self.completed();
        let failed = self.failed();

        if completed + failed >= total {
            // All done
            if failed > 0 {
                Some(format!(
                    "indexed {}/{} ({} failed)",
                    completed, total, failed
                ))
            } else {
                None // Fully complete, no need to show
            }
        } else {
            // Still processing
            Some(format!("indexing {}/{}", completed + failed, total))
        }
    }
}

/// Single-line status display that overwrites itself
pub struct StatusLine {
    last_len: usize,
}

impl StatusLine {
    pub fn new() -> Self {
        Self { last_len: 0 }
    }

    /// Update the status line (overwrites previous content)
    pub fn update(&mut self, message: &str) {
        use colored::Colorize;
        use std::io::{self, Write};

        // Clear previous line
        print!("\r{}\r", " ".repeat(self.last_len));

        // Print new message with spinner
        let formatted = format!("{} {}", "⟳".cyan(), message);
        print!("{}", formatted);
        io::stdout().flush().unwrap();

        self.last_len = formatted.len() + 5; // Extra padding for safety
    }

    /// Complete with a success message
    pub fn success(&mut self, message: &str) {
        use colored::Colorize;
        use std::io::{self, Write};

        // Clear previous line
        print!("\r{}\r", " ".repeat(self.last_len));

        // Print success message
        println!("{} {}", "✓".green(), message);
        io::stdout().flush().unwrap();

        self.last_len = 0;
    }

    /// Complete with an error message
    #[allow(dead_code)]
    pub fn error(&mut self, message: &str) {
        use colored::Colorize;
        use std::io::{self, Write};

        // Clear previous line
        print!("\r{}\r", " ".repeat(self.last_len));

        // Print error message
        println!("{} {}", "✗".red(), message);
        io::stdout().flush().unwrap();

        self.last_len = 0;
    }
}

impl Default for StatusLine {
    fn default() -> Self {
        Self::new()
    }
}

/// Real-time progress display that updates on a separate line above the prompt.
///
/// Uses ANSI escape codes to move cursor up and overwrite the progress line
/// without interfering with the user's input.
pub struct LiveProgress {
    progress: BackgroundProgress,
    stop_tx: watch::Sender<bool>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl LiveProgress {
    /// Start a live progress display for the given progress tracker.
    ///
    /// Spawns a background task that updates the progress line every 100ms.
    /// The progress is displayed on a dedicated line above the prompt.
    pub fn start(progress: BackgroundProgress) -> Self {
        use colored::Colorize;
        use std::io::Write;

        let (stop_tx, mut stop_rx) = watch::channel(false);
        let progress_clone = progress.clone();

        // Print initial progress line (will be updated in place)
        if let Some(status) = progress.status_string() {
            println!("{} {}", "⟳".cyan(), status);
        }

        let handle = tokio::spawn(async move {
            let mut last_status: Option<String> = None;

            loop {
                // Check if we should stop
                if *stop_rx.borrow() {
                    break;
                }

                // Get current status
                let current_status = progress_clone.status_string();

                // Only update if status changed
                if current_status != last_status {
                    if current_status.is_some() || last_status.is_some() {
                        let mut out = stdout();

                        // Save cursor, move up, clear line, move to column 1
                        let _ = out.execute(SavePosition);
                        let _ = out.execute(MoveUp(1));
                        let _ = out.execute(Clear(ClearType::CurrentLine));
                        let _ = out.execute(MoveToColumn(0));

                        if let Some(ref status) = current_status {
                            print!("{} {}", "⟳".cyan(), status);
                        } else {
                            // Complete - print success and done
                            print!("{} Indexing complete", "✓".green());
                        }

                        // Restore cursor position
                        let _ = out.execute(RestorePosition);
                        out.flush().ok();
                    }

                    last_status = current_status.clone();

                    // If complete (None status), we're done
                    if current_status.is_none() {
                        break;
                    }
                }

                // Wait before next update, but also listen for stop signal
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {},
                    _ = stop_rx.changed() => {
                        if *stop_rx.borrow() {
                            break;
                        }
                    }
                }
            }
        });

        Self {
            progress,
            stop_tx,
            handle: Some(handle),
        }
    }

    /// Get a reference to the progress tracker for checking status
    pub fn progress(&self) -> &BackgroundProgress {
        &self.progress
    }

    /// Stop the live progress display
    pub async fn stop(mut self) {
        use colored::Colorize;
        use std::io::Write;

        // Signal stop
        let _ = self.stop_tx.send(true);

        // Wait for task to finish
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }

        // Update the progress line with final status if still showing
        if self.progress.status_string().is_some() {
            let mut out = stdout();

            // Save cursor, move up, clear line, move to column 1
            let _ = out.execute(SavePosition);
            let _ = out.execute(MoveUp(1));
            let _ = out.execute(Clear(ClearType::CurrentLine));
            let _ = out.execute(MoveToColumn(0));

            let completed = self.progress.completed();
            let failed = self.progress.failed();
            let total = self.progress.total();
            if failed > 0 {
                print!(
                    "{} Indexed {}/{} ({} failed)",
                    "✓".green(),
                    completed,
                    total,
                    failed
                );
            } else {
                print!("{} Indexed {}/{}", "✓".green(), completed, total);
            }

            // Restore cursor position
            let _ = out.execute(RestorePosition);
            out.flush().ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_background_progress() {
        let progress = BackgroundProgress::new(10);
        assert_eq!(progress.total(), 10);
        assert_eq!(progress.completed(), 0);
        assert!(!progress.is_complete());

        progress.inc_completed();
        assert_eq!(progress.completed(), 1);

        for _ in 0..9 {
            progress.inc_completed();
        }
        assert!(progress.is_complete());
    }

    #[test]
    fn test_status_string() {
        let progress = BackgroundProgress::new(10);
        assert_eq!(progress.status_string(), Some("indexing 0/10".to_string()));

        for _ in 0..5 {
            progress.inc_completed();
        }
        assert_eq!(progress.status_string(), Some("indexing 5/10".to_string()));

        for _ in 0..5 {
            progress.inc_completed();
        }
        assert_eq!(progress.status_string(), None); // Complete, no failures
    }

    #[test]
    fn test_status_string_with_failures() {
        let progress = BackgroundProgress::new(10);
        for _ in 0..8 {
            progress.inc_completed();
        }
        for _ in 0..2 {
            progress.inc_failed();
        }
        assert_eq!(
            progress.status_string(),
            Some("indexed 8/10 (2 failed)".to_string())
        );
    }
}
