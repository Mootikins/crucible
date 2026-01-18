//! Handler System - Bridges discovered handlers with the event bus
//!
//! This module provides:
//! - `ScriptHandler` trait for implementing event handlers (Rune scripts or Rust functions)
//! - `RuneScriptHandler` - executes discovered RuneHandler scripts via RuneExecutor
//! - `BuiltinHandler` - wraps Rust closures as handlers
//! - `HandlerRegistry` - discovers and manages handlers
//!
//! ## Lifecycle
//!
//! 1. Handlers are discovered from `~/.config/crucible/plugins/` and `KILN/.crucible/plugins/`
//! 2. RuneHandler metadata is parsed via `AttributeDiscovery`
//! 3. RuneScriptHandlers are created from discovered metadata
//! 4. Handlers are registered on the EventBus
//! 5. Events trigger matching handlers in priority order
//!
//! ## Example
//!
//! ```rust,ignore
//! use crucible_rune::{HandlerRegistry, EventBus, Event};
//!
//! let mut bus = EventBus::new();
//! let registry = HandlerRegistry::discover(Some(kiln_path))?;
//! registry.register_all(&mut bus);
//!
//! // Emit event - handlers are triggered automatically
//! let event = Event::tool_after("just_test", json!({...}));
//! let (result, ctx, errors) = bus.emit(event);
//! ```

#![allow(deprecated)]

use crate::attribute_discovery::AttributeDiscovery;
use crate::discovery_paths::DiscoveryPaths;
use crate::event_bus::{Event, EventBus, EventContext, Handler, HandlerError, HandlerResult};
use crate::executor::RuneExecutor;
use crate::handler_types::RuneHandler;
use crate::RuneError;
use rune::Unit;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

/// Trait for script handler implementations
///
/// This trait abstracts over different handler implementations:
/// - Rune scripts discovered via `#[handler(...)]`
/// - Built-in Rust function handlers
pub trait ScriptHandler: Send + Sync {
    /// Unique identifier for this handler
    fn name(&self) -> &str;

    /// Event type this handler handles
    fn event_type(&self) -> &str;

    /// Pattern for matching event identifiers (glob-style)
    fn pattern(&self) -> &str;

    /// Priority (lower = earlier execution)
    fn priority(&self) -> i64;

    /// Whether this handler is enabled
    fn enabled(&self) -> bool;

    /// Execute the handler
    ///
    /// # Arguments
    /// * `ctx` - Mutable event context for metadata and emitting events
    /// * `event` - The event to process
    ///
    /// # Returns
    /// Modified event or error
    fn handle(&self, ctx: &mut EventContext, event: Event) -> HandlerResult;
}

/// Handler for Rune scripts
///
/// Wraps a discovered `RuneHandler` and executes it via `RuneExecutor`
pub struct RuneScriptHandler {
    /// The discovered handler metadata
    metadata: RuneHandler,

    /// Compiled Rune unit (cached for performance)
    unit: Arc<Unit>,

    /// Shared executor for running scripts
    executor: Arc<RuneExecutor>,
}

use once_cell::sync::Lazy;

/// Regex for stripping handler/hook attributes from Rune source
static HANDLER_ATTR_RE: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"(?m)^\s*#\[(handler|hook)\([^)]*\)\]\s*\n?").unwrap());

/// Strip handler/hook attributes from Rune source code
///
/// Rune doesn't support custom attributes on functions, so we need to remove
/// the `#[handler(...)]` and `#[hook(...)]` attributes before compiling.
fn strip_handler_attributes(source: &str) -> String {
    HANDLER_ATTR_RE.replace_all(source, "").to_string()
}

/// Execute a Rune handler function with the given context and event.
///
/// This is the core execution logic shared between `RuneScriptHandler::handle()`
/// and `clone_as_handler()`. It converts values to Rune format, calls the function,
/// and processes the result.
fn execute_rune_handler(
    executor: &RuneExecutor,
    unit: &Arc<Unit>,
    handler_name: &str,
    handler_fn: &str,
    ctx_json: JsonValue,
    event_json: JsonValue,
    event: Event,
) -> HandlerResult {
    // Convert to Rune values
    let ctx_val = executor.json_to_rune_value(ctx_json).map_err(|e| {
        HandlerError::non_fatal(handler_name, format!("Failed to convert context: {}", e))
    })?;
    let event_val = executor.json_to_rune_value(event_json).map_err(|e| {
        HandlerError::non_fatal(handler_name, format!("Failed to convert event: {}", e))
    })?;

    // Call the handler function
    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            executor
                .call_function(unit, handler_fn, (ctx_val, event_val))
                .await
        })
    });

    match result {
        Ok(result_json) => {
            if result_json.is_null() {
                // Handler returned null/None - pass through unchanged
                Ok(event)
            } else {
                // Try to deserialize back to Event
                match serde_json::from_value::<Event>(result_json) {
                    Ok(modified_event) => Ok(modified_event),
                    Err(e) => {
                        warn!(
                            "Handler {} returned invalid event structure: {}",
                            handler_name, e
                        );
                        // Return original event on parse error (fail-open)
                        Ok(event)
                    }
                }
            }
        }
        Err(e) => Err(HandlerError::non_fatal(
            handler_name,
            format!("Execution failed: {}", e),
        )),
    }
}

impl RuneScriptHandler {
    /// Create a new handler from discovered metadata
    pub fn new(metadata: RuneHandler, executor: Arc<RuneExecutor>) -> Result<Self, RuneError> {
        // Read the script
        let source = std::fs::read_to_string(&metadata.path).map_err(|e| {
            RuneError::Io(format!(
                "Failed to read handler script {:?}: {}",
                metadata.path, e
            ))
        })?;

        // Strip handler attributes before compiling (Rune doesn't support them)
        let clean_source = strip_handler_attributes(&source);

        let unit = executor.compile(&metadata.name, &clean_source)?;

        Ok(Self {
            metadata,
            unit,
            executor,
        })
    }

    /// Get the underlying metadata
    pub fn metadata(&self) -> &RuneHandler {
        &self.metadata
    }

    /// Reload the script from disk
    pub fn reload(&mut self) -> Result<(), RuneError> {
        let source = std::fs::read_to_string(&self.metadata.path)
            .map_err(|e| RuneError::Io(format!("Failed to reload handler script: {}", e)))?;

        // Strip handler attributes before compiling
        let clean_source = strip_handler_attributes(&source);

        self.unit = self.executor.compile(&self.metadata.name, &clean_source)?;
        info!("Reloaded handler script: {}", self.metadata.name);
        Ok(())
    }
}

impl ScriptHandler for RuneScriptHandler {
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
        let ctx_json =
            serde_json::to_value(ctx.metadata()).unwrap_or(JsonValue::Object(Default::default()));

        // Convert event to JSON
        let event_json = serde_json::to_value(&event).map_err(|e| {
            HandlerError::non_fatal(
                &self.metadata.name,
                format!("Failed to serialize event: {}", e),
            )
        })?;

        execute_rune_handler(
            &self.executor,
            &self.unit,
            &self.metadata.name,
            &self.metadata.handler_fn,
            ctx_json,
            event_json,
            event,
        )
    }
}

/// Built-in handler wrapping a Rust closure
///
/// Use this for handlers that need native performance or access to Rust APIs.
pub struct BuiltinHandler<F>
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

impl<F> BuiltinHandler<F>
where
    F: Fn(&mut EventContext, Event) -> HandlerResult + Send + Sync,
{
    /// Create a new built-in handler
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

impl<F> ScriptHandler for BuiltinHandler<F>
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

/// Registry for managing handlers
///
/// Discovers handlers from configured directories and provides hot-reload capability.
pub struct HandlerRegistry {
    /// Discovered Rune handlers (keyed by name)
    rune_handlers: HashMap<String, RuneScriptHandler>,

    /// Discovery paths configuration
    paths: DiscoveryPaths,

    /// Shared executor
    executor: Arc<RuneExecutor>,

    /// File paths -> handler names (for hot reload)
    path_to_handlers: HashMap<PathBuf, Vec<String>>,
}

impl HandlerRegistry {
    /// Create a new registry with default discovery paths
    pub fn new(kiln_path: Option<&Path>) -> Result<Self, RuneError> {
        let paths = DiscoveryPaths::new("plugins", kiln_path);
        let executor = Arc::new(RuneExecutor::new()?);

        Ok(Self {
            rune_handlers: HashMap::new(),
            paths,
            executor,
            path_to_handlers: HashMap::new(),
        })
    }

    /// Create from custom discovery paths
    pub fn with_paths(paths: DiscoveryPaths) -> Result<Self, RuneError> {
        let executor = Arc::new(RuneExecutor::new()?);

        Ok(Self {
            rune_handlers: HashMap::new(),
            paths,
            executor,
            path_to_handlers: HashMap::new(),
        })
    }

    /// Discover all handlers from configured paths
    pub fn discover(&mut self) -> Result<usize, RuneError> {
        let discovery = AttributeDiscovery::new();
        let handlers: Vec<RuneHandler> = discovery.discover_all(&self.paths)?;

        let mut count = 0;
        self.path_to_handlers.clear();

        for handler_meta in handlers {
            let path = handler_meta.path.clone();
            let name = handler_meta.name.clone();

            match RuneScriptHandler::new(handler_meta, self.executor.clone()) {
                Ok(handler) => {
                    debug!(
                        "Discovered handler: {} (event={}, pattern={}, priority={})",
                        handler.name(),
                        handler.event_type(),
                        handler.pattern(),
                        handler.priority()
                    );

                    self.rune_handlers.insert(name.clone(), handler);
                    self.path_to_handlers.entry(path).or_default().push(name);
                    count += 1;
                }
                Err(e) => {
                    warn!("Failed to load handler '{}': {}", name, e);
                }
            }
        }

        info!(
            "Discovered {} handlers from {} paths",
            count,
            self.paths.existing_paths().len()
        );
        Ok(count)
    }

    /// Reload handlers from a specific file (for hot-reload)
    pub fn reload_file(&mut self, path: &Path) -> Result<usize, RuneError> {
        // Remove old handlers from this file
        if let Some(old_names) = self.path_to_handlers.remove(path) {
            for name in &old_names {
                self.rune_handlers.remove(name);
            }
        }

        // Re-discover from this file
        let discovery = AttributeDiscovery::new();
        let handlers: Vec<RuneHandler> = discovery.parse_from_file(path)?;

        let mut count = 0;
        let mut new_names = Vec::new();

        for handler_meta in handlers {
            let name = handler_meta.name.clone();

            match RuneScriptHandler::new(handler_meta, self.executor.clone()) {
                Ok(handler) => {
                    debug!("Reloaded handler: {}", name);
                    self.rune_handlers.insert(name.clone(), handler);
                    new_names.push(name);
                    count += 1;
                }
                Err(e) => {
                    warn!("Failed to reload handler '{}': {}", name, e);
                }
            }
        }

        self.path_to_handlers.insert(path.to_path_buf(), new_names);
        Ok(count)
    }

    /// Get a handler by name
    pub fn get(&self, name: &str) -> Option<&RuneScriptHandler> {
        self.rune_handlers.get(name)
    }

    /// List all handler names
    pub fn list_names(&self) -> impl Iterator<Item = &str> {
        self.rune_handlers.keys().map(|s| s.as_str())
    }

    /// Get count of discovered handlers
    pub fn count(&self) -> usize {
        self.rune_handlers.len()
    }

    /// Register all discovered handlers on an EventBus
    pub fn register_all(&self, bus: &mut EventBus) {
        for handler in self.rune_handlers.values() {
            let h = handler.clone_as_handler();
            bus.register(h);
        }
    }

    /// Get discovery paths
    pub fn paths(&self) -> &DiscoveryPaths {
        &self.paths
    }
}

impl RuneScriptHandler {
    /// Clone as an EventBus Handler
    ///
    /// Creates a new Handler that wraps this RuneScriptHandler for registration
    /// on the EventBus.
    fn clone_as_handler(&self) -> Handler {
        let metadata = self.metadata.clone();
        let unit = self.unit.clone();
        let executor = self.executor.clone();

        let event_type = metadata
            .event_type
            .parse::<crate::event_bus::EventType>()
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
                    Err(e) => {
                        return Err(HandlerError::non_fatal(
                            &metadata.name,
                            format!("Failed to serialize event: {}", e),
                        ))
                    }
                };

                execute_rune_handler(
                    &executor,
                    &unit,
                    &metadata.name,
                    &metadata.handler_fn,
                    ctx_json,
                    event_json,
                    event,
                )
            },
        )
        .with_priority(metadata.priority)
        .with_enabled(metadata.enabled)
    }
}

/// Thread-safe wrapper for HandlerRegistry with hot-reload support
pub struct HandlerManager {
    registry: RwLock<HandlerRegistry>,
}

impl HandlerManager {
    /// Create a new manager with default paths
    pub fn new(kiln_path: Option<&Path>) -> Result<Self, RuneError> {
        let registry = HandlerRegistry::new(kiln_path)?;
        Ok(Self {
            registry: RwLock::new(registry),
        })
    }

    /// Create from custom paths
    pub fn with_paths(paths: DiscoveryPaths) -> Result<Self, RuneError> {
        let registry = HandlerRegistry::with_paths(paths)?;
        Ok(Self {
            registry: RwLock::new(registry),
        })
    }

    /// Discover handlers
    pub fn discover(&self) -> Result<usize, RuneError> {
        let mut reg = self.registry.write().unwrap();
        reg.discover()
    }

    /// Reload a specific file
    pub fn reload_file(&self, path: &Path) -> Result<usize, RuneError> {
        let mut reg = self.registry.write().unwrap();
        reg.reload_file(path)
    }

    /// Register all handlers on an EventBus
    pub fn register_all(&self, bus: &mut EventBus) {
        let reg = self.registry.read().unwrap();
        reg.register_all(bus);
    }

    /// Get handler count
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
    fn test_builtin_handler_creation() {
        let handler = BuiltinHandler::new("test_handler", "tool:after", "just_*", |_ctx, event| {
            Ok(event)
        });

        assert_eq!(handler.name(), "test_handler");
        assert_eq!(handler.event_type(), "tool:after");
        assert_eq!(handler.pattern(), "just_*");
        assert_eq!(handler.priority(), 100);
        assert!(handler.enabled());
    }

    #[test]
    fn test_builtin_handler_priority() {
        let handler = BuiltinHandler::new("test", "tool:after", "*", |_ctx, event| Ok(event))
            .with_priority(50);

        assert_eq!(handler.priority(), 50);
    }

    #[test]
    fn test_builtin_handler_disabled() {
        let handler = BuiltinHandler::new("test", "tool:after", "*", |_ctx, event| Ok(event))
            .with_enabled(false);

        assert!(!handler.enabled());
    }

    #[test]
    fn test_builtin_handler_execution() {
        let handler = BuiltinHandler::new("modifier", "tool:after", "*", |_ctx, mut event| {
            if let Some(obj) = event.payload.as_object_mut() {
                obj.insert("modified".to_string(), json!(true));
            }
            Ok(event)
        });

        let mut ctx = EventContext::new();
        let event = Event::tool_after("test", json!({"original": true}));
        let result = handler.handle(&mut ctx, event).unwrap();

        assert_eq!(result.payload["original"], json!(true));
        assert_eq!(result.payload["modified"], json!(true));
    }

    #[test]
    fn test_handler_registry_empty() {
        let temp = TempDir::new().unwrap();
        let paths = DiscoveryPaths::empty("handlers").with_path(temp.path().to_path_buf());
        let mut registry = HandlerRegistry::with_paths(paths).unwrap();

        let count = registry.discover().unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_handler_registry_discovers_handlers() {
        let temp = TempDir::new().unwrap();
        let handlers_dir = temp.path().join("handlers");
        fs::create_dir_all(&handlers_dir).unwrap();

        // Write a handler script
        let script = r#"
/// Test handler that modifies events
#[handler(event = "tool:after", pattern = "just_*", priority = 50)]
pub fn test_modifier(ctx, event) {
    event
}
"#;
        fs::write(handlers_dir.join("test_handler.rn"), script).unwrap();

        let paths = DiscoveryPaths::empty("handlers").with_path(handlers_dir);
        let mut registry = HandlerRegistry::with_paths(paths).unwrap();

        let count = registry.discover().unwrap();
        assert_eq!(count, 1);
        assert!(registry.get("test_modifier").is_some());
    }

    #[tokio::test]
    async fn test_handler_registry_reload_file() {
        let temp = TempDir::new().unwrap();
        let handlers_dir = temp.path().join("handlers");
        fs::create_dir_all(&handlers_dir).unwrap();

        let script_path = handlers_dir.join("reloadable.rn");

        // Initial script
        let script_v1 = r#"
#[handler(event = "tool:after", pattern = "*", priority = 100)]
pub fn reloadable_handler(ctx, event) {
    event
}
"#;
        fs::write(&script_path, script_v1).unwrap();

        let paths = DiscoveryPaths::empty("handlers").with_path(handlers_dir);
        let mut registry = HandlerRegistry::with_paths(paths).unwrap();
        registry.discover().unwrap();

        assert!(registry.get("reloadable_handler").is_some());
        assert_eq!(registry.get("reloadable_handler").unwrap().priority(), 100);

        // Update script with different priority
        let script_v2 = r#"
#[handler(event = "tool:after", pattern = "*", priority = 50)]
pub fn reloadable_handler(ctx, event) {
    event
}
"#;
        fs::write(&script_path, script_v2).unwrap();

        // Reload
        registry.reload_file(&script_path).unwrap();

        // Check priority changed
        assert!(registry.get("reloadable_handler").is_some());
        assert_eq!(registry.get("reloadable_handler").unwrap().priority(), 50);
    }

    #[test]
    fn test_handler_manager_thread_safe() {
        let temp = TempDir::new().unwrap();
        let paths = DiscoveryPaths::empty("handlers").with_path(temp.path().to_path_buf());
        let manager = HandlerManager::with_paths(paths).unwrap();

        manager.discover().unwrap();
        assert_eq!(manager.count(), 0);
    }

    #[tokio::test]
    async fn test_handler_registry_discovers_legacy_hooks() {
        // Test backwards compatibility: #[hook(...)] still works
        let temp = TempDir::new().unwrap();
        let handlers_dir = temp.path().join("handlers");
        fs::create_dir_all(&handlers_dir).unwrap();

        // Write using legacy #[hook(...)] attribute
        let script = r#"
/// Legacy handler using old attribute
#[hook(event = "tool:after", pattern = "*")]
pub fn legacy_handler(ctx, event) {
    event
}
"#;
        fs::write(handlers_dir.join("legacy.rn"), script).unwrap();

        let paths = DiscoveryPaths::empty("handlers").with_path(handlers_dir);
        let mut registry = HandlerRegistry::with_paths(paths).unwrap();

        let count = registry.discover().unwrap();
        assert_eq!(count, 1);
        assert!(registry.get("legacy_handler").is_some());
    }
}
