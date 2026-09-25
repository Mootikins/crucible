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

use crucible_core::runtime_path::{build_path, KilnRoot, PathInputs, RuntimeEntry};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where the runtime tree is on this machine, labelled by what each root
/// is rather than by its position.
#[derive(Debug, Clone, Default)]
pub struct MachineRuntime {
    /// `$CRUCIBLE_RUNTIME`: the level `env`.
    pub env: Option<PathBuf>,
    /// `<config home>/runtime`, the `cru setup` copy: the level `runtime`.
    pub user: Option<PathBuf>,
    /// The installed layout, the dev tree, then the copy extracted from the
    /// binary: the level `builtin`.
    pub shipped: Vec<PathBuf>,
}

/// The runtime roots of this machine. `$CRUCIBLE_RUNTIME` replaces the
/// others when set. `config_home` is the `crucible` config directory.
pub fn machine_runtime(config_home: Option<&Path>) -> MachineRuntime {
    match std::env::var_os("CRUCIBLE_RUNTIME") {
        Some(base) => MachineRuntime {
            env: Some(PathBuf::from(base)),
            ..MachineRuntime::default()
        },
        None => MachineRuntime {
            env: None,
            user: config_home.map(|home| home.join("runtime")),
            shipped: crucible_core::runtime_roots::shipped(),
        },
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
    let runtime = machine_runtime(config_home.as_deref());

    build_path(&PathInputs {
        env_plugin_dirs: &env_plugin_dirs,
        runtimepath: &expanded,
        config_home: config_home.as_deref(),
        env_runtime: runtime.env.as_deref(),
        user_runtime: runtime.user.as_deref(),
        runtime_roots: &runtime.shipped,
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
    /// The kiln registry, which names each attached kiln. `None` (tests, a
    /// caller with no registry) names the kilns `kiln`, `kiln-2`, ... by
    /// attach order.
    pub kiln_registry: Option<std::sync::Arc<crate::kiln_registry::KilnRegistry>>,
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
            kiln_registry: None,
        }
    }

    /// The attached kilns as sources: each takes its registered name. A kiln
    /// the registry does not know takes `kiln`, then `kiln-2`, ... by attach
    /// order.
    pub fn kiln_roots(&self, kilns: &[PathBuf]) -> Vec<KilnRoot> {
        kilns
            .iter()
            .enumerate()
            .map(|(index, path)| {
                let registered = self
                    .kiln_registry
                    .as_ref()
                    .and_then(|registry| registry.name_for(path));
                let name = match registered {
                    Some(name) => name.to_string(),
                    None if index == 0 => "kiln".to_string(),
                    None => format!("kiln-{}", index + 1),
                };
                KilnRoot::new(name, path.clone())
            })
            .collect()
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

    /// A registered kiln is a source under its registered name. A kiln the
    /// registry does not know takes a name by attach order.
    #[test]
    fn a_registered_kiln_takes_its_registered_name() {
        let data = tempfile::TempDir::new().unwrap();
        let config = serde_json::json!({ "kilns": { "notes": "/k/notes" } });
        let registry = crate::kiln_registry::KilnRegistry::from_app_config(
            crate::kiln_registry::KilnRegistryContext::for_daemon(data.path().to_path_buf()),
            Some(&config),
        )
        .unwrap();
        let roots = SourceRoots {
            kiln_registry: Some(std::sync::Arc::new(registry)),
            ..SourceRoots::default()
        };
        let named: Vec<String> = roots
            .kiln_roots(&[PathBuf::from("/k/notes"), PathBuf::from("/k/other")])
            .into_iter()
            .map(|k| k.name)
            .collect();
        assert_eq!(named, ["notes", "kiln-2"]);
    }
}
