//! Plugin lifecycle management

use crate::annotations::{
    DiscoveredCommand, DiscoveredHandler, DiscoveredParam, DiscoveredService, DiscoveredTool,
    DiscoveredView,
};
use crate::manifest::{Capability, LoadedPlugin, ManifestError, PluginManifest, PluginState};
use mlua::{Lua, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;
use tracing::{debug, info, warn};

static REGISTRATION_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Error)]
pub enum LifecycleError {
    #[error("Manifest error: {0}")]
    Manifest(#[from] ManifestError),

    #[error("Plugin not found: {0}")]
    NotFound(String),

    #[error("Plugin already loaded: {0}")]
    AlreadyLoaded(String),

    #[error("Dependency not satisfied: {plugin} requires {dependency}")]
    DependencyNotSatisfied { plugin: String, dependency: String },

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("Load error: {0}")]
    LoadError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type LifecycleResult<T> = Result<T, LifecycleError>;

/// Spec extracted from a plugin's returned Lua table.
///
/// When a plugin's `init.lua` returns a table, this struct captures the
/// declared metadata and exports. Fields that aren't present in the table
/// are left as `None`/empty.
#[derive(Debug, Clone, Default)]
pub struct PluginSpec {
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub capabilities: Vec<String>,
    pub tools: Vec<DiscoveredTool>,
    pub commands: Vec<DiscoveredCommand>,
    pub handlers: Vec<DiscoveredHandler>,
    pub views: Vec<DiscoveredView>,
    pub services: Vec<DiscoveredService>,
    pub has_setup: bool,
}

/// Handle for unregistering programmatically-added items
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegistrationHandle(u64);

impl RegistrationHandle {
    fn new() -> Self {
        Self(REGISTRATION_COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone)]
struct RegisteredItem<T> {
    item: T,
    handle: RegistrationHandle,
    owner: Option<String>,
}

pub struct PluginManager {
    plugins: HashMap<String, LoadedPlugin>,
    search_paths: Vec<PathBuf>,
    tools: Vec<RegisteredItem<DiscoveredTool>>,
    commands: Vec<RegisteredItem<DiscoveredCommand>>,
    views: Vec<RegisteredItem<DiscoveredView>>,
    handlers: Vec<RegisteredItem<DiscoveredHandler>>,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for PluginManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginManager")
            .field("plugins", &self.plugins)
            .field("search_paths", &self.search_paths)
            .field("tools_count", &self.tools.len())
            .field("commands_count", &self.commands.len())
            .field("views_count", &self.views.len())
            .field("handlers_count", &self.handlers.len())
            .finish()
    }
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            search_paths: Vec::new(),
            tools: Vec::new(),
            commands: Vec::new(),
            views: Vec::new(),
            handlers: Vec::new(),
        }
    }

    pub fn with_standard_paths(kiln_path: Option<&Path>) -> Self {
        let mut paths = Vec::new();

        if let Ok(env_paths) = std::env::var("CRUCIBLE_PLUGIN_PATH") {
            let separator = if cfg!(windows) { ';' } else { ':' };
            for p in env_paths.split(separator) {
                let path = PathBuf::from(p);
                if !p.is_empty() && !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }

        if let Some(config_dir) = dirs::config_dir() {
            paths.push(config_dir.join("crucible").join("plugins"));
        }

        if let Some(kiln) = kiln_path {
            paths.push(kiln.join("plugins"));
        }

        Self::new().with_search_paths(paths)
    }

    pub fn initialize(kiln_path: Option<&Path>) -> LifecycleResult<Self> {
        let mut manager = Self::with_standard_paths(kiln_path);
        manager.discover()?;
        manager.load_all()?;
        Ok(manager)
    }

    pub fn with_search_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.search_paths = paths;
        self
    }

    pub fn add_search_path(&mut self, path: PathBuf) {
        if !self.search_paths.contains(&path) {
            self.search_paths.push(path);
        }
    }

    pub fn discover(&mut self) -> LifecycleResult<Vec<String>> {
        let mut discovered = Vec::new();

        for search_path in &self.search_paths.clone() {
            if !search_path.exists() {
                debug!("Search path does not exist: {}", search_path.display());
                continue;
            }

            for entry in std::fs::read_dir(search_path)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    // Try manifest first, then fall back to manifest-less discovery
                    match PluginManifest::discover(&path) {
                        Ok(Some(manifest)) => {
                            let name = manifest.name.clone();
                            if self.plugins.contains_key(&name) {
                                debug!("Plugin already discovered: {}", name);
                                continue;
                            }
                            info!("Discovered plugin: {} v{}", name, manifest.version);
                            let plugin = LoadedPlugin::new(manifest, path);
                            self.plugins.insert(name.clone(), plugin);
                            discovered.push(name);
                        }
                        Ok(None) => {
                            // No manifest — check for init.lua (manifest-less plugin)
                            if path.join("init.lua").exists() {
                                match PluginManifest::from_directory_defaults(&path) {
                                    Ok(manifest) => {
                                        let name = manifest.name.clone();
                                        if self.plugins.contains_key(&name) {
                                            debug!("Plugin already discovered: {}", name);
                                            continue;
                                        }
                                        info!(
                                            "Discovered manifest-less plugin: {} (from {})",
                                            name,
                                            path.display()
                                        );
                                        let plugin = LoadedPlugin::new(manifest, path);
                                        self.plugins.insert(name.clone(), plugin);
                                        discovered.push(name);
                                    }
                                    Err(e) => {
                                        warn!(
                                            "Failed to create default manifest for {}: {}",
                                            path.display(),
                                            e
                                        );
                                    }
                                }
                            } else {
                                debug!("No manifest or init.lua in: {}", path.display());
                            }
                        }
                        Err(e) => {
                            warn!("Failed to load manifest from {}: {}", path.display(), e);
                        }
                    }
                } else if path.is_file() {
                    // Single-file plugin: .lua or .fnl file directly in plugins dir
                    let ext = path.extension().and_then(|e| e.to_str());
                    if matches!(ext, Some("lua") | Some("fnl")) {
                        let stem = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string();

                        if stem.is_empty() || self.plugins.contains_key(&stem) {
                            continue;
                        }

                        // Validate as plugin name
                        match PluginManifest::from_yaml(&format!(
                            "name: \"{}\"\nversion: \"0.0.0\"\nmain: \"{}\"\nexports:\n  auto_discover: false\n",
                            stem,
                            path.file_name().unwrap().to_string_lossy()
                        )) {
                            Ok(manifest) => {
                                let name = manifest.name.clone();
                                // Use the search_path as the plugin dir for single-file plugins
                                let plugin = LoadedPlugin::new(manifest, search_path.clone());
                                info!(
                                    "Discovered single-file plugin: {} ({})",
                                    name,
                                    path.display()
                                );
                                self.plugins.insert(name.clone(), plugin);
                                discovered.push(name);
                            }
                            Err(e) => {
                                debug!("Skipping file {}: {}", path.display(), e);
                            }
                        }
                    }
                }
            }
        }

        Ok(discovered)
    }

    pub fn load(&mut self, name: &str) -> LifecycleResult<()> {
        let (is_enabled, required_deps, main_path, current_state) = {
            let plugin = self
                .plugins
                .get(name)
                .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;

            let deps: Vec<String> = plugin
                .manifest
                .required_dependencies()
                .map(|d| d.name.clone())
                .collect();

            (
                plugin.manifest.is_enabled(),
                deps,
                plugin.main_path(),
                plugin.state,
            )
        };

        if current_state == PluginState::Active {
            return Err(LifecycleError::AlreadyLoaded(name.to_string()));
        }

        if !is_enabled {
            if let Some(plugin) = self.plugins.get_mut(name) {
                plugin.state = PluginState::Disabled;
            }
            info!("Plugin {} is disabled, skipping load", name);
            return Ok(());
        }

        for dep_name in &required_deps {
            if !self.plugins.contains_key(dep_name) {
                return Err(LifecycleError::DependencyNotSatisfied {
                    plugin: name.to_string(),
                    dependency: dep_name.clone(),
                });
            }

            let dep_plugin = &self.plugins[dep_name];
            if dep_plugin.state != PluginState::Active {
                return Err(LifecycleError::DependencyNotSatisfied {
                    plugin: name.to_string(),
                    dependency: dep_name.clone(),
                });
            }
        }

        if !main_path.exists() {
            return Err(LifecycleError::LoadError(format!(
                "Main file not found: {}",
                main_path.display()
            )));
        }

        self.discover_exports_for_plugin(name)?;

        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;
        plugin.state = PluginState::Active;
        info!("Loaded plugin: {} v{}", name, plugin.version());

        Ok(())
    }

    pub fn load_all(&mut self) -> LifecycleResult<Vec<String>> {
        let names: Vec<String> = self.plugins.keys().cloned().collect();
        let order = self.resolve_load_order(&names)?;

        let mut loaded = Vec::new();
        for name in order {
            match self.load(&name) {
                Ok(()) => loaded.push(name),
                Err(LifecycleError::AlreadyLoaded(_)) => {}
                Err(e) => {
                    warn!("Failed to load plugin {}: {}", name, e);
                    if let Some(plugin) = self.plugins.get_mut(&name) {
                        plugin.state = PluginState::Error;
                    }
                }
            }
        }

        Ok(loaded)
    }

    pub fn unload(&mut self, name: &str) -> LifecycleResult<()> {
        let (current_state, plugin_dir) = {
            let plugin = self
                .plugins
                .get(name)
                .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;
            (plugin.state, plugin.dir.clone())
        };

        if current_state != PluginState::Active {
            return Ok(());
        }

        for (other_name, other_plugin) in &self.plugins {
            if other_name == name {
                continue;
            }
            if other_plugin.state == PluginState::Active {
                for dep in &other_plugin.manifest.dependencies {
                    if dep.name == name && !dep.optional {
                        return Err(LifecycleError::LoadError(format!(
                            "Cannot unload {}: {} depends on it",
                            name, other_name
                        )));
                    }
                }
            }
        }

        let dir_prefix = plugin_dir.to_string_lossy();
        self.tools
            .retain(|t| !t.item.source_path.starts_with(dir_prefix.as_ref()));
        self.commands
            .retain(|c| !c.item.source_path.starts_with(dir_prefix.as_ref()));
        self.views
            .retain(|v| !v.item.source_path.starts_with(dir_prefix.as_ref()));
        self.handlers
            .retain(|h| !h.item.source_path.starts_with(dir_prefix.as_ref()));

        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;
        plugin.state = PluginState::Discovered;
        info!("Unloaded plugin: {}", name);

        Ok(())
    }

    pub fn reload(&mut self, name: &str) -> LifecycleResult<()> {
        self.unload(name)?;
        self.load(name)
    }

    pub fn enable(&mut self, name: &str) -> LifecycleResult<()> {
        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;

        plugin.manifest.enabled = Some(true);

        if plugin.state == PluginState::Disabled {
            self.load(name)?;
        }

        Ok(())
    }

    pub fn disable(&mut self, name: &str) -> LifecycleResult<()> {
        self.unload(name)?;

        let plugin = self
            .plugins
            .get_mut(name)
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;

        plugin.manifest.enabled = Some(false);
        plugin.state = PluginState::Disabled;

        Ok(())
    }

    fn resolve_load_order(&self, names: &[String]) -> LifecycleResult<Vec<String>> {
        let mut order = Vec::new();
        let mut visited = HashMap::new();

        for name in names {
            self.visit_for_order(name, &mut visited, &mut order)?;
        }

        Ok(order)
    }

    fn visit_for_order(
        &self,
        name: &str,
        visited: &mut HashMap<String, bool>,
        order: &mut Vec<String>,
    ) -> LifecycleResult<()> {
        match visited.get(name) {
            Some(true) => return Ok(()),
            Some(false) => {
                return Err(LifecycleError::CircularDependency(name.to_string()));
            }
            None => {}
        }

        visited.insert(name.to_string(), false);

        if let Some(plugin) = self.plugins.get(name) {
            for dep in &plugin.manifest.dependencies {
                if !dep.optional && self.plugins.contains_key(&dep.name) {
                    self.visit_for_order(&dep.name, visited, order)?;
                }
            }
        }

        visited.insert(name.to_string(), true);
        order.push(name.to_string());

        Ok(())
    }

    fn discover_exports_for_plugin(&mut self, name: &str) -> LifecycleResult<()> {
        let plugin = self
            .plugins
            .get(name)
            .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;

        let plugin_dir = plugin.dir.clone();
        let main_path = plugin.main_path();
        let auto_discover = plugin.manifest.exports.auto_discover;

        debug!(
            "Discovering exports for plugin {}: auto_discover={}, dir={}",
            name,
            auto_discover,
            plugin_dir.display()
        );

        // Try spec-based loading first (execute init.lua/init.fnl, inspect returned table)
        if main_path.exists()
            && main_path
                .extension()
                .is_some_and(|e| e == "lua" || e == "fnl")
        {
            match load_plugin_spec(&main_path) {
                Ok(Some(spec)) => {
                    debug!(
                        "Loaded spec for plugin {}: {} tools, {} commands, {} handlers, {} views",
                        name,
                        spec.tools.len(),
                        spec.commands.len(),
                        spec.handlers.len(),
                        spec.views.len()
                    );

                    // Update manifest metadata from spec if available
                    if let Some(plugin) = self.plugins.get_mut(name) {
                        if let Some(ref spec_name) = spec.name {
                            // Only override if manifest came from directory defaults
                            if plugin.manifest.version == "0.0.0" {
                                plugin.manifest.name = spec_name.clone();
                            }
                        }
                        if let Some(ref spec_version) = spec.version {
                            if plugin.manifest.version == "0.0.0" {
                                plugin.manifest.version = spec_version.clone();
                            }
                        }
                        if let Some(ref spec_desc) = spec.description {
                            if plugin.manifest.description.is_empty() {
                                plugin.manifest.description = spec_desc.clone();
                            }
                        }
                        // Merge capabilities from spec
                        for cap_str in &spec.capabilities {
                            if let Some(cap) = parse_capability(cap_str) {
                                if !plugin.manifest.capabilities.contains(&cap) {
                                    plugin.manifest.capabilities.push(cap);
                                }
                            }
                        }
                    }

                    // Register all exports from spec
                    let source_path = main_path.to_string_lossy().to_string();
                    self.register_spec_exports(spec, &source_path, name);

                    return Ok(());
                }
                Ok(None) => {
                    debug!(
                        "Plugin {} init.lua returned nil/non-table, no spec exports registered",
                        name
                    );
                }
                Err(e) => {
                    warn!("Failed to load spec for plugin {}: {}", name, e);
                }
            }
        }

        Ok(())
    }

    fn register_spec_exports(&mut self, spec: PluginSpec, source_path: &str, owner: &str) {
        for tool in spec.tools {
            if !self.tools.iter().any(|t| t.item.name == tool.name) {
                debug!("Registered tool from spec: {}", tool.name);
                self.tools.push(RegisteredItem {
                    item: tool,
                    handle: RegistrationHandle::new(),
                    owner: Some(owner.to_string()),
                });
            }
        }

        for cmd in spec.commands {
            if !self.commands.iter().any(|c| c.item.name == cmd.name) {
                debug!("Registered command from spec: {}", cmd.name);
                self.commands.push(RegisteredItem {
                    item: cmd,
                    handle: RegistrationHandle::new(),
                    owner: Some(owner.to_string()),
                });
            }
        }

        for handler in spec.handlers {
            if !self.handlers.iter().any(|h| h.item.name == handler.name) {
                debug!("Registered handler from spec: {}", handler.name);
                self.handlers.push(RegisteredItem {
                    item: handler,
                    handle: RegistrationHandle::new(),
                    owner: Some(owner.to_string()),
                });
            }
        }

        for view in spec.views {
            if !self.views.iter().any(|v| v.item.name == view.name) {
                debug!("Registered view from spec: {}", view.name);
                self.views.push(RegisteredItem {
                    item: view,
                    handle: RegistrationHandle::new(),
                    owner: Some(owner.to_string()),
                });
            }
        }

        let _ = source_path; // used for debug context
    }

    pub fn get(&self, name: &str) -> Option<&LoadedPlugin> {
        self.plugins.get(name)
    }

    pub fn list(&self) -> impl Iterator<Item = &LoadedPlugin> {
        self.plugins.values()
    }

    pub fn active_plugins(&self) -> impl Iterator<Item = &LoadedPlugin> {
        self.plugins
            .values()
            .filter(|p| p.state == PluginState::Active)
    }

    pub fn tools(&self) -> Vec<&DiscoveredTool> {
        self.tools.iter().map(|t| &t.item).collect()
    }

    pub fn commands(&self) -> Vec<&DiscoveredCommand> {
        self.commands.iter().map(|c| &c.item).collect()
    }

    pub fn views(&self) -> Vec<&DiscoveredView> {
        self.views.iter().map(|v| &v.item).collect()
    }

    pub fn handlers(&self) -> Vec<&DiscoveredHandler> {
        self.handlers.iter().map(|h| &h.item).collect()
    }

    pub fn plugin_has_capability(&self, name: &str, cap: Capability) -> bool {
        self.plugins
            .get(name)
            .is_some_and(|p| p.manifest.has_capability(cap))
    }

    pub fn register_tool(
        &mut self,
        tool: DiscoveredTool,
        owner: Option<&str>,
    ) -> RegistrationHandle {
        let handle = RegistrationHandle::new();
        self.tools.push(RegisteredItem {
            item: tool,
            handle,
            owner: owner.map(String::from),
        });
        handle
    }

    pub fn register_command(
        &mut self,
        command: DiscoveredCommand,
        owner: Option<&str>,
    ) -> RegistrationHandle {
        let handle = RegistrationHandle::new();
        self.commands.push(RegisteredItem {
            item: command,
            handle,
            owner: owner.map(String::from),
        });
        handle
    }

    pub fn register_view(
        &mut self,
        view: DiscoveredView,
        owner: Option<&str>,
    ) -> RegistrationHandle {
        let handle = RegistrationHandle::new();
        self.views.push(RegisteredItem {
            item: view,
            handle,
            owner: owner.map(String::from),
        });
        handle
    }

    pub fn register_handler(
        &mut self,
        handler: DiscoveredHandler,
        owner: Option<&str>,
    ) -> RegistrationHandle {
        let handle = RegistrationHandle::new();
        self.handlers.push(RegisteredItem {
            item: handler,
            handle,
            owner: owner.map(String::from),
        });
        handle
    }

    pub fn unregister(&mut self, handle: RegistrationHandle) -> bool {
        let mut removed = false;

        if let Some(pos) = self.tools.iter().position(|t| t.handle == handle) {
            self.tools.remove(pos);
            removed = true;
        }
        if let Some(pos) = self.commands.iter().position(|c| c.handle == handle) {
            self.commands.remove(pos);
            removed = true;
        }
        if let Some(pos) = self.views.iter().position(|v| v.handle == handle) {
            self.views.remove(pos);
            removed = true;
        }
        if let Some(pos) = self.handlers.iter().position(|h| h.handle == handle) {
            self.handlers.remove(pos);
            removed = true;
        }

        removed
    }

    pub fn unregister_by_owner(&mut self, owner: &str) -> usize {
        let before =
            self.tools.len() + self.commands.len() + self.views.len() + self.handlers.len();

        let matches_owner =
            |item_owner: &Option<String>| item_owner.as_ref().is_some_and(|o| o == owner);

        self.tools.retain(|t| !matches_owner(&t.owner));
        self.commands.retain(|c| !matches_owner(&c.owner));
        self.views.retain(|v| !matches_owner(&v.owner));
        self.handlers.retain(|h| !matches_owner(&h.owner));

        let after = self.tools.len() + self.commands.len() + self.views.len() + self.handlers.len();
        before - after
    }
}

#[derive(Debug, Clone)]
pub struct ToolBuilder {
    name: String,
    description: String,
    params: Vec<DiscoveredParam>,
    return_type: Option<String>,
    source_path: String,
    is_fennel: bool,
}

impl ToolBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            params: Vec::new(),
            return_type: None,
            source_path: "<programmatic>".to_string(),
            is_fennel: false,
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn param(mut self, name: impl Into<String>, param_type: impl Into<String>) -> Self {
        self.params.push(DiscoveredParam {
            name: name.into(),
            param_type: param_type.into(),
            description: String::new(),
            optional: false,
        });
        self
    }

    pub fn param_optional(
        mut self,
        name: impl Into<String>,
        param_type: impl Into<String>,
    ) -> Self {
        self.params.push(DiscoveredParam {
            name: name.into(),
            param_type: param_type.into(),
            description: String::new(),
            optional: true,
        });
        self
    }

    pub fn param_full(
        mut self,
        name: impl Into<String>,
        param_type: impl Into<String>,
        description: impl Into<String>,
        optional: bool,
    ) -> Self {
        self.params.push(DiscoveredParam {
            name: name.into(),
            param_type: param_type.into(),
            description: description.into(),
            optional,
        });
        self
    }

    pub fn returns(mut self, return_type: impl Into<String>) -> Self {
        self.return_type = Some(return_type.into());
        self
    }

    pub fn source_path(mut self, path: impl Into<String>) -> Self {
        self.source_path = path.into();
        self
    }

    pub fn fennel(mut self) -> Self {
        self.is_fennel = true;
        self
    }

    pub fn build(self) -> DiscoveredTool {
        DiscoveredTool {
            name: self.name,
            description: self.description,
            params: self.params,
            return_type: self.return_type,
            source_path: self.source_path,
            is_fennel: self.is_fennel,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandBuilder {
    name: String,
    description: String,
    params: Vec<DiscoveredParam>,
    input_hint: Option<String>,
    source_path: String,
    handler_fn: String,
    is_fennel: bool,
}

impl CommandBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            handler_fn: name.clone(),
            name,
            description: String::new(),
            params: Vec::new(),
            input_hint: None,
            source_path: "<programmatic>".to_string(),
            is_fennel: false,
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.input_hint = Some(hint.into());
        self
    }

    pub fn handler_fn(mut self, handler: impl Into<String>) -> Self {
        self.handler_fn = handler.into();
        self
    }

    pub fn param(mut self, name: impl Into<String>, param_type: impl Into<String>) -> Self {
        self.params.push(DiscoveredParam {
            name: name.into(),
            param_type: param_type.into(),
            description: String::new(),
            optional: false,
        });
        self
    }

    pub fn source_path(mut self, path: impl Into<String>) -> Self {
        self.source_path = path.into();
        self
    }

    pub fn build(self) -> DiscoveredCommand {
        DiscoveredCommand {
            name: self.name,
            description: self.description,
            params: self.params,
            input_hint: self.input_hint,
            source_path: self.source_path,
            handler_fn: self.handler_fn,
            is_fennel: self.is_fennel,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HandlerBuilder {
    name: String,
    event_type: String,
    pattern: String,
    priority: i64,
    description: String,
    source_path: String,
    handler_fn: String,
    is_fennel: bool,
}

impl HandlerBuilder {
    pub fn new(name: impl Into<String>, event_type: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            handler_fn: name.clone(),
            name,
            event_type: event_type.into(),
            pattern: "*".to_string(),
            priority: 100,
            description: String::new(),
            source_path: "<programmatic>".to_string(),
            is_fennel: false,
        }
    }

    pub fn pattern(mut self, pattern: impl Into<String>) -> Self {
        self.pattern = pattern.into();
        self
    }

    pub fn priority(mut self, priority: i64) -> Self {
        self.priority = priority;
        self
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn handler_fn(mut self, handler: impl Into<String>) -> Self {
        self.handler_fn = handler.into();
        self
    }

    pub fn source_path(mut self, path: impl Into<String>) -> Self {
        self.source_path = path.into();
        self
    }

    pub fn build(self) -> DiscoveredHandler {
        DiscoveredHandler {
            name: self.name,
            event_type: self.event_type,
            pattern: self.pattern,
            priority: self.priority,
            description: self.description,
            source_path: self.source_path,
            handler_fn: self.handler_fn,
            is_fennel: self.is_fennel,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ViewBuilder {
    name: String,
    description: String,
    source_path: String,
    view_fn: String,
    handler_fn: Option<String>,
    is_fennel: bool,
}

impl ViewBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            view_fn: name.clone(),
            name,
            description: String::new(),
            source_path: "<programmatic>".to_string(),
            handler_fn: None,
            is_fennel: false,
        }
    }

    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn view_fn(mut self, view: impl Into<String>) -> Self {
        self.view_fn = view.into();
        self
    }

    pub fn handler_fn(mut self, handler: impl Into<String>) -> Self {
        self.handler_fn = Some(handler.into());
        self
    }

    pub fn source_path(mut self, path: impl Into<String>) -> Self {
        self.source_path = path.into();
        self
    }

    pub fn build(self) -> DiscoveredView {
        DiscoveredView {
            name: self.name,
            description: self.description,
            source_path: self.source_path,
            view_fn: self.view_fn,
            handler_fn: self.handler_fn,
            is_fennel: self.is_fennel,
        }
    }
}

/// Parse a capability string (from Lua spec) to a Capability enum.
fn parse_capability(s: &str) -> Option<Capability> {
    match s.to_lowercase().as_str() {
        "filesystem" => Some(Capability::Filesystem),
        "network" => Some(Capability::Network),
        "shell" => Some(Capability::Shell),
        "vault" => Some(Capability::Vault),
        "agent" => Some(Capability::Agent),
        "ui" => Some(Capability::Ui),
        "config" => Some(Capability::Config),
        "system" => Some(Capability::System),
        "websocket" => Some(Capability::WebSocket),
        _ => None,
    }
}

/// Set up a permissive sandbox for spec extraction.
///
/// Stubs `require()`, `crucible`, `cru`, and `io` so that plugin init files
/// can be evaluated for their return table without crashing on missing runtime
/// dependencies. The stubs are no-ops — we only care about the spec table structure.
fn setup_spec_sandbox(lua: &Lua) -> Result<(), mlua::Error> {
    lua.load(
        r#"
-- Stub require: return an empty table that tolerates any method call
local stub_mt = {}
stub_mt.__index = function() return function() return setmetatable({}, stub_mt) end end
stub_mt.__call = function() return setmetatable({}, stub_mt) end

local _real_require = require
require = function(name)
    local ok, mod = pcall(_real_require, name)
    if ok then return mod end
    return setmetatable({}, stub_mt)
end

-- Stub crucible namespace
crucible = setmetatable({}, stub_mt)

-- Stub cru namespace
cru = setmetatable({}, stub_mt)

-- Stub io (some plugins use io.open at load time)
if not io then io = setmetatable({}, stub_mt) end
"#,
    )
    .exec()?;
    Ok(())
}

/// Execute a plugin's init.lua and extract a PluginSpec from the returned table.
///
/// Returns `Ok(Some(spec))` if the script returns a table with recognized fields,
/// `Ok(None)` if it returns nil or a non-table value,
/// or `Err` if there's a Lua execution error.
pub fn load_plugin_spec(init_path: &Path) -> LifecycleResult<Option<PluginSpec>> {
    let source = std::fs::read_to_string(init_path).map_err(LifecycleError::Io)?;

    // Compile Fennel to Lua if needed
    let is_fennel = init_path.extension().is_some_and(|ext| ext == "fnl");

    if is_fennel {
        #[cfg(feature = "fennel")]
        {
            let lua_source = crate::fennel::compile_fennel(&source).map_err(|e| {
                LifecycleError::LoadError(format!(
                    "Fennel compilation failed for {}: {}",
                    init_path.display(),
                    e
                ))
            })?;
            return load_plugin_spec_from_source(&lua_source, init_path);
        }
        #[cfg(not(feature = "fennel"))]
        {
            return Err(LifecycleError::LoadError(format!(
                "Fennel file {} requires the 'fennel' feature",
                init_path.display()
            )));
        }
    }

    load_plugin_spec_from_source(&source, init_path)
}

/// Extract a PluginSpec from Lua source code. Exposed for testing.
pub fn load_plugin_spec_from_source(
    source: &str,
    source_path: &Path,
) -> LifecycleResult<Option<PluginSpec>> {
    let lua = Lua::new();
    let source_path_str = source_path.to_string_lossy().to_string();
    let is_fennel = source_path.extension().is_some_and(|ext| ext == "fnl");

    // Set up a permissive environment so plugins that use require(), crucible.*,
    // cru.*, io.*, etc. don't crash before we can read their spec table.
    setup_spec_sandbox(&lua)
        .map_err(|e| LifecycleError::LoadError(format!("Failed to set up spec sandbox: {}", e)))?;

    // Execute the source and capture the return value
    let result: Value = lua
        .load(source)
        .set_name(source_path_str.as_str())
        .eval()
        .map_err(|e| {
            LifecycleError::LoadError(format!("Lua error in {}: {}", source_path_str, e))
        })?;

    let table = match result {
        Value::Table(t) => t,
        Value::Nil => return Ok(None),
        _ => return Ok(None),
    };

    // Determine if this is a spec table vs a plain module table.
    // A spec table has at least one recognized declarative field.
    let spec_fields = [
        "name", "version", "tools", "commands", "handlers", "views", "setup",
    ];
    let has_spec_field = spec_fields
        .iter()
        .any(|&field| !matches!(table.get::<Value>(field), Ok(Value::Nil) | Err(_)));

    if !has_spec_field {
        // Plain module table (e.g., `local M = {}; return M`) — not a spec
        return Ok(None);
    }

    let mut spec = PluginSpec {
        name: table.get::<Option<String>>("name").unwrap_or(None),
        version: table.get::<Option<String>>("version").unwrap_or(None),
        description: table.get::<Option<String>>("description").unwrap_or(None),
        ..Default::default()
    };

    // Extract capabilities
    if let Ok(Value::Table(caps)) = table.get::<Value>("capabilities") {
        for i in 1..=caps.raw_len() {
            if let Ok(s) = caps.get::<String>(i) {
                spec.capabilities.push(s);
            }
        }
    }

    // Extract tools
    if let Ok(Value::Table(tools_table)) = table.get::<Value>("tools") {
        for pair in tools_table.pairs::<String, Value>() {
            if let Ok((tool_name, Value::Table(tool_def))) = pair {
                let desc = tool_def
                    .get::<Option<String>>("desc")
                    .unwrap_or(None)
                    .unwrap_or_default();

                let mut params = Vec::new();
                if let Ok(Value::Table(params_table)) = tool_def.get::<Value>("params") {
                    for i in 1..=params_table.raw_len() {
                        if let Ok(Value::Table(param_def)) = params_table.get::<Value>(i) {
                            let pname = param_def
                                .get::<Option<String>>("name")
                                .unwrap_or(None)
                                .unwrap_or_default();
                            let ptype = param_def
                                .get::<Option<String>>("type")
                                .unwrap_or(None)
                                .unwrap_or_else(|| "string".to_string());
                            let pdesc = param_def
                                .get::<Option<String>>("desc")
                                .unwrap_or(None)
                                .unwrap_or_default();
                            let optional = param_def
                                .get::<Option<bool>>("optional")
                                .unwrap_or(None)
                                .unwrap_or(false);

                            params.push(DiscoveredParam {
                                name: pname,
                                param_type: ptype,
                                description: pdesc,
                                optional,
                            });
                        }
                    }
                }

                spec.tools.push(DiscoveredTool {
                    name: tool_name,
                    description: desc,
                    params,
                    return_type: None,
                    source_path: source_path_str.clone(),
                    is_fennel,
                });
            }
        }
    }

    // Extract commands
    if let Ok(Value::Table(cmds_table)) = table.get::<Value>("commands") {
        for pair in cmds_table.pairs::<String, Value>() {
            if let Ok((cmd_name, Value::Table(cmd_def))) = pair {
                let desc = cmd_def
                    .get::<Option<String>>("desc")
                    .unwrap_or(None)
                    .unwrap_or_default();
                let hint = cmd_def.get::<Option<String>>("hint").unwrap_or(None);

                // Extract params if present
                let mut params = Vec::new();
                if let Ok(Value::Table(params_table)) = cmd_def.get::<Value>("params") {
                    for i in 1..=params_table.raw_len() {
                        if let Ok(Value::Table(param_def)) = params_table.get::<Value>(i) {
                            let pname = param_def
                                .get::<Option<String>>("name")
                                .unwrap_or(None)
                                .unwrap_or_default();
                            let ptype = param_def
                                .get::<Option<String>>("type")
                                .unwrap_or(None)
                                .unwrap_or_else(|| "string".to_string());
                            let pdesc = param_def
                                .get::<Option<String>>("desc")
                                .unwrap_or(None)
                                .unwrap_or_default();
                            let optional = param_def
                                .get::<Option<bool>>("optional")
                                .unwrap_or(None)
                                .unwrap_or(false);

                            params.push(DiscoveredParam {
                                name: pname,
                                param_type: ptype,
                                description: pdesc,
                                optional,
                            });
                        }
                    }
                }

                spec.commands.push(DiscoveredCommand {
                    name: cmd_name.clone(),
                    description: desc,
                    params,
                    input_hint: hint,
                    source_path: source_path_str.clone(),
                    handler_fn: cmd_name,
                    is_fennel,
                });
            }
        }
    }

    // Extract handlers
    if let Ok(Value::Table(handlers_table)) = table.get::<Value>("handlers") {
        for i in 1..=handlers_table.raw_len() {
            if let Ok(Value::Table(handler_def)) = handlers_table.get::<Value>(i) {
                let event = handler_def
                    .get::<Option<String>>("event")
                    .unwrap_or(None)
                    .unwrap_or_default();
                let priority = handler_def
                    .get::<Option<i64>>("priority")
                    .unwrap_or(None)
                    .unwrap_or(100);
                let pattern = handler_def
                    .get::<Option<String>>("pattern")
                    .unwrap_or(None)
                    .unwrap_or_else(|| "*".to_string());
                let name = handler_def
                    .get::<Option<String>>("name")
                    .unwrap_or(None)
                    .unwrap_or_else(|| format!("handler_{}", i));
                let desc = handler_def
                    .get::<Option<String>>("desc")
                    .unwrap_or(None)
                    .unwrap_or_default();

                if !event.is_empty() {
                    spec.handlers.push(DiscoveredHandler {
                        name: name.clone(),
                        event_type: event,
                        pattern,
                        priority,
                        description: desc,
                        source_path: source_path_str.clone(),
                        handler_fn: name,
                        is_fennel,
                    });
                }
            }
        }
    }

    // Extract views
    if let Ok(Value::Table(views_table)) = table.get::<Value>("views") {
        for pair in views_table.pairs::<String, Value>() {
            if let Ok((view_name, Value::Table(view_def))) = pair {
                let desc = view_def
                    .get::<Option<String>>("desc")
                    .unwrap_or(None)
                    .unwrap_or_default();
                // Check if handler fn is present (it's a Lua function, so we just check for non-nil)
                let has_handler =
                    matches!(view_def.get::<Value>("handler"), Ok(Value::Function(_)));

                spec.views.push(DiscoveredView {
                    name: view_name.clone(),
                    description: desc,
                    source_path: source_path_str.clone(),
                    view_fn: view_name.clone(),
                    handler_fn: if has_handler {
                        Some(format!("{}_handler", view_name))
                    } else {
                        None
                    },
                    is_fennel,
                });
            }
        }
    }

    // Extract services
    if let Ok(Value::Table(services_table)) = table.get::<Value>("services") {
        for pair in services_table.pairs::<String, Value>() {
            if let Ok((service_name, Value::Table(service_def))) = pair {
                let desc = service_def
                    .get::<Option<String>>("desc")
                    .unwrap_or(None)
                    .unwrap_or_default();
                let has_fn = matches!(service_def.get::<Value>("fn"), Ok(Value::Function(_)));
                if has_fn {
                    spec.services.push(DiscoveredService {
                        name: service_name.clone(),
                        description: desc,
                        source_path: source_path_str.clone(),
                        service_fn: service_name,
                    });
                }
            }
        }
    }

    // Check for setup function
    spec.has_setup = matches!(table.get::<Value>("setup"), Ok(Value::Function(_)));

    Ok(Some(spec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_plugin(dir: &Path, name: &str, version: &str) -> PathBuf {
        let plugin_dir = dir.join(name);
        std::fs::create_dir_all(&plugin_dir).unwrap();

        let manifest = format!(
            r#"
name: {name}
version: "{version}"
main: init.lua
"#
        );
        std::fs::write(plugin_dir.join("plugin.yaml"), manifest).unwrap();

        let lua = format!(
            r#"
local M = {{}}
function M.test_tool()
    return "ok"
end
return {{
    name = "{name}",
    version = "{version}",
    tools = {{
        test_tool = {{
            desc = "A test tool",
            fn = M.test_tool,
        }},
    }},
}}
"#
        );
        std::fs::write(plugin_dir.join("init.lua"), lua).unwrap();

        plugin_dir
    }

    fn create_plugin_with_deps(dir: &Path, name: &str, deps: &[&str]) -> PathBuf {
        let plugin_dir = dir.join(name);
        std::fs::create_dir_all(&plugin_dir).unwrap();

        let deps_yaml: String = deps
            .iter()
            .map(|d| format!("  - name: {d}"))
            .collect::<Vec<_>>()
            .join("\n");

        let manifest = format!(
            r#"
name: {name}
version: "1.0.0"
main: init.lua
dependencies:
{deps_yaml}
"#
        );
        std::fs::write(plugin_dir.join("plugin.yaml"), manifest).unwrap();
        std::fs::write(plugin_dir.join("init.lua"), "-- empty").unwrap();

        plugin_dir
    }

    #[test]
    fn test_discover_plugins() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(temp.path(), "plugin-a", "1.0.0");
        create_test_plugin(temp.path(), "plugin-b", "2.0.0");

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);

        let discovered = manager.discover().unwrap();
        assert_eq!(discovered.len(), 2);
        assert!(discovered.contains(&"plugin-a".to_string()));
        assert!(discovered.contains(&"plugin-b".to_string()));
    }

    #[test]
    fn test_load_plugin() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(temp.path(), "test-plugin", "1.0.0");

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();
        manager.load("test-plugin").unwrap();

        let plugin = manager.get("test-plugin").unwrap();
        assert_eq!(plugin.state, PluginState::Active);
    }

    #[test]
    fn test_load_discovers_tools() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(temp.path(), "tool-plugin", "1.0.0");

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();
        manager.load("tool-plugin").unwrap();

        assert_eq!(manager.tools().len(), 1);
        assert_eq!(manager.tools()[0].name, "test_tool");
    }

    #[test]
    fn test_unload_plugin() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(temp.path(), "unload-test", "1.0.0");

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();
        manager.load("unload-test").unwrap();
        assert_eq!(manager.tools().len(), 1);

        manager.unload("unload-test").unwrap();
        let plugin = manager.get("unload-test").unwrap();
        assert_eq!(plugin.state, PluginState::Discovered);
        assert_eq!(manager.tools().len(), 0);
    }

    #[test]
    fn test_reload_plugin() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(temp.path(), "reload-test", "1.0.0");

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();
        manager.load("reload-test").unwrap();

        manager.reload("reload-test").unwrap();
        let plugin = manager.get("reload-test").unwrap();
        assert_eq!(plugin.state, PluginState::Active);
    }

    #[test]
    fn test_dependency_order() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(temp.path(), "base", "1.0.0");
        create_plugin_with_deps(temp.path(), "dependent", &["base"]);

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();

        let loaded = manager.load_all().unwrap();
        let base_idx = loaded.iter().position(|n| n == "base").unwrap();
        let dep_idx = loaded.iter().position(|n| n == "dependent").unwrap();
        assert!(base_idx < dep_idx, "base should load before dependent");
    }

    #[test]
    fn test_missing_dependency_error() {
        let temp = TempDir::new().unwrap();
        create_plugin_with_deps(temp.path(), "orphan", &["missing"]);

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();

        let result = manager.load("orphan");
        assert!(matches!(
            result,
            Err(LifecycleError::DependencyNotSatisfied { .. })
        ));
    }

    #[test]
    fn test_circular_dependency_detection() {
        let temp = TempDir::new().unwrap();
        create_plugin_with_deps(temp.path(), "a", &["b"]);
        create_plugin_with_deps(temp.path(), "b", &["a"]);

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();

        let result = manager.load_all();
        assert!(matches!(result, Err(LifecycleError::CircularDependency(_))));
    }

    #[test]
    fn test_enable_disable() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(temp.path(), "toggle-test", "1.0.0");

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();
        manager.load("toggle-test").unwrap();

        manager.disable("toggle-test").unwrap();
        let plugin = manager.get("toggle-test").unwrap();
        assert_eq!(plugin.state, PluginState::Disabled);

        manager.enable("toggle-test").unwrap();
        let plugin = manager.get("toggle-test").unwrap();
        assert_eq!(plugin.state, PluginState::Active);
    }

    #[test]
    fn test_cannot_unload_if_depended_upon() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(temp.path(), "core", "1.0.0");
        create_plugin_with_deps(temp.path(), "extension", &["core"]);

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();
        manager.load_all().unwrap();

        let result = manager.unload("core");
        assert!(matches!(result, Err(LifecycleError::LoadError(_))));
    }

    #[test]
    fn test_disabled_plugin_skipped() {
        let temp = TempDir::new().unwrap();
        let plugin_dir = temp.path().join("disabled-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();

        let manifest = r#"
name: disabled-plugin
version: "1.0.0"
enabled: false
"#;
        std::fs::write(plugin_dir.join("plugin.yaml"), manifest).unwrap();
        std::fs::write(plugin_dir.join("init.lua"), "-- empty").unwrap();

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();
        manager.load("disabled-plugin").unwrap();

        let plugin = manager.get("disabled-plugin").unwrap();
        assert_eq!(plugin.state, PluginState::Disabled);
    }

    #[test]
    fn test_active_plugins_iterator() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(temp.path(), "active", "1.0.0");

        let plugin_dir = temp.path().join("inactive");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.yaml"),
            "name: inactive\nversion: \"1.0.0\"\nenabled: false",
        )
        .unwrap();
        std::fs::write(plugin_dir.join("init.lua"), "").unwrap();

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();
        manager.load_all().unwrap();

        let active: Vec<_> = manager.active_plugins().collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name(), "active");
    }

    #[test]
    fn test_load_example_plugins_from_docs() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let plugins_dir = manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("docs")
            .join("plugins");

        if !plugins_dir.exists() {
            panic!(
                "Example plugins directory not found: {}",
                plugins_dir.display()
            );
        }

        let mut manager = PluginManager::new().with_search_paths(vec![plugins_dir.clone()]);

        let discovered = manager.discover().unwrap();
        assert!(
            discovered.len() >= 3,
            "Expected at least 3 example plugins, found {}: {:?}",
            discovered.len(),
            discovered
        );

        assert!(
            discovered.contains(&"todo-list".to_string()),
            "todo-list plugin not discovered"
        );
        assert!(
            discovered.contains(&"daily-notes".to_string()),
            "daily-notes plugin not discovered"
        );
        assert!(
            discovered.contains(&"graph-view".to_string()),
            "graph-view plugin not discovered"
        );

        let loaded = manager.load_all().unwrap();
        assert!(
            loaded.len() >= 3,
            "Expected at least 3 plugins loaded, got {}: {:?}",
            loaded.len(),
            loaded
        );

        for name in &["todo-list", "daily-notes", "graph-view"] {
            let plugin = manager
                .get(name)
                .expect(&format!("{} should be loaded", name));
            assert_eq!(
                plugin.state,
                PluginState::Active,
                "{} should be active",
                name
            );
        }

        assert!(
            !manager.tools().is_empty(),
            "Should have discovered tools from plugins"
        );
        assert!(
            !manager.commands().is_empty(),
            "Should have discovered commands from plugins"
        );
        assert!(
            !manager.views().is_empty(),
            "Should have discovered views from plugins"
        );

        let tool_names: Vec<_> = manager.tools().iter().map(|t| &t.name).collect();
        assert!(
            tool_names.contains(&&"tasks_list".to_string()),
            "tasks_list tool not found"
        );
        assert!(
            tool_names.contains(&&"daily_create".to_string()),
            "daily_create tool not found"
        );
        assert!(
            tool_names.contains(&&"graph_stats".to_string()),
            "graph_stats tool not found"
        );

        let view_names: Vec<_> = manager.views().iter().map(|v| &v.name).collect();
        assert!(
            view_names.contains(&&"graph".to_string()),
            "graph view not found"
        );

        let views = manager.views();
        let fennel_view = views
            .iter()
            .find(|v| v.name == "graph")
            .expect("graph view should exist");
        assert!(
            fennel_view.is_fennel,
            "graph view should be from Fennel source"
        );
    }

    #[test]
    fn test_register_tool_programmatic() {
        let mut manager = PluginManager::new();

        let tool = ToolBuilder::new("my_tool")
            .description("A programmatic tool")
            .param("query", "string")
            .param_optional("limit", "number")
            .build();

        let handle = manager.register_tool(tool, None);

        assert_eq!(manager.tools().len(), 1);
        assert_eq!(manager.tools()[0].name, "my_tool");
        assert_eq!(manager.tools()[0].params.len(), 2);
        assert!(!manager.tools()[0].params[0].optional);
        assert!(manager.tools()[0].params[1].optional);

        assert!(manager.unregister(handle));
        assert_eq!(manager.tools().len(), 0);
    }

    #[test]
    fn test_register_command_programmatic() {
        let mut manager = PluginManager::new();

        let cmd = CommandBuilder::new("mycmd")
            .description("A programmatic command")
            .hint("args...")
            .build();

        let handle = manager.register_command(cmd, None);

        assert_eq!(manager.commands().len(), 1);
        assert_eq!(manager.commands()[0].name, "mycmd");
        assert_eq!(
            manager.commands()[0].input_hint,
            Some("args...".to_string())
        );

        assert!(manager.unregister(handle));
        assert_eq!(manager.commands().len(), 0);
    }

    #[test]
    fn test_register_handler_programmatic() {
        let mut manager = PluginManager::new();

        let handler = HandlerBuilder::new("on_search", "tool:after")
            .pattern("search_*")
            .priority(50)
            .build();

        let handle = manager.register_handler(handler, None);

        assert_eq!(manager.handlers().len(), 1);
        assert_eq!(manager.handlers()[0].event_type, "tool:after");
        assert_eq!(manager.handlers()[0].pattern, "search_*");
        assert_eq!(manager.handlers()[0].priority, 50);

        assert!(manager.unregister(handle));
        assert_eq!(manager.handlers().len(), 0);
    }

    #[test]
    fn test_register_view_programmatic() {
        let mut manager = PluginManager::new();

        let view = ViewBuilder::new("custom_view")
            .description("A custom view")
            .handler_fn("custom_handler")
            .build();

        let handle = manager.register_view(view, None);

        assert_eq!(manager.views().len(), 1);
        assert_eq!(manager.views()[0].name, "custom_view");
        assert_eq!(
            manager.views()[0].handler_fn,
            Some("custom_handler".to_string())
        );

        assert!(manager.unregister(handle));
        assert_eq!(manager.views().len(), 0);
    }

    #[test]
    fn test_unregister_by_owner() {
        let mut manager = PluginManager::new();

        let tool1 = ToolBuilder::new("owned_tool").build();
        let tool2 = ToolBuilder::new("other_tool").build();
        let cmd = CommandBuilder::new("owned_cmd").build();

        manager.register_tool(tool1, Some("my_plugin"));
        manager.register_tool(tool2, None);
        manager.register_command(cmd, Some("my_plugin"));

        assert_eq!(manager.tools().len(), 2);
        assert_eq!(manager.commands().len(), 1);

        let removed = manager.unregister_by_owner("my_plugin");
        assert_eq!(removed, 2);
        assert_eq!(manager.tools().len(), 1);
        assert_eq!(manager.tools()[0].name, "other_tool");
        assert_eq!(manager.commands().len(), 0);
    }

    #[test]
    fn test_unregister_invalid_handle_returns_false() {
        let mut manager = PluginManager::new();
        let fake_handle = RegistrationHandle(999999);
        assert!(!manager.unregister(fake_handle));
    }

    #[test]
    fn test_mixed_spec_and_programmatic() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(temp.path(), "spec-plugin", "1.0.0");

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();
        manager.load("spec-plugin").unwrap();

        assert_eq!(manager.tools().len(), 1);
        assert_eq!(manager.tools()[0].name, "test_tool");

        let prog_tool = ToolBuilder::new("programmatic_tool").build();
        let handle = manager.register_tool(prog_tool, None);

        assert_eq!(manager.tools().len(), 2);

        let names: Vec<_> = manager.tools().iter().map(|t| &t.name).collect();
        assert!(names.contains(&&"test_tool".to_string()));
        assert!(names.contains(&&"programmatic_tool".to_string()));

        manager.unregister(handle);
        assert_eq!(manager.tools().len(), 1);
        assert_eq!(manager.tools()[0].name, "test_tool");
    }

    // ====================================================================
    // Spec-based plugin loading tests
    // ====================================================================

    #[test]
    fn test_load_plugin_spec_basic() {
        let source = r#"
return {
    name = "test-plugin",
    version = "1.2.3",
    description = "A test plugin",
    tools = {
        my_tool = {
            desc = "Do something",
            params = {
                { name = "query", type = "string", desc = "Search query" },
            },
            fn = function(args) return { result = "ok" } end,
        },
    },
}
"#;
        let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
            .unwrap()
            .expect("Should return Some(spec)");

        assert_eq!(spec.name, Some("test-plugin".to_string()));
        assert_eq!(spec.version, Some("1.2.3".to_string()));
        assert_eq!(spec.description, Some("A test plugin".to_string()));
        assert_eq!(spec.tools.len(), 1);
        assert_eq!(spec.tools[0].name, "my_tool");
        assert_eq!(spec.tools[0].description, "Do something");
        assert_eq!(spec.tools[0].params.len(), 1);
        assert_eq!(spec.tools[0].params[0].name, "query");
        assert_eq!(spec.tools[0].params[0].param_type, "string");
        assert!(!spec.tools[0].params[0].optional);
    }

    #[test]
    fn test_load_plugin_spec_all_export_types() {
        let source = r#"
local M = {}
function M.my_tool(args) return { result = "ok" } end
function M.my_command(args, ctx) end
function M.my_handler(ctx, event) return event end
function M.my_view(ctx) end

return {
    name = "full-plugin",
    version = "0.1.0",
    tools = {
        my_tool = { desc = "A tool", fn = M.my_tool },
    },
    commands = {
        my_command = { desc = "A command", hint = "[args]", fn = M.my_command },
    },
    handlers = {
        { event = "note:created", priority = 150, name = "on_note_created", fn = M.my_handler },
    },
    views = {
        ["my-view"] = { desc = "A view", fn = M.my_view },
    },
}
"#;
        let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
            .unwrap()
            .expect("Should return Some(spec)");

        assert_eq!(spec.tools.len(), 1);
        assert_eq!(spec.commands.len(), 1);
        assert_eq!(spec.handlers.len(), 1);
        assert_eq!(spec.views.len(), 1);
    }

    #[test]
    fn test_load_plugin_spec_with_setup() {
        let source = r#"
return {
    name = "setup-plugin",
    version = "0.1.0",
    setup = function(config)
        -- Called after load with plugin config
    end,
}
"#;
        let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
            .unwrap()
            .expect("Should return Some(spec)");

        assert!(spec.has_setup);
        assert_eq!(spec.name, Some("setup-plugin".to_string()));
    }

    #[test]
    fn test_load_plugin_spec_empty_table() {
        // Empty table with no recognized fields returns None (not a spec)
        let source = "return {}";
        let result = load_plugin_spec_from_source(source, Path::new("test/init.lua")).unwrap();
        assert!(
            result.is_none(),
            "Empty table should not be recognized as a spec"
        );
    }

    #[test]
    fn test_load_plugin_spec_no_return() {
        // Script that doesn't return anything (returns nil)
        let source = "local x = 42";
        let result = load_plugin_spec_from_source(source, Path::new("test/init.lua")).unwrap();
        assert!(result.is_none(), "nil return should yield None");
    }

    #[test]
    fn test_load_plugin_spec_lua_error() {
        // Syntax error in Lua
        let source = "this is not valid lua!!!";
        let result = load_plugin_spec_from_source(source, Path::new("test/init.lua"));
        assert!(result.is_err(), "Lua syntax error should return Err");
    }

    #[test]
    fn test_load_plugin_spec_runtime_error() {
        // Runtime error
        let source = r#"error("boom")"#;
        let result = load_plugin_spec_from_source(source, Path::new("test/init.lua"));
        assert!(result.is_err(), "Runtime error should return Err");
    }

    #[test]
    fn test_tool_params_required_and_optional() {
        let source = r#"
return {
    name = "params-test",
    version = "1.0.0",
    tools = {
        search = {
            desc = "Search",
            params = {
                { name = "query", type = "string", desc = "Search query" },
                { name = "limit", type = "number", desc = "Max results", optional = true },
            },
        },
    },
}
"#;
        let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
            .unwrap()
            .unwrap();

        let tool = &spec.tools[0];
        assert_eq!(tool.params.len(), 2);
        assert!(!tool.params[0].optional);
        assert!(tool.params[1].optional);
        assert_eq!(tool.params[1].param_type, "number");
    }

    #[test]
    fn test_handler_spec_fields() {
        let source = r#"
return {
    name = "handler-test",
    version = "1.0.0",
    handlers = {
        { event = "note:created", priority = 50, pattern = "*.md", name = "on_md_created" },
        { event = "tool:after", name = "log_tool" },
    },
}
"#;
        let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
            .unwrap()
            .unwrap();

        assert_eq!(spec.handlers.len(), 2);

        let h1 = &spec.handlers[0];
        assert_eq!(h1.event_type, "note:created");
        assert_eq!(h1.priority, 50);
        assert_eq!(h1.pattern, "*.md");
        assert_eq!(h1.name, "on_md_created");

        let h2 = &spec.handlers[1];
        assert_eq!(h2.event_type, "tool:after");
        assert_eq!(h2.priority, 100); // default
        assert_eq!(h2.pattern, "*"); // default
    }

    #[test]
    fn test_view_spec_with_handler() {
        let source = r#"
return {
    name = "view-test",
    version = "1.0.0",
    views = {
        ["my-view"] = {
            desc = "A custom view",
            fn = function(ctx) end,
            handler = function(key, ctx) end,
        },
    },
}
"#;
        let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
            .unwrap()
            .unwrap();

        assert_eq!(spec.views.len(), 1);
        assert_eq!(spec.views[0].name, "my-view");
        assert_eq!(spec.views[0].description, "A custom view");
        assert!(spec.views[0].handler_fn.is_some());
    }

    #[test]
    fn test_view_spec_without_handler() {
        let source = r#"
return {
    name = "view-test",
    version = "1.0.0",
    views = {
        ["simple-view"] = {
            desc = "Simple",
            fn = function(ctx) end,
        },
    },
}
"#;
        let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
            .unwrap()
            .unwrap();

        assert_eq!(spec.views.len(), 1);
        assert!(spec.views[0].handler_fn.is_none());
    }

    #[test]
    fn test_command_spec_with_hint() {
        let source = r#"
return {
    name = "cmd-test",
    version = "1.0.0",
    commands = {
        daily = { desc = "Create daily note", hint = "[title]" },
    },
}
"#;
        let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
            .unwrap()
            .unwrap();

        assert_eq!(spec.commands.len(), 1);
        assert_eq!(spec.commands[0].name, "daily");
        assert_eq!(spec.commands[0].description, "Create daily note");
        assert_eq!(spec.commands[0].input_hint, Some("[title]".to_string()));
    }

    #[test]
    fn test_capabilities_from_spec() {
        let source = r#"
return {
    name = "cap-test",
    version = "1.0.0",
    capabilities = { "vault", "ui", "config" },
}
"#;
        let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
            .unwrap()
            .unwrap();

        assert_eq!(spec.capabilities, vec!["vault", "ui", "config"]);
    }

    #[test]
    fn test_plain_module_table_not_spec() {
        // A plugin that returns a module table (not a spec) should be None
        let source = r#"
local M = {}
function M.my_tool(args) return { result = "ok" } end
function M.my_command(args, ctx) end
return M
"#;
        let result = load_plugin_spec_from_source(source, Path::new("test/init.lua")).unwrap();
        assert!(
            result.is_none(),
            "Module table with only function values should not be a spec"
        );
    }

    #[test]
    fn test_spec_with_only_name() {
        // A table with just a name field is recognized as a spec
        let source = r#"return { name = "minimal" }"#;
        let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
            .unwrap()
            .expect("Table with name should be a spec");

        assert_eq!(spec.name, Some("minimal".to_string()));
        assert!(spec.tools.is_empty());
    }

    // ====================================================================
    // Manifest-less discovery tests
    // ====================================================================

    #[test]
    fn test_discover_directory_without_manifest() {
        let temp = TempDir::new().unwrap();
        let plugin_dir = temp.path().join("my-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(plugin_dir.join("init.lua"), "-- code").unwrap();

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        let discovered = manager.discover().unwrap();

        assert_eq!(discovered.len(), 1);
        assert!(discovered.contains(&"my-plugin".to_string()));

        let plugin = manager.get("my-plugin").unwrap();
        assert_eq!(plugin.version(), "0.0.0");
    }

    #[test]
    fn test_discover_manifestless_with_spec_override() {
        let temp = TempDir::new().unwrap();
        let plugin_dir = temp.path().join("my-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        // Plugin returns a spec with custom name/version
        std::fs::write(
            plugin_dir.join("init.lua"),
            r#"return { name = "custom-name", version = "1.2.0" }"#,
        )
        .unwrap();

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();
        manager.load("my-plugin").unwrap();

        let plugin = manager.get("my-plugin").unwrap();
        // Name updated from spec (since version was 0.0.0 = directory defaults)
        assert_eq!(plugin.manifest.name, "custom-name");
        assert_eq!(plugin.version(), "1.2.0");
    }

    #[test]
    fn test_manifest_takes_precedence_over_lua_table() {
        let temp = TempDir::new().unwrap();
        let plugin_dir = temp.path().join("my-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();

        // Manifest with explicit version
        std::fs::write(
            plugin_dir.join("plugin.yaml"),
            "name: my-plugin\nversion: \"2.0.0\"\nmain: init.lua\n",
        )
        .unwrap();

        // Lua spec with different version
        std::fs::write(
            plugin_dir.join("init.lua"),
            r#"return { name = "other-name", version = "9.9.9" }"#,
        )
        .unwrap();

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();
        manager.load("my-plugin").unwrap();

        let plugin = manager.get("my-plugin").unwrap();
        // Manifest values should win (version != "0.0.0", so spec doesn't override)
        assert_eq!(plugin.manifest.name, "my-plugin");
        assert_eq!(plugin.version(), "2.0.0");
    }

    #[test]
    fn test_empty_directory_not_discovered() {
        let temp = TempDir::new().unwrap();
        let empty_dir = temp.path().join("empty-dir");
        std::fs::create_dir_all(&empty_dir).unwrap();

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        let discovered = manager.discover().unwrap();

        assert!(discovered.is_empty());
    }

    // ====================================================================
    // Spec-based plugin loading integration tests
    // ====================================================================

    fn create_spec_plugin(dir: &Path, name: &str) -> PathBuf {
        let plugin_dir = dir.join(name);
        std::fs::create_dir_all(&plugin_dir).unwrap();

        let lua = format!(
            r#"
local M = {{}}
function M.search(args) return {{ result = "ok" }} end
function M.search_command(args, ctx) end
function M.on_note(ctx, event) return event end
function M.graph_view(ctx) end
function M.graph_handler(key, ctx) end

return {{
    name = "{}",
    version = "1.0.0",
    description = "Test spec plugin",
    capabilities = {{ "vault" }},

    tools = {{
        search = {{
            desc = "Search notes",
            params = {{
                {{ name = "query", type = "string", desc = "Search query" }},
                {{ name = "limit", type = "number", desc = "Max results", optional = true }},
            }},
            fn = M.search,
        }},
    }},

    commands = {{
        search = {{ desc = "Search command", hint = "[query]", fn = M.search_command }},
    }},

    handlers = {{
        {{ event = "note:created", priority = 50, name = "on_note", fn = M.on_note }},
    }},

    views = {{
        graph = {{ desc = "Graph view", fn = M.graph_view, handler = M.graph_handler }},
    }},

    setup = function(config) end,
}}
"#,
            name
        );

        std::fs::write(plugin_dir.join("init.lua"), lua).unwrap();
        plugin_dir
    }

    #[test]
    fn test_spec_plugin_full_lifecycle() {
        let temp = TempDir::new().unwrap();
        create_spec_plugin(temp.path(), "spec-test");

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();
        manager.load("spec-test").unwrap();

        let plugin = manager.get("spec-test").unwrap();
        assert_eq!(plugin.state, PluginState::Active);
        assert_eq!(plugin.manifest.name, "spec-test");
        assert_eq!(plugin.version(), "1.0.0");

        assert_eq!(manager.tools().len(), 1);
        assert_eq!(manager.tools()[0].name, "search");
        assert_eq!(manager.tools()[0].params.len(), 2);

        assert_eq!(manager.commands().len(), 1);
        assert_eq!(manager.commands()[0].name, "search");
        assert_eq!(
            manager.commands()[0].input_hint,
            Some("[query]".to_string())
        );

        assert_eq!(manager.handlers().len(), 1);
        assert_eq!(manager.handlers()[0].event_type, "note:created");
        assert_eq!(manager.handlers()[0].priority, 50);

        assert_eq!(manager.views().len(), 1);
        assert_eq!(manager.views()[0].name, "graph");
        assert!(manager.views()[0].handler_fn.is_some());

        // Unload and verify cleanup
        manager.unload("spec-test").unwrap();
        assert_eq!(manager.tools().len(), 0);
        assert_eq!(manager.commands().len(), 0);
        assert_eq!(manager.handlers().len(), 0);
        assert_eq!(manager.views().len(), 0);
    }

    #[test]
    fn test_spec_plugin_without_manifest() {
        let temp = TempDir::new().unwrap();
        // Create a manifest-less plugin that returns a spec
        create_spec_plugin(temp.path(), "no-manifest");

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();
        manager.load("no-manifest").unwrap();

        let plugin = manager.get("no-manifest").unwrap();
        assert_eq!(plugin.state, PluginState::Active);
        // Name/version updated from spec
        assert_eq!(plugin.manifest.name, "no-manifest");
        assert_eq!(plugin.version(), "1.0.0");

        assert_eq!(manager.tools().len(), 1);
        assert_eq!(manager.commands().len(), 1);
        assert_eq!(manager.handlers().len(), 1);
        assert_eq!(manager.views().len(), 1);
    }

    #[test]
    fn test_spec_capabilities_merged_into_manifest() {
        let temp = TempDir::new().unwrap();
        create_spec_plugin(temp.path(), "cap-merge");

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();
        manager.load("cap-merge").unwrap();

        let plugin = manager.get("cap-merge").unwrap();
        assert!(plugin.manifest.has_capability(Capability::Vault));
    }

    #[test]
    fn test_multiple_spec_plugins_coexist() {
        let temp = TempDir::new().unwrap();

        // Manifest + spec plugin
        create_test_plugin(temp.path(), "manifest-plugin", "1.0.0");

        // Manifest-less spec plugin
        create_spec_plugin(temp.path(), "spec-plugin");

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();
        manager.load_all().unwrap();

        // Both should be loaded
        assert_eq!(
            manager.get("manifest-plugin").unwrap().state,
            PluginState::Active
        );
        assert_eq!(
            manager.get("spec-plugin").unwrap().state,
            PluginState::Active
        );

        // manifest-plugin has 1 tool (test_tool), spec-plugin has 1 tool (search)
        let tool_names: Vec<_> = manager.tools().iter().map(|t| t.name.clone()).collect();
        assert!(tool_names.contains(&"test_tool".to_string()));
        assert!(tool_names.contains(&"search".to_string()));
    }

    #[test]
    fn test_spec_services_parsed() {
        let source = r#"
            return {
                name = "service-plugin",
                services = {
                    gateway = {
                        desc = "WebSocket gateway",
                        fn = function() end,
                    },
                    heartbeat = {
                        desc = "Keep-alive pinger",
                        fn = function() end,
                    },
                    no_fn_service = {
                        desc = "Missing fn field -- should be skipped",
                    },
                },
            }
        "#;

        let spec = load_plugin_spec_from_source(source, Path::new("test.lua")).unwrap();
        let spec = spec.expect("should return Some");

        assert_eq!(spec.services.len(), 2);

        let names: Vec<&str> = spec.services.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"gateway"));
        assert!(names.contains(&"heartbeat"));

        let gw = spec.services.iter().find(|s| s.name == "gateway").unwrap();
        assert_eq!(gw.description, "WebSocket gateway");
        assert_eq!(gw.service_fn, "gateway");
    }
}
