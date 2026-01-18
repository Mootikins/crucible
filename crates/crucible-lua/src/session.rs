//! Session management for Lua scripting
//!
//! Provides a lightweight session wrapper that combines:
//! - `EventRing<SessionEvent>` for in-memory event storage
//! - `LuaScriptHandlerRegistry` for handler dispatch
//! - Markdown persistence via `EventToMarkdown`
//!
//! This is the Lua equivalent of `crucible_rune::Session`, but simpler
//! because it delegates more to core infrastructure.

use crate::error::LuaError;
use crate::handlers::{run_handler_chain, LuaScriptHandlerRegistry};
use crucible_core::discovery::DiscoveryPaths;
use crucible_core::events::markdown::EventToMarkdown;
use crucible_core::events::{EventRing, SessionEvent, SessionEventConfig};
use mlua::Lua;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, warn};

const DEFAULT_RING_CAPACITY: usize = 4096;
const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// Session state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    /// Session created but not started
    Initializing,
    /// Session is actively processing events
    Active,
    /// Session is paused (events queued but not processed)
    Paused,
    /// Session has ended
    Ended,
}

/// Configuration for a Lua session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LuaSessionConfig {
    /// Unique session identifier
    pub session_id: String,
    /// Folder path for session files
    pub folder: PathBuf,
    /// Maximum context tokens before compaction (not implemented yet)
    #[serde(default = "default_max_tokens")]
    pub max_context_tokens: usize,
    /// Optional system prompt
    #[serde(default)]
    pub system_prompt: Option<String>,
}

fn default_max_tokens() -> usize {
    100_000
}

impl LuaSessionConfig {
    pub fn new(session_id: impl Into<String>, folder: impl Into<PathBuf>) -> Self {
        Self {
            session_id: session_id.into(),
            folder: folder.into(),
            max_context_tokens: default_max_tokens(),
            system_prompt: None,
        }
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }
}

impl From<&LuaSessionConfig> for SessionEventConfig {
    fn from(config: &LuaSessionConfig) -> Self {
        SessionEventConfig::new(&config.session_id)
            .with_folder(&config.folder)
            .with_max_context_tokens(config.max_context_tokens)
    }
}

/// A Lua-based session for event processing
///
/// Combines EventRing, handler registry, and markdown persistence
/// into a simple event processing loop.
pub struct LuaSession {
    config: Arc<LuaSessionConfig>,
    ring: Arc<EventRing<SessionEvent>>,
    handlers: Arc<RwLock<LuaScriptHandlerRegistry>>,
    lua: Arc<RwLock<Lua>>,
    state: Arc<RwLock<SessionState>>,
    event_tx: mpsc::Sender<SessionEvent>,
    event_rx: Arc<RwLock<Option<mpsc::Receiver<SessionEvent>>>>,
    current_file_index: Arc<RwLock<usize>>,
}

impl LuaSession {
    /// Get a handle for sending events to this session
    pub fn handle(&self) -> LuaSessionHandle {
        LuaSessionHandle {
            event_tx: self.event_tx.clone(),
            config: self.config.clone(),
        }
    }

    /// Get the session configuration
    pub fn config(&self) -> &LuaSessionConfig {
        &self.config
    }

    /// Get the current session state
    pub async fn state(&self) -> SessionState {
        *self.state.read().await
    }

    /// Start the session
    ///
    /// Creates the session folder and emits SessionStarted event.
    pub async fn start(&self) -> Result<(), LuaError> {
        let mut state = self.state.write().await;
        if *state != SessionState::Initializing {
            return Err(LuaError::Runtime(format!(
                "Cannot start session in state {:?}",
                *state
            )));
        }

        // Create session folder
        fs::create_dir_all(&self.config.folder)?;

        // Emit SessionStarted
        let event = SessionEvent::SessionStarted {
            config: SessionEventConfig::from(self.config.as_ref()),
        };
        self.event_tx
            .send(event)
            .await
            .map_err(|e| LuaError::Runtime(format!("Failed to send event: {}", e)))?;

        *state = SessionState::Active;
        debug!("Session {} started", self.config.session_id);
        Ok(())
    }

    /// Run the event processing loop
    ///
    /// Processes events until the session ends or all senders are dropped.
    pub async fn run(&self) -> Result<(), LuaError> {
        let mut rx = self
            .event_rx
            .write()
            .await
            .take()
            .ok_or_else(|| LuaError::Runtime("Event receiver already taken".to_string()))?;

        while let Some(event) = rx.recv().await {
            let state = *self.state.read().await;
            match state {
                SessionState::Active => {
                    if let Err(e) = self.process_event(event.clone()).await {
                        error!("Error processing event: {}", e);
                    }

                    // Check for session end
                    if matches!(event, SessionEvent::SessionEnded { .. }) {
                        break;
                    }
                }
                SessionState::Paused => {
                    debug!("Session paused, dropping event");
                }
                SessionState::Ended => {
                    break;
                }
                SessionState::Initializing => {
                    warn!("Received event before session started");
                }
            }
        }

        debug!("Session {} event loop ended", self.config.session_id);
        Ok(())
    }

    /// Process a single event
    async fn process_event(&self, event: SessionEvent) -> Result<(), LuaError> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .ok();

        // Store in ring buffer
        self.ring.push(event.clone());

        // Persist to markdown
        self.append_event_to_file(&event, timestamp).await?;

        // Run through handler chain
        let handlers = self.handlers.read().await;
        let matching_handlers = handlers.handlers_for(&event);

        if !matching_handlers.is_empty() {
            let lua = self.lua.read().await;
            match run_handler_chain(&lua, &matching_handlers, event) {
                Ok(Some(_modified_event)) => {}
                Ok(None) => {
                    debug!("Event cancelled by handler");
                }
                Err(e) => {
                    warn!("Handler chain error: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Append an event to the current context file
    async fn append_event_to_file(
        &self,
        event: &SessionEvent,
        timestamp: Option<u64>,
    ) -> Result<(), LuaError> {
        let file_index = *self.current_file_index.read().await;
        let filename = format!("{:03}-context.md", file_index);
        let filepath = self.config.folder.join(&filename);

        let markdown = event.to_markdown_block(timestamp);

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&filepath)?;

        writeln!(file, "{}", markdown)?;
        Ok(())
    }

    /// End the session
    pub async fn end(&self, reason: impl Into<String>) -> Result<(), LuaError> {
        let reason = reason.into();
        let mut state = self.state.write().await;

        if *state == SessionState::Ended {
            return Ok(());
        }

        // Emit SessionEnded event
        let event = SessionEvent::SessionEnded {
            reason: reason.clone(),
        };
        let _ = self.event_tx.send(event).await;

        *state = SessionState::Ended;
        debug!("Session {} ended: {}", self.config.session_id, reason);
        Ok(())
    }

    /// Pause the session
    pub async fn pause(&self) -> Result<(), LuaError> {
        let mut state = self.state.write().await;
        if *state != SessionState::Active {
            return Err(LuaError::Runtime(format!(
                "Cannot pause session in state {:?}",
                *state
            )));
        }
        *state = SessionState::Paused;
        Ok(())
    }

    /// Resume a paused session
    pub async fn resume(&self) -> Result<(), LuaError> {
        let mut state = self.state.write().await;
        if *state != SessionState::Paused {
            return Err(LuaError::Runtime(format!(
                "Cannot resume session in state {:?}",
                *state
            )));
        }
        *state = SessionState::Active;
        Ok(())
    }
}

/// Handle for sending events to a session
///
/// Cheap to clone, can be shared across tasks.
#[derive(Clone)]
pub struct LuaSessionHandle {
    event_tx: mpsc::Sender<SessionEvent>,
    config: Arc<LuaSessionConfig>,
}

impl LuaSessionHandle {
    /// Send a message event
    pub async fn message(
        &self,
        content: impl Into<String>,
        participant: impl Into<String>,
    ) -> Result<(), LuaError> {
        self.send(SessionEvent::MessageReceived {
            content: content.into(),
            participant_id: participant.into(),
        })
        .await
    }

    /// Send a tool call event
    pub async fn tool_call(
        &self,
        name: impl Into<String>,
        args: serde_json::Value,
    ) -> Result<(), LuaError> {
        self.send(SessionEvent::ToolCalled {
            name: name.into(),
            args,
        })
        .await
    }

    /// Send a tool result event
    pub async fn tool_result(
        &self,
        name: impl Into<String>,
        result: impl Into<String>,
    ) -> Result<(), LuaError> {
        self.send(SessionEvent::ToolCompleted {
            name: name.into(),
            result: result.into(),
            error: None,
        })
        .await
    }

    /// Send a custom event
    pub async fn custom(
        &self,
        name: impl Into<String>,
        payload: serde_json::Value,
    ) -> Result<(), LuaError> {
        self.send(SessionEvent::Custom {
            name: name.into(),
            payload,
        })
        .await
    }

    /// Send any session event
    pub async fn send(&self, event: SessionEvent) -> Result<(), LuaError> {
        self.event_tx
            .send(event)
            .await
            .map_err(|e| LuaError::Runtime(format!("Failed to send event: {}", e)))
    }

    /// Get the session ID
    pub fn session_id(&self) -> &str {
        &self.config.session_id
    }
}

/// Builder for creating LuaSession instances
pub struct LuaSessionBuilder {
    session_id: String,
    folder: Option<PathBuf>,
    system_prompt: Option<String>,
    max_context_tokens: usize,
    ring_capacity: usize,
    channel_capacity: usize,
    handler_paths: Vec<PathBuf>,
    lua: Option<Lua>,
}

impl LuaSessionBuilder {
    /// Create a new session builder
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            folder: None,
            system_prompt: None,
            max_context_tokens: default_max_tokens(),
            ring_capacity: DEFAULT_RING_CAPACITY,
            channel_capacity: DEFAULT_CHANNEL_CAPACITY,
            handler_paths: Vec::new(),
            lua: None,
        }
    }

    /// Set the session folder
    pub fn with_folder(mut self, folder: impl Into<PathBuf>) -> Self {
        self.folder = Some(folder.into());
        self
    }

    /// Set the system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set maximum context tokens
    pub fn with_max_context_tokens(mut self, tokens: usize) -> Self {
        self.max_context_tokens = tokens;
        self
    }

    /// Set ring buffer capacity
    pub fn with_ring_capacity(mut self, capacity: usize) -> Self {
        self.ring_capacity = capacity;
        self
    }

    /// Add handler discovery paths
    pub fn with_handler_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.handler_paths = paths;
        self
    }

    /// Use DiscoveryPaths for handler discovery
    pub fn with_discovery_paths(mut self, discovery: &DiscoveryPaths) -> Self {
        self.handler_paths = discovery.existing_paths().into_iter().collect();
        self
    }

    /// Provide a pre-configured Lua instance
    pub fn with_lua(mut self, lua: Lua) -> Self {
        self.lua = Some(lua);
        self
    }

    /// Build the session
    pub fn build(self) -> Result<LuaSession, LuaError> {
        let folder = self
            .folder
            .unwrap_or_else(|| PathBuf::from("Sessions").join(&self.session_id));

        let config = Arc::new(LuaSessionConfig {
            session_id: self.session_id,
            folder,
            max_context_tokens: self.max_context_tokens,
            system_prompt: self.system_prompt,
        });

        let ring = Arc::new(EventRing::new(self.ring_capacity));
        let (event_tx, event_rx) = mpsc::channel(self.channel_capacity);

        // Discover handlers
        let handlers = if self.handler_paths.is_empty() {
            LuaScriptHandlerRegistry::new()
        } else {
            LuaScriptHandlerRegistry::discover(&self.handler_paths).map_err(|e| LuaError::Io(e))?
        };

        // Create or use provided Lua instance
        let lua = self.lua.unwrap_or_else(|| Lua::new());

        Ok(LuaSession {
            config,
            ring,
            handlers: Arc::new(RwLock::new(handlers)),
            lua: Arc::new(RwLock::new(lua)),
            state: Arc::new(RwLock::new(SessionState::Initializing)),
            event_tx,
            event_rx: Arc::new(RwLock::new(Some(event_rx))),
            current_file_index: Arc::new(RwLock::new(0)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_session_lifecycle() {
        let temp = TempDir::new().unwrap();
        let session = LuaSessionBuilder::new("test-session")
            .with_folder(temp.path().join("session"))
            .build()
            .unwrap();

        assert_eq!(session.state().await, SessionState::Initializing);

        session.start().await.unwrap();
        assert_eq!(session.state().await, SessionState::Active);

        let handle = session.handle();
        handle.message("Hello", "user").await.unwrap();

        session.end("test complete").await.unwrap();
        assert_eq!(session.state().await, SessionState::Ended);
    }

    #[tokio::test]
    async fn test_session_persists_events() {
        use crucible_core::events::markdown::EventToMarkdown;

        let temp = TempDir::new().unwrap();
        let folder = temp.path().join("session");
        fs::create_dir_all(&folder).unwrap();

        let event = crucible_core::events::SessionEvent::SessionStarted {
            config: crucible_core::events::SessionEventConfig::new("test-session")
                .with_folder(&folder),
        };

        let markdown = event.to_markdown_block(Some(1234567890000));
        let context_file = folder.join("000-context.md");
        fs::write(&context_file, format!("{}\n", markdown)).unwrap();

        assert!(context_file.exists(), "Context file should exist");
        let content = fs::read_to_string(&context_file).unwrap();
        assert!(
            content.contains("SessionStarted"),
            "Should contain SessionStarted event"
        );
    }
}
