//! Handler Chain for Topo-Sorted Event Processing
//!
//! This module provides `HandlerChain<E>`, a structure that executes ring handlers
//! in topologically sorted order based on their declared dependencies.
//!
//! ## Design
//!
//! The handler chain:
//! - Wraps `HandlerGraph<E>` with execution logic
//! - Processes events through handlers in dependency order
//! - Supports fail-open semantics (non-fatal errors don't stop the chain)
//! - Collects emitted events from handlers
//! - Provides cancellation support
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rune::handler_chain::HandlerChain;
//! use crucible_rune::handler::BoxedRingHandler;
//! use std::sync::Arc;
//!
//! let mut chain: HandlerChain<MyEvent> = HandlerChain::new();
//!
//! // Add handlers (order doesn't matter - topo sort handles it)
//! chain.add_handler(Box::new(PersistHandler))?;
//! chain.add_handler(Box::new(ReactHandler))?;
//! chain.add_handler(Box::new(EmitHandler))?;
//!
//! // Process an event through the chain
//! let event = Arc::new(MyEvent::new());
//! let result = chain.process(event, 42).await;
//!
//! // Check result
//! assert!(result.is_ok());
//! for emitted in result.emitted() {
//!     println!("Emitted: {:?}", emitted);
//! }
//! ```

use crate::dependency_graph::{DependencyError, DependencyResult, HandlerGraph};
use crate::event_ring::EventRing;
use crate::handler::{BoxedRingHandler, RingHandlerContext, RingHandlerError};
use std::sync::Arc;

/// Result of processing an event through the handler chain.
#[derive(Debug)]
pub struct ChainResult<E> {
    /// Events emitted by handlers during processing.
    pub emitted: Vec<E>,
    /// Non-fatal errors that occurred (processing continued).
    pub errors: Vec<RingHandlerError>,
    /// Whether processing was cancelled by a handler.
    pub cancelled: bool,
    /// Whether a fatal error stopped the chain.
    pub fatal: bool,
}

impl<E> Default for ChainResult<E> {
    fn default() -> Self {
        Self {
            emitted: Vec::new(),
            errors: Vec::new(),
            cancelled: false,
            fatal: false,
        }
    }
}

impl<E> ChainResult<E> {
    /// Create a new empty result.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if processing completed successfully (no fatal errors, not cancelled).
    pub fn is_ok(&self) -> bool {
        !self.fatal && !self.cancelled
    }

    /// Check if processing was stopped early (fatal error or cancelled).
    pub fn stopped_early(&self) -> bool {
        self.fatal || self.cancelled
    }

    /// Get the emitted events.
    pub fn emitted(&self) -> &[E] {
        &self.emitted
    }

    /// Take the emitted events, leaving an empty vec.
    pub fn take_emitted(&mut self) -> Vec<E> {
        std::mem::take(&mut self.emitted)
    }

    /// Get the non-fatal errors.
    pub fn errors(&self) -> &[RingHandlerError] {
        &self.errors
    }

    /// Check if there were any errors (fatal or non-fatal).
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// A chain of handlers that processes events in topological order.
///
/// `HandlerChain<E>` wraps a `HandlerGraph<E>` and provides the execution logic
/// for processing events through handlers in dependency order. It supports:
///
/// - **Fail-open semantics**: Non-fatal errors are recorded but don't stop processing
/// - **Cancellation**: Handlers can cancel further processing via context
/// - **Event emission**: Handlers can emit new events collected in the result
/// - **Fatal errors**: Handlers can signal fatal errors that stop the chain
///
/// ## Thread Safety
///
/// The chain itself is not thread-safe for mutation (adding/removing handlers).
/// However, `process()` only requires `&self` and can be called concurrently
/// from multiple threads if handlers are thread-safe (which they must be per
/// the `RingHandler` trait bounds).
pub struct HandlerChain<E> {
    /// The handler graph (stores handlers and computes topo order).
    graph: HandlerGraph<E>,
}

impl<E> Default for HandlerChain<E> {
    fn default() -> Self {
        Self {
            graph: HandlerGraph::new(),
        }
    }
}

impl<E: Send + Sync> HandlerChain<E> {
    /// Create a new empty handler chain.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a handler chain from an existing handler graph.
    pub fn from_graph(graph: HandlerGraph<E>) -> Self {
        Self { graph }
    }

    /// Add a handler to the chain.
    ///
    /// The handler's dependencies are automatically extracted and used for
    /// topological ordering.
    ///
    /// # Errors
    ///
    /// Returns an error if a handler with the same name already exists.
    pub fn add_handler(&mut self, handler: BoxedRingHandler<E>) -> DependencyResult<()> {
        self.graph.add_handler(handler)
    }

    /// Remove a handler from the chain.
    ///
    /// # Errors
    ///
    /// Returns an error if the handler doesn't exist.
    pub fn remove_handler(&mut self, name: &str) -> DependencyResult<BoxedRingHandler<E>> {
        self.graph.remove_handler(name)
    }

    /// Get a handler by name.
    pub fn get_handler(&self, name: &str) -> Option<&BoxedRingHandler<E>> {
        self.graph.get_handler(name)
    }

    /// Check if a handler exists in the chain.
    pub fn contains(&self, name: &str) -> bool {
        self.graph.contains(name)
    }

    /// Get the number of handlers in the chain.
    pub fn len(&self) -> usize {
        self.graph.len()
    }

    /// Check if the chain is empty.
    pub fn is_empty(&self) -> bool {
        self.graph.is_empty()
    }

    /// Get handler names in execution order.
    ///
    /// # Errors
    ///
    /// Returns an error if there's a dependency cycle or missing dependency.
    pub fn execution_order(&mut self) -> DependencyResult<Vec<String>> {
        self.graph.execution_order()
    }

    /// Validate the handler chain.
    ///
    /// Checks that all declared dependencies exist and there are no cycles.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails.
    pub fn validate(&mut self) -> DependencyResult<()> {
        // This will detect cycles and missing dependencies
        self.graph.execution_order()?;
        Ok(())
    }

    /// Get the direct dependencies of a handler.
    pub fn dependencies_of(&self, name: &str) -> Option<&[String]> {
        self.graph.dependencies_of(name)
    }

    /// Get all handlers that depend on the given handler.
    pub fn dependents_of(&self, name: &str) -> Vec<&str> {
        self.graph.dependents_of(name)
    }

    /// Process an event through the handler chain.
    ///
    /// Handlers are invoked in topological order. Each handler receives:
    /// - A mutable context for emitting events and metadata
    /// - An `Arc<E>` reference to the event (cheap clone)
    /// - The sequence number
    ///
    /// # Behavior
    ///
    /// - **Non-fatal errors**: Recorded in result, processing continues
    /// - **Fatal errors**: Recorded in result, processing stops
    /// - **Cancellation**: If a handler calls `ctx.cancel()`, processing stops
    /// - **Emitted events**: Collected from all handlers that ran
    ///
    /// # Errors
    ///
    /// Returns `Err` if the dependency graph is invalid (cycle or missing dep).
    /// Handler errors are captured in `ChainResult`, not returned as `Err`.
    pub async fn process(&mut self, event: Arc<E>, seq: u64) -> DependencyResult<ChainResult<E>> {
        let mut result = ChainResult::new();
        let mut ctx = RingHandlerContext::new();

        // Get handlers in sorted order
        let handlers = self.graph.sorted_handlers()?;

        for handler in handlers {
            // Process the event
            match handler.handle(&mut ctx, Arc::clone(&event), seq).await {
                Ok(()) => {
                    // Collect emitted events
                    result.emitted.extend(ctx.take_emitted());

                    // Check for cancellation
                    if ctx.is_cancelled() {
                        result.cancelled = true;
                        break;
                    }
                }
                Err(e) => {
                    // Record the error
                    let is_fatal = e.is_fatal();
                    result.errors.push(e);

                    // Collect any events emitted before the error
                    result.emitted.extend(ctx.take_emitted());

                    if is_fatal {
                        result.fatal = true;
                        break;
                    }
                    // Non-fatal: continue with next handler
                }
            }

            // Reset context for next handler (but keep metadata for cross-handler state)
            // Note: We don't call ctx.reset() because metadata should persist
            // between handlers. Only emitted events are cleared (done above).
        }

        Ok(result)
    }

    /// Process an event without requiring mutable self.
    ///
    /// This version takes the handler graph by reference and computes the
    /// order each time. Use `process()` for better performance when the
    /// chain is stable.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the dependency graph is invalid.
    pub async fn process_uncached(
        &self,
        event: Arc<E>,
        seq: u64,
    ) -> DependencyResult<ChainResult<E>>
    where
        E: Clone,
    {
        let mut result = ChainResult::new();
        let mut ctx = RingHandlerContext::new();

        // Get execution order (uncached)
        let order = self.graph.dependency_graph().execution_order_uncached()?;

        for name in order {
            if let Some(handler) = self.graph.get_handler(&name) {
                match handler.handle(&mut ctx, Arc::clone(&event), seq).await {
                    Ok(()) => {
                        result.emitted.extend(ctx.take_emitted());

                        if ctx.is_cancelled() {
                            result.cancelled = true;
                            break;
                        }
                    }
                    Err(e) => {
                        let is_fatal = e.is_fatal();
                        result.errors.push(e);
                        result.emitted.extend(ctx.take_emitted());

                        if is_fatal {
                            result.fatal = true;
                            break;
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Process an event from the ring buffer by sequence number.
    ///
    /// This is the primary integration point between the handler chain and the
    /// ring buffer. It:
    /// 1. Gets the event from the ring by sequence number
    /// 2. Runs all handlers in topological order
    /// 3. Pushes emitted events back to the ring buffer
    ///
    /// # Arguments
    ///
    /// * `ring` - The event ring buffer
    /// * `seq` - Sequence number of the event to process
    ///
    /// # Returns
    ///
    /// Returns a tuple of:
    /// - `ChainResult<E>` with processing details
    /// - `Vec<u64>` with sequence numbers of newly emitted events
    ///
    /// # Errors
    ///
    /// - `Err(DependencyError::HandlerNotFound)` if event not found in ring
    /// - `Err(DependencyError::CycleDetected)` or other graph errors
    pub async fn process_from_ring(
        &mut self,
        ring: &EventRing<E>,
        seq: u64,
    ) -> DependencyResult<(ChainResult<E>, Vec<u64>)> {
        // Get event from ring
        let event = ring.get(seq).ok_or_else(|| {
            DependencyError::HandlerNotFound(format!("Event at sequence {} not found in ring", seq))
        })?;

        // Process through handler chain
        let mut result = self.process(event, seq).await?;

        // Push emitted events back to ring and collect their sequence numbers
        let emitted_seqs: Vec<u64> = result
            .take_emitted()
            .into_iter()
            .map(|e| ring.push(e))
            .collect();

        Ok((result, emitted_seqs))
    }

    /// Process an event from the ring buffer without caching.
    ///
    /// Like `process_from_ring`, but doesn't cache the topological order.
    /// Useful when the handler graph changes frequently.
    ///
    /// # Arguments
    ///
    /// * `ring` - The event ring buffer
    /// * `seq` - Sequence number of the event to process
    ///
    /// # Returns
    ///
    /// Same as `process_from_ring`.
    pub async fn process_from_ring_uncached(
        &self,
        ring: &EventRing<E>,
        seq: u64,
    ) -> DependencyResult<(ChainResult<E>, Vec<u64>)>
    where
        E: Clone,
    {
        // Get event from ring
        let event = ring.get(seq).ok_or_else(|| {
            DependencyError::HandlerNotFound(format!("Event at sequence {} not found in ring", seq))
        })?;

        // Process through handler chain
        let mut result = self.process_uncached(event, seq).await?;

        // Push emitted events back to ring and collect their sequence numbers
        let emitted_seqs: Vec<u64> = result
            .take_emitted()
            .into_iter()
            .map(|e| ring.push(e))
            .collect();

        Ok((result, emitted_seqs))
    }

    /// Clear all handlers from the chain.
    pub fn clear(&mut self) {
        self.graph.clear();
    }

    /// Access the underlying handler graph.
    pub fn graph(&self) -> &HandlerGraph<E> {
        &self.graph
    }

    /// Access the underlying handler graph mutably.
    pub fn graph_mut(&mut self) -> &mut HandlerGraph<E> {
        &mut self.graph
    }
}

impl<E> std::fmt::Debug for HandlerChain<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandlerChain")
            .field("handler_count", &self.graph.len())
            .field("graph", &self.graph)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency_graph::DependencyError;
    use crate::handler::{RingHandler, RingHandlerResult};
    use async_trait::async_trait;

    /// Test handler that emits a transformed event.
    struct EmitHandler {
        name: &'static str,
        deps: &'static [&'static str],
    }

    #[async_trait]
    impl RingHandler<String> for EmitHandler {
        fn name(&self) -> &str {
            self.name
        }

        fn depends_on(&self) -> &[&str] {
            self.deps
        }

        async fn handle(
            &self,
            ctx: &mut RingHandlerContext<String>,
            event: Arc<String>,
            seq: u64,
        ) -> RingHandlerResult<()> {
            ctx.emit(format!("{}:{}:{}", self.name, seq, event));
            Ok(())
        }
    }

    /// Test handler that cancels processing.
    struct CancelHandler;

    #[async_trait]
    impl RingHandler<String> for CancelHandler {
        fn name(&self) -> &str {
            "cancel"
        }

        async fn handle(
            &self,
            ctx: &mut RingHandlerContext<String>,
            _event: Arc<String>,
            _seq: u64,
        ) -> RingHandlerResult<()> {
            ctx.cancel();
            Ok(())
        }
    }

    /// Test handler that returns a non-fatal error.
    struct NonFatalErrorHandler;

    #[async_trait]
    impl RingHandler<String> for NonFatalErrorHandler {
        fn name(&self) -> &str {
            "non_fatal"
        }

        async fn handle(
            &self,
            _ctx: &mut RingHandlerContext<String>,
            _event: Arc<String>,
            _seq: u64,
        ) -> RingHandlerResult<()> {
            Err(RingHandlerError::non_fatal(
                "non_fatal",
                "intentional error",
            ))
        }
    }

    #[tokio::test]
    async fn test_chain_empty() {
        let mut chain: HandlerChain<String> = HandlerChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);

        let event = Arc::new("test".to_string());
        let result = chain.process(event, 0).await.unwrap();

        assert!(result.is_ok());
        assert!(result.emitted().is_empty());
        assert!(!result.has_errors());
    }

    #[tokio::test]
    async fn test_chain_single_handler() {
        let mut chain: HandlerChain<String> = HandlerChain::new();

        chain
            .add_handler(Box::new(EmitHandler {
                name: "test",
                deps: &[],
            }))
            .unwrap();

        assert_eq!(chain.len(), 1);
        assert!(chain.contains("test"));

        let event = Arc::new("hello".to_string());
        let result = chain.process(event, 42).await.unwrap();

        assert!(result.is_ok());
        assert_eq!(result.emitted(), &["test:42:hello"]);
    }

    #[tokio::test]
    async fn test_chain_topo_order() {
        let mut chain: HandlerChain<String> = HandlerChain::new();

        // Add in reverse order
        chain
            .add_handler(Box::new(EmitHandler {
                name: "C",
                deps: &["B"],
            }))
            .unwrap();
        chain
            .add_handler(Box::new(EmitHandler {
                name: "B",
                deps: &["A"],
            }))
            .unwrap();
        chain
            .add_handler(Box::new(EmitHandler {
                name: "A",
                deps: &[],
            }))
            .unwrap();

        let event = Arc::new("x".to_string());
        let result = chain.process(event, 1).await.unwrap();

        assert!(result.is_ok());
        // Should be in A, B, C order
        assert_eq!(result.emitted(), &["A:1:x", "B:1:x", "C:1:x"]);
    }

    #[tokio::test]
    async fn test_chain_cancellation() {
        let mut chain: HandlerChain<String> = HandlerChain::new();

        chain
            .add_handler(Box::new(EmitHandler {
                name: "first",
                deps: &[],
            }))
            .unwrap();
        chain.add_handler(Box::new(CancelHandler)).unwrap();
        chain
            .add_handler(Box::new(EmitHandler {
                name: "last",
                deps: &["cancel"],
            }))
            .unwrap();

        // Update cancel handler to depend on first
        chain.remove_handler("cancel").unwrap();
        chain.remove_handler("last").unwrap();

        // Re-add with proper dependencies
        struct CancelHandlerWithDeps;
        #[async_trait]
        impl RingHandler<String> for CancelHandlerWithDeps {
            fn name(&self) -> &str {
                "cancel"
            }
            fn depends_on(&self) -> &[&str] {
                &["first"]
            }
            async fn handle(
                &self,
                ctx: &mut RingHandlerContext<String>,
                _event: Arc<String>,
                _seq: u64,
            ) -> RingHandlerResult<()> {
                ctx.cancel();
                Ok(())
            }
        }

        chain.add_handler(Box::new(CancelHandlerWithDeps)).unwrap();
        chain
            .add_handler(Box::new(EmitHandler {
                name: "last",
                deps: &["cancel"],
            }))
            .unwrap();

        let event = Arc::new("test".to_string());
        let result = chain.process(event, 0).await.unwrap();

        assert!(result.cancelled);
        assert!(result.stopped_early());
        // "last" should not have run
        assert_eq!(result.emitted(), &["first:0:test"]);
    }

    #[tokio::test]
    async fn test_chain_non_fatal_error() {
        let mut chain: HandlerChain<String> = HandlerChain::new();

        chain
            .add_handler(Box::new(EmitHandler {
                name: "first",
                deps: &[],
            }))
            .unwrap();
        chain.add_handler(Box::new(NonFatalErrorHandler)).unwrap();

        // Add handler after error with proper deps
        struct LastHandler;
        #[async_trait]
        impl RingHandler<String> for LastHandler {
            fn name(&self) -> &str {
                "last"
            }
            fn depends_on(&self) -> &[&str] {
                &["non_fatal"]
            }
            async fn handle(
                &self,
                ctx: &mut RingHandlerContext<String>,
                event: Arc<String>,
                seq: u64,
            ) -> RingHandlerResult<()> {
                ctx.emit(format!("last:{}:{}", seq, event));
                Ok(())
            }
        }

        // Update non_fatal to depend on first
        chain.remove_handler("non_fatal").unwrap();
        struct NonFatalWithDeps;
        #[async_trait]
        impl RingHandler<String> for NonFatalWithDeps {
            fn name(&self) -> &str {
                "non_fatal"
            }
            fn depends_on(&self) -> &[&str] {
                &["first"]
            }
            async fn handle(
                &self,
                _ctx: &mut RingHandlerContext<String>,
                _event: Arc<String>,
                _seq: u64,
            ) -> RingHandlerResult<()> {
                Err(RingHandlerError::non_fatal("non_fatal", "intentional"))
            }
        }

        chain.add_handler(Box::new(NonFatalWithDeps)).unwrap();
        chain.add_handler(Box::new(LastHandler)).unwrap();

        let event = Arc::new("test".to_string());
        let result = chain.process(event, 0).await.unwrap();

        // Processing should continue despite non-fatal error
        assert!(result.is_ok()); // Not fatal
        assert!(result.has_errors());
        assert_eq!(result.errors().len(), 1);
        // "last" should have run
        assert_eq!(result.emitted(), &["first:0:test", "last:0:test"]);
    }

    #[tokio::test]
    async fn test_chain_fatal_error() {
        let mut chain: HandlerChain<String> = HandlerChain::new();

        chain
            .add_handler(Box::new(EmitHandler {
                name: "first",
                deps: &[],
            }))
            .unwrap();

        // Fatal handler with deps
        struct FatalWithDeps;
        #[async_trait]
        impl RingHandler<String> for FatalWithDeps {
            fn name(&self) -> &str {
                "fatal"
            }
            fn depends_on(&self) -> &[&str] {
                &["first"]
            }
            async fn handle(
                &self,
                _ctx: &mut RingHandlerContext<String>,
                _event: Arc<String>,
                _seq: u64,
            ) -> RingHandlerResult<()> {
                Err(RingHandlerError::fatal("fatal", "critical"))
            }
        }

        chain.add_handler(Box::new(FatalWithDeps)).unwrap();
        chain
            .add_handler(Box::new(EmitHandler {
                name: "last",
                deps: &["fatal"],
            }))
            .unwrap();

        let event = Arc::new("test".to_string());
        let result = chain.process(event, 0).await.unwrap();

        assert!(result.fatal);
        assert!(result.stopped_early());
        assert!(!result.is_ok());
        // "last" should NOT have run
        assert_eq!(result.emitted(), &["first:0:test"]);
    }

    #[tokio::test]
    async fn test_chain_cycle_detection() {
        let mut chain: HandlerChain<String> = HandlerChain::new();

        chain
            .add_handler(Box::new(EmitHandler {
                name: "A",
                deps: &["B"],
            }))
            .unwrap();
        chain
            .add_handler(Box::new(EmitHandler {
                name: "B",
                deps: &["A"],
            }))
            .unwrap();

        let event = Arc::new("test".to_string());
        let result = chain.process(event, 0).await;

        assert!(matches!(result, Err(DependencyError::CycleDetected { .. })));
    }

    #[tokio::test]
    async fn test_chain_unknown_dependency() {
        let mut chain: HandlerChain<String> = HandlerChain::new();

        chain
            .add_handler(Box::new(EmitHandler {
                name: "A",
                deps: &["nonexistent"],
            }))
            .unwrap();

        let event = Arc::new("test".to_string());
        let result = chain.process(event, 0).await;

        assert!(matches!(
            result,
            Err(DependencyError::UnknownDependency { .. })
        ));
    }

    #[tokio::test]
    async fn test_chain_validate() {
        let mut chain: HandlerChain<String> = HandlerChain::new();

        chain
            .add_handler(Box::new(EmitHandler {
                name: "A",
                deps: &[],
            }))
            .unwrap();
        chain
            .add_handler(Box::new(EmitHandler {
                name: "B",
                deps: &["A"],
            }))
            .unwrap();

        assert!(chain.validate().is_ok());

        // Add a handler with unknown dependency
        chain
            .add_handler(Box::new(EmitHandler {
                name: "C",
                deps: &["unknown"],
            }))
            .unwrap();

        assert!(chain.validate().is_err());
    }

    #[tokio::test]
    async fn test_chain_execution_order() {
        let mut chain: HandlerChain<String> = HandlerChain::new();

        chain
            .add_handler(Box::new(EmitHandler {
                name: "C",
                deps: &["B"],
            }))
            .unwrap();
        chain
            .add_handler(Box::new(EmitHandler {
                name: "A",
                deps: &[],
            }))
            .unwrap();
        chain
            .add_handler(Box::new(EmitHandler {
                name: "B",
                deps: &["A"],
            }))
            .unwrap();

        let order = chain.execution_order().unwrap();
        assert_eq!(order, vec!["A", "B", "C"]);
    }

    #[tokio::test]
    async fn test_chain_clear() {
        let mut chain: HandlerChain<String> = HandlerChain::new();

        chain
            .add_handler(Box::new(EmitHandler {
                name: "test",
                deps: &[],
            }))
            .unwrap();

        assert_eq!(chain.len(), 1);

        chain.clear();

        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
    }

    #[tokio::test]
    async fn test_chain_from_graph() {
        let mut graph: HandlerGraph<String> = HandlerGraph::new();
        graph
            .add_handler(Box::new(EmitHandler {
                name: "test",
                deps: &[],
            }))
            .unwrap();

        let mut chain = HandlerChain::from_graph(graph);

        assert_eq!(chain.len(), 1);

        let event = Arc::new("hello".to_string());
        let result = chain.process(event, 0).await.unwrap();

        assert_eq!(result.emitted(), &["test:0:hello"]);
    }

    #[tokio::test]
    async fn test_chain_process_uncached() {
        let mut chain: HandlerChain<String> = HandlerChain::new();

        chain
            .add_handler(Box::new(EmitHandler {
                name: "A",
                deps: &[],
            }))
            .unwrap();
        chain
            .add_handler(Box::new(EmitHandler {
                name: "B",
                deps: &["A"],
            }))
            .unwrap();

        let event = Arc::new("test".to_string());
        let result = chain.process_uncached(event, 5).await.unwrap();

        assert!(result.is_ok());
        assert_eq!(result.emitted(), &["A:5:test", "B:5:test"]);
    }

    #[test]
    fn test_chain_debug() {
        let chain: HandlerChain<String> = HandlerChain::new();
        let debug = format!("{:?}", chain);
        assert!(debug.contains("HandlerChain"));
        assert!(debug.contains("handler_count: 0"));
    }

    #[test]
    fn test_chain_result_default() {
        let result: ChainResult<String> = ChainResult::new();

        assert!(result.is_ok());
        assert!(!result.stopped_early());
        assert!(!result.cancelled);
        assert!(!result.fatal);
        assert!(result.emitted().is_empty());
        assert!(!result.has_errors());
    }

    #[test]
    fn test_chain_result_take_emitted() {
        let mut result: ChainResult<String> = ChainResult::new();
        result.emitted.push("event1".to_string());
        result.emitted.push("event2".to_string());

        let taken = result.take_emitted();
        assert_eq!(taken, vec!["event1", "event2"]);
        assert!(result.emitted().is_empty());
    }

    // ========================
    // Ring Integration Tests
    // ========================

    use crate::event_ring::EventRing;

    #[tokio::test]
    async fn test_process_from_ring_runs_handlers_in_order() {
        let ring: EventRing<String> = EventRing::new(16);
        let mut chain: HandlerChain<String> = HandlerChain::new();

        // Add handlers in reverse order - topo sort should fix it
        chain
            .add_handler(Box::new(EmitHandler {
                name: "C",
                deps: &["B"],
            }))
            .unwrap();
        chain
            .add_handler(Box::new(EmitHandler {
                name: "B",
                deps: &["A"],
            }))
            .unwrap();
        chain
            .add_handler(Box::new(EmitHandler {
                name: "A",
                deps: &[],
            }))
            .unwrap();

        // Push an event to the ring
        let seq = ring.push("input".to_string());
        assert_eq!(seq, 0);

        // Process it
        let (result, emitted_seqs) = chain.process_from_ring(&ring, seq).await.unwrap();

        // Result should be ok
        assert!(result.is_ok());

        // Should have emitted 3 events (one per handler) in topo order
        assert_eq!(emitted_seqs.len(), 3);
        assert_eq!(emitted_seqs, vec![1, 2, 3]); // Sequence numbers 1, 2, 3

        // Verify emitted events are in the ring and in correct order
        assert_eq!(*ring.get(1).unwrap(), "A:0:input");
        assert_eq!(*ring.get(2).unwrap(), "B:0:input");
        assert_eq!(*ring.get(3).unwrap(), "C:0:input");
    }

    #[tokio::test]
    async fn test_process_from_ring_emits_to_ring() {
        let ring: EventRing<String> = EventRing::new(16);
        let mut chain: HandlerChain<String> = HandlerChain::new();

        // Single handler that emits
        chain
            .add_handler(Box::new(EmitHandler {
                name: "emitter",
                deps: &[],
            }))
            .unwrap();

        // Push initial event
        let seq = ring.push("original".to_string());

        // Process it
        let (_, emitted_seqs) = chain.process_from_ring(&ring, seq).await.unwrap();

        // Should have pushed one event back to ring
        assert_eq!(emitted_seqs.len(), 1);
        assert_eq!(emitted_seqs[0], 1);

        // Verify it's in the ring
        let emitted = ring.get(1).unwrap();
        assert_eq!(*emitted, "emitter:0:original");

        // Ring now has 2 events
        assert_eq!(ring.len(), 2);
    }

    #[tokio::test]
    async fn test_process_from_ring_event_not_found() {
        let ring: EventRing<String> = EventRing::new(16);
        let mut chain: HandlerChain<String> = HandlerChain::new();

        chain
            .add_handler(Box::new(EmitHandler {
                name: "test",
                deps: &[],
            }))
            .unwrap();

        // Try to process non-existent event
        let result = chain.process_from_ring(&ring, 99).await;

        assert!(matches!(result, Err(DependencyError::HandlerNotFound(_))));
    }

    #[tokio::test]
    async fn test_process_from_ring_uncached() {
        let ring: EventRing<String> = EventRing::new(16);
        let mut chain: HandlerChain<String> = HandlerChain::new();

        chain
            .add_handler(Box::new(EmitHandler {
                name: "A",
                deps: &[],
            }))
            .unwrap();
        chain
            .add_handler(Box::new(EmitHandler {
                name: "B",
                deps: &["A"],
            }))
            .unwrap();

        let seq = ring.push("test".to_string());

        // Use uncached version
        let (result, emitted_seqs) = chain.process_from_ring_uncached(&ring, seq).await.unwrap();

        assert!(result.is_ok());
        assert_eq!(emitted_seqs.len(), 2);

        // Verify order
        assert_eq!(*ring.get(1).unwrap(), "A:0:test");
        assert_eq!(*ring.get(2).unwrap(), "B:0:test");
    }

    #[tokio::test]
    async fn test_process_from_ring_with_cancellation() {
        let ring: EventRing<String> = EventRing::new(16);
        let mut chain: HandlerChain<String> = HandlerChain::new();

        chain
            .add_handler(Box::new(EmitHandler {
                name: "first",
                deps: &[],
            }))
            .unwrap();

        // Cancel handler
        struct CancelWithDeps;
        #[async_trait]
        impl RingHandler<String> for CancelWithDeps {
            fn name(&self) -> &str {
                "cancel"
            }
            fn depends_on(&self) -> &[&str] {
                &["first"]
            }
            async fn handle(
                &self,
                ctx: &mut RingHandlerContext<String>,
                _event: Arc<String>,
                _seq: u64,
            ) -> RingHandlerResult<()> {
                ctx.cancel();
                Ok(())
            }
        }
        chain.add_handler(Box::new(CancelWithDeps)).unwrap();

        chain
            .add_handler(Box::new(EmitHandler {
                name: "after_cancel",
                deps: &["cancel"],
            }))
            .unwrap();

        let seq = ring.push("test".to_string());
        let (result, emitted_seqs) = chain.process_from_ring(&ring, seq).await.unwrap();

        // Should be cancelled
        assert!(result.cancelled);

        // Only first handler's emit should be in ring (before cancellation)
        assert_eq!(emitted_seqs.len(), 1);
        assert_eq!(*ring.get(1).unwrap(), "first:0:test");
    }

    #[tokio::test]
    async fn test_process_from_ring_multiple_events() {
        let ring: EventRing<String> = EventRing::new(16);
        let mut chain: HandlerChain<String> = HandlerChain::new();

        chain
            .add_handler(Box::new(EmitHandler {
                name: "handler",
                deps: &[],
            }))
            .unwrap();

        // Process multiple events
        let seq1 = ring.push("event1".to_string());
        let seq2 = ring.push("event2".to_string());
        let seq3 = ring.push("event3".to_string());

        let (_, seqs1) = chain.process_from_ring(&ring, seq1).await.unwrap();
        let (_, seqs2) = chain.process_from_ring(&ring, seq2).await.unwrap();
        let (_, seqs3) = chain.process_from_ring(&ring, seq3).await.unwrap();

        // Each process emits one event
        assert_eq!(seqs1, vec![3]); // After event3
        assert_eq!(seqs2, vec![4]);
        assert_eq!(seqs3, vec![5]);

        // Verify ring contents
        assert_eq!(*ring.get(3).unwrap(), "handler:0:event1");
        assert_eq!(*ring.get(4).unwrap(), "handler:1:event2");
        assert_eq!(*ring.get(5).unwrap(), "handler:2:event3");
    }
}
