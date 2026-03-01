//! Composite handler that manages multiple handlers with coordination.

#![allow(clippy::type_complexity)]

use crate::watch::{
    error::{Error, Result},
    events::FileEvent,
    traits::EventHandler,
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// Composite handler that coordinates multiple event handlers.
pub struct CompositeHandler {
    /// Registry of managed handlers
    handlers: Arc<RwLock<Vec<Arc<dyn EventHandler>>>>,
    /// Coordination strategy
    strategy: CoordinationStrategy,
    /// Handler state tracking
    handler_states: Arc<RwLock<std::collections::HashMap<String, HandlerState>>>,
}

/// Strategy for coordinating multiple handlers.
pub enum CoordinationStrategy {
    /// Run all handlers sequentially
    Sequential,
    /// Run all handlers concurrently
    Concurrent,
    /// Run handlers based on priority groups
    PriorityGroups,
    /// Custom coordination logic
    Custom(Box<dyn Fn(&FileEvent, &[Arc<dyn EventHandler>]) -> Vec<usize> + Send + Sync>),
}

impl std::fmt::Debug for CoordinationStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sequential => write!(f, "Sequential"),
            Self::Concurrent => write!(f, "Concurrent"),
            Self::PriorityGroups => write!(f, "PriorityGroups"),
            Self::Custom(_) => write!(f, "Custom(<function>)"),
        }
    }
}

impl Clone for CoordinationStrategy {
    fn clone(&self) -> Self {
        match self {
            Self::Sequential => Self::Sequential,
            Self::Concurrent => Self::Concurrent,
            Self::PriorityGroups => Self::PriorityGroups,
            Self::Custom(_) => Self::Sequential, // Fallback to Sequential for unclonable closures
        }
    }
}

/// State of an individual handler.
#[derive(Debug, Clone)]
pub struct HandlerState {
    /// Number of successful operations
    success_count: u64,
    /// Number of failed operations
    error_count: u64,
    /// Last execution time
    last_execution: Option<chrono::DateTime<chrono::Utc>>,
    /// Whether the handler is currently enabled
    enabled: bool,
}

impl CompositeHandler {
    /// Create a new composite handler.
    pub fn new(strategy: CoordinationStrategy) -> Self {
        Self {
            handlers: Arc::new(RwLock::new(Vec::new())),
            strategy,
            handler_states: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Add a handler to the composite.
    pub async fn add_handler(&self, handler: Arc<dyn EventHandler>) {
        let mut handlers = self.handlers.write().await;
        let name = handler.name().to_string();

        // Initialize handler state
        {
            let mut states = self.handler_states.write().await;
            states.insert(
                name.clone(),
                HandlerState {
                    success_count: 0,
                    error_count: 0,
                    last_execution: None,
                    enabled: true,
                },
            );
        }

        handlers.push(handler);
        info!("Added handler to composite");
    }

    /// Remove a handler by name.
    pub async fn remove_handler(&self, name: &str) -> bool {
        let mut handlers = self.handlers.write().await;
        let initial_len = handlers.len();

        handlers.retain(|h| h.name() != name);

        {
            let mut states = self.handler_states.write().await;
            states.remove(name);
        }

        let removed = initial_len != handlers.len();
        if removed {
            info!("Removed handler '{}' from composite", name);
        }

        removed
    }

    /// Enable or disable a handler.
    pub async fn set_handler_enabled(&self, name: &str, enabled: bool) -> Result<()> {
        let mut states = self.handler_states.write().await;
        if let Some(state) = states.get_mut(name) {
            state.enabled = enabled;
            info!(
                "Handler '{}' {}",
                name,
                if enabled { "enabled" } else { "disabled" }
            );
            Ok(())
        } else {
            Err(Error::Handler(format!("Handler '{}' not found", name)))
        }
    }

    /// Get handler states.
    pub async fn get_handler_states(&self) -> std::collections::HashMap<String, HandlerState> {
        self.handler_states.read().await.clone()
    }

    /// Get the number of managed handlers.
    pub async fn handler_count(&self) -> usize {
        self.handlers.read().await.len()
    }

    async fn execute_handler(
        &self,
        handler: &Arc<dyn EventHandler>,
        event: &FileEvent,
    ) -> Result<()> {
        let handler_name = handler.name();
        let start_time = std::time::Instant::now();

        let result = handler.handle(event.clone()).await;

        // Update handler state
        {
            let mut states = self.handler_states.write().await;
            if let Some(state) = states.get_mut(handler_name) {
                state.last_execution = Some(chrono::Utc::now());
                match result {
                    Ok(()) => state.success_count += 1,
                    Err(_) => state.error_count += 1,
                }
            }
        }

        let duration = start_time.elapsed();
        debug!("Handler '{}' executed in {:?}", handler_name, duration);

        result
    }

    async fn execute_sequential(&self, event: &FileEvent) -> Result<()> {
        let handlers = self.handlers.read().await;
        let states = self.handler_states.read().await;

        for handler in handlers.iter() {
            if let Some(state) = states.get(handler.name()) {
                if !state.enabled {
                    continue;
                }
            }

            if handler.can_handle(event) {
                if let Err(e) = self.execute_handler(handler, event).await {
                    error!("Handler '{}' failed: {}", handler.name(), e);
                    // Continue with other handlers even if one fails
                }
            }
        }

        Ok(())
    }

    async fn execute_concurrent(&self, event: &FileEvent) -> Result<()> {
        let handlers = self.handlers.read().await;
        let states = self.handler_states.read().await;

        let mut tasks = Vec::new();

        for handler in handlers.iter() {
            if let Some(state) = states.get(handler.name()) {
                if !state.enabled {
                    continue;
                }
            }

            if handler.can_handle(event) {
                let handler_clone = handler.clone();
                let event_clone = event.clone();
                let composite = self.clone();

                let task = tokio::spawn(async move {
                    composite
                        .execute_handler(&handler_clone, &event_clone)
                        .await
                });

                tasks.push(task);
            }
        }

        // Wait for all tasks to complete
        for task in tasks {
            if let Err(e) = task.await {
                error!("Handler task failed: {}", e);
            }
        }

        Ok(())
    }

    async fn execute_priority_groups(&self, event: &FileEvent) -> Result<()> {
        let handlers = self.handlers.read().await;
        let states = self.handler_states.read().await;

        // Group handlers by priority
        let mut priority_groups: std::collections::HashMap<u32, Vec<Arc<dyn EventHandler>>> =
            std::collections::HashMap::new();

        for handler in handlers.iter() {
            if let Some(state) = states.get(handler.name()) {
                if !state.enabled {
                    continue;
                }
            }

            if handler.can_handle(event) {
                let priority = handler.priority();
                priority_groups
                    .entry(priority)
                    .or_default()
                    .push(handler.clone());
            }
        }

        // Sort priorities in descending order (higher priority first)
        let mut priorities: Vec<u32> = priority_groups.keys().cloned().collect();
        priorities.sort_by(|a, b| b.cmp(a));

        // Execute each priority group
        for priority in priorities {
            if let Some(group) = priority_groups.get(&priority) {
                // Execute handlers in the same priority group concurrently
                let mut tasks = Vec::new();

                for handler in group {
                    let handler_clone = handler.clone();
                    let event_clone = event.clone();
                    let composite = self.clone();

                    let task = tokio::spawn(async move {
                        composite
                            .execute_handler(&handler_clone, &event_clone)
                            .await
                    });

                    tasks.push(task);
                }

                // Wait for all handlers in this priority group to complete
                for task in tasks {
                    if let Err(e) = task.await {
                        error!("Handler task failed: {}", e);
                    }
                }
            }
        }

        Ok(())
    }
}

impl Clone for CompositeHandler {
    fn clone(&self) -> Self {
        Self {
            handlers: Arc::clone(&self.handlers),
            strategy: self.strategy.clone(),
            handler_states: Arc::clone(&self.handler_states),
        }
    }
}

#[async_trait]
impl EventHandler for CompositeHandler {
    async fn handle(&self, event: FileEvent) -> Result<()> {
        debug!("Composite handler processing event: {:?}", event.kind);

        match &self.strategy {
            CoordinationStrategy::Sequential => self.execute_sequential(&event).await,
            CoordinationStrategy::Concurrent => self.execute_concurrent(&event).await,
            CoordinationStrategy::PriorityGroups => self.execute_priority_groups(&event).await,
            CoordinationStrategy::Custom(strategy_func) => {
                let handlers = self.handlers.read().await;
                let states = self.handler_states.read().await;

                // Get indices of handlers to execute
                let handler_indices = strategy_func(&event, &handlers);

                // Execute selected handlers
                for &index in &handler_indices {
                    if let Some(handler) = handlers.get(index) {
                        if let Some(state) = states.get(handler.name()) {
                            if !state.enabled {
                                continue;
                            }
                        }

                        if let Err(e) = self.execute_handler(handler, &event).await {
                            error!("Handler '{}' failed: {}", handler.name(), e);
                        }
                    }
                }

                Ok(())
            }
        }
    }

    fn name(&self) -> &'static str {
        "composite"
    }

    fn priority(&self) -> u32 {
        50 // Lower priority since it coordinates other handlers
    }

    fn can_handle(&self, event: &FileEvent) -> bool {
        // Check if any managed handler can handle the event
        // Note: This is a synchronous method, so we cannot await RwLock.
        // We'll use try_read to avoid blocking. If we can't acquire the lock,
        // we'll conservatively return true to allow the event to be processed.
        let Ok(handlers) = self.handlers.try_read() else {
            return true;
        };
        let Ok(states) = self.handler_states.try_read() else {
            return true;
        };

        handlers.iter().any(|handler| {
            if let Some(state) = states.get(handler.name()) {
                state.enabled && handler.can_handle(event)
            } else {
                false
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock handler for testing composite behavior.
    struct MockHandler {
        handler_name: &'static str,
    }

    impl MockHandler {
        fn new(name: &'static str) -> Self {
            Self { handler_name: name }
        }
    }

    #[async_trait]
    impl EventHandler for MockHandler {
        async fn handle(&self, _event: FileEvent) -> Result<()> {
            Ok(())
        }

        fn name(&self) -> &'static str {
            self.handler_name
        }
    }

    #[tokio::test]
    async fn new_has_zero_handlers() {
        let composite = CompositeHandler::new(CoordinationStrategy::Sequential);
        assert_eq!(composite.handler_count().await, 0);
    }

    #[tokio::test]
    async fn add_handler_increments_count() {
        let composite = CompositeHandler::new(CoordinationStrategy::Sequential);
        let mock: Arc<dyn EventHandler> = Arc::new(MockHandler::new("mock_a"));
        composite.add_handler(mock).await;
        assert_eq!(composite.handler_count().await, 1);
    }

    #[tokio::test]
    async fn remove_handler_by_name() {
        let composite = CompositeHandler::new(CoordinationStrategy::Sequential);
        let mock: Arc<dyn EventHandler> = Arc::new(MockHandler::new("removable"));
        composite.add_handler(mock).await;
        assert_eq!(composite.handler_count().await, 1);

        let removed = composite.remove_handler("removable").await;
        assert!(removed);
        assert_eq!(composite.handler_count().await, 0);
    }

    #[tokio::test]
    async fn remove_nonexistent_returns_false() {
        let composite = CompositeHandler::new(CoordinationStrategy::Sequential);
        let removed = composite.remove_handler("does_not_exist").await;
        assert!(!removed);
    }

    #[tokio::test]
    async fn enable_disable_handler() {
        let composite = CompositeHandler::new(CoordinationStrategy::Sequential);
        let mock: Arc<dyn EventHandler> = Arc::new(MockHandler::new("toggle_me"));
        composite.add_handler(mock).await;

        // Disable
        composite
            .set_handler_enabled("toggle_me", false)
            .await
            .unwrap();
        let states = composite.get_handler_states().await;
        assert!(!states["toggle_me"].enabled);

        // Re-enable
        composite
            .set_handler_enabled("toggle_me", true)
            .await
            .unwrap();
        let states = composite.get_handler_states().await;
        assert!(states["toggle_me"].enabled);
    }

    #[test]
    fn coordination_strategy_debug_all() {
        let variants: Vec<CoordinationStrategy> = vec![
            CoordinationStrategy::Sequential,
            CoordinationStrategy::Concurrent,
            CoordinationStrategy::PriorityGroups,
            CoordinationStrategy::Custom(Box::new(|_event, _handlers| vec![])),
        ];
        let expected_substrings = ["Sequential", "Concurrent", "PriorityGroups", "Custom"];
        for (variant, expected) in variants.iter().zip(expected_substrings.iter()) {
            let debug_str = format!("{:?}", variant);
            assert!(
                debug_str.contains(expected),
                "Debug output '{}' should contain '{}'",
                debug_str,
                expected
            );
        }
    }

    #[test]
    fn handler_name_is_composite() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let composite = CompositeHandler::new(CoordinationStrategy::Sequential);
            assert_eq!(composite.name(), "composite");
        });
    }

    #[test]
    fn handler_priority_is_50() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let composite = CompositeHandler::new(CoordinationStrategy::Sequential);
            assert_eq!(composite.priority(), 50);
        });
    }
}
