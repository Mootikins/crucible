use serde::de::DeserializeOwned;
use std::fs;
use std::path::Path;

/// Read configuration from a primary file with fallback to workspace.toml section.
///
/// Attempts to read from `{dir}/.crucible/{filename}` first. If that file doesn't exist,
/// falls back to reading the entire `{dir}/.crucible/workspace.toml` file and deserializing it.
/// This provides backward compatibility with the legacy workspace.toml format.
///
/// # Arguments
/// * `dir` - The base directory (typically workspace root)
/// * `filename` - The primary config filename (e.g., "kiln.toml", "project.toml")
/// * `_ws_section` - Unused; kept for API clarity (workspace.toml is read as a whole)
///
/// # Returns
/// `Some(T)` if either file exists and deserializes successfully, `None` otherwise.
pub(crate) fn read_with_workspace_fallback<T: DeserializeOwned>(
    dir: &Path,
    filename: &str,
    _ws_section: &str,
) -> Option<T> {
    let crucible_dir = dir.join(".crucible");
    let primary_path = crucible_dir.join(filename);

    // Try primary file first
    if primary_path.exists() {
        let content = fs::read_to_string(&primary_path).ok()?;
        return toml::from_str::<T>(&content).ok();
    }

    // Fall back to workspace.toml
    let workspace_path = crucible_dir.join("workspace.toml");
    let content = fs::read_to_string(&workspace_path).ok()?;
    toml::from_str::<T>(&content).ok()
}
