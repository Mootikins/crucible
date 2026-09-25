//! Assembling the daemon's runtime path.
//!
//! [`crucible_core::runtime_path`] is pure: it takes roots as values and
//! resolves them. This is the impure half — the one place in the daemon that
//! reads `$CRUCIBLE_PLUGIN_PATH`, `$CRUCIBLE_RUNTIME` and `dirs::config_dir()`
//! and turns them into a `Vec<RuntimeEntry>`.
//!
//! Keeping the two apart is what makes the resolver testable. A resolver that
//! read the environment would find the developer's own `~/.config/crucible` in
//! every test — green on CI, red locally — which is the failure
//! `runtime_skill_paths` and `defaults_candidates_from` were each split out to
//! end.

use crucible_core::runtime_path::{build_path, PathInputs, RuntimeEntry};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The runtime roots the daemon searches, highest priority first.
///
/// `$CRUCIBLE_RUNTIME` replaces the auto-detected roots when set; otherwise
/// `runtime_roots::for_current_exe` supplies them, which is the user's
/// `cru setup` copy, then the installed layout, then the dev tree, then the
/// copy extracted from the binary.
fn runtime_roots() -> Vec<PathBuf> {
    match std::env::var("CRUCIBLE_RUNTIME") {
        Ok(base) => vec![PathBuf::from(base)],
        Err(_) => crucible_core::runtime_roots::for_current_exe(),
    }
}

/// The daemon's path for a session with no workspace, kiln or plugin roots.
///
/// What the plugin and defaults resolvers want: those two refuse `Workspace`,
/// `Kiln`, `Harness` and `Plugin` origins anyway, so supplying them would be
/// noise. A skills or cards caller builds a fuller path.
pub fn daemon_path(runtimepath: &[PathBuf]) -> Vec<RuntimeEntry> {
    let expanded: Vec<PathBuf> = runtimepath
        .iter()
        .map(|p| crate::kiln_manager::expand_tilde_path(p.as_path()))
        .collect();
    let env_plugin_dirs = crucible_core::paths::env_plugin_paths();
    let config_home = dirs::config_dir().map(|d| d.join("crucible"));
    let roots = runtime_roots();

    build_path(&PathInputs {
        env_plugin_dirs: &env_plugin_dirs,
        runtimepath: &expanded,
        config_home: config_home.as_deref(),
        runtime_roots: &roots,
        ..PathInputs::default()
    })
}

/// The session-independent roots of card and skill discovery: the global
/// config directory and the directories the app config names.
///
/// Both are injected as values, never read from the environment at discovery
/// time. Global cards are first in precedence, so a handler that read
/// `dirs::config_dir()` would resolve a developer's own cards in every test —
/// passing on CI, failing locally. `Default` is "no global cards, no
/// configured directories".
#[derive(Debug, Clone, Default)]
pub struct SourceRoots {
    /// The config home `<config_home>/crucible/agents` hangs off. `None`
    /// means "no global cards".
    pub config_home: Option<PathBuf>,
    /// `agent_directories` from the app config, tilde already expanded.
    ///
    /// **Deprecated.** It names one leaf directory that serves cards only.
    /// `runtimepath` names a root that serves every asset kind, so a user who
    /// wants to share cards, skills and themes from one directory writes one
    /// line instead of three knobs. Kept working;
    /// [`crate::agent_cards::warn_if_deprecated`] says so once.
    pub agent_directories: Vec<PathBuf>,
    /// `runtimepath` from the app config, tilde already expanded. Each entry
    /// is a root: its `agents/`, `skills/` and `themes/` are sources, at
    /// priority 600 minus the index.
    pub runtimepath: Vec<PathBuf>,
    /// The directory of each active plugin. Each is a source of skills,
    /// cards and themes at priority 200. [`crate::plugin_tools::PluginRegistry`]
    /// owns the list; this is a handle to it.
    pub plugin_dirs: ActivePluginDirs,
}

/// The directories of the active plugins, by plugin name.
///
/// One holder. Activation adds a plugin (`daemon_plugins/activate.rs`), and
/// the same path that runs `clear_source` for an inert plugin removes it, so a
/// disabled or broken plugin puts no text into a prompt. A clone shares the
/// list.
#[derive(Debug, Clone, Default)]
pub struct ActivePluginDirs(std::sync::Arc<std::sync::RwLock<BTreeMap<String, PathBuf>>>);

impl ActivePluginDirs {
    /// Record that `plugin` is active, with its directory.
    pub fn insert(&self, plugin: &str, dir: PathBuf) {
        self.write().insert(plugin.to_string(), dir);
    }

    /// Forget `plugin`.
    pub fn remove(&self, plugin: &str) {
        self.write().remove(plugin);
    }

    /// The directories, in plugin-name order.
    pub fn dirs(&self) -> Vec<PathBuf> {
        self.0
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .cloned()
            .collect()
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, BTreeMap<String, PathBuf>> {
        self.0
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl SourceRoots {
    /// The config home of this user and nothing else: the roots of a server
    /// that no session binds, such as `cru mcp`.
    pub fn ambient() -> Self {
        Self {
            config_home: dirs::config_dir(),
            ..Self::default()
        }
    }

    /// Read `agent_directories` and `runtimepath` out of the serialized app
    /// config. `home` expands a leading `~`; `None` leaves the path as
    /// written.
    pub fn from_app_config(
        config_home: Option<PathBuf>,
        app_config: Option<&serde_json::Value>,
        home: Option<&Path>,
    ) -> Self {
        let paths = |key: &str| -> Vec<PathBuf> {
            app_config
                .and_then(|v| v.get(key))
                .and_then(|v| v.as_array())
                .map(|dirs| {
                    dirs.iter()
                        .filter_map(|d| d.as_str())
                        .map(|d| crucible_core::config::expand_tilde(d, home))
                        .collect()
                })
                .unwrap_or_default()
        };
        Self {
            config_home,
            agent_directories: paths("agent_directories"),
            runtimepath: paths("runtimepath"),
            plugin_dirs: ActivePluginDirs::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::runtime_path::{search_paths, Origin, RuntimeAsset};

    /// The config home and the runtime copy under it are both on the path.
    ///
    /// `~/.config/crucible/plugins` and `~/.config/crucible/runtime/plugins`
    /// are different directories and both are searched today.
    #[test]
    fn the_daemon_path_carries_both_user_roots() {
        let built = daemon_path(&[]);
        let origins: Vec<Origin> = built.iter().map(|e| e.origin).collect();
        assert!(
            origins.contains(&Origin::UserConfig),
            "the config home must be a root: {origins:?}"
        );
    }

    /// No workspace, kiln, harness or plugin root reaches the plugin resolver.
    #[test]
    fn the_daemon_path_offers_plugins_no_containment_hazard() {
        let built = daemon_path(&[]);
        for entry in &built {
            assert!(
                !matches!(
                    entry.origin,
                    Origin::Workspace | Origin::Kiln | Origin::Harness | Origin::Plugin
                ),
                "daemon_path must not carry {:?}",
                entry.origin
            );
        }
        // And the resolver agrees, whatever the path happens to hold.
        let dirs = search_paths(RuntimeAsset::Plugins, &built);
        assert!(dirs.iter().all(|d| d.path.ends_with("plugins")));
    }

    /// `agent_directories` comes off the serialized app config with `~`
    /// expanded against the injected home, never the environment.
    #[test]
    fn source_roots_read_agent_directories_from_the_app_config() {
        let home = Path::new("/home/tester");
        let config = serde_json::json!({
            "agent_directories": ["~/shared-agents", "/abs/agents"],
            "kiln_path": "/unrelated",
        });
        let roots = SourceRoots::from_app_config(None, Some(&config), Some(home));
        assert_eq!(
            roots.agent_directories,
            vec![
                PathBuf::from("/home/tester/shared-agents"),
                PathBuf::from("/abs/agents")
            ]
        );

        let config = serde_json::json!({ "runtimepath": ["~/kit"] });
        let roots = SourceRoots::from_app_config(None, Some(&config), Some(home));
        assert_eq!(roots.runtimepath, vec![PathBuf::from("/home/tester/kit")]);

        let roots = SourceRoots::from_app_config(None, None, Some(home));
        assert!(roots.agent_directories.is_empty());
    }
}
