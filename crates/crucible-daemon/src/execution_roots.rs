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
//! [`baseline`] covers the trees that are known before any loader runs, so a
//! session built early in startup is not protected by less than a session built
//! late.

use std::path::PathBuf;
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
/// `$CRUCIBLE_CONFIG_DIR` moves it.
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
    if let Some(config) = crucible_core::config::CliAppConfig::default_config_path().parent() {
        trees.push(config.to_path_buf());
    }
    if let Some(config) = dirs::config_dir() {
        trees.push(config.join("crucible"));
    }

    trees
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

    /// The whole point, stated as an assertion: every tree either loader
    /// searches is one the protected set refuses writes to.
    ///
    /// Driven through the real resolvers rather than a fixed list, so a fifth
    /// resolver added later fails this the moment it forgets to record.
    #[test]
    fn the_protected_set_is_a_superset_of_the_load_set() {
        let tmp = tempfile::TempDir::new().unwrap();
        let rtp = tmp.path().join("kiln");
        std::fs::create_dir_all(rtp.join("plugins")).unwrap();

        let load_set: Vec<PathBuf> =
            crate::daemon_plugins::daemon_plugin_paths(std::slice::from_ref(&rtp))
                .into_iter()
                .map(|(dir, _)| dir)
                .chain(crate::runtime_defaults::defaults_candidates(
                    std::slice::from_ref(&rtp),
                    Some("/env/runtime"),
                ))
                .collect();
        assert!(
            load_set.len() > 2,
            "precondition: the loaders offered something to check"
        );

        let protected = crate::tools::protected::daemon_roots();
        for tree in &load_set {
            assert!(
                protected.iter().any(|root| tree.starts_with(root)),
                "the daemon loads code from {} and no protected root covers it: {protected:?}",
                tree.display()
            );
        }
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
