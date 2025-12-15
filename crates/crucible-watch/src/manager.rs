//! Main watch manager that coordinates all file watching activities.

use crate::{
    backends::{ExtendedBackendRegistry, WatcherRequirements},
    config::WatchManagerConfig,
    error::{Error, Result},
    events::FileEvent,
    handlers::{create_default_handlers, HandlerRegistry},
    traits::{EventHandler, FileWatcher, WatchConfig, WatchHandle},
    utils::{Debouncer, EventQueue, PerformanceMonitor, PerformanceStats, QueueStats},
};
use crucible_core::events::{EventEmitter, NoOpEmitter, SessionEvent};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Main manager for file watching operations.
pub struct WatchManager {
    /// Manager configuration
    #[allow(dead_code)]
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
    /// Event emitter for SessionEvent emission
    emitter: Arc<dyn EventEmitter<Event = SessionEvent>>,
}

impl WatchManager {
    /// Create a new watch manager with default NoOpEmitter.
    pub async fn new(config: WatchManagerConfig) -> Result<Self> {
        Self::with_emitter(config, Arc::new(NoOpEmitter::new())).await
    }

    /// Create a new watch manager with a custom event emitter.
    ///
    /// The emitter is used to emit `SessionEvent` variants (e.g., `FileChanged`,
    /// `FileDeleted`, `FileMoved`) when file system changes are detected.
    pub async fn with_emitter(
        config: WatchManagerConfig,
        emitter: Arc<dyn EventEmitter<Event = SessionEvent>>,
    ) -> Result<Self> {
        let manager = Self {
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
            emitter,
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

    /// Get a reference to the event emitter.
    pub fn emitter(&self) -> &Arc<dyn EventEmitter<Event = SessionEvent>> {
        &self.emitter
    }

    /// Start the watch manager.
    pub async fn start(&mut self) -> Result<()> {
        {
            let is_running = self.is_running.read().await;
            if *is_running {
                return Err(Error::AlreadyRunning);
            }
        }

        info!("Starting watch manager");

        // Create event channels
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        self.event_sender = Some(event_sender);
        self.event_receiver = Some(event_receiver);

        // Start event processing task
        self.start_event_processor().await?;

        {
            let mut is_running = self.is_running.write().await;
            *is_running = true;
        }

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
        for (id, _watcher) in watchers.drain() {
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

        // Get the event sender to pass to the watcher
        let event_sender = self
            .event_sender
            .as_ref()
            .ok_or_else(|| Error::Internal("Event sender not available".to_string()))?
            .clone();

        // Select appropriate backend
        let requirements = WatcherRequirements::high_performance(); // Could be configurable
        let mut watcher_arc = self
            .backend_registry
            .create_optimal_watcher(&requirements)
            .await?;

        // Try to get mutable access to the watcher to call watch()
        // SAFETY: This works because the Arc was just created by create_optimal_watcher()
        // and no other references exist yet. If this fails, it would indicate a bug in
        // the factory implementation that's cloning the Arc internally.
        //
        // TODO: A more robust approach would be to either:
        // 1. Pass event_sender to create_optimal_watcher() so it's set during construction
        // 2. Change WatcherBackend trait methods to take &self with interior mutability
        // For now, this is safe because we control the factory and know it doesn't clone.
        if let Some(watcher) = Arc::get_mut(&mut watcher_arc) {
            // Set the event sender so the watcher can send events to our processing pipeline
            watcher.set_event_sender(event_sender.clone());

            // Call watch to actually start monitoring the path
            let handle = watcher.watch(path.clone(), config.clone()).await?;

            // Store the watcher
            let mut watchers = self.watchers.write().await;
            watchers.insert(config.id.clone(), watcher_arc);

            info!("Added watch: {} -> {}", config.id, path.display());
            Ok(handle)
        } else {
            // If we can't get mutable access, return an error
            Err(Error::Internal(
                "Cannot get mutable access to watcher".to_string(),
            ))
        }
    }

    /// Remove a watch.
    pub async fn remove_watch(&mut self, handle: WatchHandle) -> Result<()> {
        debug!("Removing watch for: {}", handle.path.display());

        let mut watchers = self.watchers.write().await;
        let mut removed = false;

        watchers.retain(|_id, watcher| {
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
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        let event_receiver = self
            .event_receiver
            .take()
            .ok_or_else(|| Error::Internal("Event receiver not available".to_string()))?;

        let handlers = Arc::clone(&self.handlers);
        let event_queue = Arc::clone(&self.event_queue);
        let debouncer = Arc::clone(&self.debouncer);
        let performance_monitor = Arc::clone(&self.performance_monitor);

        let task = tokio::spawn(async move {
            let mut receiver = event_receiver;
            let mut shutdown = shutdown_rx;
            let mut flush_interval = tokio::time::interval(tokio::time::Duration::from_millis(50));

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
                    _ = flush_interval.tick() => {
                        // Periodically flush pending debounced events
                        if let Err(e) = Self::flush_debounced_events(
                            &handlers,
                            &event_queue,
                            &debouncer,
                            &performance_monitor,
                        ).await {
                            error!("Error flushing debounced events: {}", e);
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

    /// Flush debounced events that are ready to be processed.
    async fn flush_debounced_events(
        handlers: &Arc<RwLock<HandlerRegistry>>,
        event_queue: &Arc<Mutex<EventQueue>>,
        debouncer: &Arc<Mutex<Debouncer>>,
        performance_monitor: &Arc<Mutex<PerformanceMonitor>>,
    ) -> Result<()> {
        // Check for ready events
        let mut debouncer_guard = debouncer.lock().await;
        let now = std::time::Instant::now();

        // Manually check and emit ready events
        let ready_event = debouncer_guard.check_ready_events(now).await;
        drop(debouncer_guard);

        if let Some(event) = ready_event {
            // Queue and process the ready event
            {
                let mut queue = event_queue.lock().await;
                queue.push(event).await?;
            }

            // Process the queued events
            Self::process_queued_events(handlers, event_queue, performance_monitor).await?;
        }

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

        // Process the queued events
        Self::process_queued_events(handlers, event_queue, performance_monitor).await
    }

    /// Process all queued events through handlers.
    async fn process_queued_events(
        handlers: &Arc<RwLock<HandlerRegistry>>,
        event_queue: &Arc<Mutex<EventQueue>>,
        performance_monitor: &Arc<Mutex<PerformanceMonitor>>,
    ) -> Result<()> {
        let start_time = std::time::Instant::now();

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
                    Ok((handler_name, result, duration)) => match result {
                        Ok(()) => {
                            debug!("Handler '{}' completed in {:?}", handler_name, duration);
                        }
                        Err(e) => {
                            error!("Handler '{}' failed in {:?}: {}", handler_name, duration, e);
                        }
                    },
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
