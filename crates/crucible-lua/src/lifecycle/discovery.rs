use super::registration::{RegisteredItem, RegistrationHandle};
use super::spec::{load_plugin_spec, parse_capability, PluginSpec};
use super::{LifecycleError, LifecycleResult, PluginManager};
use crate::error::format_lua_error;
use crate::manifest::{LoadedPlugin, PluginManifest, PluginSource};
use mlua::Value;
use std::path::Path;
use tracing::{debug, info, warn};

impl PluginManager {
    /// Get the provenance source for a plugin directory.
    pub(super) fn source_for_dir(&self, plugin_dir: &Path) -> PluginSource {
        // Walk search paths to find which one contains this plugin dir
        for search_path in &self.search_paths {
            if plugin_dir.starts_with(search_path) {
                if let Some(source) = self.path_sources.get(search_path) {
                    return *source;
                }
            }
        }
        PluginSource::User
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
                    let source = self.source_for_dir(&path);
                    match PluginManifest::discover(&path) {
                        Ok(Some(manifest)) => {
                            let name = manifest.name.clone();
                            if self.plugins.contains_key(&name) {
                                debug!(
                                    "Plugin already discovered: {} (shadowed by higher-priority)",
                                    name
                                );
                                continue;
                            }
                            info!(
                                "Discovered plugin: {} v{} [{}]",
                                name, manifest.version, source
                            );
                            let plugin = LoadedPlugin::with_source(manifest, path, source);
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
                                            debug!("Plugin already discovered: {} (shadowed by higher-priority)", name);
                                            continue;
                                        }
                                        info!(
                                            "Discovered manifest-less plugin: {} [{}] (from {})",
                                            name,
                                            source,
                                            path.display()
                                        );
                                        let plugin =
                                            LoadedPlugin::with_source(manifest, path, source);
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

    pub(super) fn discover_exports_for_plugin(&mut self, name: &str) -> LifecycleResult<()> {
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
                    self.register_spec_exports(spec, name);

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

    pub(super) fn load_plugin_runtime_state(&mut self, name: &str) -> LifecycleResult<()> {
        let (main_path, plugin_dir) = {
            let plugin = self
                .plugins
                .get(name)
                .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;
            (plugin.main_path(), plugin.dir.clone())
        };

        self.configure_plugin_package_path(&plugin_dir)?;

        if let Err(error) = self
            .lua
            .load(format!("cru._current_plugin = {:?}", name))
            .exec()
        {
            warn!("Failed to set cru._current_plugin for {}: {}", name, error);
        }

        let load_result = (|| -> LifecycleResult<()> {
            let source = std::fs::read_to_string(&main_path).map_err(LifecycleError::Io)?;
            let is_fennel = main_path.extension().is_some_and(|ext| ext == "fnl");

            let lua_source = if is_fennel {
                #[cfg(feature = "fennel")]
                {
                    crate::fennel::compile_fennel(&source).map_err(|e| {
                        LifecycleError::LoadError(format!(
                            "Fennel compilation failed for {}: {}",
                            main_path.display(),
                            e
                        ))
                    })?
                }
                #[cfg(not(feature = "fennel"))]
                {
                    return Err(LifecycleError::LoadError(format!(
                        "Fennel file {} requires the 'fennel' feature",
                        main_path.display()
                    )));
                }
            } else {
                source
            };

            let chunk_name = main_path.to_string_lossy().to_string();
            let result: Value = self
                .lua
                .load(&lua_source)
                .set_name(chunk_name.as_str())
                .eval()
                .map_err(|e| LifecycleError::LoadError(format_lua_error(Some(name), &e)))?;

            match result {
                Value::Table(spec_table) => {
                    self.capture_on_unload_hook(name, &spec_table)?;
                    self.capture_on_load_hook(name, &spec_table)?;
                    let package: mlua::Table = self.lua.globals().get("package").map_err(|e| {
                        LifecycleError::LoadError(format!("Failed to access package table: {}", e))
                    })?;
                    let loaded: mlua::Table = package.get("loaded").map_err(|e| {
                        LifecycleError::LoadError(format!("Failed to access package.loaded: {}", e))
                    })?;
                    loaded.set(name, spec_table).map_err(|e| {
                        LifecycleError::LoadError(format!(
                            "Failed to cache plugin module {}: {}",
                            name, e
                        ))
                    })?;
                }
                _ => {
                    self.on_unload_hooks.remove(name);
                    self.on_load_hooks.remove(name);
                }
            }

            Ok(())
        })();

        if let Err(error) = self.lua.load("cru._current_plugin = nil").exec() {
            warn!("Failed to clear cru._current_plugin: {}", error);
        }

        load_result
    }

    pub(super) fn register_spec_exports(&mut self, spec: PluginSpec, owner: &str) {
        fn push_unique<T>(
            existing: &mut Vec<RegisteredItem<T>>,
            items: Vec<T>,
            kind: &str,
            owner: &str,
            get_name: impl Fn(&T) -> &str,
        ) {
            for item in items {
                let name = get_name(&item);
                if !existing.iter().any(|e| get_name(&e.item) == name) {
                    debug!("Registered {} from spec: {}", kind, name);
                    existing.push(RegisteredItem {
                        item,
                        handle: RegistrationHandle::new(),
                        owner: Some(owner.to_string()),
                    });
                }
            }
        }

        push_unique(&mut self.tools, spec.tools, "tool", owner, |t| &t.name);
        push_unique(&mut self.commands, spec.commands, "command", owner, |c| {
            &c.name
        });
        push_unique(&mut self.handlers, spec.handlers, "handler", owner, |h| {
            &h.name
        });
        push_unique(&mut self.views, spec.views, "view", owner, |v| &v.name);
    }
}
