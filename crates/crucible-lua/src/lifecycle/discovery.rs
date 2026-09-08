use super::registration::RegisteredItem;
use super::spec::{load_plugin_spec, PluginSpec};
use super::{LifecycleError, LifecycleResult, PluginManager};
use crate::error::format_lua_error;
use crate::manifest::{LoadedPlugin, PluginManifest, PluginSource};
use mlua::Value;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// A directory that looked like a plugin but could not be loaded as one.
///
/// Discovery failures have no `LoadedPlugin` to hang a `PluginState::Error`
/// off, so they are collected here and surfaced through `plugin.list`.
/// Otherwise "broken" and "not installed" look identical to every client.
#[derive(Debug, Clone)]
pub struct PluginDiscoveryError {
    pub path: PathBuf,
    pub error: String,
}

impl PluginManager {
    /// Directories that failed discovery on the last [`Self::discover`] call.
    pub fn discovery_errors(&self) -> &[PluginDiscoveryError] {
        &self.discovery_errors
    }

    fn record_discovery_error(&mut self, path: &Path, error: impl std::fmt::Display) {
        let error = error.to_string();
        warn!("Plugin discovery failed for {}: {}", path.display(), error);
        self.discovery_errors.push(PluginDiscoveryError {
            path: path.to_path_buf(),
            error,
        });
    }

    /// Refuse a plugin carrying the reserved declaration name, loudly.
    ///
    /// `plugins.declare` in the config is where plugin DECLARATIONS live, so
    /// a plugin named `declare` could never be configured through the store
    /// form — its section would be read as declarations. Discovering it
    /// silently would drop that configuration with no visible reason; this
    /// makes it a named discovery error instead. Returns whether the name
    /// was refused.
    fn refuse_reserved_name(&mut self, name: &str, path: &Path) -> bool {
        if name != crucible_core::config::PLUGINS_DECLARE_KEY {
            return false;
        }
        self.record_discovery_error(
            path,
            format!(
                "plugin name '{name}' is reserved: `plugins.{name}` in the config holds plugin \
                 declarations, so this plugin could never receive its own configuration. Rename \
                 the plugin directory (from {})",
                path.display()
            ),
        );
        true
    }

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
        self.discovery_errors.clear();

        for search_path in &self.search_paths.clone() {
            if !search_path.exists() {
                debug!("Search path does not exist: {}", search_path.display());
                continue;
            }

            for entry in std::fs::read_dir(search_path)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    // A plugin is a directory with an entry file.
                    //
                    // There is no manifest branch any more: `plugin.yaml` is
                    // gone, and the metadata it carried comes from the spec
                    // table the entry file returns. What used to be the
                    // manifest-less fallback is now the only path.
                    //
                    // A directory holding BOTH entry-file spellings is
                    // recorded as a discovery error, not dropped. Turning the
                    // collision into "not a plugin" made a directory that used
                    // to load simply vanish with nothing in the log — the user
                    // renames a file, leaves the old one behind, and their
                    // plugin is gone with no way to find out why.
                    let source = self.source_for_dir(&path);
                    let entry = match crate::source_files::init_file(&path) {
                        Ok(found) => found,
                        Err(ambiguous) => {
                            self.record_discovery_error(&path, ambiguous);
                            continue;
                        }
                    };
                    if entry.is_none() {
                        debug!("No init.luau or init.lua in: {}", path.display());
                        continue;
                    }
                    match PluginManifest::from_directory_defaults(&path) {
                        Ok(manifest) => {
                            let name = manifest.name.clone();
                            if self.refuse_reserved_name(&name, &path) {
                                continue;
                            }
                            if self.plugins.contains_key(&name) {
                                debug!(
                                    "Plugin already discovered: {name} (shadowed by higher-priority)"
                                );
                                continue;
                            }
                            info!(
                                "Discovered plugin: {} [{}] (from {})",
                                name,
                                source,
                                path.display()
                            );
                            let plugin = LoadedPlugin::with_source(manifest, path, source);
                            self.plugins.insert(name.clone(), plugin);
                            discovered.push(name);
                        }
                        Err(e) => {
                            self.record_discovery_error(&path, e);
                        }
                    }
                } else if path.is_file() {
                    // Single-file plugin: a source file directly in the
                    // plugins dir.
                    if crate::source_files::is_lua_source(&path) {
                        let stem = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string();

                        if stem.is_empty() || self.plugins.contains_key(&stem) {
                            continue;
                        }
                        if self.refuse_reserved_name(&stem, &path) {
                            continue;
                        }

                        // A single-file plugin's manifest is synthesized, not
                        // parsed: it used to be built by formatting YAML and
                        // reading it straight back, which validated nothing the
                        // name check above had not already done.
                        match PluginManifest::from_directory_defaults(&path) {
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

        debug!(
            "Discovering exports for plugin {}: dir={}",
            name,
            plugin_dir.display()
        );

        // Try spec-based loading first (execute init.lua, inspect returned table)
        // `fnl` used to be accepted here; Fennel is gone, and `.luau` is what
        // replaced it as the second extension.
        if main_path.exists() && crate::source_files::is_lua_source(&main_path) {
            match load_plugin_spec(&main_path) {
                Ok(Some(spec)) => {
                    debug!(
                        "Loaded spec for plugin {}: {} tools, {} commands, {} handlers",
                        name,
                        spec.tools.len(),
                        spec.commands.len(),
                        spec.handlers.len(),
                    );

                    // Update manifest metadata from spec if available
                    if let Some(plugin) = self.plugins.get_mut(name) {
                        // The spec's `name` is the plugin's identity when it
                        // states one; the directory name is only the fallback.
                        // A repo cloned as `crucible-discord` whose plugin
                        // declares `name = "discord"` must still receive its
                        // `[plugins.discord]` config — moving a directory
                        // cannot change what a plugin IS.
                        //
                        // This used to be guarded on `version == "0.0.0"`,
                        // so the spec's NAME was taken only when the VERSION
                        // happened to be the placeholder.
                        // The spec's `name` does NOT become the plugin's
                        // identity. Identity is the directory name, because
                        // that is the only name the runtimepath knows without
                        // executing Lua — the same reason `enabled` and
                        // `dependencies` are not plugin-declared either.
                        //
                        // It is recorded as `declared_name` so config lookup
                        // can honour it: a repo cloned as `crucible-discord`
                        // whose plugin declares `name = "discord"` still
                        // receives its `[plugins.discord]` section.
                        if let Some(ref spec_name) = spec.name {
                            // Validated with the SAME check the YAML reader
                            // applied. Deleting `from_yaml` deleted the only
                            // `validate()` call site, so without this a
                            // declared name with a path separator would reach
                            // `[plugins.<name>]` lookup unchecked.
                            let mut candidate = plugin.manifest.clone();
                            candidate.name = spec_name.clone();
                            match candidate.validate() {
                                Ok(()) => plugin.manifest.declared_name = Some(spec_name.clone()),
                                Err(e) => warn!(
                                    "plugin at {} declares an unusable name {spec_name:?}: {e}; \
                                     config will be looked up under the directory name",
                                    plugin.dir.display()
                                ),
                            }
                        }
                        if let Some(ref spec_version) = spec.version {
                            if plugin.manifest.synthesized {
                                let mut candidate = plugin.manifest.clone();
                                candidate.version = spec_version.clone();
                                match candidate.validate() {
                                    Ok(()) => plugin.manifest.version = spec_version.clone(),
                                    Err(e) => warn!(
                                        "plugin {} declares an unusable version \
                                         {spec_version:?}: {e}",
                                        plugin.manifest.name
                                    ),
                                }
                            }
                        }
                        if let Some(ref spec_desc) = spec.description {
                            if plugin.manifest.description.is_empty() {
                                plugin.manifest.description = spec_desc.clone();
                            }
                        }
                        if let Some(ref author) = spec.author {
                            if plugin.manifest.author.is_empty() {
                                plugin.manifest.author = author.clone();
                            }
                        }
                        if let Some(ref license) = spec.license {
                            if plugin.manifest.license.is_none() {
                                plugin.manifest.license = Some(license.clone());
                            }
                        }
                        // The one declaration the host checks. A plugin
                        // may state it in either place; both are the plugin
                        // author's word either way, and the check exists to
                        // catch an accidental takeover, not a hostile one.
                        if spec.intercepts_tools {
                            plugin.manifest.intercepts_tools = true;
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
                // An unreadable DECLARATION is fatal; every other spec
                // failure keeps failing open, so a plugin with an odd spec
                // still loads and merely exports nothing.
                Err(e @ LifecycleError::InvalidDeclaration(_)) => return Err(e),
                Err(e) => {
                    warn!("Failed to load spec for plugin {}: {}", name, e);
                }
            }
        }

        Ok(())
    }

    pub(super) fn load_plugin_runtime_state(&mut self, name: &str) -> LifecycleResult<()> {
        let (main_path, plugin_dir, may_intercept) = {
            let plugin = self
                .plugins
                .get(name)
                .ok_or_else(|| LifecycleError::NotFound(name.to_string()))?;
            (
                plugin.main_path(),
                plugin.dir.clone(),
                plugin.manifest.intercepts_tools,
            )
        };

        // The private root pops when this guard drops, at the end of the load.
        let _module_scope = self.enter_plugin_modules(&plugin_dir)?;

        // Both markers ride ONE context, in Rust-side app data. As Lua globals
        // they were forgeable: a plugin assigned itself another plugin's
        // storage namespace, or the interception right, in one line.
        //
        // Stamped at LOAD, read at registration: whether a handler may take a
        // tool call over is a property of the plugin the operator installed,
        // not of the call it later intercepts.
        let previous = crate::plugin_context::enter_plugin(&self.lua, name, may_intercept);

        let load_result = (|| -> LifecycleResult<()> {
            let source = std::fs::read_to_string(&main_path).map_err(LifecycleError::Io)?;
            let chunk_name = main_path.to_string_lossy().to_string();
            let result: Value = self
                .lua
                .load(&source)
                .set_name(chunk_name.as_str())
                .eval()
                .map_err(|e| LifecycleError::LoadError(format_lua_error(Some(name), &e)))?;

            match result {
                Value::Table(spec_table) => {
                    self.capture_on_unload_hook(name, &spec_table)?;
                    self.capture_on_load_hook(name, &spec_table)?;
                    // `require("<plugin>")` must answer with the table this
                    // load produced, not evaluate the file a second time.
                    let loaded: mlua::Table = self
                        .lua
                        .globals()
                        .get::<mlua::Table>("package")
                        .and_then(|package| package.get("loaded"))
                        .map_err(|e| LifecycleError::LoadError(format!("package.loaded: {e}")))?;
                    loaded.set(name, spec_table).map_err(|e| {
                        LifecycleError::LoadError(format!(
                            "Failed to cache plugin module {}: {}",
                            name, e
                        ))
                    })?;
                    self.modules().record_public(name, &main_path);
                }
                _ => {
                    self.on_unload_hooks.remove(name);
                    self.on_load_hooks.remove(name);
                }
            }

            Ok(())
        })();

        // Restored on every exit path, error paths included: a context left
        // behind attributes whatever loads next to the wrong plugin.
        crate::plugin_context::set_plugin_context(&self.lua, previous);

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
                        owner: Some(owner.to_string()),
                    });
                }
            }
        }

        push_unique(&mut self.tools, spec.tools, "tool", owner, |t| &t.name);
        push_unique(&mut self.commands, spec.commands, "command", owner, |c| {
            &c.name
        });
    }
}
