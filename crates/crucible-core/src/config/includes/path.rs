use std::path::{Path, PathBuf};

/// Resolve an include path relative to a base directory
///
/// - If the path starts with `/` or `~`, it's treated as absolute
/// - Otherwise, it's relative to the base directory
pub fn resolve_include_path(include_path: &str, base_dir: &Path) -> PathBuf {
    let path = if include_path.starts_with('/') {
        PathBuf::from(include_path)
    } else if include_path.starts_with('~') {
        // Expand home directory
        if let Some(home) = dirs::home_dir() {
            if include_path == "~" {
                home
            } else if let Some(rest) = include_path.strip_prefix("~/") {
                home.join(rest)
            } else {
                // ~something (not ~/) - treat as relative path
                base_dir.join(include_path)
            }
        } else {
            base_dir.join(include_path)
        }
    } else {
        base_dir.join(include_path)
    };

    path
}
