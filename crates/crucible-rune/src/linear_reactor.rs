//! Linear Reactor for Sequential Session Event Processing
//!
//! `LinearReactor` is the primary reactor implementation for Crucible sessions.
//! It processes events sequentially through a handler chain, with events stored
//! in a ring buffer for replay and debugging.
//!
//! ## Design
//!
//! The linear reactor follows a simple sequential flow:
//!
//! ```text
//! Event → Ring Buffer → Handler Chain → Emit new events → Persist
//! ```
//!
//! Key characteristics:
//! - **Single-threaded processing**: Events are processed one at a time
//! - **Deterministic ordering**: Handlers run in topological order based on dependencies
//! - **Fail-open semantics**: Non-fatal errors are logged but don't stop processing
//! - **Event sourcing**: All events are stored in the ring buffer
//!
//! ## Architecture
//!
//! ```text
//! LinearReactor
//!     ├── EventRing<SessionEvent>   (in-memory event log)
//!     ├── HandlerChain<SessionEvent> (topo-sorted handlers)
//!     ├── EventBus                   (pub/sub for Rune handlers)
//!     └── ReactorSessionConfig              (session settings)
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rune::linear_reactor::{LinearReactor, LinearReactorConfig};
//! use crucible_rune::reactor::{Reactor, ReactorSessionConfig, SessionEvent};
//! use crucible_rune::event_bus::EventContext;
//!
//! // Create configuration
//! let config = LinearReactorConfig::new()
//!     .with_ring_capacity(1024)
//!     .with_event_bus(event_bus);
//!
//! // Create reactor
//! let reactor = LinearReactor::new(config);
//!
//! // Process events
//! let mut ctx = EventContext::new();
//! let event = SessionEvent::MessageReceived {
//!     content: "Hello".into(),
//!     participant_id: "user".into(),
//! };
//! let result = reactor.handle_event(&mut ctx, event).await?;
//! ```

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::event_bus::{EventBus, EventContext};
use crate::event_ring::EventRing;
use crate::handler::BoxedRingHandler;
use crate::handler_chain::{ChainResult, HandlerChain};
use crate::reactor::{
    Reactor, ReactorContext, ReactorError, ReactorMetadata, ReactorResult, ReactorSessionConfig,
    SessionEvent,
};

/// Configuration for the LinearReactor.
#[derive(Debug, Clone)]
pub struct LinearReactorConfig {
    /// Capacity of the event ring buffer.
    pub ring_capacity: usize,
    /// Whether to bridge EventBus events to the reactor.
    pub bridge_event_bus: bool,
    /// Whether to emit events to the EventBus after processing.
    pub emit_to_event_bus: bool,
}

impl Default for LinearReactorConfig {
    fn default() -> Self {
        Self {
            ring_capacity: 4096,
            bridge_event_bus: true,
            emit_to_event_bus: true,
        }
    }
}

impl LinearReactorConfig {
    /// Create a new configuration with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the ring buffer capacity.
    pub fn with_ring_capacity(mut self, capacity: usize) -> Self {
        self.ring_capacity = capacity;
        self
    }

    /// Set whether to bridge EventBus events.
    pub fn with_bridge_event_bus(mut self, bridge: bool) -> Self {
        self.bridge_event_bus = bridge;
        self
    }

    /// Set whether to emit to EventBus after processing.
    pub fn with_emit_to_event_bus(mut self, emit: bool) -> Self {
        self.emit_to_event_bus = emit;
        self
    }
}

/// A linear reactor that processes session events sequentially.
///
/// `LinearReactor` is the primary implementation of the `Reactor` trait for
/// Crucible sessions. It provides:
///
/// - **Sequential event processing**: Events flow through handlers in order
/// - **Ring buffer storage**: In-memory event log for replay and debugging
/// - **EventBus integration**: Rune handlers can subscribe to events
/// - **Handler chain**: Topo-sorted handler execution
///
/// ## Thread Safety
///
/// The reactor uses internal locking (`RwLock`) for thread-safe access to
/// the ring buffer and handler chain. Multiple concurrent `handle_event`
/// calls will be serialized.
pub struct LinearReactor {
    /// Configuration.
    config: LinearReactorConfig,
    /// Event ring buffer for in-memory event storage.
    ring: Arc<EventRing<SessionEvent>>,
    /// Handler chain for processing events.
    chain: Arc<RwLock<HandlerChain<SessionEvent>>>,
    /// Optional EventBus for pub/sub integration.
    event_bus: Arc<RwLock<Option<EventBus>>>,
    /// Session configuration (set on session start).
    session_config: Arc<RwLock<Option<ReactorSessionConfig>>>,
}

impl LinearReactor {
    /// Create a new linear reactor with the given configuration.
    pub fn new(config: LinearReactorConfig) -> Self {
        Self {
            ring: Arc::new(EventRing::new(config.ring_capacity)),
            chain: Arc::new(RwLock::new(HandlerChain::new())),
            event_bus: Arc::new(RwLock::new(None)),
            session_config: Arc::new(RwLock::new(None)),
            config,
        }
    }

    /// Create a new linear reactor with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(LinearReactorConfig::default())
    }

    /// Set the EventBus for pub/sub integration.
    pub async fn set_event_bus(&self, bus: EventBus) {
        let mut guard = self.event_bus.write().await;
        *guard = Some(bus);
    }

    /// Get a reference to the event ring.
    pub fn ring(&self) -> &Arc<EventRing<SessionEvent>> {
        &self.ring
    }

    /// Get the current write sequence number.
    pub fn current_sequence(&self) -> u64 {
        self.ring.write_sequence()
    }

    /// Get the number of events in the ring.
    pub fn event_count(&self) -> usize {
        self.ring.len()
    }

    /// Get the oldest available event sequence.
    pub fn oldest_sequence(&self) -> u64 {
        self.ring.oldest_sequence()
    }

    /// Get an event by sequence number.
    pub fn get_event(&self, seq: u64) -> Option<Arc<SessionEvent>> {
        self.ring.get(seq)
    }

    /// Iterate over all valid events in the ring.
    pub fn iter_events(&self) -> impl Iterator<Item = Arc<SessionEvent>> + '_ {
        self.ring.iter()
    }

    /// Add a handler to the processing chain.
    ///
    /// Handlers are invoked in topological order based on their declared
    /// dependencies.
    ///
    /// # Errors
    ///
    /// Returns an error if a handler with the same name already exists.
    pub async fn add_handler(&self, handler: BoxedRingHandler<SessionEvent>) -> ReactorResult<()> {
        let mut chain = self.chain.write().await;
        chain
            .add_handler(handler)
            .map_err(|e| ReactorError::Configuration {
                message: format!("Failed to add handler: {}", e),
            })
    }

    /// Remove a handler from the processing chain.
    ///
    /// # Errors
    ///
    /// Returns an error if the handler doesn't exist.
    pub async fn remove_handler(
        &self,
        name: &str,
    ) -> ReactorResult<BoxedRingHandler<SessionEvent>> {
        let mut chain = self.chain.write().await;
        chain
            .remove_handler(name)
            .map_err(|e| ReactorError::Configuration {
                message: format!("Failed to remove handler: {}", e),
            })
    }

    /// Check if a handler exists in the chain.
    pub async fn has_handler(&self, name: &str) -> bool {
        let chain = self.chain.read().await;
        chain.contains(name)
    }

    /// Get the number of handlers in the chain.
    pub async fn handler_count(&self) -> usize {
        let chain = self.chain.read().await;
        chain.len()
    }

    /// Get the execution order of handlers.
    ///
    /// # Errors
    ///
    /// Returns an error if there's a cycle or missing dependency.
    pub async fn execution_order(&self) -> ReactorResult<Vec<String>> {
        let mut chain = self.chain.write().await;
        chain
            .execution_order()
            .map_err(|e| ReactorError::Configuration {
                message: format!("Failed to compute execution order: {}", e),
            })
    }

    /// Process an event through the handler chain.
    ///
    /// This is the internal processing method that:
    /// 1. Pushes the event to the ring buffer
    /// 2. Runs all handlers in topological order
    /// 3. Pushes emitted events back to the ring
    ///
    /// # Returns
    ///
    /// A tuple of:
    /// - The sequence number assigned to the event
    /// - The chain processing result
    /// - Sequence numbers of any emitted events
    async fn process_event(
        &self,
        event: SessionEvent,
    ) -> ReactorResult<(u64, ChainResult<SessionEvent>, Vec<u64>)> {
        // Push event to ring
        let seq = self.ring.push(event);

        // Process through handler chain
        let mut chain = self.chain.write().await;
        let (result, emitted_seqs) = chain
            .process_from_ring(&self.ring, seq)
            .await
            .map_err(|e| ReactorError::processing_failed(format!("Handler chain error: {}", e)))?;

        Ok((seq, result, emitted_seqs))
    }

    /// Emit an event to the EventBus if configured.
    async fn emit_to_bus(&self, event: &SessionEvent, ctx: &mut EventContext) {
        if !self.config.emit_to_event_bus {
            return;
        }

        let bus_guard = self.event_bus.read().await;
        if let Some(bus) = bus_guard.as_ref() {
            // Use emit_session() directly with SessionEvent
            let (_, bus_ctx, errors) = bus.emit_session(event.clone());

            // Log any errors (fail-open)
            for err in errors {
                tracing::warn!("EventBus handler error: {}", err);
            }

            // Bridge emitted events from EventBus to our context
            // Note: emitted events from bus_ctx are still in legacy Event format
            if self.config.bridge_event_bus {
                let mut bus_ctx = bus_ctx;
                for emitted in bus_ctx.take_emitted() {
                    ctx.emit(emitted);
                }
            }
        }
    }
}

impl std::fmt::Debug for LinearReactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinearReactor")
            .field("config", &self.config)
            .field("ring_capacity", &self.config.ring_capacity)
            .field("event_count", &self.ring.len())
            .field("write_sequence", &self.ring.write_sequence())
            .finish()
    }
}

/// Generate a comprehensive summary of events for compaction.
///
/// This function analyzes the event history and produces a human-readable
/// summary that captures the key information from the session, including:
/// - Event counts by type (messages, tool calls, agent responses)
/// - Tools that were used
/// - Key message excerpts
/// - Overall statistics
fn generate_compaction_summary(events: &[SessionEvent]) -> String {
    let mut message_count = 0usize;
    let mut tool_call_count = 0usize;
    let mut _tool_complete_count = 0usize;
    let mut agent_response_count = 0usize;
    let mut thinking_count = 0usize;
    let mut subagent_count = 0usize;
    let mut tools_used: Vec<String> = Vec::new();
    let mut message_excerpts: Vec<String> = Vec::new();

    for event in events {
        match event {
            SessionEvent::MessageReceived {
                content,
                participant_id,
            } => {
                message_count += 1;
                // Capture first few message excerpts (truncated)
                if message_excerpts.len() < 3 {
                    let excerpt = if content.len() > 80 {
                        format!("{}: {}...", participant_id, &content[..77])
                    } else {
                        format!("{}: {}", participant_id, content)
                    };
                    message_excerpts.push(excerpt);
                }
            }
            SessionEvent::AgentResponded { .. } => {
                agent_response_count += 1;
            }
            SessionEvent::AgentThinking { .. } => {
                thinking_count += 1;
            }
            SessionEvent::ToolCalled { name, .. } => {
                tool_call_count += 1;
                if !tools_used.contains(name) {
                    tools_used.push(name.clone());
                }
            }
            SessionEvent::ToolCompleted { .. } => {
                _tool_complete_count += 1;
            }
            SessionEvent::SubagentSpawned { .. }
            | SessionEvent::SubagentCompleted { .. }
            | SessionEvent::SubagentFailed { .. } => {
                subagent_count += 1;
            }
            _ => {}
        }
    }

    let total_events = events.len();

    let mut summary = format!(
        "Session Summary: {} messages, {} tool calls, {} agent responses, {} total events.",
        message_count, tool_call_count, agent_response_count, total_events
    );

    // Add tools used if any
    if !tools_used.is_empty() {
        summary.push_str(&format!("\n\nTools used: {}", tools_used.join(", ")));
    }

    // Add thinking events if any
    if thinking_count > 0 {
        summary.push_str(&format!("\n\nAgent thinking events: {}", thinking_count));
    }

    // Add subagent activity if any
    if subagent_count > 0 {
        summary.push_str(&format!("\n\nSubagent events: {}", subagent_count));
    }

    // Add message excerpts if any
    if !message_excerpts.is_empty() {
        summary.push_str("\n\nKey messages:\n");
        for excerpt in message_excerpts {
            summary.push_str(&format!("- {}\n", excerpt));
        }
    }

    summary
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
        // File events (small metadata)
        SessionEvent::FileChanged { .. } => 50,
        SessionEvent::FileDeleted { .. } => 50,
        SessionEvent::FileMoved { .. } => 50,
        // Storage events (small metadata)
        SessionEvent::EntityStored { .. } => 50,
        SessionEvent::EntityDeleted { .. } => 50,
        SessionEvent::BlocksUpdated { .. } => 50,
        SessionEvent::RelationStored { .. } => 50,
        SessionEvent::RelationDeleted { .. } => 50,
        SessionEvent::TagAssociated { tag, .. } => tag.len() + 50,
        // Embedding events (small metadata)
        SessionEvent::EmbeddingRequested { .. } => 50,
        SessionEvent::EmbeddingStored { .. } => 50,
        SessionEvent::EmbeddingFailed { error, .. } => error.len() + 50,
        SessionEvent::EmbeddingBatchComplete { .. } => 50,
        // Pre-events (interception points)
        SessionEvent::PreToolCall { name, .. } => name.len() + 50,
        SessionEvent::PreParse { .. } => 50,
        SessionEvent::PreLlmCall { prompt, .. } => prompt.len(),
        SessionEvent::AwaitingInput { context, .. } => {
            context.as_ref().map_or(20, |c| c.len() + 20)
        }
        // Interaction events
        SessionEvent::InteractionRequested { .. } => 100, // Request metadata
        SessionEvent::InteractionCompleted { .. } => 50,  // Response metadata
    };

    // Rough estimate: ~4 characters per token
    // Add fixed overhead for event structure
    (content_len / 4).max(1) + 10
}

#[async_trait]
impl Reactor for LinearReactor {
    /// Process an event by sequence number from the ring buffer.
    ///
    /// This delegates to the handler chain's `process_from_ring` method,
    /// which:
    /// 1. Gets the event from the ring by sequence number
    /// 2. Runs all handlers in topological order
    /// 3. Pushes emitted events back to the ring buffer
    ///
    /// The method also emits the event to the EventBus if configured.
    async fn on_event(&self, ctx: &mut ReactorContext, seq: u64) -> ReactorResult<Vec<u64>> {
        // Process through handler chain
        let mut chain = self.chain.write().await;
        let (result, emitted_seqs) = chain
            .process_from_ring(&self.ring, seq)
            .await
            .map_err(|e| ReactorError::processing_failed(format!("Handler chain error: {}", e)))?;

        // Log any handler errors (fail-open semantics)
        for err in result.errors() {
            tracing::warn!(
                seq = seq,
                handler = %err.handler_name(),
                "Handler error: {}",
                err
            );
        }

        // Check for fatal errors
        if result.fatal {
            return Err(ReactorError::processing_failed(
                "Handler chain stopped due to fatal error",
            ));
        }

        // Emit to EventBus if configured
        if let Some(event) = self.ring.get(seq) {
            self.emit_to_bus(&event, ctx.event_context_mut()).await;
        }

        // Update token count based on processed event
        // (A simple heuristic - real implementations might use tiktoken)
        if let Some(event) = self.ring.get(seq) {
            let token_estimate = estimate_event_tokens(&event);
            ctx.add_tokens(token_estimate);
        }

        Ok(emitted_seqs)
    }

    async fn handle_event(
        &self,
        ctx: &mut EventContext,
        event: SessionEvent,
    ) -> ReactorResult<SessionEvent> {
        // Process the event through the handler chain
        let (seq, result, _emitted_seqs) = self.process_event(event.clone()).await?;

        // Log any handler errors (fail-open semantics)
        for err in result.errors() {
            tracing::warn!(
                seq = seq,
                handler = %err.handler_name(),
                "Handler error: {}",
                err
            );
        }

        // Check for fatal errors or cancellation
        if result.fatal {
            return Err(ReactorError::processing_failed(
                "Handler chain stopped due to fatal error",
            ));
        }

        // Emit to EventBus if configured
        self.emit_to_bus(&event, ctx).await;

        // Return the original event (handlers may have emitted new events,
        // but the original is returned as-is per the trait contract)
        Ok(event)
    }

    async fn on_session_start(&self, config: &ReactorSessionConfig) -> ReactorResult<()> {
        // Store the session config
        let mut guard = self.session_config.write().await;
        *guard = Some(config.clone());

        // Push a SessionStarted event
        let start_event = SessionEvent::SessionStarted {
            config: config.into(),
        };
        self.ring.push(start_event);

        tracing::info!(
            session_id = %config.session_id,
            folder = %config.folder.display(),
            "Linear reactor session started"
        );

        Ok(())
    }

    async fn on_before_compact(&self, events: &[SessionEvent]) -> ReactorResult<String> {
        // Generate a comprehensive summary of the events for compaction
        let summary = generate_compaction_summary(events);
        Ok(summary)
    }

    async fn on_session_end(&self, reason: &str) -> ReactorResult<()> {
        // Push a SessionEnded event
        let end_event = SessionEvent::SessionEnded {
            reason: reason.to_string(),
        };
        self.ring.push(end_event);

        // Get session ID for logging
        let session_id = {
            let guard = self.session_config.read().await;
            guard
                .as_ref()
                .map(|c| c.session_id.clone())
                .unwrap_or_else(|| "unknown".to_string())
        };

        tracing::info!(
            session_id = %session_id,
            reason = %reason,
            events_processed = self.ring.write_sequence(),
            "Linear reactor session ended"
        );

        Ok(())
    }

    fn metadata(&self) -> ReactorMetadata {
        ReactorMetadata::new("LinearReactor")
            .with_version("1.0.0")
            .with_description(
                "Sequential event processing with ring buffer storage and handler chain execution",
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::handler::{RingHandler, RingHandlerContext, RingHandlerResult};
    use serde_json::json;
    use std::path::PathBuf;

    /// Cross-platform test path helper
    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("crucible_test_{}", name))
    }

    /// Test handler that logs events.
    struct LoggingHandler {
        name: &'static str,
    }

    #[async_trait]
    impl RingHandler<SessionEvent> for LoggingHandler {
        fn name(&self) -> &str {
            self.name
        }

        async fn handle(
            &self,
            _ctx: &mut RingHandlerContext<SessionEvent>,
            event: Arc<SessionEvent>,
            seq: u64,
        ) -> RingHandlerResult<()> {
            tracing::debug!(
                handler = self.name,
                seq = seq,
                "Processing event: {:?}",
                event
            );
            Ok(())
        }
    }

    /// Test handler that emits events.
    struct EmitHandler {
        name: &'static str,
        deps: &'static [&'static str],
    }

    #[async_trait]
    impl RingHandler<SessionEvent> for EmitHandler {
        fn name(&self) -> &str {
            self.name
        }

        fn depends_on(&self) -> &[&str] {
            self.deps
        }

        async fn handle(
            &self,
            ctx: &mut RingHandlerContext<SessionEvent>,
            _event: Arc<SessionEvent>,
            _seq: u64,
        ) -> RingHandlerResult<()> {
            ctx.emit(SessionEvent::Custom {
                name: format!("emitted_by_{}", self.name),
                payload: json!({}),
            });
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_linear_reactor_new() {
        let reactor = LinearReactor::with_defaults();

        assert_eq!(reactor.event_count(), 0);
        assert_eq!(reactor.current_sequence(), 0);
        assert_eq!(reactor.handler_count().await, 0);
    }

    #[tokio::test]
    async fn test_linear_reactor_config() {
        let config = LinearReactorConfig::new()
            .with_ring_capacity(2048)
            .with_bridge_event_bus(false)
            .with_emit_to_event_bus(false);

        assert_eq!(config.ring_capacity, 2048);
        assert!(!config.bridge_event_bus);
        assert!(!config.emit_to_event_bus);

        let reactor = LinearReactor::new(config);
        assert_eq!(reactor.config.ring_capacity, 2048);
    }

    #[tokio::test]
    async fn test_linear_reactor_add_handler() {
        let reactor = LinearReactor::with_defaults();

        reactor
            .add_handler(Box::new(LoggingHandler { name: "logger" }))
            .await
            .unwrap();

        assert!(reactor.has_handler("logger").await);
        assert_eq!(reactor.handler_count().await, 1);
    }

    #[tokio::test]
    async fn test_linear_reactor_remove_handler() {
        let reactor = LinearReactor::with_defaults();

        reactor
            .add_handler(Box::new(LoggingHandler { name: "logger" }))
            .await
            .unwrap();

        assert!(reactor.has_handler("logger").await);

        reactor.remove_handler("logger").await.unwrap();

        assert!(!reactor.has_handler("logger").await);
        assert_eq!(reactor.handler_count().await, 0);
    }

    #[tokio::test]
    async fn test_linear_reactor_execution_order() {
        let reactor = LinearReactor::with_defaults();

        reactor
            .add_handler(Box::new(EmitHandler {
                name: "C",
                deps: &["B"],
            }))
            .await
            .unwrap();
        reactor
            .add_handler(Box::new(EmitHandler {
                name: "B",
                deps: &["A"],
            }))
            .await
            .unwrap();
        reactor
            .add_handler(Box::new(EmitHandler {
                name: "A",
                deps: &[],
            }))
            .await
            .unwrap();

        let order = reactor.execution_order().await.unwrap();
        assert_eq!(order, vec!["A", "B", "C"]);
    }

    #[tokio::test]
    async fn test_linear_reactor_handle_event() {
        let reactor = LinearReactor::with_defaults();
        let mut ctx = EventContext::new();

        let event = SessionEvent::MessageReceived {
            content: "Hello".to_string(),
            participant_id: "user".to_string(),
        };

        let result = reactor.handle_event(&mut ctx, event.clone()).await.unwrap();

        // Event should be returned as-is
        match result {
            SessionEvent::MessageReceived {
                content,
                participant_id,
            } => {
                assert_eq!(content, "Hello");
                assert_eq!(participant_id, "user");
            }
            _ => panic!("Wrong event type"),
        }

        // Event should be in the ring
        assert_eq!(reactor.event_count(), 1);
    }

    #[tokio::test]
    async fn test_linear_reactor_handle_event_with_handlers() {
        let reactor = LinearReactor::with_defaults();

        reactor
            .add_handler(Box::new(LoggingHandler { name: "logger" }))
            .await
            .unwrap();

        let mut ctx = EventContext::new();
        let event = SessionEvent::MessageReceived {
            content: "Test".to_string(),
            participant_id: "user".to_string(),
        };

        let result = reactor.handle_event(&mut ctx, event).await;
        assert!(result.is_ok());

        // Event should be in the ring
        assert_eq!(reactor.event_count(), 1);
    }

    #[tokio::test]
    async fn test_linear_reactor_on_session_start() {
        let reactor = LinearReactor::with_defaults();
        let config = ReactorSessionConfig::new("test-session", test_path("test"));

        reactor.on_session_start(&config).await.unwrap();

        // SessionStarted event should be in the ring
        assert_eq!(reactor.event_count(), 1);

        let event = reactor.get_event(0).unwrap();
        match event.as_ref() {
            SessionEvent::SessionStarted { config } => {
                assert_eq!(config.session_id, "test-session");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[tokio::test]
    async fn test_linear_reactor_on_session_end() {
        let reactor = LinearReactor::with_defaults();
        let config = ReactorSessionConfig::new("test-session", test_path("test"));

        reactor.on_session_start(&config).await.unwrap();
        reactor.on_session_end("user closed").await.unwrap();

        // Both events should be in the ring
        assert_eq!(reactor.event_count(), 2);

        let event = reactor.get_event(1).unwrap();
        match event.as_ref() {
            SessionEvent::SessionEnded { reason } => {
                assert_eq!(reason, "user closed");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[tokio::test]
    async fn test_linear_reactor_on_before_compact() {
        let reactor = LinearReactor::with_defaults();

        let events = vec![
            SessionEvent::MessageReceived {
                content: "Hello".into(),
                participant_id: "user".into(),
            },
            SessionEvent::AgentResponded {
                content: "Hi there".into(),
                tool_calls: vec![],
            },
            SessionEvent::ToolCalled {
                name: "read_file".into(),
                args: json!({"path": test_path("test").to_string_lossy()}),
            },
            SessionEvent::ToolCompleted {
                name: "read_file".into(),
                result: "content".into(),
                error: None,
            },
        ];

        let summary = reactor.on_before_compact(&events).await.unwrap();

        assert!(summary.contains("1 messages"));
        assert!(summary.contains("1 tool calls"));
        assert!(summary.contains("1 agent responses"));
        assert!(summary.contains("4 total events"));
        assert!(summary.contains("read_file")); // Should list tools used
    }

    #[tokio::test]
    async fn test_linear_reactor_metadata() {
        let reactor = LinearReactor::with_defaults();
        let meta = reactor.metadata();

        assert_eq!(meta.name, "LinearReactor");
        assert_eq!(meta.version, "1.0.0");
        assert!(!meta.description.is_empty());
    }

    #[tokio::test]
    async fn test_linear_reactor_iter_events() {
        let reactor = LinearReactor::with_defaults();
        let mut ctx = EventContext::new();

        // Add some events
        for i in 0..3 {
            let event = SessionEvent::MessageReceived {
                content: format!("Message {}", i),
                participant_id: "user".into(),
            };
            reactor.handle_event(&mut ctx, event).await.unwrap();
        }

        // Iterate and collect
        let events: Vec<_> = reactor.iter_events().collect();
        assert_eq!(events.len(), 3);

        match events[0].as_ref() {
            SessionEvent::MessageReceived { content, .. } => {
                assert_eq!(content, "Message 0");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[tokio::test]
    async fn test_linear_reactor_get_event() {
        let reactor = LinearReactor::with_defaults();
        let mut ctx = EventContext::new();

        let event = SessionEvent::MessageReceived {
            content: "Test".into(),
            participant_id: "user".into(),
        };

        reactor.handle_event(&mut ctx, event).await.unwrap();

        // Get by sequence
        let retrieved = reactor.get_event(0).unwrap();
        match retrieved.as_ref() {
            SessionEvent::MessageReceived { content, .. } => {
                assert_eq!(content, "Test");
            }
            _ => panic!("Wrong event type"),
        }

        // Non-existent sequence
        assert!(reactor.get_event(999).is_none());
    }

    #[tokio::test]
    async fn test_linear_reactor_debug() {
        let reactor = LinearReactor::with_defaults();
        let debug = format!("{:?}", reactor);

        assert!(debug.contains("LinearReactor"));
        assert!(debug.contains("ring_capacity"));
        assert!(debug.contains("event_count"));
    }

    #[tokio::test]
    async fn test_linear_reactor_with_event_bus() {
        let reactor = LinearReactor::with_defaults();
        let bus = EventBus::new();

        reactor.set_event_bus(bus).await;

        // Should be able to process events with EventBus integration
        let mut ctx = EventContext::new();
        let event = SessionEvent::MessageReceived {
            content: "Test".into(),
            participant_id: "user".into(),
        };

        let result = reactor.handle_event(&mut ctx, event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_linear_reactor_handler_emits_events() {
        let reactor = LinearReactor::with_defaults();

        reactor
            .add_handler(Box::new(EmitHandler {
                name: "emitter",
                deps: &[],
            }))
            .await
            .unwrap();

        let mut ctx = EventContext::new();
        let event = SessionEvent::MessageReceived {
            content: "Trigger".into(),
            participant_id: "user".into(),
        };

        reactor.handle_event(&mut ctx, event).await.unwrap();

        // Original event + emitted event
        assert_eq!(reactor.event_count(), 2);

        // Check emitted event
        let emitted = reactor.get_event(1).unwrap();
        match emitted.as_ref() {
            SessionEvent::Custom { name, .. } => {
                assert_eq!(name, "emitted_by_emitter");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[tokio::test]
    async fn test_linear_reactor_multiple_handlers() {
        let reactor = LinearReactor::with_defaults();

        // Add handlers in dependency order
        reactor
            .add_handler(Box::new(EmitHandler {
                name: "A",
                deps: &[],
            }))
            .await
            .unwrap();
        reactor
            .add_handler(Box::new(EmitHandler {
                name: "B",
                deps: &["A"],
            }))
            .await
            .unwrap();
        reactor
            .add_handler(Box::new(EmitHandler {
                name: "C",
                deps: &["B"],
            }))
            .await
            .unwrap();

        let mut ctx = EventContext::new();
        let event = SessionEvent::MessageReceived {
            content: "Start".into(),
            participant_id: "user".into(),
        };

        reactor.handle_event(&mut ctx, event).await.unwrap();

        // Original event + 3 emitted events
        assert_eq!(reactor.event_count(), 4);
    }

    // ========================
    // on_event tests (ring buffer model)
    // ========================

    #[tokio::test]
    async fn test_linear_reactor_on_event_processes_event() {
        let reactor = LinearReactor::with_defaults();

        // Add a logging handler
        reactor
            .add_handler(Box::new(LoggingHandler { name: "logger" }))
            .await
            .unwrap();

        // Push an event to the ring
        let event = SessionEvent::MessageReceived {
            content: "Hello from on_event".to_string(),
            participant_id: "user".to_string(),
        };
        let seq = reactor.ring().push(event);

        // Create a ReactorContext
        let config = Arc::new(ReactorSessionConfig::new("test-session", test_path("test")));
        let mut ctx = ReactorContext::new(config);

        // Process via on_event
        let emitted_seqs = reactor.on_event(&mut ctx, seq).await.unwrap();

        // No events emitted by logging handler
        assert!(emitted_seqs.is_empty());

        // Event count should be 1 (the original)
        assert_eq!(reactor.event_count(), 1);
    }

    #[tokio::test]
    async fn test_linear_reactor_on_event_emits_to_ring() {
        let reactor = LinearReactor::with_defaults();

        // Add a handler that emits events
        reactor
            .add_handler(Box::new(EmitHandler {
                name: "emitter",
                deps: &[],
            }))
            .await
            .unwrap();

        // Push an event to the ring
        let event = SessionEvent::MessageReceived {
            content: "Trigger".to_string(),
            participant_id: "user".to_string(),
        };
        let seq = reactor.ring().push(event);
        assert_eq!(seq, 0);

        // Create a ReactorContext
        let config = Arc::new(ReactorSessionConfig::new("test-session", test_path("test")));
        let mut ctx = ReactorContext::new(config);

        // Process via on_event
        let emitted_seqs = reactor.on_event(&mut ctx, seq).await.unwrap();

        // Should have emitted one event
        assert_eq!(emitted_seqs.len(), 1);
        assert_eq!(emitted_seqs[0], 1); // Sequence 1

        // Ring now has 2 events
        assert_eq!(reactor.event_count(), 2);

        // Verify the emitted event
        let emitted = reactor.get_event(1).unwrap();
        match emitted.as_ref() {
            SessionEvent::Custom { name, .. } => {
                assert_eq!(name, "emitted_by_emitter");
            }
            _ => panic!("Wrong event type, got {:?}", emitted),
        }
    }

    #[tokio::test]
    async fn test_linear_reactor_on_event_with_multiple_handlers() {
        let reactor = LinearReactor::with_defaults();

        // Add handlers in dependency order
        reactor
            .add_handler(Box::new(EmitHandler {
                name: "A",
                deps: &[],
            }))
            .await
            .unwrap();
        reactor
            .add_handler(Box::new(EmitHandler {
                name: "B",
                deps: &["A"],
            }))
            .await
            .unwrap();
        reactor
            .add_handler(Box::new(EmitHandler {
                name: "C",
                deps: &["B"],
            }))
            .await
            .unwrap();

        // Push an event to the ring
        let event = SessionEvent::MessageReceived {
            content: "Test".to_string(),
            participant_id: "user".to_string(),
        };
        let seq = reactor.ring().push(event);

        // Create a ReactorContext
        let config = Arc::new(ReactorSessionConfig::new("test-session", test_path("test")));
        let mut ctx = ReactorContext::new(config);

        // Process via on_event
        let emitted_seqs = reactor.on_event(&mut ctx, seq).await.unwrap();

        // Should have emitted 3 events (one per handler)
        assert_eq!(emitted_seqs.len(), 3);

        // Ring now has 4 events (original + 3 emitted)
        assert_eq!(reactor.event_count(), 4);
    }

    #[tokio::test]
    async fn test_linear_reactor_on_event_updates_token_count() {
        let reactor = LinearReactor::with_defaults();

        // Push an event with content
        let event = SessionEvent::MessageReceived {
            content: "This is a test message with some content to estimate tokens".to_string(),
            participant_id: "user".to_string(),
        };
        let seq = reactor.ring().push(event);

        // Create a ReactorContext
        let config = Arc::new(ReactorSessionConfig::new("test-session", test_path("test")));
        let mut ctx = ReactorContext::new(config);

        assert_eq!(ctx.token_count(), 0);

        // Process via on_event
        reactor.on_event(&mut ctx, seq).await.unwrap();

        // Token count should be updated
        assert!(ctx.token_count() > 0);
    }

    #[tokio::test]
    async fn test_linear_reactor_on_event_event_not_found() {
        let reactor = LinearReactor::with_defaults();

        reactor
            .add_handler(Box::new(LoggingHandler { name: "logger" }))
            .await
            .unwrap();

        // Create a ReactorContext
        let config = Arc::new(ReactorSessionConfig::new("test-session", test_path("test")));
        let mut ctx = ReactorContext::new(config);

        // Try to process non-existent event
        let result = reactor.on_event(&mut ctx, 999).await;

        assert!(result.is_err());
    }

    // ========================
    // on_start/on_stop lifecycle tests (TASKS.md 3.2.3)
    // ========================

    #[tokio::test]
    async fn linear_reactor_emits_start_event() {
        let reactor = LinearReactor::with_defaults();

        // Initially no events
        assert_eq!(reactor.event_count(), 0);

        // Call on_session_start
        let config = ReactorSessionConfig::new("test-session", test_path("test"))
            .with_system_prompt("You are a helpful assistant.");
        reactor.on_session_start(&config).await.unwrap();

        // Should have emitted exactly one event
        assert_eq!(reactor.event_count(), 1);

        // Verify it's a SessionStarted event with correct config
        let event = reactor.get_event(0).expect("Event should exist");
        match event.as_ref() {
            SessionEvent::SessionStarted {
                config: stored_config,
            } => {
                assert_eq!(stored_config.session_id, "test-session");
                assert_eq!(
                    stored_config.system_prompt,
                    Some("You are a helpful assistant.".to_string())
                );
            }
            _ => panic!("Expected SessionStarted event, got {:?}", event.as_ref()),
        }
    }

    #[tokio::test]
    async fn linear_reactor_emits_stop_event() {
        let reactor = LinearReactor::with_defaults();

        // Start session first
        let config = ReactorSessionConfig::new("test-session", test_path("test"));
        reactor.on_session_start(&config).await.unwrap();
        assert_eq!(reactor.event_count(), 1);

        // Call on_session_end
        reactor.on_session_end("user requested").await.unwrap();

        // Should have emitted the stop event
        assert_eq!(reactor.event_count(), 2);

        // Verify it's a SessionEnded event with correct reason
        let event = reactor.get_event(1).expect("Event should exist");
        match event.as_ref() {
            SessionEvent::SessionEnded { reason } => {
                assert_eq!(reason, "user requested");
            }
            _ => panic!("Expected SessionEnded event, got {:?}", event.as_ref()),
        }
    }
}
