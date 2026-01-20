//! Plugin lifecycle management

use crate::annotations::{
    AnnotationParser, DiscoveredCommand, DiscoveredHandler, DiscoveredParam, DiscoveredTool,
    DiscoveredView,
};
use crate::manifest::{Capability, LoadedPlugin, ManifestError, PluginManifest, PluginState};
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

                if !path.is_dir() {
                    continue;
                }

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
                        debug!("No manifest found in: {}", path.display());
                    }
                    Err(e) => {
                        warn!("Failed to load manifest from {}: {}", path.display(), e);
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
        let auto_discover = plugin.manifest.exports.auto_discover;

        debug!(
            "Discovering exports for plugin {}: auto_discover={}, dir={}",
            name,
            auto_discover,
            plugin_dir.display()
        );

        if auto_discover {
            self.scan_plugin_files(&plugin_dir)?;
        } else {
            let main_path = plugin.main_path();
            debug!(
                "  main_path: {}, exists: {}",
                main_path.display(),
                main_path.exists()
            );
            if main_path.exists() {
                self.scan_file(&main_path)?;
            } else {
                warn!("Main file does not exist: {}", main_path.display());
            }
        }

        Ok(())
    }

    fn scan_plugin_files(&mut self, dir: &Path) -> LifecycleResult<()> {
        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str());
                if matches!(ext, Some("lua") | Some("fnl")) {
                    self.scan_file(path)?;
                }
            }
        }
        Ok(())
    }

    fn scan_file(&mut self, path: &Path) -> LifecycleResult<()> {
        debug!("Scanning file: {}", path.display());
        let content = std::fs::read_to_string(path)?;
        let parser = AnnotationParser::new();

        if let Ok(tools) = parser.parse_tools(&content, path) {
            debug!("  Found {} tools", tools.len());
            for tool in tools {
                if !self
                    .tools
                    .iter()
                    .any(|t| t.item.name == tool.name && t.item.source_path == tool.source_path)
                {
                    debug!("Discovered tool: {} from {}", tool.name, path.display());
                    self.tools.push(RegisteredItem {
                        item: tool,
                        handle: RegistrationHandle::new(),
                        owner: None,
                    });
                }
            }
        }

        if let Ok(commands) = parser.parse_commands(&content, path) {
            for cmd in commands {
                if !self
                    .commands
                    .iter()
                    .any(|c| c.item.name == cmd.name && c.item.source_path == cmd.source_path)
                {
                    debug!("Discovered command: {} from {}", cmd.name, path.display());
                    self.commands.push(RegisteredItem {
                        item: cmd,
                        handle: RegistrationHandle::new(),
                        owner: None,
                    });
                }
            }
        }

        if let Ok(views) = parser.parse_views(&content, path) {
            for view in views {
                if !self
                    .views
                    .iter()
                    .any(|v| v.item.name == view.name && v.item.source_path == view.source_path)
                {
                    debug!("Discovered view: {} from {}", view.name, path.display());
                    self.views.push(RegisteredItem {
                        item: view,
                        handle: RegistrationHandle::new(),
                        owner: None,
                    });
                }
            }
        }

        let handlers = parser.parse_handlers(&content, path);
        if let Ok(handlers) = handlers {
            for handler in handlers {
                if !self.handlers.iter().any(|h| {
                    h.item.name == handler.name && h.item.source_path == handler.source_path
                }) {
                    debug!(
                        "Discovered handler: {} from {}",
                        handler.name,
                        path.display()
                    );
                    self.handlers.push(RegisteredItem {
                        item: handler,
                        handle: RegistrationHandle::new(),
                        owner: None,
                    });
                }
            }
        }

        Ok(())
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
exports:
  auto_discover: true
"#
        );
        std::fs::write(plugin_dir.join("plugin.yaml"), manifest).unwrap();

        let lua = r#"
--- Test tool
-- @tool name="test_tool" desc="A test tool"
function test_tool()
    return "ok"
end
"#;
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
    fn test_mixed_annotation_and_programmatic() {
        let temp = TempDir::new().unwrap();
        create_test_plugin(temp.path(), "annotation-plugin", "1.0.0");

        let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
        manager.discover().unwrap();
        manager.load("annotation-plugin").unwrap();

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
}
