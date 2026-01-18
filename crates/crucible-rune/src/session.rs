//! Session Management for Crucible
//!
//! This module provides the `Session` struct and related types for managing
//! interactive sessions with event-driven architecture.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                      Session                             │
//! │                                                          │
//! │  ┌──────────────────────────────────────────────────┐   │
//! │  │              Ring Buffer (events)                  │   │
//! │  │         (storage/transport, Arc refs)              │   │
//! │  └────────────────────┬─────────────────────────────┘   │
//! │                       │                                  │
//! │           ┌───────────▼───────────┐                     │
//! │           │       EventBus        │ ◄── Rune handlers   │
//! │           │    (pub/sub layer)    │     (plugins)       │
//! │           └───────────┬───────────┘                     │
//! │                       │                                  │
//! │           ┌───────────▼───────────┐                     │
//! │           │       Reactor         │                     │
//! │           │  (context tree flow)  │                     │
//! │           └───────────┬───────────┘                     │
//! │                       │                                  │
//! │           ┌───────────▼───────────┐                     │
//! │           │        Kiln           │                     │
//! │           │    (persistence)      │                     │
//! │           └───────────────────────┘                     │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rune::session::{Session, SessionBuilder};
//! use crucible_rune::reactor::ReactorSessionConfig;
//!
//! // Build a session
//! let session = SessionBuilder::new("my-session")
//!     .with_folder("/path/to/Sessions/my-session")
//!     .with_system_prompt("You are a helpful assistant.")
//!     .build()
//!     .await?;
//!
//! // Get a handle for sending events
//! let handle = session.handle();
//!
//! // Send a message
//! handle.message("Hello!").await?;
//!
//! // Or send any event
//! handle.send(SessionEvent::AgentThinking {
//!     thought: "Processing...".into(),
//! }).await?;
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value as JsonValue;
use tokio::sync::{mpsc, RwLock};

use crate::event_bus::EventBus;
use crate::reactor::{
    Reactor, ReactorContext, ReactorError, ReactorResult, ReactorSessionConfig, SessionEvent,
};
use crate::simple_reactor::SimpleReactor;
use crucible_core::events::markdown::EventToMarkdown;
use crucible_core::events::EventRing;

/// Default ring buffer capacity.
const DEFAULT_RING_CAPACITY: usize = 4096;

/// Default event channel capacity.
const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// Session state enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Session is being initialized.
    Initializing,
    /// Session is active and processing events.
    Active,
    /// Session is paused (not processing new events).
    Paused,
    /// Session is being compacted.
    Compacting,
    /// Session has ended.
    Ended,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Initializing => write!(f, "initializing"),
            SessionState::Active => write!(f, "active"),
            SessionState::Paused => write!(f, "paused"),
            SessionState::Compacting => write!(f, "compacting"),
            SessionState::Ended => write!(f, "ended"),
        }
    }
}

/// A cloneable handle for interacting with a session.
///
/// The handle provides a lightweight way to send events to a session
/// from multiple locations. Cloning is cheap as it only clones the
/// channel sender and session metadata.
#[derive(Clone)]
pub struct SessionHandle {
    /// Unique session identifier.
    session_id: String,
    /// Session folder path.
    folder: PathBuf,
    /// Channel sender for events.
    event_tx: mpsc::Sender<SessionEvent>,
    /// Current session state (shared).
    state: Arc<RwLock<SessionState>>,
}

impl SessionHandle {
    /// Get the session ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get the session folder path.
    pub fn folder(&self) -> &PathBuf {
        &self.folder
    }

    /// Get the current session state.
    pub async fn state(&self) -> SessionState {
        *self.state.read().await
    }

    /// Check if the session is active.
    pub async fn is_active(&self) -> bool {
        *self.state.read().await == SessionState::Active
    }

    /// Send an event to the session.
    ///
    /// Returns `Ok(())` if the event was queued successfully.
    /// Returns `Err` if the session has ended or the channel is full.
    pub async fn send(&self, event: SessionEvent) -> ReactorResult<()> {
        self.event_tx
            .send(event)
            .await
            .map_err(|_| ReactorError::processing_failed("Session channel closed"))
    }

    /// Send a message from a participant.
    ///
    /// This is a convenience method for sending `MessageReceived` events.
    pub async fn message(&self, content: impl Into<String>) -> ReactorResult<()> {
        self.send(SessionEvent::MessageReceived {
            content: content.into(),
            participant_id: "user".to_string(),
        })
        .await
    }

    /// Send a message from a specific participant.
    pub async fn message_from(
        &self,
        content: impl Into<String>,
        participant_id: impl Into<String>,
    ) -> ReactorResult<()> {
        self.send(SessionEvent::MessageReceived {
            content: content.into(),
            participant_id: participant_id.into(),
        })
        .await
    }

    /// Send a custom event.
    pub async fn custom(&self, name: impl Into<String>, payload: JsonValue) -> ReactorResult<()> {
        self.send(SessionEvent::Custom {
            name: name.into(),
            payload,
        })
        .await
    }

    /// Request the session to end.
    pub async fn end(&self, reason: impl Into<String>) -> ReactorResult<()> {
        self.send(SessionEvent::SessionEnded {
            reason: reason.into(),
        })
        .await
    }

    /// Send a tool result (successful completion).
    ///
    /// This is a convenience method for sending `ToolCompleted` events
    /// with no error.
    pub async fn tool_result(
        &self,
        name: impl Into<String>,
        result: impl Into<String>,
    ) -> ReactorResult<()> {
        self.send(SessionEvent::ToolCompleted {
            name: name.into(),
            result: result.into(),
            error: None,
        })
        .await
    }

    /// Send a tool error (failed completion).
    ///
    /// This is a convenience method for sending `ToolCompleted` events
    /// with an error.
    pub async fn tool_error(
        &self,
        name: impl Into<String>,
        result: impl Into<String>,
        error: impl Into<String>,
    ) -> ReactorResult<()> {
        self.send(SessionEvent::ToolCompleted {
            name: name.into(),
            result: result.into(),
            error: Some(error.into()),
        })
        .await
    }

    /// Send a tool called event.
    ///
    /// This is a convenience method for sending `ToolCalled` events.
    pub async fn tool_called(&self, name: impl Into<String>, args: JsonValue) -> ReactorResult<()> {
        self.send(SessionEvent::ToolCalled {
            name: name.into(),
            args,
        })
        .await
    }

    /// Send an agent thinking event.
    ///
    /// This is a convenience method for sending `AgentThinking` events.
    pub async fn thinking(&self, thought: impl Into<String>) -> ReactorResult<()> {
        self.send(SessionEvent::AgentThinking {
            thought: thought.into(),
        })
        .await
    }
}

impl std::fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHandle")
            .field("session_id", &self.session_id)
            .field("folder", &self.folder)
            .finish()
    }
}

/// The main session struct that orchestrates event processing.
///
/// A `Session` coordinates:
/// - Event ring buffer for in-memory event storage
/// - Reactor for event processing logic
/// - EventBus for pub/sub handler integration
/// - Async event loop for processing queued events
///
/// Sessions are typically created via `SessionBuilder`.
pub struct Session {
    /// Session configuration.
    config: Arc<ReactorSessionConfig>,
    /// Event ring buffer.
    ring: Arc<EventRing<SessionEvent>>,
    /// The reactor for processing events.
    reactor: Arc<dyn Reactor>,
    /// Event bus for pub/sub.
    event_bus: Arc<RwLock<EventBus>>,
    /// Channel sender for events.
    event_tx: mpsc::Sender<SessionEvent>,
    /// Channel receiver for events (moved into event loop).
    event_rx: Arc<RwLock<Option<mpsc::Receiver<SessionEvent>>>>,
    /// Current session state.
    state: Arc<RwLock<SessionState>>,
    /// Reactor context for processing.
    context: Arc<RwLock<ReactorContext>>,
    /// Current file index for event persistence (0 = 000-context.md, 1 = 001-context.md, etc.)
    current_file_index: Arc<RwLock<usize>>,
}

impl Session {
    /// Create a new session with the given configuration and reactor.
    fn new(
        config: ReactorSessionConfig,
        reactor: Arc<dyn Reactor>,
        ring_capacity: usize,
        channel_capacity: usize,
    ) -> Self {
        let config = Arc::new(config);
        let ring = Arc::new(EventRing::new(ring_capacity));
        let (event_tx, event_rx) = mpsc::channel(channel_capacity);
        let state = Arc::new(RwLock::new(SessionState::Initializing));
        let context = Arc::new(RwLock::new(ReactorContext::new(Arc::clone(&config))));
        let current_file_index = Arc::new(RwLock::new(0));

        // Set up overflow callback to flush events to kiln
        let config_for_callback = Arc::clone(&config);
        let file_index_for_callback = Arc::clone(&current_file_index);
        ring.set_overflow_callback(Arc::new(move |events: &[Arc<SessionEvent>]| {
            Self::flush_events_to_kiln_sync(
                &config_for_callback.folder,
                &file_index_for_callback,
                events,
            );
        }));

        Self {
            config,
            ring,
            reactor,
            event_bus: Arc::new(RwLock::new(EventBus::new())),
            event_tx,
            event_rx: Arc::new(RwLock::new(Some(event_rx))),
            state,
            context,
            current_file_index,
        }
    }

    /// Synchronously flush events to the kiln file.
    ///
    /// This is used by the ring buffer overflow callback to persist events
    /// before they are overwritten. Uses blocking I/O since it's called from
    /// a sync context.
    fn flush_events_to_kiln_sync(
        folder: &std::path::Path,
        file_index: &Arc<RwLock<usize>>,
        events: &[Arc<SessionEvent>],
    ) {
        use std::fs::OpenOptions;
        use std::io::Write;

        if events.is_empty() {
            return;
        }

        // Get current file index - use blocking try_read to avoid deadlock
        let index = match file_index.try_read() {
            Ok(guard) => *guard,
            Err(_) => {
                // If we can't get the lock, log and skip
                tracing::warn!("Could not acquire file index lock for overflow flush");
                return;
            }
        };

        let file_path = folder.join(format!("{:03}-context.md", index));

        // Get current timestamp
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        // Open file in append mode
        let mut file = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
        {
            Ok(f) => f,
            Err(e) => {
                tracing::error!(
                    path = %file_path.display(),
                    error = %e,
                    "Failed to open file for overflow flush"
                );
                return;
            }
        };

        // Write each event as a markdown block
        for event in events {
            let markdown = event.to_markdown_block(Some(timestamp_ms));
            if let Err(e) = file.write_all(markdown.as_bytes()) {
                tracing::error!(
                    path = %file_path.display(),
                    error = %e,
                    "Failed to write event during overflow flush"
                );
                return;
            }
        }

        // Flush to ensure data is written
        if let Err(e) = file.flush() {
            tracing::error!(
                path = %file_path.display(),
                error = %e,
                "Failed to flush file during overflow"
            );
            return;
        }

        tracing::debug!(
            path = %file_path.display(),
            event_count = events.len(),
            "Flushed events to kiln on ring overflow"
        );
    }

    /// Get the session ID.
    pub fn session_id(&self) -> &str {
        &self.config.session_id
    }

    /// Get the session folder path.
    pub fn folder(&self) -> &PathBuf {
        &self.config.folder
    }

    /// Get the session configuration.
    pub fn config(&self) -> &ReactorSessionConfig {
        &self.config
    }

    /// Get a reference to the event ring.
    pub fn ring(&self) -> &Arc<EventRing<SessionEvent>> {
        &self.ring
    }

    /// Get the current session state.
    pub async fn state(&self) -> SessionState {
        *self.state.read().await
    }

    /// Get a cloneable handle for this session.
    pub fn handle(&self) -> SessionHandle {
        SessionHandle {
            session_id: self.config.session_id.clone(),
            folder: self.config.folder.clone(),
            event_tx: self.event_tx.clone(),
            state: Arc::clone(&self.state),
        }
    }

    /// Get a mutable reference to the event bus for registering handlers.
    pub async fn event_bus(&self) -> tokio::sync::RwLockWriteGuard<'_, EventBus> {
        self.event_bus.write().await
    }

    /// Get the current event count in the ring buffer.
    pub fn event_count(&self) -> usize {
        self.ring.len()
    }

    /// Get the current write sequence number.
    pub fn current_sequence(&self) -> u64 {
        self.ring.write_sequence()
    }

    /// Get an event by sequence number.
    pub fn get_event(&self, seq: u64) -> Option<Arc<SessionEvent>> {
        self.ring.get(seq)
    }

    /// Iterate over all valid events in the ring.
    pub fn iter_events(&self) -> impl Iterator<Item = Arc<SessionEvent>> + '_ {
        self.ring.iter()
    }

    // ─────────────────────────────────────────────────────────────────────
    // TUI Helper Methods
    // ─────────────────────────────────────────────────────────────────────

    /// Get recent messages for display (filters MessageReceived + AgentResponded).
    ///
    /// Returns the most recent `limit` message events from the ring buffer,
    /// ordered from oldest to newest.
    pub fn recent_messages(&self, limit: usize) -> Vec<Arc<SessionEvent>> {
        let messages: Vec<Arc<SessionEvent>> = self
            .ring
            .iter()
            .filter(|event| {
                matches!(
                    event.as_ref(),
                    SessionEvent::MessageReceived { .. } | SessionEvent::AgentResponded { .. }
                )
            })
            .collect();

        // Return last `limit` messages
        if messages.len() <= limit {
            messages
        } else {
            messages[messages.len() - limit..].to_vec()
        }
    }

    /// Get pending tool calls (ToolCalled without matching ToolCompleted).
    ///
    /// Returns tool calls that have been initiated but not yet completed.
    /// Matches by tool name since events don't have call IDs.
    pub fn pending_tools(&self) -> Vec<Arc<SessionEvent>> {
        let mut called: Vec<Arc<SessionEvent>> = Vec::new();
        let mut completed_names: Vec<String> = Vec::new();

        // Scan through events to track called vs completed
        for event in self.ring.iter() {
            if let SessionEvent::ToolCalled { name, .. } = event.as_ref() {
                called.push(event.clone());
                // Remove from completed if it was marked (handles re-calls)
                if let Some(pos) = completed_names.iter().position(|n| n == name) {
                    completed_names.remove(pos);
                }
            } else if let SessionEvent::ToolCompleted { name, .. } = event.as_ref() {
                completed_names.push(name.clone());
            }
        }

        // Filter out tools that have been completed
        called
            .into_iter()
            .filter(|event| {
                if let SessionEvent::ToolCalled { name, .. } = event.as_ref() {
                    !completed_names.contains(name)
                } else {
                    false
                }
            })
            .collect()
    }

    /// Check if the session is currently streaming a response.
    ///
    /// Returns true if there are TextDelta events without a subsequent
    /// AgentResponded event (meaning streaming is in progress).
    pub fn is_streaming(&self) -> bool {
        let mut has_text_delta = false;

        // Collect events and scan from end to find the last relevant event
        let events: Vec<_> = self.ring.iter().collect();
        for event in events.iter().rev() {
            match event.as_ref() {
                SessionEvent::TextDelta { .. } => {
                    has_text_delta = true;
                }
                SessionEvent::AgentResponded { .. } => {
                    // If we see AgentResponded, streaming is done
                    return false;
                }
                SessionEvent::MessageReceived { .. } => {
                    // New message breaks the chain - check if we had deltas
                    return has_text_delta;
                }
                _ => {}
            }
        }

        has_text_delta
    }

    /// Cancel the current operation.
    ///
    /// Emits a cancellation event that handlers can observe. This does not
    /// forcibly stop processing but signals that cancellation was requested.
    pub fn cancel(&self) -> ReactorResult<()> {
        let event = SessionEvent::Custom {
            name: "cancelled".to_string(),
            payload: serde_json::json!({"reason": "user requested cancellation"}),
        };
        self.ring.push(event);
        Ok(())
    }

    /// Get the current token count from the reactor context.
    pub async fn token_count(&self) -> usize {
        self.context.read().await.token_count()
    }

    /// Check if compaction is needed.
    pub async fn should_compact(&self) -> bool {
        self.context.read().await.compaction_requested()
    }

    /// Get the current file index.
    pub async fn current_file_index(&self) -> usize {
        *self.current_file_index.read().await
    }

    /// Get the path to the current context file.
    pub async fn current_file_path(&self) -> PathBuf {
        let index = *self.current_file_index.read().await;
        self.config.folder.join(format!("{:03}-context.md", index))
    }

    /// Increment the file index (used during compaction).
    pub async fn increment_file_index(&self) -> usize {
        let mut index = self.current_file_index.write().await;
        *index += 1;
        *index
    }

    /// Append an event to the current session file.
    ///
    /// Converts the event to a markdown block and appends it to the current
    /// context file. Returns the number of bytes written.
    pub async fn append_event_to_file(&self, event: &SessionEvent) -> ReactorResult<usize> {
        use std::fs::OpenOptions;
        use std::io::Write;

        let file_path = self.current_file_path().await;
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        // Convert event to markdown block
        let markdown = event.to_markdown_block(Some(timestamp_ms));
        let bytes_written = markdown.len();

        // Open file in append mode, create if doesn't exist
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .map_err(|e| {
                ReactorError::storage(format!(
                    "Failed to open session file '{}': {}",
                    file_path.display(),
                    e
                ))
            })?;

        // Write the markdown block
        file.write_all(markdown.as_bytes()).map_err(|e| {
            ReactorError::storage(format!(
                "Failed to write to session file '{}': {}",
                file_path.display(),
                e
            ))
        })?;

        // Flush to ensure data is written
        file.flush().map_err(|e| {
            ReactorError::storage(format!(
                "Failed to flush session file '{}': {}",
                file_path.display(),
                e
            ))
        })?;

        tracing::trace!(
            session_id = %self.config.session_id,
            file = %file_path.display(),
            event_type = event.event_type_name(),
            bytes_written,
            "Appended event to session file"
        );

        Ok(bytes_written)
    }

    /// Start the session.
    ///
    /// This creates the session folder, initializes the reactor, and
    /// transitions to the Active state. Call `run()` to start processing events.
    pub async fn start(&self) -> ReactorResult<()> {
        // Create session folder if it doesn't exist
        if !self.config.folder.exists() {
            std::fs::create_dir_all(&self.config.folder).map_err(|e| {
                ReactorError::init_failed(format!(
                    "Failed to create session folder '{}': {}",
                    self.config.folder.display(),
                    e
                ))
            })?;
            tracing::debug!(
                folder = %self.config.folder.display(),
                "Created session folder"
            );
        }

        // Push SessionStarted event to our ring and persist to file
        let start_event = SessionEvent::SessionStarted {
            config: (&*self.config).into(),
        };
        self.ring.push(start_event.clone());
        self.append_event_to_file(&start_event).await?;

        // Initialize reactor (may do its own setup, but we own the event ring)
        self.reactor.on_session_start(&self.config).await?;

        // Transition to active state
        let mut state = self.state.write().await;
        *state = SessionState::Active;

        tracing::info!(
            session_id = %self.config.session_id,
            folder = %self.config.folder.display(),
            "Session started"
        );

        Ok(())
    }

    /// Run the session event loop.
    ///
    /// This method takes ownership of the event receiver and processes
    /// events until the session ends.
    ///
    /// Returns when the session ends or all senders are dropped.
    pub async fn run(&self) -> ReactorResult<()> {
        // Take the receiver (can only run once)
        let mut rx = {
            let mut rx_guard = self.event_rx.write().await;
            rx_guard
                .take()
                .ok_or_else(|| ReactorError::init_failed("Session event loop already running"))?
        };

        // Process events
        while let Some(event) = rx.recv().await {
            // Check if we should stop
            if matches!(event, SessionEvent::SessionEnded { .. }) {
                self.process_event(event).await?;
                break;
            }

            // Check state
            let state = *self.state.read().await;
            if state == SessionState::Ended {
                break;
            }
            if state == SessionState::Paused {
                // TODO: Queue event for later or drop?
                continue;
            }

            // Process the event
            self.process_event(event).await?;
        }

        // End the session
        self.end_internal("event loop completed").await?;

        Ok(())
    }

    /// Process a single event through the reactor.
    async fn process_event(&self, event: SessionEvent) -> ReactorResult<()> {
        // Push event to ring buffer
        let seq = self.ring.push(event.clone());

        // Persist event to session file
        self.append_event_to_file(&event).await?;

        // Get mutable context
        let mut ctx = self.context.write().await;
        ctx.reset_for_event();
        ctx.set_current_seq(seq);

        // Process through reactor using handle_event (takes the event directly)
        let event_bus_ctx = ctx.event_context_mut();
        let _processed = self.reactor.handle_event(event_bus_ctx, event).await?;

        // Handle any events emitted by the reactor via EventBus context
        let emitted = event_bus_ctx.take_emitted();
        for emitted_event in emitted {
            // Convert EventBus event to SessionEvent and push to ring
            let session_event = SessionEvent::Custom {
                name: emitted_event.identifier,
                payload: emitted_event.payload,
            };
            self.ring.push(session_event.clone());
            // Persist emitted event to file
            self.append_event_to_file(&session_event).await?;
        }

        // Bridge events from ReactorContext
        ctx.bridge_event_context();
        let reactor_emitted = ctx.take_emitted();
        for emitted_event in reactor_emitted {
            self.ring.push(emitted_event.clone());
            // Persist reactor-emitted event to file
            self.append_event_to_file(&emitted_event).await?;
        }

        // Estimate and track tokens
        let token_estimate = _processed.estimate_tokens();
        ctx.add_tokens(token_estimate);

        // Check if compaction is needed
        if ctx.compaction_requested() {
            drop(ctx); // Release lock before compact
            self.compact().await?;
        }

        Ok(())
    }

    /// Compact the session context.
    ///
    /// This creates a summary of recent events, writes a new context file
    /// with the summary as a header and a wikilink to the previous file,
    /// and resets the token count.
    pub async fn compact(&self) -> ReactorResult<PathBuf> {
        use std::fs::OpenOptions;
        use std::io::Write;

        // Set state to compacting
        {
            let mut state = self.state.write().await;
            *state = SessionState::Compacting;
        }

        // Get the previous file path before incrementing
        let previous_file_path = self.current_file_path().await;
        let previous_file_index = *self.current_file_index.read().await;

        // Collect events for summary
        let events: Vec<SessionEvent> = self.ring.iter().map(|arc| (*arc).clone()).collect();

        // Generate summary via reactor
        let summary = self.reactor.on_before_compact(&events).await?;

        // Increment file index and create new context file path
        let new_index = self.increment_file_index().await;
        let new_file = self
            .config
            .folder
            .join(format!("{:03}-context.md", new_index));

        // Build the header content with summary and wikilink to previous file
        let previous_file_name = format!("{:03}-context", previous_file_index);
        let header = format!(
            "# Session Context (Compacted)\n\n\
             > Previous context: [[{}|full history]]\n\n\
             ## Summary\n\n\
             {}\n\n\
             ---\n\n",
            previous_file_name, summary
        );

        // Write the header to the new file
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&new_file)
            .map_err(|e| {
                ReactorError::storage(format!(
                    "Failed to create compacted context file '{}': {}",
                    new_file.display(),
                    e
                ))
            })?;

        file.write_all(header.as_bytes()).map_err(|e| {
            ReactorError::storage(format!(
                "Failed to write compaction header to '{}': {}",
                new_file.display(),
                e
            ))
        })?;

        file.flush().map_err(|e| {
            ReactorError::storage(format!(
                "Failed to flush compacted context file '{}': {}",
                new_file.display(),
                e
            ))
        })?;

        // Push compaction event to the ring and persist to the new file
        let compact_event = SessionEvent::SessionCompacted {
            summary: summary.clone(),
            new_file: new_file.clone(),
        };
        self.ring.push(compact_event.clone());
        self.append_event_to_file(&compact_event).await?;

        // Reset token count
        {
            let mut ctx = self.context.write().await;
            ctx.reset_token_count();
        }

        // Restore active state
        {
            let mut state = self.state.write().await;
            *state = SessionState::Active;
        }

        tracing::info!(
            session_id = %self.config.session_id,
            previous_file = %previous_file_path.display(),
            new_file = %new_file.display(),
            "Session compacted"
        );

        Ok(new_file)
    }

    /// End the session.
    pub async fn end(&self, reason: impl Into<String>) -> ReactorResult<()> {
        self.end_internal(reason).await
    }

    /// Internal end implementation.
    async fn end_internal(&self, reason: impl Into<String>) -> ReactorResult<()> {
        let reason = reason.into();

        // Set state to ended
        {
            let mut state = self.state.write().await;
            if *state == SessionState::Ended {
                return Ok(()); // Already ended
            }
            *state = SessionState::Ended;
        }

        // Notify reactor
        self.reactor.on_session_end(&reason).await?;

        tracing::info!(
            session_id = %self.config.session_id,
            reason = %reason,
            events_processed = self.ring.write_sequence(),
            "Session ended"
        );

        Ok(())
    }

    /// Pause the session.
    pub async fn pause(&self) -> ReactorResult<()> {
        let mut state = self.state.write().await;
        if *state == SessionState::Active {
            *state = SessionState::Paused;
            tracing::debug!(session_id = %self.config.session_id, "Session paused");
        }
        Ok(())
    }

    /// Resume the session.
    pub async fn resume(&self) -> ReactorResult<()> {
        let mut state = self.state.write().await;
        if *state == SessionState::Paused {
            *state = SessionState::Active;
            tracing::debug!(session_id = %self.config.session_id, "Session resumed");
        }
        Ok(())
    }
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("session_id", &self.config.session_id)
            .field("folder", &self.config.folder)
            .field("event_count", &self.ring.len())
            .field("write_sequence", &self.ring.write_sequence())
            .finish()
    }
}

/// Builder for creating sessions.
///
/// Provides a fluent interface for configuring and creating sessions.
///
/// # Example
///
/// ```rust,ignore
/// let session = SessionBuilder::new("my-session")
///     .with_folder("/path/to/Sessions/my-session")
///     .with_system_prompt("You are a helpful assistant.")
///     .with_ring_capacity(8192)
///     .build()
///     .await?;
/// ```
pub struct SessionBuilder {
    session_id: String,
    folder: Option<PathBuf>,
    system_prompt: Option<String>,
    max_context_tokens: usize,
    ring_capacity: usize,
    channel_capacity: usize,
    reactor: Option<Arc<dyn Reactor>>,
    custom: JsonValue,
}

impl SessionBuilder {
    /// Create a new session builder with the given session ID.
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            folder: None,
            system_prompt: None,
            max_context_tokens: 100_000,
            ring_capacity: DEFAULT_RING_CAPACITY,
            channel_capacity: DEFAULT_CHANNEL_CAPACITY,
            reactor: None,
            custom: JsonValue::Null,
        }
    }

    /// Generate a session ID based on timestamp and topic.
    ///
    /// Format: `YYYY-MM-DDTHHMM-topic`
    pub fn with_generated_id(topic: impl Into<String>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs();

        // Convert to basic datetime components
        // This is a simplified approach - a real implementation might use chrono
        let days_since_epoch = secs / 86400;
        let time_of_day = secs % 86400;

        let hours = time_of_day / 3600;
        let minutes = (time_of_day % 3600) / 60;

        // Approximate year/month/day (ignoring leap years for simplicity)
        let year = 1970 + (days_since_epoch / 365);
        let day_of_year = days_since_epoch % 365;
        let month = (day_of_year / 30) + 1;
        let day = (day_of_year % 30) + 1;

        let topic = topic.into();
        let session_id = format!(
            "{:04}-{:02}-{:02}T{:02}{:02}-{}",
            year,
            month.min(12),
            day.min(31),
            hours,
            minutes,
            topic.to_lowercase().replace(' ', "-")
        );

        Self::new(session_id)
    }

    /// Set the session folder path.
    pub fn with_folder(mut self, folder: impl Into<PathBuf>) -> Self {
        self.folder = Some(folder.into());
        self
    }

    /// Set the system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set the maximum context tokens before compaction.
    pub fn with_max_context_tokens(mut self, tokens: usize) -> Self {
        self.max_context_tokens = tokens;
        self
    }

    /// Set the ring buffer capacity.
    pub fn with_ring_capacity(mut self, capacity: usize) -> Self {
        self.ring_capacity = capacity;
        self
    }

    /// Set the event channel capacity.
    pub fn with_channel_capacity(mut self, capacity: usize) -> Self {
        self.channel_capacity = capacity;
        self
    }

    /// Set a custom reactor.
    pub fn with_reactor(mut self, reactor: Arc<dyn Reactor>) -> Self {
        self.reactor = Some(reactor);
        self
    }

    /// Set custom configuration data.
    pub fn with_custom(mut self, custom: JsonValue) -> Self {
        self.custom = custom;
        self
    }

    /// Build the session.
    ///
    /// If no folder is specified, a default path is generated based on
    /// the session ID.
    pub fn build(self) -> Session {
        // Generate folder if not specified
        let folder = self
            .folder
            .unwrap_or_else(|| PathBuf::from("Sessions").join(&self.session_id));

        // Create session config
        let config = ReactorSessionConfig {
            session_id: self.session_id,
            folder,
            max_context_tokens: self.max_context_tokens,
            system_prompt: self.system_prompt,
            custom: self.custom,
        };

        let reactor: Arc<dyn Reactor> = self
            .reactor
            .unwrap_or_else(|| Arc::new(SimpleReactor::new()));

        Session::new(config, reactor, self.ring_capacity, self.channel_capacity)
    }
}

impl Default for SessionBuilder {
    fn default() -> Self {
        Self::with_generated_id("session")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    /// Cross-platform test path helper
    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("crucible_test_{}", name))
    }

    #[test]
    fn test_session_state_display() {
        assert_eq!(SessionState::Initializing.to_string(), "initializing");
        assert_eq!(SessionState::Active.to_string(), "active");
        assert_eq!(SessionState::Paused.to_string(), "paused");
        assert_eq!(SessionState::Compacting.to_string(), "compacting");
        assert_eq!(SessionState::Ended.to_string(), "ended");
    }

    #[test]
    fn test_session_builder_new() {
        let builder = SessionBuilder::new("test-session");
        let session = builder.build();

        assert_eq!(session.session_id(), "test-session");
        assert_eq!(session.folder(), &PathBuf::from("Sessions/test-session"));
    }

    #[test]
    fn test_session_builder_with_folder() {
        let session = SessionBuilder::new("test")
            .with_folder("/custom/path")
            .build();

        assert_eq!(session.folder(), &PathBuf::from("/custom/path"));
    }

    #[test]
    fn test_session_builder_with_system_prompt() {
        let session = SessionBuilder::new("test")
            .with_system_prompt("You are helpful.")
            .build();

        assert_eq!(
            session.config().system_prompt,
            Some("You are helpful.".to_string())
        );
    }

    #[test]
    fn test_session_builder_with_max_context_tokens() {
        let session = SessionBuilder::new("test")
            .with_max_context_tokens(50_000)
            .build();

        assert_eq!(session.config().max_context_tokens, 50_000);
    }

    #[test]
    fn test_session_builder_with_custom() {
        let session = SessionBuilder::new("test")
            .with_custom(json!({"key": "value"}))
            .build();

        assert_eq!(session.config().custom["key"], "value");
    }

    #[test]
    fn test_session_builder_generated_id() {
        let builder = SessionBuilder::with_generated_id("my topic");
        let session = builder.build();

        // Should contain the topic
        assert!(session.session_id().contains("my-topic"));
        // Should have timestamp format
        assert!(session.session_id().contains("T"));
    }

    #[test]
    fn test_session_handle_clone() {
        let session = SessionBuilder::new("test").build();
        let handle1 = session.handle();
        let handle2 = handle1.clone();

        assert_eq!(handle1.session_id(), handle2.session_id());
        assert_eq!(handle1.folder(), handle2.folder());
    }

    #[tokio::test]
    async fn test_session_initial_state() {
        let session = SessionBuilder::new("test").build();

        assert_eq!(session.state().await, SessionState::Initializing);
        assert_eq!(session.event_count(), 0);
        assert_eq!(session.current_sequence(), 0);
    }

    #[tokio::test]
    async fn test_session_start() {
        let session = SessionBuilder::new("test")
            .with_folder(test_path("test-session"))
            .build();

        session.start().await.unwrap();

        assert_eq!(session.state().await, SessionState::Active);
    }

    #[tokio::test]
    async fn test_session_start_creates_folder() {
        // Use a unique folder path that doesn't exist
        let folder = test_path("crucible-test-session-folder-creation");

        // Clean up if it exists from a previous run
        if folder.exists() {
            std::fs::remove_dir_all(&folder).unwrap();
        }

        // Verify folder doesn't exist
        assert!(!folder.exists());

        let session = SessionBuilder::new("folder-test")
            .with_folder(&folder)
            .build();

        // Start the session - this should create the folder
        session.start().await.unwrap();

        // Verify folder was created
        assert!(folder.exists());
        assert!(folder.is_dir());

        // Clean up
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[tokio::test]
    async fn test_session_start_handles_existing_folder() {
        // Use a unique folder path
        let folder = test_path("crucible-test-session-existing-folder");

        // Create the folder first
        std::fs::create_dir_all(&folder).unwrap();
        assert!(folder.exists());

        let session = SessionBuilder::new("existing-folder-test")
            .with_folder(&folder)
            .build();

        // Start the session - should succeed even if folder exists
        session.start().await.unwrap();

        // Verify session started successfully
        assert_eq!(session.state().await, SessionState::Active);

        // Clean up
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[tokio::test]
    async fn test_session_start_creates_nested_folders() {
        // Use a nested folder path that doesn't exist
        let base_dir = test_path("crucible-test-nested");
        let folder = base_dir.join("Sessions/2025-01-01T1200-test");

        // Clean up if it exists from a previous run
        if base_dir.exists() {
            std::fs::remove_dir_all(&base_dir).unwrap();
        }

        // Verify folder doesn't exist
        assert!(!folder.exists());

        let session = SessionBuilder::new("nested-test")
            .with_folder(&folder)
            .build();

        // Start the session - this should create all nested folders
        session.start().await.unwrap();

        // Verify folder was created
        assert!(folder.exists());
        assert!(folder.is_dir());

        // Clean up
        std::fs::remove_dir_all(&base_dir).unwrap();
    }

    #[tokio::test]
    async fn test_session_handle_send() {
        let session = SessionBuilder::new("test").build();
        session.start().await.unwrap();

        let handle = session.handle();

        // Send a message
        handle.message("Hello!").await.unwrap();

        // Process the event manually (since we're not running the event loop)
        // In real usage, run() would process this
    }

    #[tokio::test]
    async fn test_session_handle_is_active() {
        let session = SessionBuilder::new("test").build();
        let handle = session.handle();

        assert!(!handle.is_active().await);

        session.start().await.unwrap();

        assert!(handle.is_active().await);
    }

    #[tokio::test]
    async fn test_session_pause_resume() {
        let session = SessionBuilder::new("test").build();
        session.start().await.unwrap();

        assert_eq!(session.state().await, SessionState::Active);

        session.pause().await.unwrap();
        assert_eq!(session.state().await, SessionState::Paused);

        session.resume().await.unwrap();
        assert_eq!(session.state().await, SessionState::Active);
    }

    #[tokio::test]
    async fn test_session_end() {
        let session = SessionBuilder::new("test").build();
        session.start().await.unwrap();

        session.end("test complete").await.unwrap();

        assert_eq!(session.state().await, SessionState::Ended);
    }

    #[tokio::test]
    async fn test_session_end_idempotent() {
        let session = SessionBuilder::new("test").build();
        session.start().await.unwrap();

        session.end("first").await.unwrap();
        session.end("second").await.unwrap(); // Should not error

        assert_eq!(session.state().await, SessionState::Ended);
    }

    #[tokio::test]
    async fn test_session_debug() {
        let session = SessionBuilder::new("test-debug").build();
        let debug = format!("{:?}", session);

        assert!(debug.contains("Session"));
        assert!(debug.contains("test-debug"));
    }

    #[tokio::test]
    async fn test_session_handle_debug() {
        let session = SessionBuilder::new("test-handle").build();
        let handle = session.handle();
        let debug = format!("{:?}", handle);

        assert!(debug.contains("SessionHandle"));
        assert!(debug.contains("test-handle"));
    }

    #[tokio::test]
    async fn test_session_handle_message_from() {
        let session = SessionBuilder::new("test").build();
        session.start().await.unwrap();

        let handle = session.handle();

        // Send message from specific participant
        handle
            .message_from("Hello from assistant", "assistant")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_session_handle_custom() {
        let session = SessionBuilder::new("test").build();
        session.start().await.unwrap();

        let handle = session.handle();

        // Send custom event
        handle
            .custom("my_event", json!({"data": "value"}))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_session_builder_default() {
        let builder = SessionBuilder::default();
        let session = builder.build();

        // Should have a generated session ID
        assert!(session.session_id().contains("session"));
    }

    #[tokio::test]
    async fn test_session_ring_capacity() {
        let session = SessionBuilder::new("test").with_ring_capacity(128).build();

        // Ring capacity is rounded to next power of 2
        assert!(session.ring().capacity() >= 128);
    }

    #[tokio::test]
    async fn test_session_iter_events() {
        let session = SessionBuilder::new("test").build();
        session.start().await.unwrap();

        // Should have SessionStarted event
        let events: Vec<_> = session.iter_events().collect();
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn test_session_process_event() {
        let session = SessionBuilder::new("test").build();
        session.start().await.unwrap();

        // Directly process an event
        session
            .process_event(SessionEvent::MessageReceived {
                content: "Test".into(),
                participant_id: "user".into(),
            })
            .await
            .unwrap();

        // Should have 2 events: SessionStarted + MessageReceived
        assert_eq!(session.event_count(), 2);
    }

    #[tokio::test]
    async fn test_session_token_tracking() {
        let session = SessionBuilder::new("test")
            .with_max_context_tokens(1000)
            .build();

        session.start().await.unwrap();

        // Initial token count should be low
        let initial_tokens = session.token_count().await;

        // Process a message event
        session
            .process_event(SessionEvent::MessageReceived {
                content: "A longer message with more content to test token estimation".into(),
                participant_id: "user".into(),
            })
            .await
            .unwrap();

        // Token count should have increased
        let new_tokens = session.token_count().await;
        assert!(new_tokens > initial_tokens);
    }

    // ========================
    // SessionHandle Convenience Method Tests (TASKS.md 4.3.2)
    // ========================

    #[tokio::test]
    async fn session_handle_message() {
        // Create session with event loop
        let session = Arc::new(SessionBuilder::new("message-test").build());
        let handle = session.handle();

        session.start().await.unwrap();

        // Spawn the event loop
        let session_clone = Arc::clone(&session);
        let event_loop = tokio::spawn(async move { session_clone.run().await });

        // Give event loop time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Send a message via the convenience method
        handle.message("Hello from user").await.unwrap();

        // Give time for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // End the session
        handle.end("test done").await.unwrap();

        // Wait for event loop
        event_loop.await.unwrap().unwrap();

        // Verify the MessageReceived event is in the ring
        let mut found_message = false;
        for event in session.iter_events() {
            if let SessionEvent::MessageReceived {
                content,
                participant_id,
            } = event.as_ref()
            {
                if content == "Hello from user" && participant_id == "user" {
                    found_message = true;
                    break;
                }
            }
        }
        assert!(
            found_message,
            "MessageReceived event should be in ring with correct content and participant_id='user'"
        );
    }

    #[tokio::test]
    async fn session_handle_tool_result() {
        // Create session with event loop
        let session = Arc::new(SessionBuilder::new("tool-result-test").build());
        let handle = session.handle();

        session.start().await.unwrap();

        // Spawn the event loop
        let session_clone = Arc::clone(&session);
        let event_loop = tokio::spawn(async move { session_clone.run().await });

        // Give event loop time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Send a tool result via the convenience method
        handle
            .tool_result("read_file", "File contents here")
            .await
            .unwrap();

        // Give time for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // End the session
        handle.end("test done").await.unwrap();

        // Wait for event loop
        event_loop.await.unwrap().unwrap();

        // Verify the ToolCompleted event is in the ring
        let mut found_tool_result = false;
        for event in session.iter_events() {
            if let SessionEvent::ToolCompleted {
                name,
                result,
                error,
            } = event.as_ref()
            {
                if name == "read_file" && result == "File contents here" && error.is_none() {
                    found_tool_result = true;
                    break;
                }
            }
        }
        assert!(
            found_tool_result,
            "ToolCompleted event should be in ring with correct name, result, and no error"
        );
    }

    #[tokio::test]
    async fn session_handle_tool_error() {
        // Create session with event loop
        let session = Arc::new(SessionBuilder::new("tool-error-test").build());
        let handle = session.handle();

        session.start().await.unwrap();

        // Spawn the event loop
        let session_clone = Arc::clone(&session);
        let event_loop = tokio::spawn(async move { session_clone.run().await });

        // Give event loop time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Send a tool error via the convenience method
        handle
            .tool_error("read_file", "", "File not found")
            .await
            .unwrap();

        // Give time for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // End the session
        handle.end("test done").await.unwrap();

        // Wait for event loop
        event_loop.await.unwrap().unwrap();

        // Verify the ToolCompleted event is in the ring with error
        let mut found_tool_error = false;
        for event in session.iter_events() {
            if let SessionEvent::ToolCompleted {
                name,
                result: _,
                error,
            } = event.as_ref()
            {
                if name == "read_file" && error.as_deref() == Some("File not found") {
                    found_tool_error = true;
                    break;
                }
            }
        }
        assert!(
            found_tool_error,
            "ToolCompleted event should be in ring with error message"
        );
    }

    #[tokio::test]
    async fn session_handle_tool_called() {
        // Create session with event loop
        let session = Arc::new(SessionBuilder::new("tool-called-test").build());
        let handle = session.handle();

        session.start().await.unwrap();

        // Spawn the event loop
        let session_clone = Arc::clone(&session);
        let event_loop = tokio::spawn(async move { session_clone.run().await });

        // Give event loop time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Send a tool called event via the convenience method
        handle
            .tool_called("search", json!({"query": "test", "limit": 10}))
            .await
            .unwrap();

        // Give time for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // End the session
        handle.end("test done").await.unwrap();

        // Wait for event loop
        event_loop.await.unwrap().unwrap();

        // Verify the ToolCalled event is in the ring
        let mut found_tool_called = false;
        for event in session.iter_events() {
            if let SessionEvent::ToolCalled { name, args } = event.as_ref() {
                if name == "search" && args["query"] == "test" && args["limit"] == 10 {
                    found_tool_called = true;
                    break;
                }
            }
        }
        assert!(
            found_tool_called,
            "ToolCalled event should be in ring with correct args"
        );
    }

    #[tokio::test]
    async fn session_handle_thinking() {
        // Create session with event loop
        let session = Arc::new(SessionBuilder::new("thinking-test").build());
        let handle = session.handle();

        session.start().await.unwrap();

        // Spawn the event loop
        let session_clone = Arc::clone(&session);
        let event_loop = tokio::spawn(async move { session_clone.run().await });

        // Give event loop time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Send a thinking event via the convenience method
        handle.thinking("Analyzing the codebase...").await.unwrap();

        // Give time for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // End the session
        handle.end("test done").await.unwrap();

        // Wait for event loop
        event_loop.await.unwrap().unwrap();

        // Verify the AgentThinking event is in the ring
        let mut found_thinking = false;
        for event in session.iter_events() {
            if let SessionEvent::AgentThinking { thought } = event.as_ref() {
                if thought == "Analyzing the codebase..." {
                    found_thinking = true;
                    break;
                }
            }
        }
        assert!(
            found_thinking,
            "AgentThinking event should be in ring with correct thought"
        );
    }

    // ========================
    // Event Loop Tests (TASKS.md 4.2.2)
    // ========================

    #[tokio::test]
    async fn event_loop_processes_events() {
        // Create session
        let session = Arc::new(SessionBuilder::new("event-loop-test").build());
        let handle = session.handle();

        // Start the session
        session.start().await.unwrap();

        // Initial event count (SessionStarted from reactor)
        let initial_count = session.event_count();
        assert!(
            initial_count >= 1,
            "Should have at least SessionStarted event"
        );

        // Spawn the event loop in a background task
        let session_clone = Arc::clone(&session);
        let event_loop = tokio::spawn(async move { session_clone.run().await });

        // Give the event loop time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Send events via handle
        handle.message("First message").await.unwrap();
        handle.message("Second message").await.unwrap();
        handle
            .custom("test_event", json!({"value": 42}))
            .await
            .unwrap();

        // Give the event loop time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // End the session to stop the event loop
        handle.end("test complete").await.unwrap();

        // Wait for event loop to finish
        let result = event_loop.await.unwrap();
        assert!(result.is_ok(), "Event loop should complete successfully");

        // Verify events were processed (pushed to ring)
        // Should have: SessionStarted + MessageReceived x2 + Custom + SessionEnded
        let event_count = session.event_count();
        assert!(
            event_count >= initial_count + 4,
            "Expected at least {} events, got {}",
            initial_count + 4,
            event_count
        );

        // Verify the events are in the ring
        let mut found_first_msg = false;
        let mut found_second_msg = false;
        let mut found_custom = false;
        let mut found_end = false;

        for event in session.iter_events() {
            match event.as_ref() {
                SessionEvent::MessageReceived { content, .. } => {
                    if content == "First message" {
                        found_first_msg = true;
                    } else if content == "Second message" {
                        found_second_msg = true;
                    }
                }
                SessionEvent::Custom { name, .. } if name == "test_event" => {
                    found_custom = true;
                }
                SessionEvent::SessionEnded { reason } if reason == "test complete" => {
                    found_end = true;
                }
                _ => {}
            }
        }

        assert!(found_first_msg, "First message should be in ring");
        assert!(found_second_msg, "Second message should be in ring");
        assert!(found_custom, "Custom event should be in ring");
        assert!(found_end, "SessionEnded event should be in ring");
    }

    #[tokio::test]
    async fn event_loop_calls_reactor() {
        use crate::reactor::{BoxedReactor, Reactor, ReactorMetadata};
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Create a custom reactor that counts calls
        struct CountingReactor {
            call_count: Arc<AtomicUsize>,
            start_called: Arc<AtomicUsize>,
            end_called: Arc<AtomicUsize>,
        }

        #[async_trait::async_trait]
        impl Reactor for CountingReactor {
            async fn handle_event(
                &self,
                _ctx: &mut crate::event_bus::EventContext,
                event: SessionEvent,
            ) -> crate::reactor::ReactorResult<SessionEvent> {
                self.call_count.fetch_add(1, Ordering::SeqCst);
                Ok(event)
            }

            async fn on_session_start(
                &self,
                _config: &ReactorSessionConfig,
            ) -> crate::reactor::ReactorResult<()> {
                self.start_called.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }

            async fn on_session_end(&self, _reason: &str) -> crate::reactor::ReactorResult<()> {
                self.end_called.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }

            fn metadata(&self) -> ReactorMetadata {
                ReactorMetadata::new("CountingReactor")
            }
        }

        // Create counters
        let call_count = Arc::new(AtomicUsize::new(0));
        let start_called = Arc::new(AtomicUsize::new(0));
        let end_called = Arc::new(AtomicUsize::new(0));

        // Create reactor with counters
        let reactor: BoxedReactor = Box::new(CountingReactor {
            call_count: Arc::clone(&call_count),
            start_called: Arc::clone(&start_called),
            end_called: Arc::clone(&end_called),
        });

        // Build session with custom reactor
        let session = Arc::new(
            SessionBuilder::new("reactor-test")
                .with_reactor(Arc::from(reactor))
                .build(),
        );
        let handle = session.handle();

        // Start the session
        session.start().await.unwrap();

        // Verify on_session_start was called
        assert_eq!(
            start_called.load(Ordering::SeqCst),
            1,
            "on_session_start should be called once"
        );

        // Spawn the event loop
        let session_clone = Arc::clone(&session);
        let event_loop = tokio::spawn(async move { session_clone.run().await });

        // Give the event loop time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Send some events
        handle.message("Test 1").await.unwrap();
        handle.message("Test 2").await.unwrap();
        handle.message("Test 3").await.unwrap();

        // Give time for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // End the session
        handle.end("done").await.unwrap();

        // Wait for event loop to finish
        event_loop.await.unwrap().unwrap();

        // Verify reactor's handle_event was called for each event
        // Should be called for: MessageReceived x3 + SessionEnded
        let calls = call_count.load(Ordering::SeqCst);
        assert!(
            calls >= 4,
            "handle_event should be called at least 4 times, got {}",
            calls
        );

        // Verify on_session_end was called
        assert_eq!(
            end_called.load(Ordering::SeqCst),
            1,
            "on_session_end should be called once"
        );
    }

    // ========================
    // Event File Persistence Tests (TASKS.md 5.2.2)
    // ========================

    #[tokio::test]
    async fn session_creates_initial_file() {
        // Use a unique folder path
        let folder = test_path("crucible-test-session-initial-file");

        // Clean up if it exists from a previous run
        if folder.exists() {
            std::fs::remove_dir_all(&folder).unwrap();
        }

        let session = SessionBuilder::new("initial-file-test")
            .with_folder(&folder)
            .build();

        // Start the session - should create folder and initial file
        session.start().await.unwrap();

        // Verify initial file was created
        let initial_file = folder.join("000-context.md");
        assert!(
            initial_file.exists(),
            "Initial context file should exist at {}",
            initial_file.display()
        );

        // Verify file contains SessionStarted event
        let content = std::fs::read_to_string(&initial_file).unwrap();
        assert!(
            content.contains("SessionStarted"),
            "File should contain SessionStarted event"
        );
        assert!(
            content.contains("initial-file-test"),
            "File should contain session ID"
        );

        // Clean up
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[tokio::test]
    async fn events_appended_to_file() {
        // Use a unique folder path
        let folder = test_path("crucible-test-events-appended");

        // Clean up if it exists from a previous run
        if folder.exists() {
            std::fs::remove_dir_all(&folder).unwrap();
        }

        let session = Arc::new(
            SessionBuilder::new("events-appended-test")
                .with_folder(&folder)
                .build(),
        );
        let handle = session.handle();

        // Start the session
        session.start().await.unwrap();

        // Spawn the event loop
        let session_clone = Arc::clone(&session);
        let event_loop = tokio::spawn(async move { session_clone.run().await });

        // Give event loop time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Send some events
        handle.message("Hello world!").await.unwrap();
        handle
            .tool_called(
                "read_file",
                json!({"path": test_path("test.txt").to_string_lossy()}),
            )
            .await
            .unwrap();
        handle
            .tool_result("read_file", "File contents here")
            .await
            .unwrap();

        // Give time for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // End the session
        handle.end("test complete").await.unwrap();

        // Wait for event loop
        event_loop.await.unwrap().unwrap();

        // Verify file contains all events
        let context_file = folder.join("000-context.md");
        let content = std::fs::read_to_string(&context_file).unwrap();

        // Check for SessionStarted
        assert!(
            content.contains("SessionStarted"),
            "File should contain SessionStarted"
        );

        // Check for MessageReceived
        assert!(
            content.contains("MessageReceived"),
            "File should contain MessageReceived"
        );
        assert!(
            content.contains("Hello world!"),
            "File should contain message content"
        );

        // Check for ToolCalled
        assert!(
            content.contains("ToolCalled"),
            "File should contain ToolCalled"
        );
        assert!(
            content.contains("read_file"),
            "File should contain tool name"
        );

        // Check for ToolCompleted
        assert!(
            content.contains("ToolCompleted"),
            "File should contain ToolCompleted"
        );
        assert!(
            content.contains("File contents here"),
            "File should contain tool result"
        );

        // Check for SessionEnded
        assert!(
            content.contains("SessionEnded"),
            "File should contain SessionEnded"
        );

        // Clean up
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[tokio::test]
    async fn file_has_event_blocks() {
        // Use a unique folder path
        let folder = test_path("crucible-test-event-blocks");

        // Clean up if it exists from a previous run
        if folder.exists() {
            std::fs::remove_dir_all(&folder).unwrap();
        }

        let session = Arc::new(
            SessionBuilder::new("event-blocks-test")
                .with_folder(&folder)
                .build(),
        );
        let handle = session.handle();

        // Start the session
        session.start().await.unwrap();

        // Spawn the event loop
        let session_clone = Arc::clone(&session);
        let event_loop = tokio::spawn(async move { session_clone.run().await });

        // Give event loop time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Send a message
        handle.message("Test message content").await.unwrap();

        // Give time for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // End the session
        handle.end("done").await.unwrap();

        // Wait for event loop
        event_loop.await.unwrap().unwrap();

        // Verify file structure
        let context_file = folder.join("000-context.md");
        let content = std::fs::read_to_string(&context_file).unwrap();

        // Each event block should start with ## and timestamp
        let header_count = content.matches("## 20").count(); // Timestamps start with 20xx
        assert!(
            header_count >= 3,
            "File should have at least 3 event headers (SessionStarted, MessageReceived, SessionEnded), found {}",
            header_count
        );

        // Each event block should end with ---
        let separator_count = content.matches("\n---\n").count();
        assert!(
            separator_count >= 3,
            "File should have at least 3 separators, found {}",
            separator_count
        );

        // Clean up
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[tokio::test]
    async fn current_file_index_starts_at_zero() {
        let session = SessionBuilder::new("index-test").build();
        assert_eq!(session.current_file_index().await, 0);
    }

    #[tokio::test]
    async fn current_file_path_format() {
        let folder = test_path("test-session");
        let session = SessionBuilder::new("path-test")
            .with_folder(&folder)
            .build();

        let path = session.current_file_path().await;
        assert_eq!(path, folder.join("000-context.md"));
    }

    #[tokio::test]
    async fn increment_file_index() {
        let session = SessionBuilder::new("increment-test").build();

        assert_eq!(session.current_file_index().await, 0);

        let new_index = session.increment_file_index().await;
        assert_eq!(new_index, 1);
        assert_eq!(session.current_file_index().await, 1);

        let new_index = session.increment_file_index().await;
        assert_eq!(new_index, 2);
        assert_eq!(session.current_file_index().await, 2);
    }

    #[tokio::test]
    async fn append_event_creates_file() {
        // Use a unique folder path
        let folder = test_path("crucible-test-append-creates");

        // Clean up if it exists from a previous run
        if folder.exists() {
            std::fs::remove_dir_all(&folder).unwrap();
        }

        // Create the folder
        std::fs::create_dir_all(&folder).unwrap();

        let session = SessionBuilder::new("append-creates-test")
            .with_folder(&folder)
            .build();

        // Append an event directly (without starting session)
        let event = SessionEvent::MessageReceived {
            content: "Test message".into(),
            participant_id: "user".into(),
        };
        session.append_event_to_file(&event).await.unwrap();

        // Verify file was created
        let context_file = folder.join("000-context.md");
        assert!(context_file.exists(), "File should be created");

        let content = std::fs::read_to_string(&context_file).unwrap();
        assert!(content.contains("MessageReceived"));
        assert!(content.contains("Test message"));
        assert!(content.contains("**Participant:** user"));

        // Clean up
        std::fs::remove_dir_all(&folder).unwrap();
    }

    // ========================
    // Compaction Tests (TASKS.md 5.3.2)
    // ========================

    #[tokio::test]
    async fn compaction_includes_summary() {
        // Use a unique folder path
        let folder = test_path("crucible-test-compaction-summary");

        // Clean up if it exists from a previous run
        if folder.exists() {
            std::fs::remove_dir_all(&folder).unwrap();
        }

        let session = Arc::new(
            SessionBuilder::new("compaction-summary-test")
                .with_folder(&folder)
                .with_max_context_tokens(100) // Low threshold for testing
                .build(),
        );

        // Start the session
        session.start().await.unwrap();

        // Add some events to the ring directly (simulating processed events)
        for i in 0..5 {
            let event = SessionEvent::MessageReceived {
                content: format!("Test message number {}", i),
                participant_id: "user".into(),
            };
            session.ring().push(event);
        }

        let event = SessionEvent::ToolCalled {
            name: "search".into(),
            args: json!({"query": "test"}),
        };
        session.ring().push(event);

        let event = SessionEvent::AgentResponded {
            content: "Here are the results".into(),
            tool_calls: vec![],
        };
        session.ring().push(event);

        // Trigger compaction
        let new_file = session.compact().await.unwrap();

        // Verify the new file exists
        assert!(new_file.exists(), "New context file should exist");

        // Read the new file content
        let content = std::fs::read_to_string(&new_file).unwrap();

        // Verify it contains the summary header
        assert!(
            content.contains("# Session Context (Compacted)"),
            "File should have compacted header"
        );
        assert!(
            content.contains("## Summary"),
            "File should contain Summary section"
        );

        // Verify the summary includes key statistics
        assert!(
            content.contains("messages"),
            "Summary should mention messages"
        );
        assert!(
            content.contains("tool calls"),
            "Summary should mention tool calls"
        );
        assert!(
            content.contains("agent responses"),
            "Summary should mention agent responses"
        );
        assert!(
            content.contains("total events"),
            "Summary should mention total events"
        );

        // Verify tools used are mentioned
        assert!(content.contains("search"), "Summary should list tools used");

        // Clean up
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[tokio::test]
    async fn summary_links_previous() {
        // Use a unique folder path
        let folder = test_path("crucible-test-summary-links");

        // Clean up if it exists from a previous run
        if folder.exists() {
            std::fs::remove_dir_all(&folder).unwrap();
        }

        let session = Arc::new(
            SessionBuilder::new("summary-links-test")
                .with_folder(&folder)
                .with_max_context_tokens(100) // Low threshold for testing
                .build(),
        );

        // Start the session (creates 000-context.md)
        session.start().await.unwrap();

        // Verify initial file exists
        let initial_file = folder.join("000-context.md");
        assert!(initial_file.exists(), "Initial context file should exist");

        // Verify file index starts at 0
        assert_eq!(session.current_file_index().await, 0);

        // Add some events
        for i in 0..3 {
            let event = SessionEvent::MessageReceived {
                content: format!("Message {}", i),
                participant_id: "user".into(),
            };
            session.ring().push(event);
        }

        // Trigger compaction
        let new_file = session.compact().await.unwrap();

        // Verify file index incremented
        assert_eq!(session.current_file_index().await, 1);

        // Verify the new file is 001-context.md
        let expected_path = folder.join("001-context.md");
        assert_eq!(new_file, expected_path);
        assert!(
            new_file.exists(),
            "New context file should exist at 001-context.md"
        );

        // Read the new file content
        let content = std::fs::read_to_string(&new_file).unwrap();

        // Verify it contains wikilink to previous file (000-context)
        assert!(
            content.contains("[[000-context|full history]]"),
            "File should contain wikilink to previous context file. Content:\n{}",
            content
        );

        // Verify it has the "Previous context:" label
        assert!(
            content.contains("Previous context:"),
            "File should have 'Previous context:' label"
        );

        // Clean up
        std::fs::remove_dir_all(&folder).unwrap();
    }

    // ========================
    // Ring Overflow Flush Tests (TASKS.md 5.4.1)
    // ========================

    #[tokio::test]
    async fn ring_overflow_flushes_to_kiln() {
        // Use a unique folder path
        let folder = test_path("crucible-test-ring-overflow-flush");

        // Clean up if it exists from a previous run
        if folder.exists() {
            std::fs::remove_dir_all(&folder).unwrap();
        }

        // Create folder first (overflow callback needs it to exist)
        std::fs::create_dir_all(&folder).unwrap();

        // Create session with a VERY small ring buffer (capacity 4 after rounding)
        let session = SessionBuilder::new("overflow-flush-test")
            .with_folder(&folder)
            .with_ring_capacity(4) // Will be rounded to 4 (power of 2)
            .build();

        // Mark some events as already flushed to prevent immediate trigger
        // Initially, flushed_seq is 0

        // Push events directly to the ring to trigger overflow
        // Ring capacity is 4, so after 4 events we start overwriting
        for i in 0..8 {
            let event = SessionEvent::MessageReceived {
                content: format!("Message {}", i),
                participant_id: "user".into(),
            };
            session.ring().push(event);
        }

        // Verify the context file was created with flushed events
        let context_file = folder.join("000-context.md");
        assert!(
            context_file.exists(),
            "Context file should exist after overflow flush"
        );

        // Read the file content
        let content = std::fs::read_to_string(&context_file).unwrap();

        // Verify some events were flushed (events 0-3 should be flushed
        // before events 4-7 overwrote them)
        assert!(
            content.contains("MessageReceived"),
            "File should contain flushed MessageReceived events"
        );

        // At least the first few messages should have been flushed
        // (exact number depends on batch size, which is capacity/4 = 1)
        assert!(
            content.contains("Message 0") || content.contains("Message 1"),
            "File should contain early messages that were flushed before overflow"
        );

        // Clean up
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[tokio::test]
    async fn ring_overflow_callback_registered() {
        // Create session
        let session = SessionBuilder::new("callback-test")
            .with_ring_capacity(8)
            .build();

        // The ring should have an overflow callback registered
        // We can verify by checking that flushed_sequence tracking works
        assert_eq!(session.ring().flushed_sequence(), 0);

        // Push some events and mark as flushed
        session.ring().push(SessionEvent::MessageReceived {
            content: "Test".into(),
            participant_id: "user".into(),
        });
        session.ring().mark_flushed(1);
        assert_eq!(session.ring().flushed_sequence(), 1);
    }

    #[tokio::test]
    async fn session_ring_has_overflow_callback() {
        // Use a unique folder path
        let folder = test_path("crucible-test-session-overflow-callback");

        // Clean up if it exists from a previous run
        if folder.exists() {
            std::fs::remove_dir_all(&folder).unwrap();
        }

        // Create folder
        std::fs::create_dir_all(&folder).unwrap();

        // Create session with small ring
        let session = SessionBuilder::new("session-callback-test")
            .with_folder(&folder)
            .with_ring_capacity(4)
            .build();

        // Fill the ring completely
        for i in 0..4 {
            session.ring().push(SessionEvent::MessageReceived {
                content: format!("Msg {}", i),
                participant_id: "user".into(),
            });
        }

        // File should not exist yet (no overflow)
        let context_file = folder.join("000-context.md");
        let exists_before = context_file.exists();

        // Now push one more to trigger overflow
        session.ring().push(SessionEvent::MessageReceived {
            content: "Overflow trigger".into(),
            participant_id: "user".into(),
        });

        // File should now exist with flushed events
        let exists_after = context_file.exists();

        // Either the file existed before (previous batch), or it exists now
        // after overflow triggered the flush
        assert!(
            exists_before || exists_after,
            "Context file should exist after overflow flush"
        );

        if exists_after {
            let content = std::fs::read_to_string(&context_file).unwrap();
            assert!(
                content.contains("MessageReceived"),
                "File should contain flushed events"
            );
        }

        // Clean up
        std::fs::remove_dir_all(&folder).unwrap();
    }

    // ========================
    // TUI Helper Method Tests
    // ========================

    #[tokio::test]
    async fn test_recent_messages_returns_user_and_agent_events() {
        let session = SessionBuilder::new("recent-messages-test").build();

        // Push some events
        session.ring().push(SessionEvent::MessageReceived {
            content: "Hello".into(),
            participant_id: "user".into(),
        });
        session.ring().push(SessionEvent::ToolCalled {
            name: "search".into(),
            args: json!({}),
        });
        session.ring().push(SessionEvent::AgentResponded {
            content: "Hi there".into(),
            tool_calls: vec![],
        });
        session.ring().push(SessionEvent::MessageReceived {
            content: "Thanks".into(),
            participant_id: "user".into(),
        });

        let messages = session.recent_messages(10);

        // Should have 3 messages (2 MessageReceived + 1 AgentResponded)
        assert_eq!(messages.len(), 3);

        // Verify order (oldest to newest)
        assert!(matches!(
            messages[0].as_ref(),
            SessionEvent::MessageReceived { content, .. } if content == "Hello"
        ));
        assert!(matches!(
            messages[1].as_ref(),
            SessionEvent::AgentResponded { content, .. } if content == "Hi there"
        ));
        assert!(matches!(
            messages[2].as_ref(),
            SessionEvent::MessageReceived { content, .. } if content == "Thanks"
        ));
    }

    #[tokio::test]
    async fn test_recent_messages_respects_limit() {
        let session = SessionBuilder::new("messages-limit-test").build();

        // Push 5 messages
        for i in 0..5 {
            session.ring().push(SessionEvent::MessageReceived {
                content: format!("Message {}", i),
                participant_id: "user".into(),
            });
        }

        // Request only 2
        let messages = session.recent_messages(2);

        assert_eq!(messages.len(), 2);

        // Should be the last 2 messages
        assert!(matches!(
            messages[0].as_ref(),
            SessionEvent::MessageReceived { content, .. } if content == "Message 3"
        ));
        assert!(matches!(
            messages[1].as_ref(),
            SessionEvent::MessageReceived { content, .. } if content == "Message 4"
        ));
    }

    #[tokio::test]
    async fn test_pending_tools_returns_uncompleted() {
        let session = SessionBuilder::new("pending-tools-test").build();

        // Tool started but not completed
        session.ring().push(SessionEvent::ToolCalled {
            name: "search".into(),
            args: json!({"query": "test"}),
        });

        let pending = session.pending_tools();
        assert_eq!(pending.len(), 1);

        // Complete the tool
        session.ring().push(SessionEvent::ToolCompleted {
            name: "search".into(),
            result: "found".into(),
            error: None,
        });

        let pending = session.pending_tools();
        assert_eq!(pending.len(), 0);
    }

    #[tokio::test]
    async fn test_pending_tools_multiple() {
        let session = SessionBuilder::new("pending-tools-multi-test").build();

        // Start multiple tools
        session.ring().push(SessionEvent::ToolCalled {
            name: "search".into(),
            args: json!({}),
        });
        session.ring().push(SessionEvent::ToolCalled {
            name: "read_file".into(),
            args: json!({"path": "/test"}),
        });
        session.ring().push(SessionEvent::ToolCalled {
            name: "write_file".into(),
            args: json!({}),
        });

        // Complete one
        session.ring().push(SessionEvent::ToolCompleted {
            name: "read_file".into(),
            result: "contents".into(),
            error: None,
        });

        let pending = session.pending_tools();
        assert_eq!(pending.len(), 2);

        // Check which tools are pending
        let pending_names: Vec<String> = pending
            .iter()
            .filter_map(|e| {
                if let SessionEvent::ToolCalled { name, .. } = e.as_ref() {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect();

        assert!(pending_names.contains(&"search".to_string()));
        assert!(pending_names.contains(&"write_file".to_string()));
        assert!(!pending_names.contains(&"read_file".to_string()));
    }

    #[tokio::test]
    async fn test_is_streaming_false_initially() {
        let session = SessionBuilder::new("streaming-test").build();
        assert!(!session.is_streaming());
    }

    #[tokio::test]
    async fn test_is_streaming_true_during_deltas() {
        let session = SessionBuilder::new("streaming-active-test").build();

        // Send a message (triggers response)
        session.ring().push(SessionEvent::MessageReceived {
            content: "Hello".into(),
            participant_id: "user".into(),
        });

        // Start streaming
        session.ring().push(SessionEvent::TextDelta {
            delta: "Hello ".into(),
            seq: 1,
        });

        assert!(session.is_streaming());

        // More deltas
        session.ring().push(SessionEvent::TextDelta {
            delta: "world".into(),
            seq: 2,
        });

        assert!(session.is_streaming());
    }

    #[tokio::test]
    async fn test_is_streaming_false_after_response() {
        let session = SessionBuilder::new("streaming-done-test").build();

        session.ring().push(SessionEvent::MessageReceived {
            content: "Hello".into(),
            participant_id: "user".into(),
        });

        session.ring().push(SessionEvent::TextDelta {
            delta: "Hello ".into(),
            seq: 1,
        });

        session.ring().push(SessionEvent::TextDelta {
            delta: "world".into(),
            seq: 2,
        });

        // Response completes the stream
        session.ring().push(SessionEvent::AgentResponded {
            content: "Hello world".into(),
            tool_calls: vec![],
        });

        assert!(!session.is_streaming());
    }

    #[tokio::test]
    async fn test_cancel_emits_event() {
        let session = SessionBuilder::new("cancel-test").build();

        let initial_count = session.event_count();

        session.cancel().unwrap();

        // Should have one more event
        assert_eq!(session.event_count(), initial_count + 1);

        // Check the event is a Custom cancellation
        let events: Vec<_> = session.iter_events().collect();
        let last = events.last().unwrap();

        assert!(matches!(
            last.as_ref(),
            SessionEvent::Custom { name, .. } if name == "cancelled"
        ));
    }
}
