//! Main watch manager that coordinates all file watching activities.

use crate::{
    traits::{FileWatcher, WatchConfig, WatchHandle, EventHandler},
    backends::{ExtendedBackendRegistry, WatcherRequirements},
    handlers::{HandlerRegistry, create_default_handlers},
    config::WatchManagerConfig,
    error::{Error, Result},
    events::{FileEvent, FileEventKind, EventFilter},
    utils::{Debouncer, EventQueue, PerformanceMonitor},
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Main manager for file watching operations.
pub struct WatchManager {
    /// Manager configuration
    config: WatchManagerConfig,
    /// Backend registry
    backend_registry: ExtendedBackendRegistry,
    /// Active watchers
    watchers: Arc<RwLock<HashMap<String, Arc<dyn FileWatcher>>>>,
    /// Event handlers
    handlers: Arc<RwLock<HandlerRegistry>>,
    /// Event queue for processing
    event_queue: Arc<Mutex<EventQueue>>,
    /// Debouncer for events
    debouncer: Arc<Mutex<Debouncer>>,
    /// Performance monitor
    performance_monitor: Arc<Mutex<PerformanceMonitor>>,
    /// Event processing task
    processor_task: Option<JoinHandle<()>>,
    /// Event sender
    event_sender: Option<mpsc::UnboundedSender<FileEvent>>,
    /// Event receiver
    event_receiver: Option<mpsc::UnboundedReceiver<FileEvent>>,
    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,
    /// Running state
    is_running: Arc<RwLock<bool>>,
}

impl WatchManager {
    /// Create a new watch manager.
    pub async fn new(config: WatchManagerConfig) -> Result<Self> {
        let mut manager = Self {
            config: config.clone(),
            backend_registry: ExtendedBackendRegistry::new(),
            watchers: Arc::new(RwLock::new(HashMap::new())),
            handlers: Arc::new(RwLock::new(HandlerRegistry::new())),
            event_queue: Arc::new(Mutex::new(EventQueue::new(config.queue_capacity))),
            debouncer: Arc::new(Mutex::new(Debouncer::new(config.debounce_delay))),
            performance_monitor: Arc::new(Mutex::new(PerformanceMonitor::new())),
            processor_task: None,
            event_sender: None,
            event_receiver: None,
            shutdown_tx: None,
            is_running: Arc::new(RwLock::new(false)),
        };

        // Initialize default handlers if enabled
        if config.enable_default_handlers {
            let default_handlers = create_default_handlers()?;
            let mut handlers = manager.handlers.write().await;
            for handler in default_handlers.handlers() {
                handlers.register(handler.clone());
            }
        }

        Ok(manager)
    }

    /// Start the watch manager.
    pub async fn start(&mut self) -> Result<()> {
        let mut is_running = self.is_running.write().await;
        if *is_running {
            return Err(Error::AlreadyRunning);
        }

        info!("Starting watch manager");

        // Create event channels
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        self.event_sender = Some(event_sender);
        self.event_receiver = Some(event_receiver);

        // Start event processing task
        self.start_event_processor().await?;

        *is_running = true;
        info!("Watch manager started successfully");

        Ok(())
    }

    /// Stop the watch manager.
    pub async fn shutdown(&mut self) -> Result<()> {
        let mut is_running = self.is_running.write().await;
        if !*is_running {
            return Ok(());
        }

        info!("Shutting down watch manager");

        // Send shutdown signal
        if let Some(ref shutdown_tx) = self.shutdown_tx {
            let _ = shutdown_tx.send(()).await;
        }

        // Wait for processor task to finish
        if let Some(task) = self.processor_task.take() {
            let _ = task.await;
        }

        // Stop all watchers
        let mut watchers = self.watchers.write().await;
        for (id, watcher) in watchers.drain() {
            debug!("Stopping watcher: {}", id);
            // Note: Watchers should implement proper cleanup
        }

        *is_running = false;
        info!("Watch manager shutdown complete");

        Ok(())
    }

    /// Add a watch for the specified path.
    pub async fn add_watch(&mut self, path: PathBuf, config: WatchConfig) -> Result<WatchHandle> {
        debug!("Adding watch for: {}", path.display());

        if !*self.is_running.read().await {
            return Err(Error::NotRunning);
        }

        // Select appropriate backend
        let requirements = WatcherRequirements::high_performance(); // Could be configurable
        let watcher = self.backend_registry.create_optimal_watcher(&requirements).await?;

        // Add the watch
        let handle = {
            let mut watcher_mut = Arc::try_unwrap(watcher)
                .map_err(|_| Error::Internal("Cannot unwrap watcher".to_string()))?;
            watcher_mut.watch(path.clone(), config.clone()).await?
        };

        // Store the watcher
        let mut watchers = self.watchers.write().await;
        watchers.insert(config.id.clone(), watcher);

        info!("Added watch: {} -> {}", config.id, path.display());
        Ok(handle)
    }

    /// Remove a watch.
    pub async fn remove_watch(&mut self, handle: WatchHandle) -> Result<()> {
        debug!("Removing watch for: {}", handle.path.display());

        let mut watchers = self.watchers.write().await;
        let mut removed = false;

        watchers.retain(|id, watcher| {
            // Check if this watcher handles the path
            // This is a simplified check - in practice, you'd track handles better
            let handles = watcher.active_watches();
            if handles.contains(&handle) {
                removed = true;
                false
            } else {
                true
            }
        });

        if removed {
            info!("Removed watch: {}", handle.path.display());
        } else {
            warn!("Watch not found: {}", handle.path.display());
        }

        Ok(())
    }

    /// Register an event handler.
    pub async fn register_handler(&self, handler: Arc<dyn EventHandler>) -> Result<()> {
        let mut handlers = self.handlers.write().await;
        handlers.register(handler.clone());
        info!("Registered event handler: {}", handler.name());
        Ok(())
    }

    /// Unregister an event handler.
    pub async fn unregister_handler(&self, name: &str) -> bool {
        let mut handlers = self.handlers.write().await;
        let removed = handlers.unregister(name);
        if removed {
            info!("Unregistered event handler: {}", name);
        }
        removed
    }

    /// Get performance statistics.
    pub async fn get_performance_stats(&self) -> PerformanceStats {
        let monitor = self.performance_monitor.lock().await;
        monitor.get_stats()
    }

    /// Get manager status.
    pub async fn get_status(&self) -> ManagerStatus {
        let is_running = *self.is_running.read().await;
        let watchers_count = self.watchers.read().await.len();
        let handlers_count = self.handlers.read().await.len();
        let queue_stats = self.event_queue.lock().await.get_stats();

        ManagerStatus {
            is_running,
            active_watches: watchers_count,
            registered_handlers: handlers_count,
            queue_stats,
        }
    }

    /// Start the event processing task.
    async fn start_event_processor(&mut self) -> Result<()> {
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        let event_receiver = self.event_receiver.take()
            .ok_or_else(|| Error::Internal("Event receiver not available".to_string()))?;

        let handlers = Arc::clone(&self.handlers);
        let event_queue = Arc::clone(&self.event_queue);
        let debouncer = Arc::clone(&self.debouncer);
        let performance_monitor = Arc::clone(&self.performance_monitor);

        let task = tokio::spawn(async move {
            let mut receiver = event_receiver;
            let mut shutdown = shutdown_rx;

            loop {
                tokio::select! {
                    Some(event) = receiver.recv() => {
                        if let Err(e) = Self::process_event(
                            event,
                            &handlers,
                            &event_queue,
                            &debouncer,
                            &performance_monitor,
                        ).await {
                            error!("Error processing event: {}", e);
                        }
                    }
                    _ = shutdown.recv() => {
                        info!("Event processor shutting down");
                        break;
                    }
                }
            }
        });

        self.processor_task = Some(task);
        Ok(())
    }

    /// Process a single event through the pipeline.
    async fn process_event(
        event: FileEvent,
        handlers: &Arc<RwLock<HandlerRegistry>>,
        event_queue: &Arc<Mutex<EventQueue>>,
        debouncer: &Arc<Mutex<Debouncer>>,
        performance_monitor: &Arc<Mutex<PerformanceMonitor>>,
    ) -> Result<()> {
        let start_time = std::time::Instant::now();

        // Debounce event
        {
            let mut debouncer_guard = debouncer.lock().await;
            if let Some(debounced_event) = debouncer_guard.process_event(event.clone()).await {
                // Queue the debounced event
                let mut queue = event_queue.lock().await;
                queue.push(debounced_event).await?;
            } else {
                // Event was debounced (filtered out)
                return Ok(());
            }
        }

        // Process queued events
        let events_to_process = {
            let mut queue = event_queue.lock().await;
            queue.drain_all()
        };

        // Process each event through handlers
        let handlers_guard = handlers.read().await;
        for event in events_to_process {
            let matching_handlers = handlers_guard.get_handlers_for_event(&event);

            // Execute handlers concurrently
            let mut handler_tasks = Vec::new();
            for handler in matching_handlers {
                let event_clone = event.clone();
                let handler_clone = handler.clone();

                let task = tokio::spawn(async move {
                    let handler_start = std::time::Instant::now();
                    let result = handler_clone.handle(event_clone).await;
                    let duration = handler_start.elapsed();

                    (handler_clone.name(), result, duration)
                });

                handler_tasks.push(task);
            }

            // Wait for all handlers to complete
            for task in handler_tasks {
                match task.await {
                    Ok((handler_name, result, duration)) => {
                        match result {
                            Ok(()) => {
                                debug!("Handler '{}' completed in {:?}", handler_name, duration);
                            }
                            Err(e) => {
                                error!("Handler '{}' failed in {:?}: {}", handler_name, duration, e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Handler task panicked: {}", e);
                    }
                }
            }
        }

        // Update performance metrics
        let processing_time = start_time.elapsed();
        let mut monitor = performance_monitor.lock().await;
        monitor.record_event_processed(processing_time);

        Ok(())
    }
}

/// Performance statistics for the watch manager.
#[derive(Debug, Clone)]
pub struct PerformanceStats {
    /// Total events processed
    pub total_events_processed: u64,
    /// Average processing time per event
    pub avg_processing_time_ms: f64,
    /// Current queue size
    pub current_queue_size: usize,
    /// Maximum queue size observed
    pub max_queue_size: usize,
    /// Events dropped due to queue overflow
    pub events_dropped: u64,
    /// Handler statistics
    pub handler_stats: HashMap<String, HandlerStats>,
}

/// Statistics for individual handlers.
#[derive(Debug, Clone)]
pub struct HandlerStats {
    /// Total events handled
    pub total_events: u64,
    /// Average handling time
    pub avg_handling_time_ms: f64,
    /// Number of errors
    pub error_count: u64,
}

/// Status information for the watch manager.
#[derive(Debug, Clone)]
pub struct ManagerStatus {
    /// Whether the manager is running
    pub is_running: bool,
    /// Number of active watches
    pub active_watches: usize,
    /// Number of registered handlers
    pub registered_handlers: usize,
    /// Queue statistics
    pub queue_stats: QueueStats,
}

/// Statistics for the event queue.
#[derive(Debug, Clone)]
pub struct QueueStats {
    /// Current queue size
    pub current_size: usize,
    /// Maximum capacity
    pub capacity: usize,
    /// Number of events processed
    pub processed: u64,
    /// Number of events dropped
    pub dropped: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::composite::CompositeHandler;
    use crate::traits::{WatchConfig, DebounceConfig};

    #[tokio::test]
    async fn test_watch_manager_lifecycle() {
        let config = WatchManagerConfig::default();
        let mut manager = WatchManager::new(config).await.unwrap();

        // Test starting
        assert!(manager.start().await.is_ok());
        assert!(manager.get_status().await.is_running);

        // Test stopping
        assert!(manager.shutdown().await.is_ok());
        assert!(!manager.get_status().await.is_running);
    }

    #[tokio::test]
    async fn test_handler_registration() {
        let config = WatchManagerConfig::default();
        let manager = WatchManager::new(config).await.unwrap();

        let composite_handler = Arc::new(CompositeHandler::new(
            crate::handlers::composite::CoordinationStrategy::Sequential
        ));

        assert!(manager.register_handler(composite_handler.clone()).await.is_ok());
        assert!(manager.unregister_handler(composite_handler.name()).await);
        assert!(!manager.unregister_handler("nonexistent").await);
    }

    #[tokio::test]
    async fn test_performance_monitoring() {
        let config = WatchManagerConfig::default();
        let manager = WatchManager::new(config).await.unwrap();

        let stats = manager.get_performance_stats().await;
        assert_eq!(stats.total_events_processed, 0);

        let status = manager.get_status().await;
        assert_eq!(status.active_watches, 0);
        assert_eq!(status.registered_handlers, 0);
    }
}