//! Compaction Trigger Detection for Session Context Management
//!
//! This module provides configurable compaction trigger detection for sessions.
//! Compaction occurs when the session context exceeds configured limits, creating
//! a summary and starting a new context file.
//!
//! ## Trigger Types
//!
//! - **Token count**: Triggers when estimated tokens exceed threshold (default: 100k)
//! - **Message count**: Triggers after N messages from participants
//! - **Event count**: Triggers after N total events
//! - **Time-based**: Triggers after a duration since last compaction
//! - **Manual**: Explicit compaction request
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rune::compaction::{CompactionTrigger, CompactionConfig};
//!
//! // Create with defaults
//! let trigger = CompactionTrigger::new(CompactionConfig::default());
//!
//! // Record events
//! trigger.record_event(&event);
//! trigger.add_tokens(estimated_tokens);
//!
//! // Check if compaction needed
//! if let Some(reason) = trigger.should_compact() {
//!     println!("Compaction triggered: {}", reason);
//!     trigger.reset();
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::reactor::SessionEvent;

/// Configuration for compaction trigger detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionConfig {
    /// Maximum tokens before compaction (0 = disabled).
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    /// Maximum messages from participants before compaction (0 = disabled).
    #[serde(default)]
    pub max_messages: usize,

    /// Maximum total events before compaction (0 = disabled).
    #[serde(default)]
    pub max_events: usize,

    /// Maximum duration since session start or last compaction (None = disabled).
    #[serde(default, with = "humantime_serde")]
    pub max_duration: Option<Duration>,

    /// Whether to enable automatic compaction detection.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_max_tokens() -> usize {
    100_000
}

fn default_enabled() -> bool {
    true
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            max_tokens: default_max_tokens(),
            max_messages: 0,    // Disabled by default
            max_events: 0,      // Disabled by default
            max_duration: None, // Disabled by default
            enabled: true,
        }
    }
}

impl CompactionConfig {
    /// Create a new configuration with all triggers disabled.
    pub fn disabled() -> Self {
        Self {
            max_tokens: 0,
            max_messages: 0,
            max_events: 0,
            max_duration: None,
            enabled: false,
        }
    }

    /// Set maximum tokens threshold.
    pub fn with_max_tokens(mut self, tokens: usize) -> Self {
        self.max_tokens = tokens;
        self
    }

    /// Set maximum messages threshold.
    pub fn with_max_messages(mut self, messages: usize) -> Self {
        self.max_messages = messages;
        self
    }

    /// Set maximum events threshold.
    pub fn with_max_events(mut self, events: usize) -> Self {
        self.max_events = events;
        self
    }

    /// Set maximum duration threshold.
    pub fn with_max_duration(mut self, duration: Duration) -> Self {
        self.max_duration = Some(duration);
        self
    }

    /// Enable or disable automatic compaction.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Reason why compaction was triggered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionReason {
    /// Token count exceeded threshold.
    TokenLimit { current: usize, limit: usize },
    /// Message count exceeded threshold.
    MessageLimit { current: usize, limit: usize },
    /// Event count exceeded threshold.
    EventLimit { current: usize, limit: usize },
    /// Duration exceeded threshold.
    DurationLimit { elapsed: Duration, limit: Duration },
    /// Manually requested.
    ManualRequest,
}

impl std::fmt::Display for CompactionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TokenLimit { current, limit } => {
                write!(f, "token limit exceeded ({} >= {})", current, limit)
            }
            Self::MessageLimit { current, limit } => {
                write!(f, "message limit exceeded ({} >= {})", current, limit)
            }
            Self::EventLimit { current, limit } => {
                write!(f, "event limit exceeded ({} >= {})", current, limit)
            }
            Self::DurationLimit { elapsed, limit } => {
                write!(
                    f,
                    "duration limit exceeded ({:.1}s >= {:.1}s)",
                    elapsed.as_secs_f64(),
                    limit.as_secs_f64()
                )
            }
            Self::ManualRequest => write!(f, "manual compaction request"),
        }
    }
}

/// Compaction trigger state and detection.
///
/// Tracks various metrics and determines when compaction should occur.
/// Thread-safe via atomic operations.
pub struct CompactionTrigger {
    /// Configuration.
    config: CompactionConfig,
    /// Current token count.
    token_count: AtomicUsize,
    /// Current message count (MessageReceived events).
    message_count: AtomicUsize,
    /// Current total event count.
    event_count: AtomicUsize,
    /// Start time for duration tracking.
    start_time: std::sync::RwLock<Instant>,
    /// Manual compaction request flag.
    manual_request: AtomicBool,
    /// Number of compactions performed.
    compaction_count: AtomicU64,
}

impl CompactionTrigger {
    /// Create a new compaction trigger with the given configuration.
    pub fn new(config: CompactionConfig) -> Self {
        Self {
            config,
            token_count: AtomicUsize::new(0),
            message_count: AtomicUsize::new(0),
            event_count: AtomicUsize::new(0),
            start_time: std::sync::RwLock::new(Instant::now()),
            manual_request: AtomicBool::new(false),
            compaction_count: AtomicU64::new(0),
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(CompactionConfig::default())
    }

    /// Get the current configuration.
    pub fn config(&self) -> &CompactionConfig {
        &self.config
    }

    /// Get the current token count.
    pub fn token_count(&self) -> usize {
        self.token_count.load(Ordering::SeqCst)
    }

    /// Get the current message count.
    pub fn message_count(&self) -> usize {
        self.message_count.load(Ordering::SeqCst)
    }

    /// Get the current event count.
    pub fn event_count(&self) -> usize {
        self.event_count.load(Ordering::SeqCst)
    }

    /// Get elapsed time since start or last reset.
    pub fn elapsed(&self) -> Duration {
        self.start_time.read().unwrap().elapsed()
    }

    /// Get the total number of compactions performed.
    pub fn compaction_count(&self) -> u64 {
        self.compaction_count.load(Ordering::SeqCst)
    }

    /// Add tokens to the count.
    pub fn add_tokens(&self, count: usize) {
        self.token_count.fetch_add(count, Ordering::SeqCst);
    }

    /// Set the token count directly.
    pub fn set_token_count(&self, count: usize) {
        self.token_count.store(count, Ordering::SeqCst);
    }

    /// Record a session event.
    ///
    /// Updates message count and event count based on event type.
    /// Also estimates and adds tokens for the event.
    pub fn record_event(&self, event: &SessionEvent) {
        // Always increment event count
        self.event_count.fetch_add(1, Ordering::SeqCst);

        // Increment message count for participant messages
        if matches!(event, SessionEvent::MessageReceived { .. }) {
            self.message_count.fetch_add(1, Ordering::SeqCst);
        }

        // Estimate tokens for the event
        let token_estimate = estimate_event_tokens(event);
        self.add_tokens(token_estimate);
    }

    /// Request manual compaction.
    pub fn request_compaction(&self) {
        self.manual_request.store(true, Ordering::SeqCst);
    }

    /// Check if manual compaction was requested.
    pub fn is_manual_requested(&self) -> bool {
        self.manual_request.load(Ordering::SeqCst)
    }

    /// Check if compaction should occur.
    ///
    /// Returns `Some(reason)` if any trigger threshold is exceeded,
    /// or `None` if no compaction is needed.
    pub fn should_compact(&self) -> Option<CompactionReason> {
        // Check if disabled
        if !self.config.enabled {
            return None;
        }

        // Check manual request first
        if self.manual_request.load(Ordering::SeqCst) {
            return Some(CompactionReason::ManualRequest);
        }

        // Check token limit
        if self.config.max_tokens > 0 {
            let current = self.token_count.load(Ordering::SeqCst);
            if current >= self.config.max_tokens {
                return Some(CompactionReason::TokenLimit {
                    current,
                    limit: self.config.max_tokens,
                });
            }
        }

        // Check message limit
        if self.config.max_messages > 0 {
            let current = self.message_count.load(Ordering::SeqCst);
            if current >= self.config.max_messages {
                return Some(CompactionReason::MessageLimit {
                    current,
                    limit: self.config.max_messages,
                });
            }
        }

        // Check event limit
        if self.config.max_events > 0 {
            let current = self.event_count.load(Ordering::SeqCst);
            if current >= self.config.max_events {
                return Some(CompactionReason::EventLimit {
                    current,
                    limit: self.config.max_events,
                });
            }
        }

        // Check duration limit
        if let Some(max_duration) = self.config.max_duration {
            let elapsed = self.elapsed();
            if elapsed >= max_duration {
                return Some(CompactionReason::DurationLimit {
                    elapsed,
                    limit: max_duration,
                });
            }
        }

        None
    }

    /// Check if compaction is needed (convenience method).
    pub fn needs_compaction(&self) -> bool {
        self.should_compact().is_some()
    }

    /// Reset all counters after compaction.
    ///
    /// This should be called after a successful compaction to reset
    /// all trigger state for the new context window.
    pub fn reset(&self) {
        self.token_count.store(0, Ordering::SeqCst);
        self.message_count.store(0, Ordering::SeqCst);
        self.event_count.store(0, Ordering::SeqCst);
        self.manual_request.store(false, Ordering::SeqCst);
        *self.start_time.write().unwrap() = Instant::now();
        self.compaction_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Reset only the manual request flag without resetting counters.
    pub fn clear_manual_request(&self) {
        self.manual_request.store(false, Ordering::SeqCst);
    }

    /// Get a snapshot of current metrics.
    pub fn metrics(&self) -> CompactionMetrics {
        CompactionMetrics {
            token_count: self.token_count(),
            message_count: self.message_count(),
            event_count: self.event_count(),
            elapsed: self.elapsed(),
            compaction_count: self.compaction_count(),
            manual_requested: self.is_manual_requested(),
        }
    }
}

impl Default for CompactionTrigger {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl std::fmt::Debug for CompactionTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompactionTrigger")
            .field("config", &self.config)
            .field("token_count", &self.token_count())
            .field("message_count", &self.message_count())
            .field("event_count", &self.event_count())
            .field("elapsed_secs", &self.elapsed().as_secs_f64())
            .field("compaction_count", &self.compaction_count())
            .finish()
    }
}

/// Snapshot of compaction metrics.
#[derive(Debug, Clone)]
pub struct CompactionMetrics {
    /// Current token count.
    pub token_count: usize,
    /// Current message count.
    pub message_count: usize,
    /// Current event count.
    pub event_count: usize,
    /// Time since start or last compaction.
    pub elapsed: Duration,
    /// Total compactions performed.
    pub compaction_count: u64,
    /// Whether manual compaction was requested.
    pub manual_requested: bool,
}

impl CompactionMetrics {
    /// Calculate progress toward token limit (0.0 to 1.0+).
    pub fn token_progress(&self, limit: usize) -> f64 {
        if limit == 0 {
            0.0
        } else {
            self.token_count as f64 / limit as f64
        }
    }

    /// Calculate progress toward message limit (0.0 to 1.0+).
    pub fn message_progress(&self, limit: usize) -> f64 {
        if limit == 0 {
            0.0
        } else {
            self.message_count as f64 / limit as f64
        }
    }

    /// Calculate progress toward event limit (0.0 to 1.0+).
    pub fn event_progress(&self, limit: usize) -> f64 {
        if limit == 0 {
            0.0
        } else {
            self.event_count as f64 / limit as f64
        }
    }

    /// Calculate progress toward duration limit (0.0 to 1.0+).
    pub fn duration_progress(&self, limit: Duration) -> f64 {
        if limit.is_zero() {
            0.0
        } else {
            self.elapsed.as_secs_f64() / limit.as_secs_f64()
        }
    }
}

/// Estimate the number of tokens in a session event.
///
/// This is a simple heuristic - real implementations should use
/// a proper tokenizer like tiktoken. The estimate uses a rough
/// approximation of 4 characters per token for English text.
fn estimate_event_tokens(event: &SessionEvent) -> usize {
    let content_len = match event {
        SessionEvent::MessageReceived { content, .. } => content.len(),
        SessionEvent::AgentResponded { content, .. } => content.len(),
        SessionEvent::AgentThinking { thought } => thought.len(),
        SessionEvent::ToolCalled { args, .. } => args.to_string().len(),
        SessionEvent::ToolCompleted { result, error, .. } => {
            result.len() + error.as_ref().map(|e| e.len()).unwrap_or(0)
        }
        SessionEvent::SessionCompacted { summary, .. } => summary.len(),
        SessionEvent::SessionEnded { reason } => reason.len(),
        SessionEvent::SubagentSpawned { prompt, .. } => prompt.len(),
        SessionEvent::SubagentCompleted { result, .. } => result.len(),
        SessionEvent::SubagentFailed { error, .. } => error.len(),
        SessionEvent::Custom { payload, .. } => payload.to_string().len(),
        SessionEvent::SessionStarted { .. } => 100, // Fixed overhead
        // Streaming events
        SessionEvent::TextDelta { delta, .. } => delta.len(),
        // Note events (small metadata)
        SessionEvent::NoteParsed { .. } => 50,
        SessionEvent::NoteCreated { title, .. } => {
            title.as_ref().map(|t| t.len()).unwrap_or(0) + 50
        }
        SessionEvent::NoteModified { .. } => 50,
        // MCP/Tool events
        SessionEvent::McpAttached { server, .. } => server.len() + 50,
        SessionEvent::ToolDiscovered { name, schema, .. } => {
            name.len() + schema.as_ref().map(|s| s.to_string().len()).unwrap_or(0)
        }
        // File events (from crucible-core)
        SessionEvent::FileChanged { .. } => 50,
        SessionEvent::FileDeleted { .. } => 50,
        SessionEvent::FileMoved { .. } => 50,
        // Storage events (from crucible-core)
        SessionEvent::EntityStored { .. } => 50,
        SessionEvent::EntityDeleted { .. } => 50,
        SessionEvent::BlocksUpdated { .. } => 50,
        SessionEvent::RelationStored { .. } => 50,
        SessionEvent::RelationDeleted { .. } => 50,
        SessionEvent::TagAssociated { tag, .. } => tag.len() + 50,
        // Embedding events (from crucible-core)
        SessionEvent::EmbeddingRequested { .. } => 50,
        SessionEvent::EmbeddingStored { .. } => 50,
        SessionEvent::EmbeddingFailed { .. } => 50,
        SessionEvent::EmbeddingBatchComplete { .. } => 50,
        // Pre-events (interception points)
        SessionEvent::PreToolCall { name, .. } => name.len() + 50,
        SessionEvent::PreParse { .. } => 50,
        SessionEvent::PreLlmCall { prompt, .. } => prompt.len(),
    };

    // Rough estimate: ~4 characters per token
    // Add fixed overhead for event structure
    (content_len / 4).max(1) + 10
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;
    use std::time::Duration;

    /// Cross-platform test path helper
    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("crucible_test_{}", name))
    }

    #[test]
    fn test_config_default() {
        let config = CompactionConfig::default();
        assert_eq!(config.max_tokens, 100_000);
        assert_eq!(config.max_messages, 0);
        assert_eq!(config.max_events, 0);
        assert!(config.max_duration.is_none());
        assert!(config.enabled);
    }

    #[test]
    fn test_config_disabled() {
        let config = CompactionConfig::disabled();
        assert_eq!(config.max_tokens, 0);
        assert!(!config.enabled);
    }

    #[test]
    fn test_config_builder() {
        let config = CompactionConfig::default()
            .with_max_tokens(50_000)
            .with_max_messages(100)
            .with_max_events(500)
            .with_max_duration(Duration::from_secs(3600));

        assert_eq!(config.max_tokens, 50_000);
        assert_eq!(config.max_messages, 100);
        assert_eq!(config.max_events, 500);
        assert_eq!(config.max_duration, Some(Duration::from_secs(3600)));
    }

    #[test]
    fn test_trigger_new() {
        let trigger = CompactionTrigger::with_defaults();

        assert_eq!(trigger.token_count(), 0);
        assert_eq!(trigger.message_count(), 0);
        assert_eq!(trigger.event_count(), 0);
        assert_eq!(trigger.compaction_count(), 0);
        assert!(!trigger.is_manual_requested());
    }

    #[test]
    fn test_trigger_add_tokens() {
        let trigger = CompactionTrigger::with_defaults();

        trigger.add_tokens(100);
        assert_eq!(trigger.token_count(), 100);

        trigger.add_tokens(50);
        assert_eq!(trigger.token_count(), 150);

        trigger.set_token_count(200);
        assert_eq!(trigger.token_count(), 200);
    }

    #[test]
    fn test_trigger_record_event() {
        let trigger = CompactionTrigger::with_defaults();

        // Record a message event
        let msg_event = SessionEvent::MessageReceived {
            content: "Hello world".into(),
            participant_id: "user".into(),
        };
        trigger.record_event(&msg_event);

        assert_eq!(trigger.event_count(), 1);
        assert_eq!(trigger.message_count(), 1);
        assert!(trigger.token_count() > 0);

        // Record a non-message event
        let path = test_path("test.txt");
        let tool_event = SessionEvent::ToolCalled {
            name: "read_file".into(),
            args: json!({"path": path.to_string_lossy()}),
        };
        trigger.record_event(&tool_event);

        assert_eq!(trigger.event_count(), 2);
        assert_eq!(trigger.message_count(), 1); // Still 1
    }

    #[test]
    fn test_trigger_token_limit() {
        let config = CompactionConfig::default().with_max_tokens(100);
        let trigger = CompactionTrigger::new(config);

        // Below limit
        trigger.add_tokens(50);
        assert!(trigger.should_compact().is_none());

        // At limit
        trigger.add_tokens(50);
        let reason = trigger.should_compact();
        assert!(matches!(reason, Some(CompactionReason::TokenLimit { .. })));

        if let Some(CompactionReason::TokenLimit { current, limit }) = reason {
            assert_eq!(current, 100);
            assert_eq!(limit, 100);
        }
    }

    #[test]
    fn test_trigger_message_limit() {
        let config = CompactionConfig::default()
            .with_max_tokens(0) // Disable token trigger
            .with_max_messages(3);
        let trigger = CompactionTrigger::new(config);

        for i in 0..3 {
            trigger.record_event(&SessionEvent::MessageReceived {
                content: format!("Message {}", i),
                participant_id: "user".into(),
            });
        }

        let reason = trigger.should_compact();
        assert!(matches!(
            reason,
            Some(CompactionReason::MessageLimit { .. })
        ));
    }

    #[test]
    fn test_trigger_event_limit() {
        let config = CompactionConfig::default()
            .with_max_tokens(0) // Disable token trigger
            .with_max_events(5);
        let trigger = CompactionTrigger::new(config);

        for _ in 0..5 {
            trigger.record_event(&SessionEvent::AgentThinking {
                thought: "Processing...".into(),
            });
        }

        let reason = trigger.should_compact();
        assert!(matches!(reason, Some(CompactionReason::EventLimit { .. })));
    }

    #[test]
    fn test_trigger_manual_request() {
        let trigger = CompactionTrigger::with_defaults();

        assert!(!trigger.is_manual_requested());
        assert!(trigger.should_compact().is_none());

        trigger.request_compaction();

        assert!(trigger.is_manual_requested());
        let reason = trigger.should_compact();
        assert!(matches!(reason, Some(CompactionReason::ManualRequest)));
    }

    #[test]
    fn test_trigger_disabled() {
        let config = CompactionConfig::disabled();
        let trigger = CompactionTrigger::new(config);

        trigger.add_tokens(1_000_000);
        trigger.request_compaction();

        // Should still return None when disabled
        assert!(trigger.should_compact().is_none());
    }

    #[test]
    fn test_trigger_reset() {
        let config = CompactionConfig::default().with_max_tokens(100);
        let trigger = CompactionTrigger::new(config);

        trigger.add_tokens(150);
        trigger.request_compaction();
        assert!(trigger.should_compact().is_some());
        assert_eq!(trigger.compaction_count(), 0);

        trigger.reset();

        assert_eq!(trigger.token_count(), 0);
        assert_eq!(trigger.message_count(), 0);
        assert_eq!(trigger.event_count(), 0);
        assert!(!trigger.is_manual_requested());
        assert_eq!(trigger.compaction_count(), 1);
        assert!(trigger.should_compact().is_none());
    }

    #[test]
    fn test_trigger_needs_compaction() {
        let config = CompactionConfig::default().with_max_tokens(100);
        let trigger = CompactionTrigger::new(config);

        assert!(!trigger.needs_compaction());

        trigger.add_tokens(100);
        assert!(trigger.needs_compaction());
    }

    #[test]
    fn test_trigger_metrics() {
        let config = CompactionConfig::default().with_max_tokens(1000);
        let trigger = CompactionTrigger::new(config);

        trigger.add_tokens(500);
        trigger.record_event(&SessionEvent::MessageReceived {
            content: "Test".into(),
            participant_id: "user".into(),
        });

        let metrics = trigger.metrics();

        // Token count includes manually added + event tokens
        assert!(metrics.token_count > 500);
        assert_eq!(metrics.message_count, 1);
        assert_eq!(metrics.event_count, 1);
        assert_eq!(metrics.compaction_count, 0);
        assert!(!metrics.manual_requested);

        // Check progress calculations
        assert!(metrics.token_progress(1000) > 0.5);
        assert_eq!(metrics.message_progress(10), 0.1);
    }

    #[test]
    fn test_reason_display() {
        let reason = CompactionReason::TokenLimit {
            current: 100_000,
            limit: 100_000,
        };
        assert!(reason.to_string().contains("token limit exceeded"));

        let reason = CompactionReason::MessageLimit {
            current: 50,
            limit: 50,
        };
        assert!(reason.to_string().contains("message limit exceeded"));

        let reason = CompactionReason::DurationLimit {
            elapsed: Duration::from_secs(3600),
            limit: Duration::from_secs(3600),
        };
        assert!(reason.to_string().contains("duration limit exceeded"));

        let reason = CompactionReason::ManualRequest;
        assert!(reason.to_string().contains("manual"));
    }

    #[test]
    fn test_estimate_event_tokens() {
        let event = SessionEvent::MessageReceived {
            content: "This is a test message with about 40 characters".into(),
            participant_id: "user".into(),
        };
        let tokens = estimate_event_tokens(&event);

        // ~40 chars / 4 = 10, + 10 overhead = 20
        assert!((10..=30).contains(&tokens));

        let event = SessionEvent::SessionStarted {
            config: crate::reactor::SessionEventConfig::new("test"),
        };
        let tokens = estimate_event_tokens(&event);
        // Fixed 100 / 4 = 25, + 10 = 35
        assert_eq!(tokens, 35);
    }

    #[test]
    fn test_trigger_priority() {
        // When multiple triggers would fire, manual request takes priority
        let config = CompactionConfig::default()
            .with_max_tokens(10)
            .with_max_messages(1);
        let trigger = CompactionTrigger::new(config);

        trigger.add_tokens(100);
        trigger.record_event(&SessionEvent::MessageReceived {
            content: "Test".into(),
            participant_id: "user".into(),
        });
        trigger.request_compaction();

        // Manual request should be returned first
        let reason = trigger.should_compact();
        assert!(matches!(reason, Some(CompactionReason::ManualRequest)));
    }

    #[test]
    fn test_clear_manual_request() {
        let trigger = CompactionTrigger::with_defaults();

        trigger.request_compaction();
        assert!(trigger.is_manual_requested());

        trigger.clear_manual_request();
        assert!(!trigger.is_manual_requested());
    }

    #[test]
    fn test_trigger_debug() {
        let trigger = CompactionTrigger::with_defaults();
        let debug = format!("{:?}", trigger);

        assert!(debug.contains("CompactionTrigger"));
        assert!(debug.contains("token_count"));
        assert!(debug.contains("message_count"));
    }

    #[test]
    fn test_metrics_progress_edge_cases() {
        let metrics = CompactionMetrics {
            token_count: 50,
            message_count: 5,
            event_count: 10,
            elapsed: Duration::from_secs(30),
            compaction_count: 0,
            manual_requested: false,
        };

        // Zero limits should return 0.0
        assert_eq!(metrics.token_progress(0), 0.0);
        assert_eq!(metrics.message_progress(0), 0.0);
        assert_eq!(metrics.event_progress(0), 0.0);
        assert_eq!(metrics.duration_progress(Duration::ZERO), 0.0);

        // Normal progress
        assert_eq!(metrics.token_progress(100), 0.5);
        assert_eq!(metrics.message_progress(10), 0.5);
        assert_eq!(metrics.event_progress(20), 0.5);
        assert_eq!(metrics.duration_progress(Duration::from_secs(60)), 0.5);
    }
}
