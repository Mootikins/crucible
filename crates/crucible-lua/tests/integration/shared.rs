//! Shared helpers for integration tests.

use std::path::Path;

pub(super) fn create_plugin_files(root: &Path, name: &str, init_source: &str, module_source: &str) {
    let plugin_dir = root.join(name);
    std::fs::create_dir_all(&plugin_dir).unwrap();

    std::fs::write(plugin_dir.join("init.lua"), init_source).unwrap();
    // `<plugin>/core.lua`, required as `<plugin>.core`: the plugin's own
    // directory IS its module namespace, under the root its parent is.
    std::fs::write(plugin_dir.join("core.lua"), module_source).unwrap();
}
