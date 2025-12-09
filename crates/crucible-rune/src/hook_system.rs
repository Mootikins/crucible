//! Hook System - Bridges discovered hooks with the event bus
//!
//! This module provides:
//! - `Hook` trait for implementing event handlers (Rune scripts or Rust functions)
//! - `RuneHookHandler` - executes discovered RuneHook scripts via RuneExecutor
//! - `BuiltinHook` - wraps Rust closures as hooks
//! - `HookRegistry` - discovers and manages hooks
//!
//! ## Lifecycle
//!
//! 1. Hooks are discovered from `~/.crucible/hooks/` and `KILN/.crucible/hooks/`
//! 2. RuneHook metadata is parsed via `AttributeDiscovery`
//! 3. RuneHookHandlers are created from discovered metadata
//! 4. Handlers are registered on the EventBus
//! 5. Events trigger matching handlers in priority order
//!
//! ## Example
//!
//! ```rust,ignore
//! use crucible_rune::{HookRegistry, EventBus, Event};
//!
//! let mut bus = EventBus::new();
//! let registry = HookRegistry::discover(Some(kiln_path))?;
//! registry.register_all(&mut bus);
//!
//! // Emit event - hooks are triggered automatically
//! let event = Event::tool_after("just_test", json!({...}));
//! let (result, ctx, errors) = bus.emit(event);
//! ```

use crate::attribute_discovery::AttributeDiscovery;
use crate::discovery_paths::DiscoveryPaths;
use crate::event_bus::{Event, EventBus, EventContext, Handler, HandlerError, HandlerResult};
use crate::executor::RuneExecutor;
use crate::hook_types::RuneHook;
use crate::RuneError;
use rune::Unit;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

/// Trait for hook implementations
///
/// This trait abstracts over different hook implementations:
/// - Rune scripts discovered via `#[hook(...)]`
/// - Built-in Rust function hooks
pub trait Hook: Send + Sync {
    /// Unique identifier for this hook
    fn name(&self) -> &str;

    /// Event type this hook handles
    fn event_type(&self) -> &str;

    /// Pattern for matching event identifiers (glob-style)
    fn pattern(&self) -> &str;

    /// Priority (lower = earlier execution)
    fn priority(&self) -> i64;

    /// Whether this hook is enabled
    fn enabled(&self) -> bool;

    /// Execute the hook
    ///
    /// # Arguments
    /// * `ctx` - Mutable event context for metadata and emitting events
    /// * `event` - The event to process
    ///
    /// # Returns
    /// Modified event or error
    fn handle(&self, ctx: &mut EventContext, event: Event) -> HandlerResult;
}

/// Handler for Rune script hooks
///
/// Wraps a discovered `RuneHook` and executes it via `RuneExecutor`
pub struct RuneHookHandler {
    /// The discovered hook metadata
    metadata: RuneHook,

    /// Compiled Rune unit (cached for performance)
    unit: Arc<Unit>,

    /// Shared executor for running scripts
    executor: Arc<RuneExecutor>,
}

impl RuneHookHandler {
    /// Create a new handler from discovered metadata
    pub fn new(metadata: RuneHook, executor: Arc<RuneExecutor>) -> Result<Self, RuneError> {
        // Read and compile the script
        let source = std::fs::read_to_string(&metadata.path)
            .map_err(|e| RuneError::Io(format!("Failed to read hook script {:?}: {}", metadata.path, e)))?;

        let unit = executor.compile(&metadata.name, &source)?;

        Ok(Self {
            metadata,
            unit,
            executor,
        })
    }

    /// Get the underlying metadata
    pub fn metadata(&self) -> &RuneHook {
        &self.metadata
    }

    /// Reload the script from disk
    pub fn reload(&mut self) -> Result<(), RuneError> {
        let source = std::fs::read_to_string(&self.metadata.path)
            .map_err(|e| RuneError::Io(format!("Failed to reload hook script: {}", e)))?;

        self.unit = self.executor.compile(&self.metadata.name, &source)?;
        info!("Reloaded hook script: {}", self.metadata.name);
        Ok(())
    }
}

impl Hook for RuneHookHandler {
    fn name(&self) -> &str {
        &self.metadata.name
    }

    fn event_type(&self) -> &str {
        &self.metadata.event_type
    }

    fn pattern(&self) -> &str {
        &self.metadata.pattern
    }

    fn priority(&self) -> i64 {
        self.metadata.priority
    }

    fn enabled(&self) -> bool {
        self.metadata.enabled
    }

    fn handle(&self, ctx: &mut EventContext, event: Event) -> HandlerResult {
        // Convert context metadata to JSON for Rune
        let ctx_json = serde_json::to_value(ctx.metadata())
            .unwrap_or(JsonValue::Object(Default::default()));

        // Convert event to JSON
        let event_json = serde_json::to_value(&event)
            .map_err(|e| HandlerError::non_fatal(&self.metadata.name, format!("Failed to serialize event: {}", e)))?;

        // Convert to Rune values
        let ctx_val = self.executor.json_to_rune_value(ctx_json)
            .map_err(|e| HandlerError::non_fatal(&self.metadata.name, format!("Failed to convert context: {}", e)))?;
        let event_val = self.executor.json_to_rune_value(event_json)
            .map_err(|e| HandlerError::non_fatal(&self.metadata.name, format!("Failed to convert event: {}", e)))?;

        // Call the handler function
        // We need to run this synchronously - wrap in a block_on for now
        // In a real async context, this would be called from an async context
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.executor
                    .call_function(&self.unit, &self.metadata.handler_fn, (ctx_val, event_val))
                    .await
            })
        });

        match result {
            Ok(result_json) => {
                // Parse result back to Event
                if result_json.is_null() {
                    // Handler returned null/None - pass through unchanged
                    Ok(event)
                } else {
                    // Try to deserialize back to Event
                    match serde_json::from_value::<Event>(result_json) {
                        Ok(modified_event) => Ok(modified_event),
                        Err(e) => {
                            warn!("Hook {} returned invalid event structure: {}", self.metadata.name, e);
                            // Return original event on parse error (fail-open)
                            Ok(event)
                        }
                    }
                }
            }
            Err(e) => {
                Err(HandlerError::non_fatal(&self.metadata.name, format!("Execution failed: {}", e)))
            }
        }
    }
}

/// Built-in hook wrapping a Rust closure
///
/// Use this for hooks that need native performance or access to Rust APIs.
pub struct BuiltinHook<F>
where
    F: Fn(&mut EventContext, Event) -> HandlerResult + Send + Sync,
{
    name: String,
    event_type: String,
    pattern: String,
    priority: i64,
    enabled: bool,
    handler_fn: F,
}

impl<F> BuiltinHook<F>
where
    F: Fn(&mut EventContext, Event) -> HandlerResult + Send + Sync,
{
    /// Create a new built-in hook
    pub fn new(
        name: impl Into<String>,
        event_type: impl Into<String>,
        pattern: impl Into<String>,
        handler_fn: F,
    ) -> Self {
        Self {
            name: name.into(),
            event_type: event_type.into(),
            pattern: pattern.into(),
            priority: 100,
            enabled: true,
            handler_fn,
        }
    }

    /// Set priority
    pub fn with_priority(mut self, priority: i64) -> Self {
        self.priority = priority;
        self
    }

    /// Set enabled state
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

impl<F> Hook for BuiltinHook<F>
where
    F: Fn(&mut EventContext, Event) -> HandlerResult + Send + Sync,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn event_type(&self) -> &str {
        &self.event_type
    }

    fn pattern(&self) -> &str {
        &self.pattern
    }

    fn priority(&self) -> i64 {
        self.priority
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    fn handle(&self, ctx: &mut EventContext, event: Event) -> HandlerResult {
        (self.handler_fn)(ctx, event)
    }
}

/// Registry for managing hooks
///
/// Discovers hooks from configured directories and provides hot-reload capability.
pub struct HookRegistry {
    /// Discovered Rune hooks (keyed by name)
    rune_hooks: HashMap<String, RuneHookHandler>,

    /// Discovery paths configuration
    paths: DiscoveryPaths,

    /// Shared executor
    executor: Arc<RuneExecutor>,

    /// File paths -> hook names (for hot reload)
    path_to_hooks: HashMap<PathBuf, Vec<String>>,
}

impl HookRegistry {
    /// Create a new registry with default discovery paths
    pub fn new(kiln_path: Option<&Path>) -> Result<Self, RuneError> {
        let paths = DiscoveryPaths::new("hooks", kiln_path);
        let executor = Arc::new(RuneExecutor::new()?);

        Ok(Self {
            rune_hooks: HashMap::new(),
            paths,
            executor,
            path_to_hooks: HashMap::new(),
        })
    }

    /// Create from custom discovery paths
    pub fn with_paths(paths: DiscoveryPaths) -> Result<Self, RuneError> {
        let executor = Arc::new(RuneExecutor::new()?);

        Ok(Self {
            rune_hooks: HashMap::new(),
            paths,
            executor,
            path_to_hooks: HashMap::new(),
        })
    }

    /// Discover all hooks from configured paths
    pub fn discover(&mut self) -> Result<usize, RuneError> {
        let discovery = AttributeDiscovery::new();
        let hooks: Vec<RuneHook> = discovery.discover_all(&self.paths)?;

        let mut count = 0;
        self.path_to_hooks.clear();

        for hook_meta in hooks {
            let path = hook_meta.path.clone();
            let name = hook_meta.name.clone();

            match RuneHookHandler::new(hook_meta, self.executor.clone()) {
                Ok(handler) => {
                    debug!("Discovered hook: {} (event={}, pattern={}, priority={})",
                           handler.name(), handler.event_type(), handler.pattern(), handler.priority());

                    self.rune_hooks.insert(name.clone(), handler);
                    self.path_to_hooks
                        .entry(path)
                        .or_default()
                        .push(name);
                    count += 1;
                }
                Err(e) => {
                    warn!("Failed to load hook '{}': {}", name, e);
                }
            }
        }

        info!("Discovered {} hooks from {} paths", count, self.paths.existing_paths().len());
        Ok(count)
    }

    /// Reload hooks from a specific file (for hot-reload)
    pub fn reload_file(&mut self, path: &Path) -> Result<usize, RuneError> {
        // Remove old hooks from this file
        if let Some(old_names) = self.path_to_hooks.remove(path) {
            for name in &old_names {
                self.rune_hooks.remove(name);
            }
        }

        // Re-discover from this file
        let discovery = AttributeDiscovery::new();
        let hooks: Vec<RuneHook> = discovery.parse_from_file(path)?;

        let mut count = 0;
        let mut new_names = Vec::new();

        for hook_meta in hooks {
            let name = hook_meta.name.clone();

            match RuneHookHandler::new(hook_meta, self.executor.clone()) {
                Ok(handler) => {
                    debug!("Reloaded hook: {}", name);
                    self.rune_hooks.insert(name.clone(), handler);
                    new_names.push(name);
                    count += 1;
                }
                Err(e) => {
                    warn!("Failed to reload hook '{}': {}", name, e);
                }
            }
        }

        self.path_to_hooks.insert(path.to_path_buf(), new_names);
        Ok(count)
    }

    /// Get a hook by name
    pub fn get(&self, name: &str) -> Option<&RuneHookHandler> {
        self.rune_hooks.get(name)
    }

    /// List all hook names
    pub fn list_names(&self) -> impl Iterator<Item = &str> {
        self.rune_hooks.keys().map(|s| s.as_str())
    }

    /// Get count of discovered hooks
    pub fn count(&self) -> usize {
        self.rune_hooks.len()
    }

    /// Register all discovered hooks on an EventBus
    pub fn register_all(&self, bus: &mut EventBus) {
        for handler in self.rune_hooks.values() {
            let h = handler.clone_as_handler();
            bus.register(h);
        }
    }

    /// Get discovery paths
    pub fn paths(&self) -> &DiscoveryPaths {
        &self.paths
    }
}

impl RuneHookHandler {
    /// Clone as an EventBus Handler
    ///
    /// Creates a new Handler that wraps this RuneHookHandler for registration
    /// on the EventBus.
    fn clone_as_handler(&self) -> Handler {
        let metadata = self.metadata.clone();
        let unit = self.unit.clone();
        let executor = self.executor.clone();

        let event_type = crate::event_bus::EventType::from_str(&metadata.event_type)
            .unwrap_or(crate::event_bus::EventType::Custom);

        Handler::new(
            metadata.name.clone(),
            event_type,
            metadata.pattern.clone(),
            move |ctx, event| {
                // Convert context metadata to JSON for Rune
                let ctx_json = serde_json::to_value(ctx.metadata())
                    .unwrap_or(JsonValue::Object(Default::default()));

                // Convert event to JSON
                let event_json = match serde_json::to_value(&event) {
                    Ok(j) => j,
                    Err(e) => return Err(HandlerError::non_fatal(&metadata.name, format!("Failed to serialize event: {}", e))),
                };

                // Convert to Rune values
                let ctx_val = match executor.json_to_rune_value(ctx_json) {
                    Ok(v) => v,
                    Err(e) => return Err(HandlerError::non_fatal(&metadata.name, format!("Failed to convert context: {}", e))),
                };
                let event_val = match executor.json_to_rune_value(event_json) {
                    Ok(v) => v,
                    Err(e) => return Err(HandlerError::non_fatal(&metadata.name, format!("Failed to convert event: {}", e))),
                };

                // Call the handler function
                let result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        executor
                            .call_function(&unit, &metadata.handler_fn, (ctx_val, event_val))
                            .await
                    })
                });

                match result {
                    Ok(result_json) => {
                        if result_json.is_null() {
                            Ok(event)
                        } else {
                            match serde_json::from_value::<Event>(result_json) {
                                Ok(modified_event) => Ok(modified_event),
                                Err(e) => {
                                    warn!("Hook {} returned invalid event structure: {}", metadata.name, e);
                                    Ok(event)
                                }
                            }
                        }
                    }
                    Err(e) => {
                        Err(HandlerError::non_fatal(&metadata.name, format!("Execution failed: {}", e)))
                    }
                }
            },
        )
        .with_priority(metadata.priority)
        .with_enabled(metadata.enabled)
    }
}

/// Thread-safe wrapper for HookRegistry with hot-reload support
pub struct HookManager {
    registry: RwLock<HookRegistry>,
}

impl HookManager {
    /// Create a new manager with default paths
    pub fn new(kiln_path: Option<&Path>) -> Result<Self, RuneError> {
        let registry = HookRegistry::new(kiln_path)?;
        Ok(Self {
            registry: RwLock::new(registry),
        })
    }

    /// Create from custom paths
    pub fn with_paths(paths: DiscoveryPaths) -> Result<Self, RuneError> {
        let registry = HookRegistry::with_paths(paths)?;
        Ok(Self {
            registry: RwLock::new(registry),
        })
    }

    /// Discover hooks
    pub fn discover(&self) -> Result<usize, RuneError> {
        let mut reg = self.registry.write().unwrap();
        reg.discover()
    }

    /// Reload a specific file
    pub fn reload_file(&self, path: &Path) -> Result<usize, RuneError> {
        let mut reg = self.registry.write().unwrap();
        reg.reload_file(path)
    }

    /// Register all hooks on an EventBus
    pub fn register_all(&self, bus: &mut EventBus) {
        let reg = self.registry.read().unwrap();
        reg.register_all(bus);
    }

    /// Get hook count
    pub fn count(&self) -> usize {
        let reg = self.registry.read().unwrap();
        reg.count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_builtin_hook_creation() {
        let hook = BuiltinHook::new(
            "test_hook",
            "tool:after",
            "just_*",
            |_ctx, event| Ok(event),
        );

        assert_eq!(hook.name(), "test_hook");
        assert_eq!(hook.event_type(), "tool:after");
        assert_eq!(hook.pattern(), "just_*");
        assert_eq!(hook.priority(), 100);
        assert!(hook.enabled());
    }

    #[test]
    fn test_builtin_hook_priority() {
        let hook = BuiltinHook::new(
            "test",
            "tool:after",
            "*",
            |_ctx, event| Ok(event),
        )
        .with_priority(50);

        assert_eq!(hook.priority(), 50);
    }

    #[test]
    fn test_builtin_hook_disabled() {
        let hook = BuiltinHook::new(
            "test",
            "tool:after",
            "*",
            |_ctx, event| Ok(event),
        )
        .with_enabled(false);

        assert!(!hook.enabled());
    }

    #[test]
    fn test_builtin_hook_execution() {
        let hook = BuiltinHook::new(
            "modifier",
            "tool:after",
            "*",
            |_ctx, mut event| {
                if let Some(obj) = event.payload.as_object_mut() {
                    obj.insert("modified".to_string(), json!(true));
                }
                Ok(event)
            },
        );

        let mut ctx = EventContext::new();
        let event = Event::tool_after("test", json!({"original": true}));
        let result = hook.handle(&mut ctx, event).unwrap();

        assert_eq!(result.payload["original"], json!(true));
        assert_eq!(result.payload["modified"], json!(true));
    }

    #[test]
    fn test_hook_registry_empty() {
        let temp = TempDir::new().unwrap();
        let paths = DiscoveryPaths::empty("hooks").with_path(temp.path().to_path_buf());
        let mut registry = HookRegistry::with_paths(paths).unwrap();

        let count = registry.discover().unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_hook_registry_discovers_hooks() {
        let temp = TempDir::new().unwrap();
        let hooks_dir = temp.path().join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();

        // Write a hook script
        let script = r#"
/// Test hook that modifies events
#[hook(event = "tool:after", pattern = "just_*", priority = 50)]
pub fn test_modifier(ctx, event) {
    event
}
"#;
        fs::write(hooks_dir.join("test_hook.rn"), script).unwrap();

        let paths = DiscoveryPaths::empty("hooks").with_path(hooks_dir);
        let mut registry = HookRegistry::with_paths(paths).unwrap();

        let count = registry.discover().unwrap();
        assert_eq!(count, 1);
        assert!(registry.get("test_modifier").is_some());
    }

    #[tokio::test]
    async fn test_hook_registry_reload_file() {
        let temp = TempDir::new().unwrap();
        let hooks_dir = temp.path().join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();

        let script_path = hooks_dir.join("reloadable.rn");

        // Initial script
        let script_v1 = r#"
#[hook(event = "tool:after", pattern = "*", priority = 100)]
pub fn reloadable_hook(ctx, event) {
    event
}
"#;
        fs::write(&script_path, script_v1).unwrap();

        let paths = DiscoveryPaths::empty("hooks").with_path(hooks_dir);
        let mut registry = HookRegistry::with_paths(paths).unwrap();
        registry.discover().unwrap();

        assert!(registry.get("reloadable_hook").is_some());
        assert_eq!(registry.get("reloadable_hook").unwrap().priority(), 100);

        // Update script with different priority
        let script_v2 = r#"
#[hook(event = "tool:after", pattern = "*", priority = 50)]
pub fn reloadable_hook(ctx, event) {
    event
}
"#;
        fs::write(&script_path, script_v2).unwrap();

        // Reload
        registry.reload_file(&script_path).unwrap();

        // Check priority changed
        assert!(registry.get("reloadable_hook").is_some());
        assert_eq!(registry.get("reloadable_hook").unwrap().priority(), 50);
    }

    #[test]
    fn test_hook_manager_thread_safe() {
        let temp = TempDir::new().unwrap();
        let paths = DiscoveryPaths::empty("hooks").with_path(temp.path().to_path_buf());
        let manager = HookManager::with_paths(paths).unwrap();

        manager.discover().unwrap();
        assert_eq!(manager.count(), 0);
    }
}
