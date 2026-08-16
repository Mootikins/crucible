//! The trees the daemon loads or executes code from — one list, kept by
//! construction.
//!
//! Four resolvers used to answer "where does the daemon get code from" and they
//! disagreed:
//!
//! - [`crate::daemon_plugins::daemon_plugin_paths`] reads `$CRUCIBLE_PLUGIN_PATH`,
//!   the user's `plugins/`, `<runtimepath>/plugins` and `$CRUCIBLE_RUNTIME/plugins`
//! - [`crate::runtime_defaults::defaults_candidates`] executes
//!   `<entry>/defaults/init.lua` in every session VM
//! - `$CRUCIBLE_CONFIG_DIR/config.toml` carries `runtimepath`, `[acp.agents.*]`
//!   command paths, `[permissions]` and `[security.shell]` — writing it is
//!   arbitrary execution on the next start
//! - [`crate::tools::protected::daemon_roots`], the write-denied set, was built
//!   from `runtime_roots::for_current_exe()`, which deliberately consults no
//!   environment variable at all
//!
//! Every disagreement fell the same way: a tree the daemon executes that the
//! protected set never named, so an agent could write it and be run with host
//! privileges on the next start. That is CVE-2026-25725's shape.
//!
//! The fix is not to make the two lists *agree* — a second list that has to be
//! kept in step is the same defect deferred. Each resolver passes its answer
//! through [`record`], and the protected set is what came back. **A tree cannot
//! be loaded without being protected, because naming it for the loader is what
//! protects it.**
//!
//! [`baseline`] covers the trees that are knowable before any loader runs — the
//! env vars, the exe-relative layout, the config directory, and the trees
//! `config.toml`'s own `runtimepath` names — so a session built early in
//! startup is not protected by less than a session built late. Recording is
//! what keeps the set honest; the baseline is what keeps it from depending on
//! WHEN it is asked.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Trees a resolver has handed out, beyond [`baseline`].
///
/// Grows only, and only with directories and files the daemon reads code or
/// configuration from; bounded by the number of distinct trees a process
/// resolves. A `Mutex<Vec<_>>` rather than a `OnceLock` because the config
/// `runtimepath` is not known until the config is read, and a kiln can be on it.
fn registry() -> &'static Mutex<Vec<PathBuf>> {
    static REGISTRY: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

/// Record `trees` as places the daemon loads or executes code from.
///
/// Called by each resolver on its own result, so the recorded set is the
/// resolved set by construction rather than by a reviewer noticing. Recording a
/// path that does not exist is correct and deliberate — protection is judged on
/// the name, because the file the agent plants is the file that did not exist
/// (rule 2 in [`crate::tools::protected`]).
pub(crate) fn record(trees: impl IntoIterator<Item = PathBuf>) {
    let mut registry = registry().lock().expect("execution roots lock poisoned");
    for tree in trees {
        if !registry.contains(&tree) {
            registry.push(tree);
        }
    }
}

/// The trees the daemon loads or executes from before any resolver has run.
///
/// The env-var layer `runtime_roots::for_current_exe()` deliberately omits, plus
/// the config directory — `config.toml` names the runtimepath, the ACP agent
/// commands and the shell policy, so a write there is execution too, and
/// `$CRUCIBLE_CONFIG_DIR` moves it — plus the trees that same file's
/// `runtimepath` names, which used to be covered only once a loader had
/// recorded them.
pub(crate) fn baseline() -> Vec<PathBuf> {
    let mut trees = crucible_core::runtime_roots::for_current_exe();

    if let Ok(runtime) = std::env::var("CRUCIBLE_RUNTIME") {
        trees.push(PathBuf::from(runtime));
    }
    if let Ok(plugin_path) = std::env::var("CRUCIBLE_PLUGIN_PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        trees.extend(
            plugin_path
                .split(sep)
                .filter(|p| !p.is_empty())
                .map(PathBuf::from),
        );
    }
    // Both spellings: the env var relocates the file, and `dirs::config_dir()`
    // is still where `agents/`, `skills/` and `plugins/` are looked for.
    let config_file = crucible_core::config::CliAppConfig::default_config_path();
    if let Some(config) = config_file.parent() {
        trees.push(config.to_path_buf());
    }
    if let Some(config) = dirs::config_dir() {
        trees.push(config.join("crucible"));
    }
    trees.extend(runtimepath_execution_trees(&config_file));

    trees
}

/// The executed subdirectories of every `runtimepath` entry `config.toml`
/// names.
///
/// This is the ordering dependency removed. Both loaders record their
/// runtimepath answers, but a session whose containment is built *before* they
/// run — and plugin bootstrap is not the first thing a daemon does — saw a
/// protected set that named the runtimepath nowhere. The tree is a session
/// scope root in the documented case (`docs/Help/Extending/Creating
/// Plugins.md`: `runtimepath = ["~/kilns/work"]`), so "unprotected" there means
/// "writable by the agent", and `<kiln>/plugins/evil/init.lua` runs with host
/// privileges on the next start.
///
/// The file is parsed here rather than taken as a loaded `CliAppConfig`
/// argument precisely because [`baseline`] must answer before anything has
/// loaded one. One key out of a raw TOML table is the smallest thing that does
/// it; an absent or malformed file yields nothing, which is what the daemon's
/// own loader falls back to as well.
fn runtimepath_execution_trees(config_file: &Path) -> Vec<PathBuf> {
    let Ok(contents) = std::fs::read_to_string(config_file) else {
        return Vec::new();
    };
    let Ok(table) = contents.parse::<toml::Table>() else {
        return Vec::new();
    };
    let Some(entries) = table.get("runtimepath").and_then(toml::Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(toml::Value::as_str)
        .flat_map(|entry| {
            let entry = crate::kiln_manager::expand_tilde_path(Path::new(entry));
            // The subdirectories, never the entry itself: a KILN on the
            // runtimepath is the documented case, and protecting the whole tree
            // would make the user's own notes read-only to buy nothing. These
            // two are what the loaders search — `daemon_plugin_paths` takes
            // `<entry>/plugins`, `defaults_candidates` takes
            // `<entry>/defaults/init.lua`. `defaults/` is taken whole because
            // what runs out of it is Lua, and Lua that runs reads its siblings.
            //
            // Existence is not consulted, unlike `daemon_plugin_paths`, which
            // only adds a `plugins/` it can see: the directory an agent creates
            // is by definition the one that did not exist (rule 2 in
            // [`crate::tools::protected`]).
            [entry.join("plugins"), entry.join("defaults")]
        })
        .collect()
}

/// Every tree the daemon loads or executes from, as far as this process knows.
///
/// [`baseline`] plus everything a resolver has recorded. Order is
/// baseline-first; callers use it as a set, not a search path.
pub(crate) fn all() -> Vec<PathBuf> {
    let mut trees = baseline();
    let registry = registry().lock().expect("execution roots lock poisoned");
    for tree in registry.iter() {
        if !trees.contains(tree) {
            trees.push(tree.clone());
        }
    }
    trees
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point, stated as an assertion: every tree this module has been
    /// told the daemon loads from is one the protected set refuses writes to.
    ///
    /// The load set is read out of [`all`] — the recorded set itself — rather
    /// than by naming resolvers, so it grows with whatever records and needs no
    /// maintenance when a fifth resolver arrives. What that catches is
    /// [`crate::tools::protected::daemon_roots`] drifting back into rebuilding
    /// its own list, which is the original defect.
    ///
    /// What no test in this process can catch is a resolver that never calls
    /// [`record`] at all — nothing here knows it exists. That one is held by
    /// construction (a resolver records its own result, so forgetting means
    /// deleting a line from the function that computes the answer) and by
    /// review, not by this assertion. The two real resolvers are still driven
    /// first, so their recording is exercised rather than assumed.
    #[test]
    fn the_protected_set_is_a_superset_of_the_load_set() {
        let tmp = tempfile::TempDir::new().unwrap();
        let rtp = tmp.path().join("kiln");
        std::fs::create_dir_all(rtp.join("plugins")).unwrap();

        let resolved: Vec<PathBuf> =
            crate::daemon_plugins::daemon_plugin_paths(std::slice::from_ref(&rtp))
                .into_iter()
                .map(|(dir, _)| dir)
                .chain(crate::runtime_defaults::defaults_candidates(
                    std::slice::from_ref(&rtp),
                    Some("/env/runtime"),
                ))
                .collect();
        assert!(
            resolved.len() > 2,
            "precondition: the loaders offered something to check"
        );

        let load_set = all();
        for tree in &resolved {
            assert!(
                load_set.contains(tree),
                "a resolver handed out {} without recording it: {load_set:?}",
                tree.display()
            );
        }

        let protected = crate::tools::protected::daemon_roots();
        for tree in &load_set {
            assert!(
                protected.iter().any(|root| tree.starts_with(root)),
                "the daemon loads code from {} and no protected root covers it: {protected:?}",
                tree.display()
            );
        }
    }

    /// The ordering dependency, removed.
    ///
    /// A tree named ONLY by `config.toml`'s `runtimepath` is protected with no
    /// loader having run — which is the state a session built early in startup
    /// sees, and the state in which `<kiln>/plugins/evil/init.lua` used to be
    /// writable.
    ///
    /// `CRUCIBLE_CONFIG_DIR` is set rather than worked around: it is the
    /// variable [`crucible_core::config::CliAppConfig::default_config_path`]
    /// reads, and "the baseline resolves the config file the daemon would
    /// actually load" is exactly the behavior under test.
    #[test]
    fn a_config_declared_runtimepath_is_protected_before_any_loader_runs() {
        let tmp = tempfile::TempDir::new().unwrap();
        let kiln = tmp.path().join("kilns").join("work");
        std::fs::write(
            tmp.path().join("config.toml"),
            format!("runtimepath = [{:?}]\n", kiln.to_string_lossy()),
        )
        .unwrap();
        let _guard = crucible_core::test_support::EnvVarGuard::set(
            "CRUCIBLE_CONFIG_DIR",
            tmp.path().to_string_lossy().into_owned(),
        );

        let protected = crate::tools::protected::daemon_roots();
        for executed in [kiln.join("plugins"), kiln.join("defaults")] {
            assert!(
                !executed.exists(),
                "precondition: the directory an agent would plant does not exist"
            );
            assert!(
                protected.contains(&executed),
                "{} is Lua the daemon executes and nothing had to load first: {protected:?}",
                executed.display()
            );
        }
        assert!(
            !protected.contains(&kiln),
            "and the entry ITSELF stays writable — a kiln on the runtimepath is \
             the documented case, and its notes are not plugins: {protected:?}"
        );
    }

    /// A `runtimepath` entry is spelled the way a user spells it in TOML, which
    /// is with a `~`. An unexpanded `~/kilns/work/plugins` matches no path a
    /// write is ever judged against, so it protects nothing.
    #[test]
    fn a_tilde_in_the_configured_runtimepath_is_expanded() {
        let Some(home) = dirs::home_dir() else {
            return;
        };
        let tmp = tempfile::TempDir::new().unwrap();
        let config_file = tmp.path().join("config.toml");
        std::fs::write(&config_file, "runtimepath = [\"~/kilns/work\"]\n").unwrap();

        let trees = runtimepath_execution_trees(&config_file);

        assert!(
            trees.contains(&home.join("kilns/work/plugins")),
            "{trees:?}"
        );
        assert!(
            trees.contains(&home.join("kilns/work/defaults")),
            "{trees:?}"
        );
    }

    /// A machine with no config file, and one whose config is mid-edit, must
    /// still get a baseline — a parse error is not a reason to hand a session
    /// LESS protection than it would otherwise have.
    #[test]
    fn an_absent_or_malformed_config_yields_no_runtimepath_trees() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert!(runtimepath_execution_trees(&tmp.path().join("nothing.toml")).is_empty());

        let broken = tmp.path().join("broken.toml");
        std::fs::write(&broken, "runtimepath = [unclosed\n").unwrap();
        assert!(runtimepath_execution_trees(&broken).is_empty());

        let no_key = tmp.path().join("plain.toml");
        std::fs::write(&no_key, "[chat]\nmodel = \"x\"\n").unwrap();
        assert!(runtimepath_execution_trees(&no_key).is_empty());
    }

    /// The env-var layer `runtime_roots::for_current_exe()` omits by design.
    /// Named individually because each was a separate live escape.
    #[test]
    fn the_baseline_names_the_config_directory() {
        let baseline = baseline();
        let config = crucible_core::config::CliAppConfig::default_config_path();
        let dir = config.parent().expect("config.toml has a directory");
        assert!(
            baseline.contains(&dir.to_path_buf()),
            "config.toml carries runtimepath, ACP command paths and the shell \
             policy — writing it is execution on the next start: {baseline:?}"
        );
    }

    /// Recording is idempotent: a resolver called once per session must not
    /// grow the set without bound.
    #[test]
    fn recording_the_same_tree_twice_keeps_one_entry() {
        let tree = PathBuf::from("/execution-roots-test/only-once");
        record([tree.clone()]);
        record([tree.clone()]);
        assert_eq!(all().iter().filter(|t| *t == &tree).count(), 1);
    }
}
