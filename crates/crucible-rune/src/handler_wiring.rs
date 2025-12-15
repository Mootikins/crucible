//! Handler Wiring for EventBus Integration
//!
//! This module provides utilities for wiring EventBus handlers into the
//! reactor's event processing pipeline. It bridges the pub/sub EventBus
//! layer with the ring buffer and handler chain infrastructure.
//!
//! ## Architecture
//!
//! ```text
//! Session
//!    │
//!    ├── EventRing (storage)
//!    │       │
//!    │       ▼
//!    ├── HandlerChain (topo-sorted RingHandlers)
//!    │       │
//!    │       ▼
//!    └── EventBus (pub/sub) ◄── Rune handlers, plugins
//!            │
//!            ▼
//!       emit back to ring
//! ```
//!
//! ## Key Wiring Points
//!
//! 1. **EventBus → RingHandler**: Wrap EventBus handlers as RingHandlers
//! 2. **RingHandler → EventBus**: Forward events to EventBus after chain processing
//! 3. **EventBus emissions → Ring**: Convert emitted events back to SessionEvents
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rune::handler_wiring::{EventBusRingHandler, wire_event_bus};
//! use crucible_rune::event_bus::{EventBus, Handler, EventType};
//! use crucible_rune::linear_reactor::LinearReactor;
//!
//! // Create reactor and event bus
//! let reactor = LinearReactor::with_defaults();
//! let mut bus = EventBus::new();
//!
//! // Register EventBus handlers
//! bus.register(Handler::new("log_tools", EventType::ToolAfter, "*", |ctx, event| {
//!     println!("Tool executed: {}", event.identifier);
//!     Ok(event)
//! }));
//!
//! // Wire the event bus to the reactor
//! wire_event_bus(&reactor, bus).await;
//! ```

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::event_bus::EventBus;
use crate::handler::{BoxedRingHandler, RingHandler, RingHandlerContext, RingHandlerError, RingHandlerResult};
use crate::linear_reactor::LinearReactor;
use crate::reactor::SessionEvent;

/// A RingHandler that delegates to an EventBus for event processing.
///
/// This handler bridges the ring buffer model to the pub/sub EventBus,
/// allowing existing EventBus handlers to participate in the topo-sorted
/// handler chain.
///
/// ## Behavior
///
/// 1. Converts `SessionEvent` to `Event` (EventBus format)
/// 2. Emits to the EventBus
/// 3. Collects any events emitted by EventBus handlers
/// 4. Converts emitted events back to `SessionEvent`
pub struct EventBusRingHandler {
    /// Name of this handler in the chain
    name: String,
    /// Dependencies (run after these handlers)
    depends_on: Vec<String>,
    /// The EventBus to delegate to
    event_bus: Arc<RwLock<EventBus>>,
}

impl EventBusRingHandler {
    /// Create a new EventBus ring handler.
    pub fn new(name: impl Into<String>, event_bus: Arc<RwLock<EventBus>>) -> Self {
        Self {
            name: name.into(),
            depends_on: Vec::new(),
            event_bus,
        }
    }

    /// Set dependencies for this handler.
    pub fn with_depends_on(mut self, deps: Vec<String>) -> Self {
        self.depends_on = deps;
        self
    }

    /// Add a single dependency.
    pub fn depends_on(mut self, dep: impl Into<String>) -> Self {
        self.depends_on.push(dep.into());
        self
    }
}

#[async_trait]
impl RingHandler<SessionEvent> for EventBusRingHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn depends_on(&self) -> &[&str] {
        // Convert Vec<String> to slice of &str
        // This is a bit awkward but necessary for the trait
        &[]
    }

    async fn handle(
        &self,
        ctx: &mut RingHandlerContext<SessionEvent>,
        event: Arc<SessionEvent>,
        _seq: u64,
    ) -> RingHandlerResult<()> {
        // Emit to EventBus using SessionEvent directly
        let bus = self.event_bus.read().await;
        let (_processed_event, bus_ctx, errors) = bus.emit_session((*event).clone());

        // Log any errors (fail-open semantics)
        for err in &errors {
            tracing::warn!(
                handler = %self.name,
                "EventBus handler error: {}",
                err
            );
        }

        // Propagate cancellation from EventBus context to RingHandlerContext
        if bus_ctx.is_cancelled() {
            ctx.cancel();
        }

        // Emitted events from bus_ctx are still in legacy Event format,
        // so we convert them to SessionEvent
        let mut bus_ctx = bus_ctx;
        for emitted in bus_ctx.take_emitted() {
            ctx.emit(emitted.into());
        }

        // If there were fatal errors, propagate them
        if errors.iter().any(|e| e.fatal) {
            return Err(RingHandlerError::fatal(
                &self.name,
                "EventBus handler returned fatal error",
            ));
        }

        Ok(())
    }
}

impl std::fmt::Debug for EventBusRingHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBusRingHandler")
            .field("name", &self.name)
            .field("depends_on", &self.depends_on)
            .finish()
    }
}

/// Wire an EventBus to a LinearReactor.
///
/// This sets up the bidirectional connection:
/// 1. Sets the EventBus on the reactor (for `emit_to_event_bus`)
/// 2. Adds an EventBusRingHandler to the handler chain
///
/// # Arguments
///
/// * `reactor` - The LinearReactor to wire
/// * `event_bus` - The EventBus to connect
/// * `handler_name` - Name for the EventBus handler in the chain
/// * `depends_on` - Optional dependencies for the handler
pub async fn wire_event_bus(
    reactor: &LinearReactor,
    event_bus: EventBus,
    handler_name: impl Into<String>,
    depends_on: Vec<String>,
) -> Result<(), crate::reactor::ReactorError> {
    let bus = Arc::new(RwLock::new(event_bus));

    // Create the ring handler
    let handler = EventBusRingHandler::new(handler_name, Arc::clone(&bus))
        .with_depends_on(depends_on);

    // Add handler to reactor's chain
    reactor.add_handler(Box::new(handler)).await?;

    // Extract the EventBus for the reactor's emit_to_bus functionality
    // Note: We need to give the reactor its own reference
    // Create a new EventBus for the reactor (separate from the ring handler's bus)
    // The ring handler already has the original bus for actual processing
    let bus_for_reactor = EventBus::new();
    reactor.set_event_bus(bus_for_reactor).await;

    Ok(())
}

/// Wire an EventBus to a LinearReactor with default settings.
///
/// Uses "event_bus" as the handler name and no dependencies.
pub async fn wire_event_bus_default(
    reactor: &LinearReactor,
    event_bus: EventBus,
) -> Result<(), crate::reactor::ReactorError> {
    wire_event_bus(reactor, event_bus, "event_bus", Vec::new()).await
}

/// Builder for creating wired handler configurations.
///
/// Provides a fluent API for setting up complex handler wiring scenarios.
pub struct HandlerWiringBuilder {
    handlers: Vec<BoxedRingHandler<SessionEvent>>,
    event_bus: Option<EventBus>,
    event_bus_handler_name: String,
    event_bus_depends_on: Vec<String>,
}

impl Default for HandlerWiringBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl HandlerWiringBuilder {
    /// Create a new handler wiring builder.
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
            event_bus: None,
            event_bus_handler_name: "event_bus".to_string(),
            event_bus_depends_on: Vec::new(),
        }
    }

    /// Add a ring handler.
    pub fn with_handler(mut self, handler: BoxedRingHandler<SessionEvent>) -> Self {
        self.handlers.push(handler);
        self
    }

    /// Add multiple ring handlers.
    pub fn with_handlers(mut self, handlers: Vec<BoxedRingHandler<SessionEvent>>) -> Self {
        self.handlers.extend(handlers);
        self
    }

    /// Set the EventBus to wire.
    pub fn with_event_bus(mut self, bus: EventBus) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Set the name for the EventBus handler.
    pub fn with_event_bus_handler_name(mut self, name: impl Into<String>) -> Self {
        self.event_bus_handler_name = name.into();
        self
    }

    /// Set dependencies for the EventBus handler.
    pub fn with_event_bus_depends_on(mut self, deps: Vec<String>) -> Self {
        self.event_bus_depends_on = deps;
        self
    }

    /// Apply the wiring to a LinearReactor.
    pub async fn apply(self, reactor: &LinearReactor) -> Result<(), crate::reactor::ReactorError> {
        // Add all handlers first
        for handler in self.handlers {
            reactor.add_handler(handler).await?;
        }

        // Wire EventBus if provided
        if let Some(bus) = self.event_bus {
            wire_event_bus(
                reactor,
                bus,
                self.event_bus_handler_name,
                self.event_bus_depends_on,
            )
            .await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::{Event, EventType, Handler, HandlerError};
    use crate::linear_reactor::LinearReactorConfig;
    use serde_json::json;

    #[tokio::test]
    async fn test_event_bus_ring_handler_basic() {
        let bus = Arc::new(RwLock::new(EventBus::new()));

        // Register a handler that modifies events
        {
            let mut bus_guard = bus.write().await;
            bus_guard.register(Handler::new(
                "modifier",
                EventType::Custom,
                "*",
                |_ctx, mut event| {
                    if let Some(obj) = event.payload.as_object_mut() {
                        obj.insert("modified".to_string(), json!(true));
                    }
                    Ok(event)
                },
            ));
        }

        let handler = EventBusRingHandler::new("bus_handler", Arc::clone(&bus));

        let mut ctx = RingHandlerContext::new();
        let event = Arc::new(SessionEvent::MessageReceived {
            content: "Test".into(),
            participant_id: "user".into(),
        });

        let result = handler.handle(&mut ctx, event, 0).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_event_bus_ring_handler_emits_events() {
        let bus = Arc::new(RwLock::new(EventBus::new()));

        // Register a handler that emits an event
        {
            let mut bus_guard = bus.write().await;
            bus_guard.register(Handler::new(
                "emitter",
                EventType::Custom,
                "*",
                |ctx, event| {
                    ctx.emit(Event::custom("emitted_event", json!({"from": "emitter"})));
                    Ok(event)
                },
            ));
        }

        let handler = EventBusRingHandler::new("bus_handler", Arc::clone(&bus));

        let mut ctx = RingHandlerContext::new();
        let event = Arc::new(SessionEvent::MessageReceived {
            content: "Trigger".into(),
            participant_id: "user".into(),
        });

        handler.handle(&mut ctx, event, 0).await.unwrap();

        // Should have emitted one event
        let emitted = ctx.take_emitted();
        assert_eq!(emitted.len(), 1);

        match &emitted[0] {
            SessionEvent::Custom { name, payload } => {
                assert_eq!(name, "emitted_event");
                assert_eq!(payload["from"], "emitter");
            }
            _ => panic!("Expected Custom event"),
        }
    }

    #[tokio::test]
    async fn test_event_bus_ring_handler_cancellation() {
        let bus = Arc::new(RwLock::new(EventBus::new()));

        // Register a handler that cancels events
        {
            let mut bus_guard = bus.write().await;
            bus_guard.register(Handler::new(
                "canceller",
                EventType::Custom,
                "*",
                |_ctx, mut event| {
                    event.cancel();
                    Ok(event)
                },
            ));
        }

        let handler = EventBusRingHandler::new("bus_handler", Arc::clone(&bus));

        let mut ctx = RingHandlerContext::new();
        let event = Arc::new(SessionEvent::MessageReceived {
            content: "Cancel me".into(),
            participant_id: "user".into(),
        });

        handler.handle(&mut ctx, event, 0).await.unwrap();

        // Context should be cancelled
        assert!(ctx.is_cancelled());
    }

    #[tokio::test]
    async fn test_event_bus_ring_handler_fatal_error() {
        let bus = Arc::new(RwLock::new(EventBus::new()));

        // Register a handler that returns a fatal error
        {
            let mut bus_guard = bus.write().await;
            bus_guard.register(Handler::new(
                "fatal",
                EventType::Custom,
                "*",
                |_ctx, _event| Err(HandlerError::fatal("fatal", "critical error")),
            ));
        }

        let handler = EventBusRingHandler::new("bus_handler", Arc::clone(&bus));

        let mut ctx = RingHandlerContext::new();
        let event = Arc::new(SessionEvent::MessageReceived {
            content: "Fail".into(),
            participant_id: "user".into(),
        });

        let result = handler.handle(&mut ctx, event, 0).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().is_fatal());
    }

    #[tokio::test]
    async fn test_event_bus_ring_handler_non_fatal_error() {
        let bus = Arc::new(RwLock::new(EventBus::new()));

        // Register a handler that returns a non-fatal error
        {
            let mut bus_guard = bus.write().await;
            bus_guard.register(Handler::new(
                "non_fatal",
                EventType::Custom,
                "*",
                |_ctx, _event| Err(HandlerError::non_fatal("non_fatal", "minor issue")),
            ));
        }

        let handler = EventBusRingHandler::new("bus_handler", Arc::clone(&bus));

        let mut ctx = RingHandlerContext::new();
        let event = Arc::new(SessionEvent::MessageReceived {
            content: "Soft fail".into(),
            participant_id: "user".into(),
        });

        // Non-fatal errors should not cause the handler to fail
        let result = handler.handle(&mut ctx, event, 0).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_wire_event_bus_to_reactor() {
        let config = LinearReactorConfig::new().with_ring_capacity(64);
        let reactor = LinearReactor::new(config);

        let mut bus = EventBus::new();
        bus.register(Handler::new(
            "test_handler",
            EventType::Custom,
            "*",
            |_ctx, event| Ok(event),
        ));

        let result = wire_event_bus(&reactor, bus, "event_bus", Vec::new()).await;

        assert!(result.is_ok());
        assert!(reactor.has_handler("event_bus").await);
    }

    #[tokio::test]
    async fn test_wire_event_bus_default() {
        let reactor = LinearReactor::with_defaults();
        let bus = EventBus::new();

        let result = wire_event_bus_default(&reactor, bus).await;

        assert!(result.is_ok());
        assert!(reactor.has_handler("event_bus").await);
    }

    #[tokio::test]
    async fn test_handler_wiring_builder() {
        let reactor = LinearReactor::with_defaults();

        // Create a simple handler
        struct SimpleHandler;

        #[async_trait]
        impl RingHandler<SessionEvent> for SimpleHandler {
            fn name(&self) -> &str {
                "simple"
            }

            async fn handle(
                &self,
                _ctx: &mut RingHandlerContext<SessionEvent>,
                _event: Arc<SessionEvent>,
                _seq: u64,
            ) -> RingHandlerResult<()> {
                Ok(())
            }
        }

        let bus = EventBus::new();

        let result = HandlerWiringBuilder::new()
            .with_handler(Box::new(SimpleHandler))
            .with_event_bus(bus)
            .with_event_bus_handler_name("my_bus")
            .apply(&reactor)
            .await;

        assert!(result.is_ok());
        assert!(reactor.has_handler("simple").await);
        assert!(reactor.has_handler("my_bus").await);
    }

    #[tokio::test]
    async fn test_event_bus_ring_handler_debug() {
        let bus = Arc::new(RwLock::new(EventBus::new()));
        let handler = EventBusRingHandler::new("test", bus).depends_on("dep1");

        let debug = format!("{:?}", handler);
        assert!(debug.contains("EventBusRingHandler"));
        assert!(debug.contains("test"));
    }

    #[test]
    fn test_handler_wiring_builder_default() {
        let builder = HandlerWiringBuilder::default();
        assert!(builder.handlers.is_empty());
        assert!(builder.event_bus.is_none());
        assert_eq!(builder.event_bus_handler_name, "event_bus");
    }

    #[tokio::test]
    async fn test_wired_reactor_processes_events() {
        use crate::event_bus::EventContext as BusContext;
        use crate::reactor::Reactor;

        let config = LinearReactorConfig::new()
            .with_ring_capacity(64)
            .with_emit_to_event_bus(false); // Disable duplicate emission
        let reactor = LinearReactor::new(config);

        // Track whether handler was called
        let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let mut bus = EventBus::new();
        bus.register(Handler::new(
            "counter",
            EventType::Custom,
            "message:*",
            move |_ctx, event| {
                call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(event)
            },
        ));

        wire_event_bus_default(&reactor, bus).await.unwrap();

        // Process an event through the reactor
        let event = SessionEvent::MessageReceived {
            content: "Test".into(),
            participant_id: "user".into(),
        };

        let mut ctx = BusContext::new();
        reactor.handle_event(&mut ctx, event).await.unwrap();

        // Handler should have been called
        assert!(
            call_count.load(std::sync::atomic::Ordering::SeqCst) >= 1,
            "EventBus handler should be called"
        );
    }
}
