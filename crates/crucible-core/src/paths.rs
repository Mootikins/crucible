//! The plugin search roots that every loader shares.
//!
//! Two loaders build a plugin path list: the daemon
//! (`daemon_plugin_paths`) and the standalone `PluginManager` in
//! `crucible-lua` (`with_standard_paths`). Each used to read
//! `CRUCIBLE_PLUGIN_PATH` and `~/.config/crucible/plugins/` on its own, and
//! the two copies drifted (one removed duplicates, one did not). `crucible-lua`
//! cannot depend on the daemon, so the shared part lives here. The daemon
//! appends its `runtimepath` and the shipped runtime after these.

use std::path::PathBuf;

/// The directories that `CRUCIBLE_PLUGIN_PATH` names, in order, with
/// duplicates removed. Highest priority of all plugin roots.
pub fn env_plugin_paths() -> Vec<PathBuf> {
    let Ok(env_paths) = std::env::var("CRUCIBLE_PLUGIN_PATH") else {
        return Vec::new();
    };
    let separator = if cfg!(windows) { ';' } else { ':' };
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in env_paths.split(separator) {
        let path = PathBuf::from(entry);
        if !entry.is_empty() && !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

/// The user's plugin directory: `~/.config/crucible/plugins/`.
pub fn user_plugins_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("crucible").join("plugins"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_plugins_dir_ends_with_crucible_plugins() {
        let dir = user_plugins_dir().expect("config dir");
        assert!(dir.ends_with("crucible/plugins"));
    }
}
