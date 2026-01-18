//! Chain-based reactor using SessionHandlerChain for dependency-ordered processing.

use async_trait::async_trait;
use std::collections::HashSet;
use tokio::sync::RwLock;

use crate::event_bus::EventContext;
use crate::handler::BoxedHandler;
use crate::handler_chain::SessionHandlerChain;
use crate::reactor::{Reactor, ReactorMetadata, ReactorResult, SessionEvent};

/// Reactor that processes events through a dependency-ordered handler chain.
///
/// This bridges the unified `Handler` trait with the `Reactor` pattern,
/// allowing handlers with dependencies to participate in session event processing.
pub struct ChainReactor {
    chain: RwLock<SessionHandlerChain>,
}

impl ChainReactor {
    pub fn new() -> Self {
        Self {
            chain: RwLock::new(SessionHandlerChain::new()),
        }
    }

    pub fn from_chain(chain: SessionHandlerChain) -> Self {
        Self {
            chain: RwLock::new(chain),
        }
    }

    pub async fn add_handler(
        &self,
        handler: BoxedHandler,
    ) -> Result<(), crate::dependency_graph::DependencyError> {
        let mut chain = self.chain.write().await;
        chain.add_handler(handler)
    }

    pub async fn remove_handler(
        &self,
        name: &str,
    ) -> Result<BoxedHandler, crate::dependency_graph::DependencyError> {
        let mut chain = self.chain.write().await;
        chain.remove_handler(name)
    }

    pub async fn handler_count(&self) -> usize {
        self.chain.read().await.len()
    }

    pub async fn contains(&self, name: &str) -> bool {
        self.chain.read().await.contains(name)
    }
}

impl Default for ChainReactor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Reactor for ChainReactor {
    async fn handle_event(
        &self,
        ctx: &mut EventContext,
        event: SessionEvent,
    ) -> ReactorResult<SessionEvent> {
        let mut chain = self.chain.write().await;
        let (result, processed_event) = chain
            .process(event.clone())
            .await
            .map_err(|e| crate::reactor::ReactorError::processing_failed(e.to_string()))?;

        if result.cancelled {
            ctx.cancel();
        }

        if result.fatal {
            return Err(crate::reactor::ReactorError::processing_failed(
                result.soft_errors.join("; "),
            ));
        }

        Ok(processed_event)
    }

    async fn on_before_compact(&self, events: &[SessionEvent]) -> ReactorResult<String> {
        let mut messages = 0;
        let mut tool_calls = 0;
        let mut agent_responses = 0;
        let mut tools_used: HashSet<String> = HashSet::new();

        for event in events {
            match event {
                SessionEvent::MessageReceived { .. } => messages += 1,
                SessionEvent::ToolCalled { name, .. } => {
                    tool_calls += 1;
                    tools_used.insert(name.clone());
                }
                SessionEvent::AgentResponded { .. } => agent_responses += 1,
                _ => {}
            }
        }

        let tools_list: Vec<_> = tools_used.into_iter().collect();
        let handler_count = self.chain.read().await.len();

        Ok(format!(
            "Session contained {} messages, {} tool calls, {} agent responses, {} total events. \
             Processed by {} handlers. Tools used: {}",
            messages,
            tool_calls,
            agent_responses,
            events.len(),
            handler_count,
            if tools_list.is_empty() {
                "none".to_string()
            } else {
                tools_list.join(", ")
            }
        ))
    }

    fn metadata(&self) -> ReactorMetadata {
        ReactorMetadata::new("ChainReactor")
            .with_version("1.0.0")
            .with_description("Reactor using dependency-ordered handler chain")
    }
}

impl std::fmt::Debug for ChainReactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChainReactor").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::{Handler, HandlerContext, HandlerResult};

    struct TestHandler {
        name: &'static str,
        deps: &'static [&'static str],
    }

    #[async_trait]
    impl Handler for TestHandler {
        fn name(&self) -> &str {
            self.name
        }

        fn dependencies(&self) -> &[&str] {
            self.deps
        }

        async fn handle(
            &self,
            _ctx: &mut HandlerContext,
            event: SessionEvent,
        ) -> HandlerResult<SessionEvent> {
            HandlerResult::ok(event)
        }
    }

    #[tokio::test]
    async fn test_chain_reactor_new() {
        let reactor = ChainReactor::new();
        assert_eq!(reactor.handler_count().await, 0);
    }

    #[tokio::test]
    async fn test_chain_reactor_add_handler() {
        let reactor = ChainReactor::new();

        reactor
            .add_handler(Box::new(TestHandler {
                name: "test",
                deps: &[],
            }))
            .await
            .unwrap();

        assert_eq!(reactor.handler_count().await, 1);
        assert!(reactor.contains("test").await);
    }

    #[tokio::test]
    async fn test_chain_reactor_handle_event() {
        let reactor = ChainReactor::new();

        reactor
            .add_handler(Box::new(TestHandler {
                name: "handler1",
                deps: &[],
            }))
            .await
            .unwrap();

        let event = SessionEvent::MessageReceived {
            content: "test".into(),
            participant_id: "user".into(),
        };

        let mut ctx = EventContext::new();
        let result = reactor.handle_event(&mut ctx, event).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_chain_reactor_metadata() {
        let reactor = ChainReactor::new();
        let metadata = reactor.metadata();

        assert_eq!(metadata.name, "ChainReactor");
    }
}
