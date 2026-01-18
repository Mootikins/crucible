//! Handler wiring for EventBus integration.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::event_bus::EventBus;
use crate::handler::{Handler, HandlerContext, HandlerResult};
use crate::reactor::SessionEvent;

pub struct SessionEventBusHandler {
    name: String,
    dependencies: Vec<String>,
    event_bus: Arc<RwLock<EventBus>>,
}

impl SessionEventBusHandler {
    pub fn new(name: impl Into<String>, event_bus: Arc<RwLock<EventBus>>) -> Self {
        Self {
            name: name.into(),
            dependencies: Vec::new(),
            event_bus,
        }
    }

    pub fn with_dependencies(mut self, deps: Vec<String>) -> Self {
        self.dependencies = deps;
        self
    }

    pub fn add_dependency(mut self, dep: impl Into<String>) -> Self {
        self.dependencies.push(dep.into());
        self
    }
}

#[async_trait]
impl Handler for SessionEventBusHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn dependencies(&self) -> &[&str] {
        &[]
    }

    async fn handle(
        &self,
        ctx: &mut HandlerContext,
        event: SessionEvent,
    ) -> HandlerResult<SessionEvent> {
        let bus = self.event_bus.read().await;
        let (_processed_event, bus_ctx, errors) = bus.emit_session(event.clone());

        for err in &errors {
            tracing::warn!(handler = %self.name, "EventBus handler error: {}", err);
        }

        if bus_ctx.is_cancelled() {
            return HandlerResult::cancelled(event);
        }

        let mut bus_ctx = bus_ctx;
        for emitted in bus_ctx.take_emitted() {
            ctx.emit(crate::reactor::event_to_session_event(emitted));
        }

        if errors.iter().any(|e| e.is_fatal()) {
            return HandlerResult::fatal_msg("EventBus handler returned fatal error");
        }

        HandlerResult::ok(event)
    }
}

impl std::fmt::Debug for SessionEventBusHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionEventBusHandler")
            .field("name", &self.name)
            .field("dependencies", &self.dependencies)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::EventBus;

    #[test]
    fn test_session_event_bus_handler_new() {
        let bus = Arc::new(RwLock::new(EventBus::new()));
        let handler = SessionEventBusHandler::new("test_handler", bus);
        assert_eq!(handler.name, "test_handler");
        assert!(handler.dependencies.is_empty());
    }

    #[test]
    fn test_session_event_bus_handler_with_dependencies() {
        let bus = Arc::new(RwLock::new(EventBus::new()));
        let handler = SessionEventBusHandler::new("test_handler", bus)
            .with_dependencies(vec!["dep1".to_string(), "dep2".to_string()]);
        assert_eq!(handler.dependencies, vec!["dep1", "dep2"]);
    }

    #[test]
    fn test_session_event_bus_handler_debug() {
        let bus = Arc::new(RwLock::new(EventBus::new()));
        let handler = SessionEventBusHandler::new("debug_test", bus);
        let debug = format!("{:?}", handler);
        assert!(debug.contains("SessionEventBusHandler"));
        assert!(debug.contains("debug_test"));
    }
}
