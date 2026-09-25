//! Building the ordered root list from config, environment and session.
//!
//! The one place that turns scattered inputs into a `Vec<RuntimeEntry>`.
//! Every input arrives as a value: nothing here reads an environment variable
//! or calls `dirs::`. That is not style. A resolver that read the environment
//! would resolve the developer's own `~/.config/crucible` inside every test —
//! passing on CI and failing locally, which is how the sibling bugs in
//! `runtime_skill_paths` and `defaults_candidates_from` were found.

use super::asset::RuntimeAsset;
use super::entry::{
    level_priority, LevelPriorities, Origin, RuntimeEntry, AGENT_DIRECTORIES_WITHIN,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The empty harness table `PathInputs::default()` borrows.
///
/// `&BTreeMap` has no `Default` the way `&[T]` does, and a derived `Default`
/// on a struct holding one does not compile. A `const`-constructed static is
/// the borrow-free way to say "no harness rows", which is also the shipped
/// default: a row IS the opt-in.
static NO_HARNESSES: BTreeMap<String, PathBuf> = BTreeMap::new();

/// The empty level table `PathInputs::default()` borrows: every level keeps
/// its default priority.
static NO_OVERRIDES: LevelPriorities = BTreeMap::new();

/// An attached kiln as a source: its name, its directory and its priority.
///
/// The name is the kiln's registered name. The priority comes only from the
/// user's config (`kilns.<name>.priority`); every other kiln has the level
/// `kiln`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KilnRoot {
    pub name: String,
    pub path: PathBuf,
    pub priority: i32,
    /// The position inside one priority. A personal kiln is 0: first in
    /// level 900.
    pub within: u8,
}

impl KilnRoot {
    /// A kiln at the default priority of the level `kiln`.
    pub fn new(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            priority: super::entry::PriorityLevel::Kiln.default_priority(),
            within: 0,
        }
    }
}

/// Everything the path is built from.
///
/// Borrowed rather than owned so a caller assembles it from config without
/// cloning; `Default` is "nothing configured", which yields an empty path.
#[derive(Debug)]
pub struct PathInputs<'a> {
    /// `$CRUCIBLE_PLUGIN_PATH`, already split. These are plugin *directories*,
    /// not roots above them.
    pub env_plugin_dirs: &'a [PathBuf],
    /// `<workspace>`, and the relative roots to look for inside it.
    pub workspace: Option<&'a Path>,
    pub workspace_roots: &'a [String],
    /// The attached kilns, in attach order. Only `<kiln>/.crucible` is ever a
    /// root; a kiln's visible top level belongs to notes.
    pub kilns: &'a [KilnRoot],
    /// `runtimepath` from the config, in the order the user wrote it.
    pub runtimepath: &'a [PathBuf],
    /// `[harnesses]`: name to home root. A row IS the opt-in — an absent row
    /// means that harness is not read.
    pub harnesses: &'a BTreeMap<String, PathBuf>,
    /// `~/.config/crucible`. Distinct from the runtime root below it.
    pub config_home: Option<&'a Path>,
    /// `$CRUCIBLE_RUNTIME`: a root at the level `env`.
    pub env_runtime: Option<&'a Path>,
    /// `~/.config/crucible/runtime`, the `cru setup` copy: the level
    /// `runtime`.
    pub user_runtime: Option<&'a Path>,
    /// The shipped roots, highest first, from `runtime_roots::shipped`: the
    /// level `builtin`. Passed in so tests do not materialise the bundled
    /// tree.
    pub runtime_roots: &'a [PathBuf],
    /// Deprecated `agent_directories`: card directories, not roots.
    pub agent_directories: &'a [PathBuf],
    /// Directories of plugins that are loaded AND active.
    ///
    /// Must exclude disabled and broken plugins. `loaded_plugin_dirs`
    /// deliberately includes both, and passing that list unfiltered would let
    /// a disabled plugin put text into an agent's system prompt.
    pub plugin_dirs: &'a [PathBuf],
    /// `sources.priority`: a new priority for each level it names. A kiln's
    /// priority is already resolved in its [`KilnRoot`].
    pub levels: &'a LevelPriorities,
}

impl Default for PathInputs<'_> {
    fn default() -> Self {
        Self {
            env_plugin_dirs: &[],
            workspace: None,
            workspace_roots: &[],
            kilns: &[],
            runtimepath: &[],
            harnesses: &NO_HARNESSES,
            config_home: None,
            env_runtime: None,
            user_runtime: None,
            runtime_roots: &[],
            agent_directories: &[],
            plugin_dirs: &[],
            levels: &NO_OVERRIDES,
        }
    }
}

/// The ordered path, highest precedence first.
///
/// Origin order is the order below, and it is the same order
/// [`Origin`]'s declaration gives. Position is precedence for every asset
/// kind; `search_paths` preserves it.
pub fn build_path(inputs: &PathInputs<'_>) -> Vec<RuntimeEntry> {
    let mut path = Vec::new();

    for (index, dir) in inputs.env_plugin_dirs.iter().enumerate() {
        let mut entry = RuntimeEntry::leaf(dir.clone(), Origin::Env, RuntimeAsset::Plugins);
        entry.name = format!("env-{}", index + 1);
        entry.priority -= offset(index);
        path.push(entry);
    }

    if let Some(env_runtime) = inputs.env_runtime {
        let mut entry = RuntimeEntry::root(env_runtime, Origin::EnvRuntime);
        entry.priority -= offset(inputs.env_plugin_dirs.len());
        path.push(entry);
    }

    if let Some(workspace) = inputs.workspace.filter(|w| !w.as_os_str().is_empty()) {
        for name in inputs.workspace_roots {
            let mut entry = RuntimeEntry::root(workspace.join(name), Origin::Workspace);
            // `.crucible` is Crucible's own; `.agents`, `.claude` and the
            // other harness directories rank just below it.
            if name != ".crucible" {
                entry.name = format!("workspace-{}", name.trim_start_matches('.'));
                entry.priority -= 10;
            }
            path.push(entry);
        }
    }

    for kiln in inputs.kilns {
        let mut entry = RuntimeEntry::root(kiln.path.join(".crucible"), Origin::Kiln);
        entry.name = kiln.name.clone();
        entry.priority = kiln.priority;
        entry.within = kiln.within;
        path.push(entry);
    }

    for (index, root) in inputs.runtimepath.iter().enumerate() {
        path.push(RuntimeEntry::root(root.clone(), Origin::Config(index)));
    }

    for (name, root) in inputs.harnesses {
        let mut entry =
            RuntimeEntry::root(root.clone(), Origin::Harness).with_harness(name.clone());
        entry.name = name.clone();
        path.push(entry);
    }

    // `agent_directories` BEFORE the config home, because that is the order
    // they already have: `card_directories` listed the global directory first
    // in a lowest-first list, so a configured directory shadowed it. Same
    // relative priority, now expressed highest-first like everything else.
    for (index, dir) in inputs.agent_directories.iter().enumerate() {
        let mut entry = RuntimeEntry::leaf(dir.clone(), Origin::UserConfig, RuntimeAsset::Cards);
        entry.name = format!("agent-dir-{}", index + 1);
        entry.within = AGENT_DIRECTORIES_WITHIN;
        path.push(entry);
    }

    if let Some(config_home) = inputs.config_home {
        path.push(RuntimeEntry::root(config_home, Origin::UserConfig).with_harness("crucible"));
    }

    for dir in inputs.plugin_dirs {
        let mut entry = RuntimeEntry::root(dir.clone(), Origin::Plugin);
        if let Some(name) = dir.file_name() {
            entry.name = name.to_string_lossy().into_owned();
        }
        path.push(entry);
    }

    if let Some(user_runtime) = inputs.user_runtime {
        path.push(RuntimeEntry::root(user_runtime, Origin::UserRuntime));
    }

    for (index, root) in inputs.runtime_roots.iter().enumerate() {
        let mut entry = RuntimeEntry::root(root.clone(), Origin::Bundled);
        // The shipped roots are fallbacks in a fixed order, not rivals.
        if index > 0 {
            entry.name = format!("builtin-{}", index + 1);
            entry.within = u8::try_from(index).unwrap_or(u8::MAX);
        }
        path.push(entry);
    }

    // A level the config moved moves every source in it by the same amount,
    // so the order inside the level stays.
    for entry in path.iter_mut().filter(|e| e.origin != Origin::Kiln) {
        let level = entry.origin.level();
        entry.priority += level_priority(inputs.levels, level) - level.default_priority();
    }

    path
}

/// The priority drop of the entry at `index` of a list, so a later entry
/// ranks below the one before it.
fn offset(index: usize) -> i32 {
    i32::try_from(index).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_path::search_paths;

    fn inputs<'a>() -> PathInputs<'a> {
        PathInputs::default()
    }

    /// The two user roots are both present, and the config home outranks the
    /// runtime copy under it.
    ///
    /// Collapsing them is what made a theme written by `cru setup` invisible.
    #[test]
    fn both_user_roots_are_present_config_home_first() {
        let config_home = PathBuf::from("/home/u/.config/crucible");
        let runtime = config_home.join("runtime");
        let built = build_path(&PathInputs {
            config_home: Some(&config_home),
            user_runtime: Some(&runtime),
            ..inputs()
        });

        let paths: Vec<_> = built.iter().map(|e| e.path().to_path_buf()).collect();
        assert_eq!(
            paths,
            vec![config_home.clone(), config_home.join("runtime")]
        );
        assert_eq!(built[0].origin, Origin::UserConfig);
        assert_eq!(built[1].origin, Origin::UserRuntime);
    }

    /// A kiln contributes only `<kiln>/.crucible`, and no executing kind sees
    /// it. This is the invariant `execution_roots` rests on, at the build
    /// layer rather than the table layer.
    #[test]
    fn an_attached_kiln_contributes_no_plugin_or_defaults_directory() {
        let kiln = KilnRoot::new("k", "/k");
        let built = build_path(&PathInputs {
            kilns: std::slice::from_ref(&kiln),
            ..inputs()
        });

        assert!(search_paths(RuntimeAsset::Plugins, &built).is_empty());
        assert!(search_paths(RuntimeAsset::Defaults, &built).is_empty());
        assert_eq!(search_paths(RuntimeAsset::Skills, &built).len(), 1);
    }

    /// An empty workspace path is "no workspace", not the current directory.
    ///
    /// `Path::new("").join(".crucible")` is RELATIVE, so it would resolve
    /// against whatever directory the daemon was started in.
    #[test]
    fn an_empty_workspace_contributes_nothing() {
        let empty = PathBuf::new();
        let roots = vec![".crucible".to_string()];
        let built = build_path(&PathInputs {
            workspace: Some(&empty),
            workspace_roots: &roots,
            ..inputs()
        });
        assert!(built.is_empty());
    }

    /// `runtimepath` keeps the order the user wrote, and outranks the
    /// harnesses and both user roots.
    #[test]
    fn runtimepath_keeps_its_order_and_outranks_user_roots() {
        let rtp = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        let config_home = PathBuf::from("/home/u/.config/crucible");
        let built = build_path(&PathInputs {
            runtimepath: &rtp,
            config_home: Some(&config_home),
            ..inputs()
        });

        let found = search_paths(RuntimeAsset::Skills, &built);
        assert_eq!(
            found.iter().map(|s| s.path.clone()).collect::<Vec<_>>(),
            vec![
                PathBuf::from("/a/skills"),
                PathBuf::from("/b/skills"),
                config_home.join("skills"),
            ]
        );
        assert_eq!(built[0].origin, Origin::Config(0));
        assert_eq!(built[1].origin, Origin::Config(1));
    }

    /// A harness row carries its name through as provenance.
    #[test]
    fn a_harness_row_is_tagged_with_its_name() {
        let mut harnesses = BTreeMap::new();
        harnesses.insert("claude".to_string(), PathBuf::from("/home/u/.claude"));
        let built = build_path(&PathInputs {
            harnesses: &harnesses,
            ..inputs()
        });

        let found = search_paths(RuntimeAsset::Skills, &built);
        assert_eq!(found[0].path, PathBuf::from("/home/u/.claude/skills"));
        assert_eq!(found[0].harness.as_deref(), Some("claude"));
    }

    /// No harness rows means no harness paths. A row IS the opt-in.
    #[test]
    fn no_harness_rows_means_no_harness_paths() {
        let built = build_path(&inputs());
        assert!(build_path(&inputs()).is_empty());
        assert!(search_paths(RuntimeAsset::Skills, &built).is_empty());
    }

    /// `agent_directories` is a card directory, not a root above one.
    #[test]
    fn agent_directories_are_card_leaves() {
        let dirs = vec![PathBuf::from("/shared/cards")];
        let built = build_path(&PathInputs {
            agent_directories: &dirs,
            ..inputs()
        });

        let cards = search_paths(RuntimeAsset::Cards, &built);
        assert_eq!(cards[0].path, PathBuf::from("/shared/cards"));
        assert!(search_paths(RuntimeAsset::Skills, &built).is_empty());
    }

    /// `CRUCIBLE_PLUGIN_PATH` is a plugin directory, not a root above one.
    #[test]
    fn env_plugin_dirs_are_plugin_leaves() {
        let dirs = vec![PathBuf::from("/x")];
        let built = build_path(&PathInputs {
            env_plugin_dirs: &dirs,
            ..inputs()
        });
        assert_eq!(
            search_paths(RuntimeAsset::Plugins, &built)[0].path,
            PathBuf::from("/x")
        );
    }

    /// A plugin's own directory supplies skills, below every user-named root.
    #[test]
    fn a_plugin_root_sits_below_the_user_roots() {
        let config_home = PathBuf::from("/home/u/.config/crucible");
        let plugins = vec![PathBuf::from("/home/u/.config/crucible/plugins/helper")];
        let built = build_path(&PathInputs {
            config_home: Some(&config_home),
            plugin_dirs: &plugins,
            ..inputs()
        });

        let found = search_paths(RuntimeAsset::Skills, &built);
        assert_eq!(
            found.iter().map(|s| s.path.clone()).collect::<Vec<_>>(),
            vec![
                config_home.join("skills"),
                PathBuf::from("/home/u/.config/crucible/plugins/helper/skills"),
            ],
            "a plugin's skills must never outrank the user's own"
        );
    }

    /// Every entry gets a source name of its own, and the sources inside
    /// level 900 are in a fixed order.
    #[test]
    fn every_entry_has_a_distinct_source_name() {
        let env = vec![PathBuf::from("/e1"), PathBuf::from("/e2")];
        let roots = vec![".crucible".to_string(), ".agents".to_string()];
        let kilns = vec![KilnRoot::new("notes", "/k1"), KilnRoot::new("team", "/k2")];
        let rtp = vec![PathBuf::from("/r")];
        let dirs = vec![PathBuf::from("/d1"), PathBuf::from("/d2")];
        let config_home = PathBuf::from("/home/u/.config/crucible");
        let plugins = vec![PathBuf::from("/p/helper")];
        let user_runtime = PathBuf::from("/rt");
        let env_runtime = PathBuf::from("/er");
        let runtime = vec![PathBuf::from("/b1"), PathBuf::from("/b2")];
        let built = build_path(&PathInputs {
            env_plugin_dirs: &env,
            workspace: Some(Path::new("/ws")),
            workspace_roots: &roots,
            kilns: &kilns,
            runtimepath: &rtp,
            config_home: Some(&config_home),
            agent_directories: &dirs,
            plugin_dirs: &plugins,
            env_runtime: Some(&env_runtime),
            user_runtime: Some(&user_runtime),
            runtime_roots: &runtime,
            ..inputs()
        });
        let named: Vec<(&str, i32, u8)> = built
            .iter()
            .map(|e| (e.name.as_str(), e.priority, e.within))
            .collect();
        assert_eq!(
            named,
            [
                ("env-1", 1000, 0),
                ("env-2", 999, 0),
                ("env-runtime", 998, 0),
                ("workspace", 800, 0),
                ("workspace-agents", 790, 0),
                ("notes", 700, 0),
                ("team", 700, 0),
                ("config-1", 600, 0),
                ("agent-dir-1", 900, 1),
                ("agent-dir-2", 900, 1),
                ("personal", 900, 2),
                ("helper", 200, 0),
                ("runtime", 300, 0),
                ("builtin", 100, 0),
                ("builtin-2", 100, 1),
            ]
        );
    }

    /// A shipped root is `builtin`, whatever its position. The first root
    /// used to be labelled as the user's `cru setup` copy without a check,
    /// so `$CRUCIBLE_RUNTIME` or an installed tree took the level `runtime`.
    #[test]
    fn a_shipped_root_is_builtin_whatever_its_position() {
        let shipped = vec![PathBuf::from("/usr/share/crucible/runtime")];
        let built = build_path(&PathInputs {
            runtime_roots: &shipped,
            ..inputs()
        });
        assert_eq!(built[0].origin, Origin::Bundled);
        assert_eq!(built[0].name, "builtin");
    }

    /// `sources.priority` moves the levels it names, and nothing else.
    #[test]
    fn a_level_override_moves_its_sources() {
        let config_home = PathBuf::from("/c");
        let roots = vec![".crucible".to_string(), ".agents".to_string()];
        let levels = LevelPriorities::from([(super::super::entry::PriorityLevel::Workspace, 950)]);
        let built = build_path(&PathInputs {
            config_home: Some(&config_home),
            workspace: Some(Path::new("/ws")),
            workspace_roots: &roots,
            levels: &levels,
            ..inputs()
        });
        let named: Vec<(&str, i32)> = built
            .iter()
            .map(|e| (e.name.as_str(), e.priority))
            .collect();
        assert_eq!(
            named,
            [
                ("workspace", 950),
                ("workspace-agents", 940),
                ("personal", 900)
            ]
        );
    }
}
