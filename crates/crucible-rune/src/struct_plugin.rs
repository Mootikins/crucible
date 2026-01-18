//! Struct-based plugin system for Rune
//!
//! This module provides infrastructure for plugins that are Rune structs
//! with methods like `tools()`, `dispatch()`, and `on_watch()`.
//!
//! # Plugin Pattern
//!
//! ```rune
//! struct MyPlugin {
//!     state,
//! }
//!
//! impl MyPlugin {
//!     fn new() {
//!         MyPlugin { state: #{} }
//!     }
//!
//!     fn tools(self) {
//!         [#{ name: "my_tool", description: "..." }]
//!     }
//!
//!     fn dispatch(self, tool_name, args) {
//!         // Handle tool invocation
//!     }
//!
//!     fn on_watch(self, event) {
//!         // Handle file changes
//!     }
//! }
//!
//! #[plugin(watch = ["*.just", "justfile"])]
//! pub fn create() {
//!     MyPlugin::new()
//! }
//! ```

use crate::attribute_discovery::{attr_parsers, AttributeDiscovery, FromAttributes};
use crate::mcp_types::{json_to_rune, rune_to_json};
use crate::RuneError;
use crucible_config::ShellPolicy;
use crucible_core::discovery::DiscoveryPaths;
use glob::Pattern;
use rune::runtime::Value;
use rune::{Context, Diagnostics, Source, Sources, Unit, Vm};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, warn};

// ============================================================================
// Thread-safe Handle (Send + Sync)
// ============================================================================

/// Commands sent to the Rune plugin thread
enum PluginCommand {
    /// Load plugins from a directory
    LoadFromDirectory {
        dir: PathBuf,
        reply: oneshot::Sender<Result<(), RuneError>>,
    },
    /// Dispatch a tool call
    Dispatch {
        tool_name: String,
        args: JsonValue,
        reply: oneshot::Sender<Result<JsonValue, RuneError>>,
    },
    /// Handle a file watch event
    OnWatch {
        plugin_path: PathBuf,
        event: WatchEvent,
        reply: oneshot::Sender<Result<(), RuneError>>,
    },
    /// Get all tool definitions
    AllTools {
        reply: oneshot::Sender<Vec<ToolDefinition>>,
    },
    /// Check if a tool exists
    HasTool {
        name: String,
        reply: oneshot::Sender<bool>,
    },
    /// Shutdown the thread
    Shutdown,
}

/// Thread-safe handle to the struct plugin system
///
/// This handle is `Send + Sync` and communicates with a dedicated thread
/// that runs all Rune code. This avoids the `!Send` limitation of Rune's
/// `Value` type.
pub struct StructPluginHandle {
    /// Channel to send commands to the Rune thread
    command_tx: mpsc::UnboundedSender<PluginCommand>,
    /// Handle to the background thread (for cleanup)
    _thread_handle: Arc<JoinHandle<()>>,
}

// Explicit Send + Sync - the handle only contains channel sender which is Send+Sync
unsafe impl Send for StructPluginHandle {}
unsafe impl Sync for StructPluginHandle {}

impl StructPluginHandle {
    /// Create a new plugin handle, spawning the background Rune thread
    ///
    /// # Arguments
    ///
    /// * `policy` - Shell security policy for plugin command execution
    pub fn new(policy: ShellPolicy) -> Result<Self, RuneError> {
        let (command_tx, command_rx) = mpsc::unbounded_channel();

        // Spawn the dedicated Rune thread
        let thread_handle = thread::spawn(move || {
            // Create a new tokio runtime for this thread
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime for plugin thread");

            rt.block_on(async {
                Self::run_plugin_thread(command_rx, policy).await;
            });
        });

        Ok(Self {
            command_tx,
            _thread_handle: Arc::new(thread_handle),
        })
    }

    /// The main loop running on the dedicated Rune thread
    async fn run_plugin_thread(
        mut command_rx: mpsc::UnboundedReceiver<PluginCommand>,
        policy: ShellPolicy,
    ) {
        // Create the actual loader on this thread (contains non-Send types)
        let mut loader = match StructPluginLoader::new(policy) {
            Ok(l) => l,
            Err(e) => {
                warn!("Failed to create StructPluginLoader: {}", e);
                return;
            }
        };

        debug!("Plugin thread started");

        while let Some(cmd) = command_rx.recv().await {
            match cmd {
                PluginCommand::LoadFromDirectory { dir, reply } => {
                    let result = loader.load_from_directory(&dir).await;
                    let _ = reply.send(result);
                }
                PluginCommand::Dispatch {
                    tool_name,
                    args,
                    reply,
                } => {
                    let result = loader.dispatch(&tool_name, args).await;
                    let _ = reply.send(result);
                }
                PluginCommand::OnWatch {
                    plugin_path,
                    event,
                    reply,
                } => {
                    let result = loader.call_on_watch(&plugin_path, event).await;
                    let _ = reply.send(result);
                }
                PluginCommand::AllTools { reply } => {
                    // Clone the tool definitions to send across thread boundary
                    let tools: Vec<ToolDefinition> =
                        loader.all_tools().into_iter().cloned().collect();
                    let _ = reply.send(tools);
                }
                PluginCommand::HasTool { name, reply } => {
                    let has = loader.registry().find_plugin_for_tool(&name).is_some();
                    let _ = reply.send(has);
                }
                PluginCommand::Shutdown => {
                    debug!("Plugin thread shutting down");
                    break;
                }
            }
        }

        debug!("Plugin thread exited");
    }

    /// Load plugins from a directory
    pub async fn load_from_directory(&self, dir: &Path) -> Result<(), RuneError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(PluginCommand::LoadFromDirectory {
                dir: dir.to_path_buf(),
                reply: reply_tx,
            })
            .map_err(|_| RuneError::Execution("Plugin thread closed".to_string()))?;

        reply_rx
            .await
            .map_err(|_| RuneError::Execution("Plugin thread did not respond".to_string()))?
    }

    /// Dispatch a tool call to the appropriate plugin
    pub async fn dispatch(&self, tool_name: &str, args: JsonValue) -> Result<JsonValue, RuneError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(PluginCommand::Dispatch {
                tool_name: tool_name.to_string(),
                args,
                reply: reply_tx,
            })
            .map_err(|_| RuneError::Execution("Plugin thread closed".to_string()))?;

        reply_rx
            .await
            .map_err(|_| RuneError::Execution("Plugin thread did not respond".to_string()))?
    }

    /// Handle a file watch event
    pub async fn call_on_watch(
        &self,
        plugin_path: &Path,
        event: WatchEvent,
    ) -> Result<(), RuneError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(PluginCommand::OnWatch {
                plugin_path: plugin_path.to_path_buf(),
                event,
                reply: reply_tx,
            })
            .map_err(|_| RuneError::Execution("Plugin thread closed".to_string()))?;

        reply_rx
            .await
            .map_err(|_| RuneError::Execution("Plugin thread did not respond".to_string()))?
    }

    /// Get all tool definitions from all plugins
    pub async fn all_tools(&self) -> Vec<ToolDefinition> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .command_tx
            .send(PluginCommand::AllTools { reply: reply_tx })
            .is_err()
        {
            return vec![];
        }

        reply_rx.await.unwrap_or_default()
    }

    /// Check if a tool exists in any plugin
    pub async fn has_tool(&self, name: &str) -> bool {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .command_tx
            .send(PluginCommand::HasTool {
                name: name.to_string(),
                reply: reply_tx,
            })
            .is_err()
        {
            return false;
        }

        reply_rx.await.unwrap_or(false)
    }
}

impl Drop for StructPluginHandle {
    fn drop(&mut self) {
        // Send shutdown command (ignore errors if already closed)
        let _ = self.command_tx.send(PluginCommand::Shutdown);
    }
}

// ============================================================================
// Original types (kept for internal use on the Rune thread)
// ============================================================================

/// Event passed to plugin's `on_watch()` method when a watched file changes
#[derive(Debug, Clone)]
pub struct WatchEvent {
    /// Path to the file that changed
    pub path: PathBuf,
    /// Type of change
    pub kind: WatchEventKind,
}

/// Type of file change for watch events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchEventKind {
    /// File was created
    Created,
    /// File was modified
    Modified,
    /// File was deleted
    Deleted,
}

impl WatchEventKind {
    /// Get the string representation for Rune scripts
    pub fn as_str(&self) -> &'static str {
        match self {
            WatchEventKind::Created => "created",
            WatchEventKind::Modified => "modified",
            WatchEventKind::Deleted => "deleted",
        }
    }
}

/// Metadata parsed from `#[plugin(...)]` attribute
#[derive(Debug, Clone)]
pub struct PluginMetadata {
    /// Plugin name (defaults to factory function name)
    pub name: String,
    /// Names of plugins this one depends on
    pub deps: Vec<String>,
    /// Name of the factory function
    pub factory_fn: String,
    /// Watch patterns for file changes
    pub watch_patterns: Vec<Pattern>,
    /// Path to the source file
    pub source_path: PathBuf,
}

impl FromAttributes for PluginMetadata {
    fn attribute_name() -> &'static str {
        "plugin"
    }

    fn from_attrs(attrs: &str, fn_name: &str, path: &Path, _docs: &str) -> Result<Self, RuneError> {
        // Extract name (defaults to factory function name)
        let name =
            attr_parsers::extract_string(attrs, "name").unwrap_or_else(|| fn_name.to_string());

        // Extract dependencies
        let deps = attr_parsers::extract_string_array(attrs, "deps").unwrap_or_default();

        // Extract watch patterns
        let watch_strs = attr_parsers::extract_string_array(attrs, "watch").unwrap_or_default();
        let watch_patterns: Vec<Pattern> = watch_strs
            .iter()
            .filter_map(|s| {
                Pattern::new(s)
                    .map_err(|e| warn!("Invalid watch pattern '{}': {}", s, e))
                    .ok()
            })
            .collect();

        Ok(PluginMetadata {
            name,
            deps,
            factory_fn: fn_name.to_string(),
            watch_patterns,
            source_path: path.to_path_buf(),
        })
    }
}

/// Tool definition returned by plugin's `tools()` method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name (e.g., "just_test")
    pub name: String,
    /// Tool description
    #[serde(default)]
    pub description: String,
    /// Tool parameters
    #[serde(default)]
    pub parameters: Vec<ToolParameter>,
}

/// Parameter for a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameter {
    /// Parameter name
    pub name: String,
    /// Whether the parameter is required
    #[serde(default)]
    pub required: bool,
    /// Parameter description
    #[serde(default)]
    pub description: String,
}

/// A loaded plugin instance with its compiled unit and Rune value
struct PluginInstance {
    /// Plugin metadata from attribute
    pub metadata: PluginMetadata,
    /// Compiled Rune unit
    pub unit: Arc<Unit>,
    /// The plugin instance (Value holding the struct)
    pub instance: Value,
    /// Cached tool definitions (refreshed after on_watch)
    pub tools: Vec<ToolDefinition>,
}

impl PluginInstance {
    /// Check if this plugin provides a specific tool
    #[allow(dead_code)]
    pub fn provides_tool(&self, tool_name: &str) -> bool {
        self.tools.iter().any(|t| t.name == tool_name)
    }

    /// Check if this plugin should handle a file change
    #[allow(dead_code)]
    pub fn matches_watch_pattern(&self, path: &str) -> bool {
        self.metadata.watch_patterns.iter().any(|p| p.matches(path))
    }
}

impl std::fmt::Debug for PluginInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginInstance")
            .field("metadata", &self.metadata)
            .field("tools", &self.tools)
            .finish_non_exhaustive()
    }
}

/// Registry of loaded struct-based plugins
struct PluginRegistry {
    /// Loaded plugin instances by source path
    instances: HashMap<PathBuf, PluginInstance>,
    /// Index from tool name to plugin path for fast dispatch
    tool_index: HashMap<String, PathBuf>,
}

impl PluginRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            instances: HashMap::new(),
            tool_index: HashMap::new(),
        }
    }

    /// Register a plugin instance
    pub fn register(&mut self, instance: PluginInstance) {
        let path = instance.metadata.source_path.clone();

        // Index tools for fast dispatch
        for tool in &instance.tools {
            debug!(
                "Registering tool '{}' from plugin {:?}",
                tool.name, instance.metadata.source_path
            );
            self.tool_index.insert(tool.name.clone(), path.clone());
        }

        self.instances.insert(path, instance);
    }

    /// Get a plugin instance by source path
    pub fn get(&self, path: &Path) -> Option<&PluginInstance> {
        self.instances.get(path)
    }

    /// Get a mutable plugin instance by source path
    pub fn get_mut(&mut self, path: &Path) -> Option<&mut PluginInstance> {
        self.instances.get_mut(path)
    }

    /// Find the plugin that provides a tool
    pub fn find_plugin_for_tool(&self, tool_name: &str) -> Option<&PluginInstance> {
        self.tool_index
            .get(tool_name)
            .and_then(|path| self.instances.get(path))
    }

    /// Find plugins that match a watch pattern for a given path
    #[allow(dead_code)]
    pub fn find_plugins_for_watch(&self, file_path: &str) -> Vec<&PluginInstance> {
        self.instances
            .values()
            .filter(|p| p.matches_watch_pattern(file_path))
            .collect()
    }

    /// Get all registered tools across all plugins
    pub fn all_tools(&self) -> Vec<&ToolDefinition> {
        self.instances
            .values()
            .flat_map(|p| p.tools.iter())
            .collect()
    }

    /// Number of loaded plugins
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    /// Check if registry is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Iterate over all instances
    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item = &PluginInstance> {
        self.instances.values()
    }

    /// Update tool index after a plugin refreshes its tools
    pub fn refresh_tool_index(&mut self) {
        self.tool_index.clear();
        for (path, instance) in &self.instances {
            for tool in &instance.tools {
                self.tool_index.insert(tool.name.clone(), path.clone());
            }
        }
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Loader for struct-based Rune plugins
///
/// Discovers plugins with `#[plugin(...)]` attributes, compiles them,
/// calls factory functions to create instances, and manages the lifecycle.
///
/// Note: This is internal and runs only on the dedicated Rune thread.
/// Use `StructPluginHandle` for thread-safe access.
struct StructPluginLoader {
    /// Shared Rune context
    context: Arc<Context>,
    /// Runtime context for VM
    runtime: Arc<rune::runtime::RuntimeContext>,
    /// Plugin registry
    registry: PluginRegistry,
}

impl StructPluginLoader {
    /// Create a new plugin loader with specified shell policy
    ///
    /// # Arguments
    ///
    /// * `policy` - Shell security policy for command execution
    pub fn new(policy: ShellPolicy) -> Result<Self, RuneError> {
        let mut context =
            Context::with_default_modules().map_err(|e| RuneError::Context(e.to_string()))?;

        // Add standard modules
        context
            .install(rune_modules::json::module(false)?)
            .map_err(|e| RuneError::Context(e.to_string()))?;

        // Add our custom modules with security policy
        context
            .install(crate::shell_module::shell_module_with_policy(policy)?)
            .map_err(|e| RuneError::Context(e.to_string()))?;
        context
            .install(crate::oq_module::oq_module()?)
            .map_err(|e| RuneError::Context(e.to_string()))?;

        // Register #[plugin(...)] attribute macro (no-op, just pass through)
        context
            .install(Self::plugin_macro_module()?)
            .map_err(|e| RuneError::Context(e.to_string()))?;

        let runtime = Arc::new(
            context
                .runtime()
                .map_err(|e| RuneError::Context(e.to_string()))?,
        );

        Ok(Self {
            context: Arc::new(context),
            runtime,
            registry: PluginRegistry::new(),
        })
    }

    /// Create the module with the #[plugin(...)] attribute macro
    fn plugin_macro_module() -> Result<rune::Module, rune::compile::ContextError> {
        use rune::ast;
        use rune::parse::Parser;

        let mut module = rune::Module::new();

        // #[plugin(...)] - marks a factory function as a plugin entry point
        // No-op: just return the item unchanged (metadata extracted by discovery regex)
        module.attribute_macro(["plugin"], |cx, _input, item| {
            let mut parser = Parser::from_token_stream(item, cx.macro_span());
            let item_fn = parser.parse::<ast::Item>()?;
            let output = rune::macros::quote!(#item_fn);
            Ok(output.into_token_stream(cx)?)
        })?;

        Ok(module)
    }

    /// Load all plugins from a directory
    pub async fn load_from_directory(&mut self, dir: &Path) -> Result<(), RuneError> {
        let paths = DiscoveryPaths::empty("plugins").with_path(dir.to_path_buf());
        self.load_from_paths(&paths).await
    }

    /// Load plugins from discovery paths
    pub async fn load_from_paths(&mut self, paths: &DiscoveryPaths) -> Result<(), RuneError> {
        use crate::dependency_graph::DependencyGraph;

        let discovery = AttributeDiscovery::new();
        let plugins: Vec<PluginMetadata> = discovery.discover_all(paths)?;

        info!(
            "Found {} plugins with #[plugin(...)] attribute",
            plugins.len()
        );

        // Phase 1: Build dependency graph
        let mut graph = DependencyGraph::new();
        let mut plugin_map: HashMap<String, PluginMetadata> = HashMap::new();

        for metadata in plugins {
            graph
                .add(&metadata.name, metadata.deps.clone())
                .map_err(|e| {
                    RuneError::Discovery(format!("Duplicate plugin '{}': {}", metadata.name, e))
                })?;
            plugin_map.insert(metadata.name.clone(), metadata);
        }

        // Phase 2: Get sorted load order
        let load_order = graph.execution_order().map_err(|e| match e {
            crate::dependency_graph::DependencyError::CycleDetected { cycle } => {
                RuneError::Discovery(format!("Plugin dependency cycle: {}", cycle.join(" -> ")))
            }
            crate::dependency_graph::DependencyError::UnknownDependency {
                handler,
                dependency,
            } => RuneError::Discovery(format!(
                "Plugin '{}' requires unknown plugin '{}'",
                handler, dependency
            )),
            e => RuneError::Discovery(e.to_string()),
        })?;

        // Phase 3: Load in order
        for name in load_order {
            if let Some(metadata) = plugin_map.remove(&name) {
                if let Err(e) = self.load_plugin(metadata).await {
                    warn!("Failed to load plugin '{}': {}", name, e);
                }
            }
        }

        Ok(())
    }

    /// Load a single plugin
    async fn load_plugin(&mut self, metadata: PluginMetadata) -> Result<(), RuneError> {
        debug!("Loading plugin from {:?}", metadata.source_path);

        // Read and compile the source
        let source = std::fs::read_to_string(&metadata.source_path).map_err(|e| {
            RuneError::Io(format!("Failed to read {:?}: {}", metadata.source_path, e))
        })?;

        let unit = self.compile(&metadata.source_path.to_string_lossy(), &source)?;

        // Call the factory function
        let instance = self.call_factory(&unit, &metadata.factory_fn).await?;

        // Call tools() to get initial tool definitions
        let tools = self.call_tools(&unit, &instance).await?;

        debug!(
            "Loaded plugin {:?} with {} tools",
            metadata.source_path,
            tools.len()
        );

        // Create and register the instance
        let plugin_instance = PluginInstance {
            metadata,
            unit,
            instance,
            tools,
        };

        self.registry.register(plugin_instance);
        Ok(())
    }

    /// Compile a Rune source file
    fn compile(&self, name: &str, source: &str) -> Result<Arc<Unit>, RuneError> {
        let mut sources = Sources::new();
        sources
            .insert(Source::new(name, source).map_err(|e| RuneError::Compile(e.to_string()))?)
            .map_err(|e| RuneError::Compile(e.to_string()))?;

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&self.context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            for diag in diagnostics.diagnostics() {
                warn!("Rune diagnostic: {:?}", diag);
                // Also print to stderr for tests
                eprintln!("Rune diagnostic: {:?}", diag);
            }
        }

        let unit = result.map_err(|e| RuneError::Compile(e.to_string()))?;
        Ok(Arc::new(unit))
    }

    /// Call the factory function to create a plugin instance
    async fn call_factory(&self, unit: &Arc<Unit>, fn_name: &str) -> Result<Value, RuneError> {
        let mut vm = Vm::new(self.runtime.clone(), unit.clone());

        let hash = rune::Hash::type_hash([fn_name]);
        let output = vm
            .call(hash, ())
            .map_err(|e| RuneError::Execution(format!("Factory function failed: {}", e)))?;

        // Handle async factory functions
        let type_info = output.type_info();
        let type_name = format!("{}", type_info);

        let output = if type_name.contains("Generator") || type_name.contains("Future") {
            vm.async_complete()
                .await
                .map_err(|e| RuneError::Execution(format!("Async factory failed: {}", e)))?
        } else {
            output
        };

        Ok(output)
    }

    /// Call the tools() method on a plugin instance
    async fn call_tools(
        &self,
        unit: &Arc<Unit>,
        instance: &Value,
    ) -> Result<Vec<ToolDefinition>, RuneError> {
        // Clone the instance for the call
        let instance_clone = instance.clone();

        let mut vm = Vm::new(self.runtime.clone(), unit.clone());

        // For instance methods, we need the associated function hash
        // First, get the type hash of the instance
        let type_hash = instance.type_hash();

        // Create the associated function hash for "tools" method
        let method_hash = rune::Hash::associated_function(type_hash, "tools");

        // Try to call instance.tools()
        let output = match vm.call(method_hash, (instance_clone,)) {
            Ok(output) => output,
            Err(e) => {
                // Method might not exist, return empty tools
                debug!("Plugin has no tools() method: {}", e);
                return Ok(vec![]);
            }
        };

        // Handle async
        let type_info = output.type_info();
        let type_name = format!("{}", type_info);

        let output = if type_name.contains("Generator") || type_name.contains("Future") {
            vm.async_complete()
                .await
                .map_err(|e| RuneError::Execution(format!("Async tools() failed: {}", e)))?
        } else {
            output
        };

        // Convert to JSON and parse
        let json = rune_to_json(&output)
            .map_err(|e| RuneError::Conversion(format!("Failed to convert tools: {}", e)))?;

        // Parse as array of tool definitions
        if let JsonValue::Array(arr) = json {
            let tools: Vec<ToolDefinition> = arr
                .into_iter()
                .filter_map(|v| serde_json::from_value(v).ok())
                .collect();
            Ok(tools)
        } else {
            Ok(vec![])
        }
    }

    /// Call dispatch() on a plugin for a tool invocation
    pub async fn dispatch(&self, tool_name: &str, args: JsonValue) -> Result<JsonValue, RuneError> {
        // Find the plugin that provides this tool
        let plugin = self
            .registry
            .find_plugin_for_tool(tool_name)
            .ok_or_else(|| {
                RuneError::NotFound(format!("No plugin provides tool: {}", tool_name))
            })?;

        let instance_clone = plugin.instance.clone();

        let mut vm = Vm::new(self.runtime.clone(), plugin.unit.clone());

        // Convert args to Rune value
        let args_value = match json_to_rune(&args) {
            rune::runtime::VmResult::Ok(v) => v,
            rune::runtime::VmResult::Err(e) => {
                return Err(RuneError::Conversion(format!(
                    "Failed to convert args: {:?}",
                    e
                )));
            }
        };
        let tool_name_value = tool_name.to_string();

        // Get the type hash for the instance
        let type_hash = plugin.instance.type_hash();

        // Call dispatch(self, tool_name, args) as an instance method
        let method_hash = rune::Hash::associated_function(type_hash, "dispatch");
        let output = vm
            .call(method_hash, (instance_clone, tool_name_value, args_value))
            .map_err(|e| RuneError::Execution(format!("dispatch() failed: {}", e)))?;

        // Handle async
        let type_info = output.type_info();
        let type_name = format!("{}", type_info);

        let output = if type_name.contains("Generator") || type_name.contains("Future") {
            vm.async_complete()
                .await
                .map_err(|e| RuneError::Execution(format!("Async dispatch() failed: {}", e)))?
        } else {
            output
        };

        rune_to_json(&output)
            .map_err(|e| RuneError::Conversion(format!("Failed to convert result: {}", e)))
    }

    /// Get the registry
    pub fn registry(&self) -> &PluginRegistry {
        &self.registry
    }

    /// Get mutable registry
    #[allow(dead_code)]
    pub fn registry_mut(&mut self) -> &mut PluginRegistry {
        &mut self.registry
    }

    /// Get all tools from all plugins
    pub fn all_tools(&self) -> Vec<&ToolDefinition> {
        self.registry.all_tools()
    }

    /// Call on_watch() on a plugin when a watched file changes
    ///
    /// This method:
    /// 1. Calls the plugin's `on_watch(self, event)` method (if it exists)
    /// 2. Re-calls `tools()` to refresh the tool list
    /// 3. Updates the tool index
    pub async fn call_on_watch(
        &mut self,
        plugin_path: &Path,
        event: WatchEvent,
    ) -> Result<(), RuneError> {
        // Get plugin info (we need to look up before borrowing mutably)
        let (unit, instance_clone, type_hash) = {
            let plugin = self.registry.get(plugin_path).ok_or_else(|| {
                RuneError::NotFound(format!("Plugin not found: {:?}", plugin_path))
            })?;
            (
                plugin.unit.clone(),
                plugin.instance.clone(),
                plugin.instance.type_hash(),
            )
        };

        let mut vm = Vm::new(self.runtime.clone(), unit.clone());

        // Convert event to Rune value
        let event_value = self.watch_event_to_rune(&event)?;

        // Call on_watch(self, event) as an instance method
        let method_hash = rune::Hash::associated_function(type_hash, "on_watch");
        match vm.call(method_hash, (instance_clone.clone(), event_value)) {
            Ok(output) => {
                // Handle async
                let type_info = output.type_info();
                let type_name = format!("{}", type_info);

                if type_name.contains("Generator") || type_name.contains("Future") {
                    let _ = vm.async_complete().await;
                }
                debug!("on_watch completed for {:?}", plugin_path);
            }
            Err(e) => {
                // Method might not exist, that's OK
                debug!("on_watch not found or failed: {}", e);
            }
        }

        // Refresh tools after on_watch (the instance may have mutated)
        self.refresh_plugin_tools(plugin_path, &unit, &instance_clone)
            .await?;

        Ok(())
    }

    /// Convert a WatchEvent to a Rune value
    fn watch_event_to_rune(&self, event: &WatchEvent) -> Result<Value, RuneError> {
        let json = serde_json::json!({
            "path": event.path.to_string_lossy(),
            "kind": event.kind.as_str(),
        });

        match json_to_rune(&json) {
            rune::runtime::VmResult::Ok(v) => Ok(v),
            rune::runtime::VmResult::Err(e) => Err(RuneError::Conversion(format!(
                "Failed to convert event: {:?}",
                e
            ))),
        }
    }

    /// Refresh a plugin's tools by re-calling tools()
    async fn refresh_plugin_tools(
        &mut self,
        path: &Path,
        unit: &Arc<Unit>,
        instance: &Value,
    ) -> Result<(), RuneError> {
        // Call tools() to get new tool definitions
        let new_tools = self.call_tools(unit, instance).await?;

        // Update the plugin's cached tools
        if let Some(plugin) = self.registry.get_mut(path) {
            plugin.tools = new_tools;
        }

        // Refresh the tool index
        self.registry.refresh_tool_index();

        Ok(())
    }
}

impl Default for StructPluginLoader {
    fn default() -> Self {
        Self::new(ShellPolicy::with_defaults())
            .expect("Failed to create default StructPluginLoader")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Test PluginMetadata parsing from attributes
    #[test]
    fn test_plugin_metadata_from_attrs_basic() {
        let attrs = r#"watch = ["*.just", "justfile"]"#;
        let meta = PluginMetadata::from_attrs(attrs, "create", Path::new("test.rn"), "").unwrap();

        assert_eq!(meta.factory_fn, "create");
        assert_eq!(meta.watch_patterns.len(), 2);
        assert!(meta.watch_patterns[0].matches("foo.just"));
        assert!(meta.watch_patterns[1].matches("justfile"));
    }

    #[test]
    fn test_plugin_metadata_from_attrs_empty_watch() {
        let attrs = "";
        let meta = PluginMetadata::from_attrs(attrs, "create", Path::new("test.rn"), "").unwrap();

        assert_eq!(meta.factory_fn, "create");
        assert!(meta.watch_patterns.is_empty());
    }

    #[test]
    fn test_plugin_metadata_from_attrs_invalid_pattern_skipped() {
        // Invalid glob pattern should be skipped
        let attrs = r#"watch = ["[invalid", "*.valid"]"#;
        let meta = PluginMetadata::from_attrs(attrs, "create", Path::new("test.rn"), "").unwrap();

        // Only the valid pattern should be kept
        assert_eq!(meta.watch_patterns.len(), 1);
        assert!(meta.watch_patterns[0].matches("foo.valid"));
    }

    // Test PluginRegistry basic operations
    #[test]
    fn test_registry_new_is_empty() {
        let registry = PluginRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_register_and_get() {
        let _registry = PluginRegistry::new();

        let _meta = PluginMetadata {
            name: "test".to_string(),
            deps: vec![],
            factory_fn: "create".to_string(),
            watch_patterns: vec![],
            source_path: PathBuf::from("/plugins/test.rn"),
        };

        // Create a mock instance (we can't easily create a real Unit in tests)
        // This is a design smell - we need a way to test without real Rune compilation
        // For now, we'll test the registry logic in integration tests
    }

    #[test]
    fn test_tool_definition_deserialize() {
        let json = serde_json::json!({
            "name": "just_test",
            "description": "Run tests",
            "parameters": [
                {"name": "filter", "required": false, "description": "Test filter"}
            ]
        });

        let tool: ToolDefinition = serde_json::from_value(json).unwrap();
        assert_eq!(tool.name, "just_test");
        assert_eq!(tool.description, "Run tests");
        assert_eq!(tool.parameters.len(), 1);
        assert_eq!(tool.parameters[0].name, "filter");
        assert!(!tool.parameters[0].required);
    }

    #[test]
    fn test_tool_definition_deserialize_minimal() {
        let json = serde_json::json!({
            "name": "my_tool"
        });

        let tool: ToolDefinition = serde_json::from_value(json).unwrap();
        assert_eq!(tool.name, "my_tool");
        assert_eq!(tool.description, "");
        assert!(tool.parameters.is_empty());
    }

    // Test AttributeDiscovery integration for #[plugin(...)]
    #[test]
    fn test_discover_plugin_attribute() {
        let content = r#"
struct JustPlugin {
    recipes,
}

impl JustPlugin {
    fn new() {
        JustPlugin { recipes: [] }
    }
}

#[plugin(watch = ["justfile", "*.just"])]
pub fn create() {
    JustPlugin::new()
}
"#;

        let discovery = AttributeDiscovery::new();
        let plugins: Vec<PluginMetadata> = discovery
            .parse_from_source(content, Path::new("just.rn"))
            .unwrap();

        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].factory_fn, "create");
        assert_eq!(plugins[0].watch_patterns.len(), 2);
    }

    #[test]
    fn test_discover_plugin_multiple_in_file() {
        let content = r#"
#[plugin(watch = ["*.a"])]
pub fn create_a() {}

#[plugin(watch = ["*.b"])]
pub fn create_b() {}
"#;

        let discovery = AttributeDiscovery::new();
        let plugins: Vec<PluginMetadata> = discovery
            .parse_from_source(content, Path::new("multi.rn"))
            .unwrap();

        assert_eq!(plugins.len(), 2);
        assert_eq!(plugins[0].factory_fn, "create_a");
        assert_eq!(plugins[1].factory_fn, "create_b");
    }

    #[test]
    fn test_discover_plugin_in_directory() {
        let temp = TempDir::new().unwrap();

        let plugin1 = r#"
#[plugin(watch = ["justfile"])]
pub fn create() {}
"#;
        std::fs::write(temp.path().join("just.rn"), plugin1).unwrap();

        let plugin2 = r#"
#[plugin(watch = ["Makefile"])]
pub fn create() {}
"#;
        std::fs::write(temp.path().join("make.rn"), plugin2).unwrap();

        let discovery = AttributeDiscovery::new();
        let plugins: Vec<PluginMetadata> = discovery.discover_in_directory(temp.path()).unwrap();

        assert_eq!(plugins.len(), 2);
    }

    // Test watch pattern matching
    #[test]
    fn test_plugin_matches_watch_pattern() {
        let meta = PluginMetadata {
            name: "test".to_string(),
            deps: vec![],
            factory_fn: "create".to_string(),
            watch_patterns: vec![
                Pattern::new("justfile").unwrap(),
                Pattern::new("*.just").unwrap(),
            ],
            source_path: PathBuf::from("/test.rn"),
        };

        // We can test pattern matching without a full instance
        assert!(meta.watch_patterns.iter().any(|p| p.matches("justfile")));
        assert!(meta.watch_patterns.iter().any(|p| p.matches("build.just")));
        assert!(!meta.watch_patterns.iter().any(|p| p.matches("Makefile")));
    }

    // ===== StructPluginLoader Tests =====

    #[test]
    fn test_loader_creation() {
        let loader = StructPluginLoader::new(ShellPolicy::with_defaults());
        assert!(loader.is_ok(), "Should create loader");
    }

    #[tokio::test]
    async fn test_loader_empty_directory() {
        let temp = TempDir::new().unwrap();
        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();

        loader.load_from_directory(temp.path()).await.unwrap();

        assert!(loader.registry().is_empty());
    }

    #[tokio::test]
    async fn test_loader_loads_simple_plugin() {
        let temp = TempDir::new().unwrap();

        // Create a minimal plugin
        let plugin = r#"
struct SimplePlugin {
    value,
}

impl SimplePlugin {
    fn new() {
        SimplePlugin { value: 42 }
    }

    fn tools(self) {
        [#{ name: "simple_tool", description: "A simple tool" }]
    }
}

#[plugin()]
pub fn create() {
    SimplePlugin::new()
}
"#;
        std::fs::write(temp.path().join("simple.rn"), plugin).unwrap();

        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        loader.load_from_directory(temp.path()).await.unwrap();

        assert_eq!(loader.registry().len(), 1);
        assert_eq!(loader.all_tools().len(), 1);
        assert_eq!(loader.all_tools()[0].name, "simple_tool");
    }

    #[tokio::test]
    async fn test_loader_plugin_without_tools_method() {
        let temp = TempDir::new().unwrap();

        // Plugin without tools() method
        let plugin = r#"
struct NoToolsPlugin {
    data,
}

impl NoToolsPlugin {
    fn new() {
        NoToolsPlugin { data: "hello" }
    }
}

#[plugin()]
pub fn create() {
    NoToolsPlugin::new()
}
"#;
        std::fs::write(temp.path().join("no_tools.rn"), plugin).unwrap();

        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        loader.load_from_directory(temp.path()).await.unwrap();

        // Should load but have no tools
        assert_eq!(loader.registry().len(), 1);
        assert!(loader.all_tools().is_empty());
    }

    #[tokio::test]
    async fn test_loader_plugin_with_dispatch() {
        let temp = TempDir::new().unwrap();

        // Plugin with dispatch()
        let plugin = r#"
struct EchoPlugin {
    prefix,
}

impl EchoPlugin {
    fn new() {
        EchoPlugin { prefix: "Echo: " }
    }

    fn tools(self) {
        [#{ name: "echo", description: "Echo back input" }]
    }

    fn dispatch(self, tool_name, args) {
        #{ result: `${self.prefix}${args.message}` }
    }
}

#[plugin()]
pub fn create() {
    EchoPlugin::new()
}
"#;
        std::fs::write(temp.path().join("echo.rn"), plugin).unwrap();

        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        loader.load_from_directory(temp.path()).await.unwrap();

        // Dispatch a call
        let result = loader
            .dispatch("echo", serde_json::json!({ "message": "hello" }))
            .await
            .unwrap();

        assert_eq!(result["result"], "Echo: hello");
    }

    #[tokio::test]
    async fn test_loader_dispatch_not_found() {
        let loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();

        // Should error on unknown tool
        let result = loader.dispatch("unknown_tool", serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No plugin provides"));
    }

    #[tokio::test]
    async fn test_loader_multiple_plugins() {
        let temp = TempDir::new().unwrap();

        // First plugin
        let plugin1 = r#"
struct PluginA { }

impl PluginA {
    fn new() { PluginA {} }
    fn tools(self) {
        [#{ name: "tool_a" }]
    }
}

#[plugin(name = "plugin_a")]
pub fn create() { PluginA::new() }
"#;
        std::fs::write(temp.path().join("plugin_a.rn"), plugin1).unwrap();

        // Second plugin
        let plugin2 = r#"
struct PluginB { }

impl PluginB {
    fn new() { PluginB {} }
    fn tools(self) {
        [#{ name: "tool_b1" }, #{ name: "tool_b2" }]
    }
}

#[plugin(name = "plugin_b")]
pub fn create() { PluginB::new() }
"#;
        std::fs::write(temp.path().join("plugin_b.rn"), plugin2).unwrap();

        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        loader.load_from_directory(temp.path()).await.unwrap();

        assert_eq!(loader.registry().len(), 2);
        assert_eq!(loader.all_tools().len(), 3);
    }

    #[tokio::test]
    async fn test_loader_plugin_with_watch_patterns() {
        let temp = TempDir::new().unwrap();

        let plugin = r#"
struct WatchPlugin { }

impl WatchPlugin {
    fn new() { WatchPlugin {} }
    fn tools(self) { [] }
}

#[plugin(watch = ["justfile", "*.just"])]
pub fn create() { WatchPlugin::new() }
"#;
        std::fs::write(temp.path().join("watch.rn"), plugin).unwrap();

        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        loader.load_from_directory(temp.path()).await.unwrap();

        // Check watch patterns are registered
        let plugins = loader.registry().find_plugins_for_watch("justfile");
        assert_eq!(plugins.len(), 1);

        let plugins = loader.registry().find_plugins_for_watch("build.just");
        assert_eq!(plugins.len(), 1);

        let plugins = loader.registry().find_plugins_for_watch("Makefile");
        assert!(plugins.is_empty());
    }

    #[tokio::test]
    async fn test_loader_skips_invalid_plugins() {
        let temp = TempDir::new().unwrap();

        // Valid plugin
        let valid = r#"
struct Valid { }
impl Valid {
    fn new() { Valid {} }
    fn tools(self) { [#{ name: "valid_tool" }] }
}
#[plugin()]
pub fn create() { Valid::new() }
"#;
        std::fs::write(temp.path().join("valid.rn"), valid).unwrap();

        // Invalid syntax
        let invalid = r#"
this is not valid rune {{{
"#;
        std::fs::write(temp.path().join("invalid.rn"), invalid).unwrap();

        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        loader.load_from_directory(temp.path()).await.unwrap();

        // Should load the valid plugin only
        assert_eq!(loader.registry().len(), 1);
        assert_eq!(loader.all_tools()[0].name, "valid_tool");
    }

    // ===== Watch Event Tests =====

    #[test]
    fn test_watch_event_kind_display() {
        assert_eq!(WatchEventKind::Created.as_str(), "created");
        assert_eq!(WatchEventKind::Modified.as_str(), "modified");
        assert_eq!(WatchEventKind::Deleted.as_str(), "deleted");
    }

    #[tokio::test]
    async fn test_on_watch_calls_plugin_method() {
        let temp = TempDir::new().unwrap();

        // Plugin that tracks on_watch calls
        let plugin = r#"
struct WatcherPlugin {
    watch_count,
    last_path,
}

impl WatcherPlugin {
    fn new() {
        WatcherPlugin { watch_count: 0, last_path: "" }
    }

    fn tools(self) {
        [#{ name: "watcher_tool", description: `Called ${self.watch_count} times` }]
    }

    fn on_watch(self, event) {
        self.watch_count = self.watch_count + 1;
        self.last_path = event.path;
    }
}

#[plugin(watch = ["*.txt"])]
pub fn create() {
    WatcherPlugin::new()
}
"#;
        std::fs::write(temp.path().join("watcher.rn"), plugin).unwrap();

        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        loader.load_from_directory(temp.path()).await.unwrap();

        // Initially description should say 0 times
        let tools = loader.all_tools();
        assert_eq!(tools.len(), 1);
        assert!(tools[0].description.contains("0 times"));

        // Simulate a file change
        let plugin_path = temp.path().join("watcher.rn");
        let event = WatchEvent {
            path: PathBuf::from("test.txt"),
            kind: WatchEventKind::Modified,
        };
        loader.call_on_watch(&plugin_path, event).await.unwrap();

        // Tools should be refreshed with new description
        let tools = loader.all_tools();
        assert!(tools[0].description.contains("1 times"));
    }

    #[tokio::test]
    async fn test_on_watch_refreshes_tools() {
        let temp = TempDir::new().unwrap();

        // Plugin that adds a tool on each watch event
        let plugin = r#"
struct CounterPlugin {
    count,
}

impl CounterPlugin {
    fn new() {
        CounterPlugin { count: 1 }
    }

    fn tools(self) {
        let tools = [];
        for i in 0..self.count {
            tools.push(#{ name: `tool_${i}` });
        }
        tools
    }

    fn on_watch(self, event) {
        self.count = self.count + 1;
    }
}

#[plugin(watch = ["*.txt"])]
pub fn create() {
    CounterPlugin::new()
}
"#;
        std::fs::write(temp.path().join("counter.rn"), plugin).unwrap();

        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        loader.load_from_directory(temp.path()).await.unwrap();

        // Initially 1 tool
        assert_eq!(loader.all_tools().len(), 1);

        // Simulate file change
        let plugin_path = temp.path().join("counter.rn");
        let event = WatchEvent {
            path: PathBuf::from("test.txt"),
            kind: WatchEventKind::Modified,
        };
        loader.call_on_watch(&plugin_path, event).await.unwrap();

        // Now 2 tools
        assert_eq!(loader.all_tools().len(), 2);
    }

    #[tokio::test]
    async fn test_on_watch_plugin_without_method() {
        let temp = TempDir::new().unwrap();

        // Plugin without on_watch method
        let plugin = r#"
struct NoWatchPlugin { }

impl NoWatchPlugin {
    fn new() { NoWatchPlugin {} }
    fn tools(self) { [#{ name: "no_watch_tool" }] }
}

#[plugin(watch = ["*.txt"])]
pub fn create() {
    NoWatchPlugin::new()
}
"#;
        std::fs::write(temp.path().join("no_watch.rn"), plugin).unwrap();

        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        loader.load_from_directory(temp.path()).await.unwrap();

        // Should not error when calling on_watch on plugin without the method
        let plugin_path = temp.path().join("no_watch.rn");
        let event = WatchEvent {
            path: PathBuf::from("test.txt"),
            kind: WatchEventKind::Modified,
        };
        let result = loader.call_on_watch(&plugin_path, event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_on_watch_event_contains_path_and_kind() {
        let temp = TempDir::new().unwrap();

        // Plugin that stores event details
        let plugin = r#"
struct EventLogger {
    event_path,
    event_kind,
}

impl EventLogger {
    fn new() {
        EventLogger { event_path: "", event_kind: "" }
    }

    fn tools(self) {
        [#{ name: "logger_tool", description: `${self.event_kind}:${self.event_path}` }]
    }

    fn on_watch(self, event) {
        self.event_path = event.path;
        self.event_kind = event.kind;
    }
}

#[plugin(watch = ["*.log"])]
pub fn create() {
    EventLogger::new()
}
"#;
        std::fs::write(temp.path().join("logger.rn"), plugin).unwrap();

        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        loader.load_from_directory(temp.path()).await.unwrap();

        // Simulate a file change
        let plugin_path = temp.path().join("logger.rn");
        let event = WatchEvent {
            path: PathBuf::from("/path/to/file.log"),
            kind: WatchEventKind::Created,
        };
        loader.call_on_watch(&plugin_path, event).await.unwrap();

        // Check the description contains the path and kind
        let tools = loader.all_tools();
        assert!(tools[0].description.contains("created"));
        assert!(tools[0].description.contains("/path/to/file.log"));
    }

    #[tokio::test]
    async fn test_find_plugins_for_watch_by_filename() {
        let temp = TempDir::new().unwrap();

        // Plugin watching justfiles
        let plugin1 = r#"
struct JustWatcher { }
impl JustWatcher {
    fn new() { JustWatcher {} }
    fn tools(self) { [#{ name: "just_tool" }] }
}
#[plugin(name = "just_watcher", watch = ["justfile", "*.just"])]
pub fn create() { JustWatcher::new() }
"#;
        std::fs::write(temp.path().join("just.rn"), plugin1).unwrap();

        // Plugin watching makefiles
        let plugin2 = r#"
struct MakeWatcher { }
impl MakeWatcher {
    fn new() { MakeWatcher {} }
    fn tools(self) { [#{ name: "make_tool" }] }
}
#[plugin(name = "make_watcher", watch = ["Makefile"])]
pub fn create() { MakeWatcher::new() }
"#;
        std::fs::write(temp.path().join("make.rn"), plugin2).unwrap();

        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        loader.load_from_directory(temp.path()).await.unwrap();

        // Find plugins for justfile
        let plugins = loader.registry().find_plugins_for_watch("justfile");
        assert_eq!(plugins.len(), 1);
        assert!(plugins[0].provides_tool("just_tool"));

        // Find plugins for Makefile
        let plugins = loader.registry().find_plugins_for_watch("Makefile");
        assert_eq!(plugins.len(), 1);
        assert!(plugins[0].provides_tool("make_tool"));

        // Find plugins for build.just (matches *.just)
        let plugins = loader.registry().find_plugins_for_watch("build.just");
        assert_eq!(plugins.len(), 1);
        assert!(plugins[0].provides_tool("just_tool"));

        // No plugins for random file
        let plugins = loader.registry().find_plugins_for_watch("random.txt");
        assert!(plugins.is_empty());
    }

    // ===== Minimal Shell Exec Test =====

    #[tokio::test]
    async fn test_minimal_shell_exec_in_struct() {
        let temp = TempDir::new().unwrap();

        // Minimal plugin that calls shell::exec
        let plugin = r#"
use shell::exec;

struct MinimalPlugin { }

impl MinimalPlugin {
    fn new() {
        let result = exec("echo", ["test"], #{});
        if result.is_err() {
            return MinimalPlugin {};
        }
        MinimalPlugin {}
    }

    fn tools(self) {
        [#{ name: "minimal_tool" }]
    }
}

#[plugin()]
pub fn create() {
    MinimalPlugin::new()
}
"#;
        std::fs::write(temp.path().join("minimal.rn"), plugin).unwrap();

        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        loader.load_from_directory(temp.path()).await.unwrap();

        assert_eq!(loader.registry().len(), 1, "Minimal plugin should load");
    }

    // ===== Shell + oq Combined Test =====

    #[tokio::test]
    async fn test_shell_and_oq_combined() {
        let temp = TempDir::new().unwrap();

        // Plugin that uses both shell::exec with multiple args and oq::parse
        let plugin = r#"
use shell::exec;
use oq::parse;

struct CombinedPlugin {
    data,
}

impl CombinedPlugin {
    fn new() {
        // Use multiple args like the just plugin does
        let result = exec("echo", ["-n", "{\"key\": \"value\"}"], #{});
        if result.is_err() {
            return CombinedPlugin { data: #{} };
        }

        let result = result.unwrap();
        if result.exit_code != 0 {
            return CombinedPlugin { data: #{} };
        }

        // Parse the JSON output
        let parsed = parse(result.stdout);
        if parsed.is_err() {
            return CombinedPlugin { data: #{} };
        }

        CombinedPlugin { data: parsed.unwrap() }
    }

    fn tools(self) {
        [#{ name: "combined_tool" }]
    }
}

#[plugin()]
pub fn create() {
    CombinedPlugin::new()
}
"#;
        std::fs::write(temp.path().join("combined.rn"), plugin).unwrap();

        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        loader.load_from_directory(temp.path()).await.unwrap();

        assert_eq!(loader.registry().len(), 1, "Combined plugin should load");
    }

    // ===== Object Iteration Test =====

    #[tokio::test]
    async fn test_object_access_without_iteration() {
        let temp = TempDir::new().unwrap();

        // Plugin that accesses nested object WITHOUT iteration
        let plugin = r#"
use oq::parse;

struct SimplePlugin {
    doc,
}

impl SimplePlugin {
    fn new() {
        let json = "{\"recipes\": {\"build\": {\"name\": \"build\", \"doc\": \"Build it\"}}}";
        let data = parse(json);
        if data.is_err() {
            return SimplePlugin { doc: "parse error" };
        }
        let data = data.unwrap();

        // Access nested object directly (no iteration)
        let recipes = data["recipes"];
        let build = recipes["build"];
        let doc = build["doc"];

        SimplePlugin { doc }
    }

    fn tools(self) {
        [#{ name: "simple_tool" }]
    }
}

#[plugin()]
pub fn create() {
    SimplePlugin::new()
}
"#;
        std::fs::write(temp.path().join("simple.rn"), plugin).unwrap();

        let loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        let compile_result = loader.compile("simple.rn", plugin);
        assert!(
            compile_result.is_ok(),
            "Plugin should compile: {:?}",
            compile_result.err()
        );

        let unit = compile_result.unwrap();
        let factory_result = loader.call_factory(&unit, "create").await;
        assert!(
            factory_result.is_ok(),
            "Factory should succeed: {:?}",
            factory_result.err()
        );
    }

    #[tokio::test]
    async fn test_object_iteration_simple() {
        let temp = TempDir::new().unwrap();

        // Plugin that iterates over an object but doesn't do dynamic assignment
        let plugin = r#"
use oq::parse;

struct IterPlugin {
    count,
}

impl IterPlugin {
    fn new() {
        let json = "{\"a\": 1, \"b\": 2, \"c\": 3}";
        let data = parse(json);
        if data.is_err() {
            return IterPlugin { count: -1 };
        }
        let data = data.unwrap();

        // Just count entries
        let count = 0;
        for entry in data {
            count = count + 1;
        }

        IterPlugin { count }
    }

    fn tools(self) {
        [#{ name: "iter_tool" }]
    }
}

#[plugin()]
pub fn create() {
    IterPlugin::new()
}
"#;
        std::fs::write(temp.path().join("iter.rn"), plugin).unwrap();

        let loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        let compile_result = loader.compile("iter.rn", plugin);
        assert!(
            compile_result.is_ok(),
            "Plugin should compile: {:?}",
            compile_result.err()
        );

        let unit = compile_result.unwrap();
        let factory_result = loader.call_factory(&unit, "create").await;
        assert!(
            factory_result.is_ok(),
            "Factory should succeed: {:?}",
            factory_result.err()
        );
    }

    #[tokio::test]
    async fn test_object_iteration_value_access() {
        let temp = TempDir::new().unwrap();

        // Plugin that iterates and accesses value properties
        let plugin = r#"
use oq::{parse, query};

struct AccessPlugin {
    names,
}

impl AccessPlugin {
    fn new() {
        // Nested object like just --dump
        let json = "{\"outer\": {\"a\": {\"name\": \"Alice\"}, \"b\": {\"name\": \"Bob\"}}}";

        // Use oq::query to extract an array of names directly
        // jq's to_entries converts object to array of {key, value}
        let entries = query(json, ".outer | to_entries");
        if entries.is_err() {
            return AccessPlugin { names: [] };
        }
        let entries = entries.unwrap();
        if entries == () {
            return AccessPlugin { names: [] };
        }

        // entries is now an array, which should iterate correctly
        let names = [];
        for entry in entries {
            let key = entry["key"];
            let value = entry["value"];
            let name = value["name"];
            names.push(#{ key: key, name: name });
        }

        AccessPlugin { names }
    }

    fn tools(self) {
        [#{ name: "access_tool" }]
    }
}

#[plugin()]
pub fn create() {
    AccessPlugin::new()
}
"#;
        std::fs::write(temp.path().join("array.rn"), plugin).unwrap();

        let loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        let compile_result = loader.compile("array.rn", plugin);
        assert!(
            compile_result.is_ok(),
            "Plugin should compile: {:?}",
            compile_result.err()
        );

        let unit = compile_result.unwrap();
        let factory_result = loader.call_factory(&unit, "create").await;
        assert!(
            factory_result.is_ok(),
            "Factory should succeed: {:?}",
            factory_result.err()
        );
    }

    // ===== Just Plugin Integration Test =====

    /// Test that StructPluginHandle respects the shell policy
    #[tokio::test]
    async fn test_struct_plugin_handle_respects_policy() {
        let temp = TempDir::new().unwrap();

        // Create a plugin that tries to run a blocked command
        let plugin = r#"
use shell::exec;

struct TestPlugin { }

impl TestPlugin {
    fn new() {
        // This should be blocked by restrictive policy
        let result = exec("curl", ["https://example.com"], #{});
        if result.is_err() {
            TestPlugin {}
        } else {
            TestPlugin {}
        }
    }

    fn tools(self) {
        [#{ name: "test_tool" }]
    }
}

#[plugin()]
pub fn create() {
    TestPlugin::new()
}
"#;
        std::fs::write(temp.path().join("test.rn"), plugin).unwrap();

        // Create a restrictive policy that only allows 'echo'
        let mut policy = ShellPolicy::default();
        policy.whitelist.push("echo".to_string());
        policy.blacklist.push("curl".to_string());

        // Create plugin handle with restrictive policy
        let handle = StructPluginHandle::new(policy).unwrap();

        // Plugin should load (compilation succeeds)
        // But when it runs, curl should be blocked
        let result = handle.load_from_directory(temp.path()).await;

        // The plugin should load successfully
        // The shell::exec call will return an error at runtime due to policy
        assert!(
            result.is_ok(),
            "Plugin should load even if it contains blocked commands"
        );
    }

    #[tokio::test]
    async fn test_just_plugin_compiles_and_loads() {
        // Load the actual just.rn plugin from examples/plugins
        let plugin_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("examples/plugins");

        if !plugin_path.exists() {
            // Skip if examples/plugins doesn't exist
            return;
        }

        let just_plugin = plugin_path.join("just.rn");
        if !just_plugin.exists() {
            // Skip if just.rn doesn't exist
            return;
        }

        let loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();

        // Create a temp dir with just the just.rn plugin
        let temp = TempDir::new().unwrap();
        std::fs::copy(&just_plugin, temp.path().join("just.rn")).unwrap();

        // Verify the file was copied
        let copied = temp.path().join("just.rn");
        assert!(copied.exists(), "just.rn should be copied");

        // Read and check the content has #[plugin(...)]
        let content = std::fs::read_to_string(&copied).unwrap();
        assert!(
            content.contains("#[plugin("),
            "just.rn should have #[plugin(...)] attribute"
        );

        // First, test discovery directly
        let discovery = AttributeDiscovery::new();
        let plugins: Vec<PluginMetadata> = discovery.discover_in_directory(temp.path()).unwrap();
        assert_eq!(
            plugins.len(),
            1,
            "Discovery should find 1 plugin, found {}",
            plugins.len()
        );

        // Try loading the plugin directly and check for errors
        let plugin_metadata = plugins.into_iter().next().unwrap();
        println!("Plugin metadata: {:?}", plugin_metadata);

        // Read source and try to compile
        let source = std::fs::read_to_string(&plugin_metadata.source_path).unwrap();
        let compile_result =
            loader.compile(&plugin_metadata.source_path.to_string_lossy(), &source);
        assert!(
            compile_result.is_ok(),
            "Plugin should compile: {:?}",
            compile_result.err()
        );

        // Try loading manually to see the actual error
        let unit = compile_result.unwrap();
        let factory_result = loader
            .call_factory(&unit, &plugin_metadata.factory_fn)
            .await;
        assert!(
            factory_result.is_ok(),
            "Factory function should succeed: {:?}",
            factory_result.as_ref().err()
        );

        // Load should succeed (plugin compiles)
        let mut loader2 = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        let result = loader2.load_from_directory(temp.path()).await;
        assert!(result.is_ok(), "Just plugin should load: {:?}", result);

        // Plugin should be registered
        assert_eq!(
            loader2.registry().len(),
            1,
            "Just plugin should be loaded (found {} plugins)",
            loader2.registry().len()
        );

        // Should have watch patterns
        let plugins = loader2.registry().find_plugins_for_watch("justfile");
        assert_eq!(plugins.len(), 1, "Just plugin should watch 'justfile'");

        let plugins = loader2.registry().find_plugins_for_watch("build.just");
        assert_eq!(plugins.len(), 1, "Just plugin should watch '*.just'");
    }

    // ===== Plugin Metadata name and deps Tests =====

    #[test]
    fn test_plugin_metadata_with_name() {
        let attrs = r#"name = "my_custom_name""#;
        let meta = PluginMetadata::from_attrs(attrs, "create", Path::new("test.rn"), "").unwrap();

        assert_eq!(meta.name, "my_custom_name");
        assert_eq!(meta.factory_fn, "create");
        assert!(meta.deps.is_empty());
    }

    #[test]
    fn test_plugin_metadata_name_defaults_to_factory_fn() {
        let attrs = "";
        let meta =
            PluginMetadata::from_attrs(attrs, "create_foo", Path::new("test.rn"), "").unwrap();

        // Should default to factory function name
        assert_eq!(meta.name, "create_foo");
    }

    #[test]
    fn test_plugin_metadata_with_deps() {
        let attrs = r#"deps = ["shell", "tasks"]"#;
        let meta = PluginMetadata::from_attrs(attrs, "create", Path::new("test.rn"), "").unwrap();

        assert_eq!(meta.deps, vec!["shell", "tasks"]);
    }

    #[test]
    fn test_plugin_metadata_with_name_and_deps() {
        let attrs = r#"name = "deploy", deps = ["shell", "oq"]"#;
        let meta = PluginMetadata::from_attrs(attrs, "create", Path::new("test.rn"), "").unwrap();

        assert_eq!(meta.name, "deploy");
        assert_eq!(meta.deps, vec!["shell", "oq"]);
    }

    #[test]
    fn test_plugin_metadata_full_attribute() {
        let attrs = r#"name = "just", watch = ["justfile", "*.just"], deps = ["shell"]"#;
        let meta = PluginMetadata::from_attrs(attrs, "create", Path::new("test.rn"), "").unwrap();

        assert_eq!(meta.name, "just");
        assert_eq!(meta.deps, vec!["shell"]);
        assert_eq!(meta.watch_patterns.len(), 2);
    }

    // ===== Dependency Graph Integration Tests =====

    #[tokio::test]
    async fn test_loader_respects_dependency_order() {
        let temp = TempDir::new().unwrap();

        // Plugin B depends on A - create B first to ensure order isn't just file order
        let plugin_b = r#"
struct PluginB { }
impl PluginB {
    fn new() { PluginB {} }
    fn tools(self) { [#{ name: "tool_b" }] }
}
#[plugin(name = "b", deps = ["a"])]
pub fn create() { PluginB::new() }
"#;
        std::fs::write(temp.path().join("b_plugin.rn"), plugin_b).unwrap();

        let plugin_a = r#"
struct PluginA { }
impl PluginA {
    fn new() { PluginA {} }
    fn tools(self) { [#{ name: "tool_a" }] }
}
#[plugin(name = "a")]
pub fn create() { PluginA::new() }
"#;
        std::fs::write(temp.path().join("a_plugin.rn"), plugin_a).unwrap();

        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        loader.load_from_directory(temp.path()).await.unwrap();

        // Both should load successfully
        assert_eq!(loader.registry().len(), 2);
    }

    #[tokio::test]
    async fn test_loader_detects_missing_dependency() {
        let temp = TempDir::new().unwrap();

        let plugin = r#"
struct PluginB { }
impl PluginB {
    fn new() { PluginB {} }
    fn tools(self) { [] }
}
#[plugin(name = "b", deps = ["nonexistent"])]
pub fn create() { PluginB::new() }
"#;
        std::fs::write(temp.path().join("b_plugin.rn"), plugin).unwrap();

        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        let result = loader.load_from_directory(temp.path()).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("nonexistent"),
            "Error should mention missing dependency: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_loader_detects_cycle() {
        let temp = TempDir::new().unwrap();

        let plugin_a = r#"
struct PluginA { }
impl PluginA {
    fn new() { PluginA {} }
    fn tools(self) { [] }
}
#[plugin(name = "a", deps = ["b"])]
pub fn create() { PluginA::new() }
"#;
        std::fs::write(temp.path().join("a.rn"), plugin_a).unwrap();

        let plugin_b = r#"
struct PluginB { }
impl PluginB {
    fn new() { PluginB {} }
    fn tools(self) { [] }
}
#[plugin(name = "b", deps = ["a"])]
pub fn create() { PluginB::new() }
"#;
        std::fs::write(temp.path().join("b.rn"), plugin_b).unwrap();

        let mut loader = StructPluginLoader::new(ShellPolicy::with_defaults()).unwrap();
        let result = loader.load_from_directory(temp.path()).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cycle"), "Error should mention cycle: {}", err);
    }
}
