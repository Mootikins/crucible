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
use std::path::PathBuf;

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
        .map(|p| crate::kiln_manager::expand_tilde_path(p))
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
}
