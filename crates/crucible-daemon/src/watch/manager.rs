//! Main watch manager that coordinates all file watching activities.

use crate::watch::{
    backends::NotifyWatcher,
    error::{Error, Result},
    events::FileEvent,
    handlers::{create_default_handlers, HandlerRegistry},
    traits::{DebounceConfig, EventHandler, WatchConfig, WatchHandle},
    utils::{Debouncer, EventQueue},
};
use crucible_core::events::{EventEmitter, NoOpEmitter, SessionEvent};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// How the watch manager is built: queue depth, debounce, default handlers.
///
/// This is all that survived `watch/config.rs`. That file held 23 types
/// describing a `[watch]` configuration section — backpressure strategies,
/// CPU and memory budgets, metric export, time-window filters — that no user
/// could ever set: `CliAppConfig` has no `watch` field, nothing constructed
/// `config::WatchConfig`, and no `[watch]` section appeared in any doc or TOML.
/// It also declared its own `WatchConfig` and `DebounceConfig`, colliding with
/// the live ones in `traits.rs`, which is why `watch/mod.rs` used to alias them
/// apart as `ConfigWatchConfig` and `TraitWatchConfig`.
#[derive(Debug, Clone)]
pub struct WatchManagerConfig {
    /// Queue capacity
    pub queue_capacity: usize,
    /// Debounce settings for the manager's event debouncer.
    pub debounce: DebounceConfig,
    /// Enable default handlers
    pub enable_default_handlers: bool,
}

impl Default for WatchManagerConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 10000,
            debounce: DebounceConfig::default(),
            enable_default_handlers: true,
        }
    }
}

/// Main manager for file watching operations.
pub struct WatchManager {
    /// Active watchers
    watchers: Arc<RwLock<HashMap<String, NotifyWatcher>>>,
    /// Event handlers
    handlers: Arc<RwLock<HandlerRegistry>>,
    /// Event queue for processing
    event_queue: Arc<Mutex<EventQueue>>,
    /// Debouncer for events
    debouncer: Arc<Mutex<Debouncer>>,
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
            watchers: Arc::new(RwLock::new(HashMap::new())),
            handlers: Arc::new(RwLock::new(HandlerRegistry::new())),
            event_queue: Arc::new(Mutex::new(EventQueue::new(config.queue_capacity))),
            debouncer: Arc::new(Mutex::new(Debouncer::new(config.debounce.clone()))),
            processor_task: None,
            event_sender: None,
            event_receiver: None,
            shutdown_tx: None,
            is_running: Arc::new(RwLock::new(false)),
            emitter,
        };

        // Initialize default handlers if enabled. They get the manager's own
        // emitter — a handler holding the default `NoOpEmitter` swallows every
        // event it handles, which is indistinguishable from not being
        // registered at all.
        if config.enable_default_handlers {
            let default_handlers = create_default_handlers(Arc::clone(&manager.emitter))?;
            let mut handlers = manager.handlers.write().await;
            for handler in default_handlers.handlers() {
                handlers.register(handler.clone());
            }
        }

        Ok(manager)
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

        // Preserve the manager's supported platforms; there is no polling fallback.
        if !matches!(std::env::consts::OS, "linux" | "macos" | "windows") {
            return Err(Error::BackendUnavailable(
                "Native watching is unsupported on this platform".into(),
            ));
        }
        let mut watcher = NotifyWatcher::new();
        watcher.set_event_sender(event_sender);

        let handle = watcher.watch(path.clone(), config.clone()).await?;

        let mut watchers = self.watchers.write().await;
        watchers.insert(config.id.clone(), watcher);

        info!("Added watch: {} -> {}", config.id, path.display());
        Ok(handle)
    }

    /// Add many paths to one backend, registered together under `group_id`.
    ///
    /// [`Self::add_watch`] builds a backend per call, and each notify backend
    /// owns an inotify instance — a resource Linux caps at 128 per user. A
    /// plan that covers a repository directory by directory is hundreds of
    /// paths, so they have to share one instance or the cap is the limit on
    /// how many repositories a daemon can watch at all.
    ///
    /// Each entry is `(path, recursive)`. Remove the whole group with
    /// [`Self::remove_watch_group`].
    pub async fn add_watch_group(
        &mut self,
        group_id: &str,
        paths: &[(PathBuf, bool)],
        template: &WatchConfig,
    ) -> Result<usize> {
        if !*self.is_running.read().await {
            return Err(Error::NotRunning);
        }
        if paths.is_empty() {
            return Ok(0);
        }

        let event_sender = self
            .event_sender
            .as_ref()
            .ok_or_else(|| Error::Internal("Event sender not available".to_string()))?
            .clone();

        // Preserve the manager's supported platforms; there is no polling fallback.
        if !matches!(std::env::consts::OS, "linux" | "macos" | "windows") {
            return Err(Error::BackendUnavailable(
                "Native watching is unsupported on this platform".into(),
            ));
        }
        let mut watcher = NotifyWatcher::new();
        watcher.set_event_sender(event_sender);

        // A path that cannot be watched is one directory going unobserved, not
        // a reason to abandon the other several hundred. It is counted out of
        // the return so the caller can say how much of the plan landed.
        let mut added = 0usize;
        for (index, (path, recursive)) in paths.iter().enumerate() {
            let config = template
                .clone()
                .with_id(format!("{group_id}#{index}"))
                .with_recursive(*recursive);
            match watcher.watch(path.clone(), config).await {
                Ok(_) => added += 1,
                Err(e) => {
                    debug!(path = %path.display(), error = %e, "watch path skipped");
                }
            }
        }

        if added == 0 {
            return Err(Error::Watch(format!(
                "no path of {} could be watched",
                paths.len()
            )));
        }

        self.watchers
            .write()
            .await
            .insert(group_id.to_string(), watcher);
        info!("Added watch group: {} ({} paths)", group_id, added);
        Ok(added)
    }

    /// Remove a group added by [`Self::add_watch_group`].
    ///
    /// Dropping the backend drops its debouncer, which releases every inotify
    /// watch the group held. Returns whether the group existed.
    pub async fn remove_watch_group(&mut self, group_id: &str) -> Result<bool> {
        let removed = self.watchers.write().await.remove(group_id).is_some();
        if removed {
            info!("Removed watch group: {}", group_id);
        } else {
            warn!("Watch group not found: {}", group_id);
        }
        Ok(removed)
    }

    /// Register an event handler.
    pub async fn register_handler(&self, handler: Arc<dyn EventHandler>) -> Result<()> {
        let mut handlers = self.handlers.write().await;
        handlers.register(handler.clone());
        info!("Registered event handler: {}", handler.name());
        Ok(())
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
    ) -> Result<()> {
        // Check for ready events
        let mut debouncer_guard = debouncer.lock().await;
        let now = std::time::Instant::now();

        // Manually check and emit ready events
        let ready_events = debouncer_guard.check_ready_events(now).await;
        drop(debouncer_guard);

        if !ready_events.is_empty() {
            // EVERY ready event is queued. Taking only one here is what lost
            // note writes: the debouncer has already removed them all from its
            // pending map, so an event dropped at this point is gone.
            {
                let mut queue = event_queue.lock().await;
                for event in ready_events {
                    queue.push(event).await?;
                }
            }

            // Process the queued events
            Self::process_queued_events(handlers, event_queue).await?;
        }

        Ok(())
    }

    /// Process a single event through the pipeline.
    async fn process_event(
        event: FileEvent,
        handlers: &Arc<RwLock<HandlerRegistry>>,
        event_queue: &Arc<Mutex<EventQueue>>,
        debouncer: &Arc<Mutex<Debouncer>>,
    ) -> Result<()> {
        // Debounce event
        {
            let mut debouncer_guard = debouncer.lock().await;
            let debounced = debouncer_guard.process_event(event.clone()).await;
            if debounced.is_empty() {
                // Event is pending in the debouncer; it emits on a later tick.
                return Ok(());
            }
            let mut queue = event_queue.lock().await;
            for debounced_event in debounced {
                queue.push(debounced_event).await?;
            }
        }

        // Process the queued events
        Self::process_queued_events(handlers, event_queue).await
    }

    /// Process all queued events through handlers.
    async fn process_queued_events(
        handlers: &Arc<RwLock<HandlerRegistry>>,
        event_queue: &Arc<Mutex<EventQueue>>,
    ) -> Result<()> {
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

        Ok(())
    }
}
