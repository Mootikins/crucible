use super::fragment::{read_fragment, Fragment};
use super::{LifecycleResult, PluginManager};
use crate::manifest::{LoadedPlugin, PluginManifest, PluginSource};
use mlua::Lua;
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

/// Every entry in `dir`, in ascending order of file name.
///
/// `std::fs::read_dir` returns entries in an order the platform does not
/// specify, so DISCOVERY order was a property of the disk.
///
/// **This does not decide the load order, and an earlier version of this
/// comment claimed it did.** `PluginManager::load_all` sorts its whole key set
/// (`lifecycle/loading.rs`), so the load order was already one alphabetical
/// sort across every directory, whatever `read_dir` answered. The handler
/// tie-break reads that order, not this one.
///
/// What this sort decides is narrower: which plugin wins a duplicated name.
/// Discovery walks the search paths in `Origin` order and the first name seen
/// wins, so an unsorted read inside one directory made shadowing depend on the
/// disk. It also makes a discovery log reproducible.
///
/// The file name is the key because the directory name IS a plugin's
/// identity — the only name the host knows before it runs any Lua. File names
/// are unique inside one directory, so the order is total.
fn sorted_entries(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut paths = std::fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<PathBuf>>>()?;
    paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    Ok(paths)
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

    /// Walk the search paths and register every plugin directory.
    ///
    /// Discovery reads each plugin's fragment (`spec.luau`) in `lua`, and
    /// runs no plugin code: the fragment's environment can describe and
    /// cannot act (see `fragment.rs`). `lua` is the daemon VM, so no second
    /// VM exists. A plugin without a fragment is registered under its
    /// directory name, with no version and no intercept grant. A fragment
    /// that does not read is a discovery error for that directory, and the
    /// directory is not registered.
    pub fn discover(&mut self, lua: &Lua) -> LifecycleResult<Vec<String>> {
        let mut discovered = Vec::new();
        self.discovery_errors.clear();

        for search_path in &self.search_paths.clone() {
            if !search_path.exists() {
                debug!("Search path does not exist: {}", search_path.display());
                continue;
            }

            for path in sorted_entries(search_path)? {
                if path.is_dir() {
                    // A plugin is a directory with an entry file.
                    //
                    // There is no manifest branch any more: `plugin.yaml` is
                    // gone, and the metadata it carried comes from the
                    // fragment. What used to be the manifest-less fallback is
                    // now the only path.
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
                    let mut manifest = match PluginManifest::from_directory_defaults(&path) {
                        Ok(manifest) => manifest,
                        Err(e) => {
                            self.record_discovery_error(&path, e);
                            continue;
                        }
                    };
                    let name = manifest.name.clone();
                    if self.refuse_reserved_name(&name, &path) {
                        continue;
                    }
                    if self.plugins.contains_key(&name) {
                        debug!("Plugin already discovered: {name} (shadowed by higher-priority)");
                        continue;
                    }
                    // The fragment is read before the plugin is registered,
                    // so a broken fragment registers nothing. The error is
                    // recorded, not swallowed: a skipped directory looks like
                    // one that was never installed.
                    match read_fragment(lua, &path) {
                        Ok(Some(fragment)) => apply_fragment(&mut manifest, &fragment),
                        Ok(None) => {}
                        Err(e) => {
                            self.record_discovery_error(&path, e);
                            continue;
                        }
                    }
                    info!(
                        "Discovered plugin: {} [{}] (from {}, version {}, intercepts tools: {})",
                        name,
                        source,
                        path.display(),
                        manifest.version.as_deref().unwrap_or("none"),
                        manifest.intercepts_tools,
                    );
                    let plugin = LoadedPlugin::with_source(manifest, path, source);
                    self.plugins.insert(name.clone(), plugin);
                    discovered.push(name);
                } else if path.is_file() {
                    // Single-file plugin: a source file directly in the
                    // plugins dir. It has no directory, so it has no fragment.
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
}

/// Copy a fragment's fields onto a manifest that `from_directory_defaults`
/// built.
///
/// The fragment's `name` does NOT become the plugin's identity. Identity is
/// the directory name, because that is the only name the runtimepath knows
/// without reading any file. It is recorded as `declared_name` so config
/// lookup can honour it: a repo cloned as `crucible-discord` whose fragment
/// declares `name = "discord"` still receives its `[plugins.discord]`
/// section.
///
/// A name or a version that fails the manifest's own check is refused with a
/// warning and the field stays as the directory gave it. Deleting the YAML
/// reader deleted the only `validate()` call site, so without this a declared
/// name with a path separator would reach `[plugins.<name>]` lookup
/// unchecked.
fn apply_fragment(manifest: &mut PluginManifest, fragment: &Fragment) {
    if let Some(declared) = &fragment.name {
        let mut candidate = manifest.clone();
        candidate.name = declared.clone();
        match candidate.validate() {
            Ok(()) => manifest.declared_name = Some(declared.clone()),
            Err(e) => warn!(
                "plugin {} declares an unusable name {declared:?}: {e}; config will be looked \
                 up under the directory name",
                manifest.name
            ),
        }
    }
    if let Some(version) = &fragment.version {
        let mut candidate = manifest.clone();
        candidate.version = Some(version.clone());
        match candidate.validate() {
            Ok(()) => manifest.version = Some(version.clone()),
            Err(e) => warn!(
                "plugin {} declares an unusable version {version:?}: {e}",
                manifest.name
            ),
        }
    }
    if let Some(description) = &fragment.description {
        manifest.description = description.clone();
    }
    if let Some(author) = &fragment.author {
        manifest.author = author.clone();
    }
    if let Some(license) = &fragment.license {
        manifest.license = Some(license.clone());
    }
    // The one declaration the host checks. The fragment is the only place
    // it can be stated: a grant in `init.luau` is a plugin granting itself
    // one at activation, which is what the fragment exists to prevent.
    manifest.intercepts_tools = fragment.intercepts_tools;
    manifest.opts = fragment.opts.clone();
}
