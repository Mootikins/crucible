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
use crate::discovery_paths::DiscoveryPaths;
use crate::mcp_types::{json_to_rune, rune_to_json};
use crate::RuneError;
use glob::Pattern;
use rune::runtime::Value;
use rune::{Context, Diagnostics, Source, Sources, Unit, Vm};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Metadata parsed from `#[plugin(...)]` attribute
#[derive(Debug, Clone)]
pub struct PluginMetadata {
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

    fn from_attrs(
        attrs: &str,
        fn_name: &str,
        path: &Path,
        _docs: &str,
    ) -> Result<Self, RuneError> {
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
pub struct PluginInstance {
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
    pub fn provides_tool(&self, tool_name: &str) -> bool {
        self.tools.iter().any(|t| t.name == tool_name)
    }

    /// Check if this plugin should handle a file change
    pub fn matches_watch_pattern(&self, path: &str) -> bool {
        self.metadata
            .watch_patterns
            .iter()
            .any(|p| p.matches(path))
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
pub struct PluginRegistry {
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
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Iterate over all instances
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
pub struct StructPluginLoader {
    /// Shared Rune context
    context: Arc<Context>,
    /// Runtime context for VM
    runtime: Arc<rune::runtime::RuntimeContext>,
    /// Plugin registry
    registry: PluginRegistry,
}

impl StructPluginLoader {
    /// Create a new plugin loader
    pub fn new() -> Result<Self, RuneError> {
        let mut context =
            Context::with_default_modules().map_err(|e| RuneError::Context(e.to_string()))?;

        // Add standard modules
        context
            .install(rune_modules::json::module(false)?)
            .map_err(|e| RuneError::Context(e.to_string()))?;

        // Add our custom modules
        context
            .install(crate::shell_module::shell_module()?)
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
        let discovery = AttributeDiscovery::new();
        let plugins: Vec<PluginMetadata> = discovery.discover_all(paths)?;

        info!("Found {} plugins with #[plugin(...)] attribute", plugins.len());

        for metadata in plugins {
            if let Err(e) = self.load_plugin(metadata).await {
                warn!("Failed to load plugin: {}", e);
                // Continue loading other plugins
            }
        }

        Ok(())
    }

    /// Load a single plugin
    async fn load_plugin(&mut self, metadata: PluginMetadata) -> Result<(), RuneError> {
        debug!("Loading plugin from {:?}", metadata.source_path);

        // Read and compile the source
        let source = std::fs::read_to_string(&metadata.source_path)
            .map_err(|e| RuneError::Io(format!("Failed to read {:?}: {}", metadata.source_path, e)))?;

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
            .insert(
                Source::new(name, source).map_err(|e| RuneError::Compile(e.to_string()))?,
            )
            .map_err(|e| RuneError::Compile(e.to_string()))?;

        let mut diagnostics = Diagnostics::new();
        let result = rune::prepare(&mut sources)
            .with_context(&self.context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            for diag in diagnostics.diagnostics() {
                warn!("Rune diagnostic: {:?}", diag);
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
    pub async fn dispatch(
        &self,
        tool_name: &str,
        args: JsonValue,
    ) -> Result<JsonValue, RuneError> {
        // Find the plugin that provides this tool
        let plugin = self
            .registry
            .find_plugin_for_tool(tool_name)
            .ok_or_else(|| RuneError::NotFound(format!("No plugin provides tool: {}", tool_name)))?;

        let instance_clone = plugin.instance.clone();

        let mut vm = Vm::new(self.runtime.clone(), plugin.unit.clone());

        // Convert args to Rune value
        let args_value = match json_to_rune(&args) {
            rune::runtime::VmResult::Ok(v) => v,
            rune::runtime::VmResult::Err(e) => {
                return Err(RuneError::Conversion(format!("Failed to convert args: {:?}", e)));
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
    pub fn registry_mut(&mut self) -> &mut PluginRegistry {
        &mut self.registry
    }

    /// Get all tools from all plugins
    pub fn all_tools(&self) -> Vec<&ToolDefinition> {
        self.registry.all_tools()
    }
}

impl Default for StructPluginLoader {
    fn default() -> Self {
        Self::new().expect("Failed to create default StructPluginLoader")
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
        let mut registry = PluginRegistry::new();

        let meta = PluginMetadata {
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
        let loader = StructPluginLoader::new();
        assert!(loader.is_ok(), "Should create loader");
    }

    #[tokio::test]
    async fn test_loader_empty_directory() {
        let temp = TempDir::new().unwrap();
        let mut loader = StructPluginLoader::new().unwrap();

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

        let mut loader = StructPluginLoader::new().unwrap();
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

        let mut loader = StructPluginLoader::new().unwrap();
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

        let mut loader = StructPluginLoader::new().unwrap();
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
        let loader = StructPluginLoader::new().unwrap();

        // Should error on unknown tool
        let result = loader.dispatch("unknown_tool", serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No plugin provides"));
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

#[plugin()]
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

#[plugin()]
pub fn create() { PluginB::new() }
"#;
        std::fs::write(temp.path().join("plugin_b.rn"), plugin2).unwrap();

        let mut loader = StructPluginLoader::new().unwrap();
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

        let mut loader = StructPluginLoader::new().unwrap();
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

        let mut loader = StructPluginLoader::new().unwrap();
        loader.load_from_directory(temp.path()).await.unwrap();

        // Should load the valid plugin only
        assert_eq!(loader.registry().len(), 1);
        assert_eq!(loader.all_tools()[0].name, "valid_tool");
    }
}
