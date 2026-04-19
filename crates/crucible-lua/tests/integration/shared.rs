//! Shared helpers for integration tests.

use std::path::Path;
use tempfile::TempDir;

/// Helper to create a temp dir with tool files
pub(super) async fn setup_tool_dir() -> TempDir {
    TempDir::new().unwrap()
}

pub(super) fn create_plugin_files(root: &Path, name: &str, init_source: &str, module_source: &str) {
    let plugin_dir = root.join(name);
    std::fs::create_dir_all(plugin_dir.join(name)).unwrap();

    std::fs::write(
        plugin_dir.join("plugin.yaml"),
        format!(
            "name: {name}\nversion: \"1.0.0\"\nmain: init.lua\nexports:\n  auto_discover: true\n"
        ),
    )
    .unwrap();
    std::fs::write(plugin_dir.join("init.lua"), init_source).unwrap();
    std::fs::write(plugin_dir.join(name).join("core.lua"), module_source).unwrap();
}
