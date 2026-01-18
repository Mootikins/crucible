//! Handler chain for topologically sorted event processing.

use crate::dependency_graph::{DependencyError, DependencyResult, SessionHandlerGraph};
use crate::handler::{BoxedHandler, HandlerContext, HandlerResult};
use crucible_core::events::SessionEvent;

#[derive(Debug, Default)]
pub struct SessionChainResult {
    pub soft_errors: Vec<String>,
    pub cancelled: bool,
    pub fatal: bool,
}

impl SessionChainResult {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_ok(&self) -> bool {
        !self.fatal && !self.cancelled
    }

    pub fn stopped_early(&self) -> bool {
        self.fatal || self.cancelled
    }

    pub fn has_errors(&self) -> bool {
        !self.soft_errors.is_empty()
    }
}

pub struct SessionHandlerChain {
    graph: SessionHandlerGraph,
}

impl Default for SessionHandlerChain {
    fn default() -> Self {
        Self {
            graph: SessionHandlerGraph::new(),
        }
    }
}

impl SessionHandlerChain {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_graph(graph: SessionHandlerGraph) -> Self {
        Self { graph }
    }

    pub fn add_handler(&mut self, handler: BoxedHandler) -> DependencyResult<()> {
        self.graph.add_handler(handler)
    }

    pub fn remove_handler(&mut self, name: &str) -> DependencyResult<BoxedHandler> {
        self.graph.remove_handler(name)
    }

    pub fn get_handler(&self, name: &str) -> Option<&BoxedHandler> {
        self.graph.get_handler(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.graph.contains(name)
    }

    pub fn len(&self) -> usize {
        self.graph.len()
    }

    pub fn is_empty(&self) -> bool {
        self.graph.is_empty()
    }

    pub fn execution_order(&mut self) -> DependencyResult<Vec<String>> {
        self.graph.execution_order()
    }

    pub fn validate(&mut self) -> DependencyResult<()> {
        self.graph.execution_order()?;
        Ok(())
    }

    pub fn dependencies_of(&self, name: &str) -> Option<&[String]> {
        self.graph.dependencies_of(name)
    }

    pub fn dependents_of(&self, name: &str) -> Vec<&str> {
        self.graph.dependents_of(name)
    }

    pub async fn process(
        &mut self,
        event: SessionEvent,
    ) -> DependencyResult<(SessionChainResult, SessionEvent)> {
        let mut result = SessionChainResult::new();
        let mut ctx = HandlerContext::new();
        let mut current_event = event;

        let handlers = self.graph.sorted_handlers()?;

        for handler in handlers {
            match handler.handle(&mut ctx, current_event).await {
                HandlerResult::Continue(e) => {
                    current_event = e;
                }
                HandlerResult::Cancel => {
                    result.cancelled = true;
                    return Err(DependencyError::HandlerNotFound(
                        "Event cancelled (no event to return)".to_string(),
                    ));
                }
                HandlerResult::Cancelled(e) => {
                    result.cancelled = true;
                    return Ok((result, e));
                }
                HandlerResult::SoftError { event: e, error } => {
                    result.soft_errors.push(error);
                    current_event = e;
                }
                HandlerResult::FatalError(e) => {
                    result.fatal = true;
                    return Err(DependencyError::HandlerNotFound(format!(
                        "Fatal error: {}",
                        e
                    )));
                }
            }
        }

        Ok((result, current_event))
    }

    pub fn clear(&mut self) {
        self.graph.clear();
    }

    pub fn graph(&self) -> &SessionHandlerGraph {
        &self.graph
    }

    pub fn graph_mut(&mut self) -> &mut SessionHandlerGraph {
        &mut self.graph
    }
}

impl std::fmt::Debug for SessionHandlerChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHandlerChain")
            .field("handler_count", &self.graph.len())
            .field("graph", &self.graph)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use crucible_core::events::Handler;

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

    #[test]
    fn test_session_chain_empty() {
        let chain = SessionHandlerChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
    }

    #[test]
    fn test_session_chain_add_handler() {
        let mut chain = SessionHandlerChain::new();
        chain
            .add_handler(Box::new(TestHandler {
                name: "test",
                deps: &[],
            }))
            .unwrap();

        assert!(!chain.is_empty());
        assert_eq!(chain.len(), 1);
        assert!(chain.contains("test"));
    }

    #[test]
    fn test_session_chain_execution_order() {
        let mut chain = SessionHandlerChain::new();

        chain
            .add_handler(Box::new(TestHandler {
                name: "C",
                deps: &["B"],
            }))
            .unwrap();
        chain
            .add_handler(Box::new(TestHandler {
                name: "B",
                deps: &["A"],
            }))
            .unwrap();
        chain
            .add_handler(Box::new(TestHandler {
                name: "A",
                deps: &[],
            }))
            .unwrap();

        let order = chain.execution_order().unwrap();
        assert_eq!(order, vec!["A", "B", "C"]);
    }

    #[tokio::test]
    async fn test_session_chain_process() {
        let mut chain = SessionHandlerChain::new();
        chain
            .add_handler(Box::new(TestHandler {
                name: "handler1",
                deps: &[],
            }))
            .unwrap();

        let event = SessionEvent::MessageReceived {
            content: "test".into(),
            participant_id: "user".into(),
        };

        let (result, _) = chain.process(event).await.unwrap();
        assert!(result.is_ok());
        assert!(!result.has_errors());
    }
}
