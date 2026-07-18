//! Core configuration types and structures.

mod cli_app;
mod errors;
mod provider;
pub mod registry;
mod server;
mod types;

#[cfg(test)]
mod tests;

#[cfg(feature = "toml")]
pub use cli_app::register_project_in_config;
pub use cli_app::{CliAppConfig, ProcessingConfig};
pub use errors::{ConfigError, ConfigValidationError};
pub use provider::EffectiveLlmConfig;
pub use server::{LoggingConfig, ScmConfig, ServerConfig, WebConfig};
pub use types::{
    parse_duration_string, plugin_name_from_url, Config, PluginEntry, PluginsConfig, ScheduleEntry,
};

/// Returns the Crucible home directory (`~/.crucible/`).
///
/// This is the default location for session storage when no kiln is explicitly
/// specified. Uses `$CRUCIBLE_HOME` if set, otherwise `$HOME/.crucible/`.
///
/// # Panics
///
/// Returns a fallback path (`/tmp/.crucible`) if the home directory cannot
/// be determined (should never happen in practice).
pub fn crucible_home() -> std::path::PathBuf {
    if let Ok(home) = std::env::var("CRUCIBLE_HOME") {
        return std::path::PathBuf::from(home);
    }
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join(".crucible")
}

/// Check if a path is the crucible home directory.
///
/// Used by storage code to avoid double `.crucible/` nesting when the
/// persist kiln is the default crucible home.
pub fn is_crucible_home(path: &std::path::Path) -> bool {
    // Canonicalize both sides (falling back to the as-given path when the path
    // doesn't exist yet) so a symlinked or trailing-slash home still matches —
    // otherwise sessions_base routes to `<home>/.crucible/.crucible/sessions`.
    let canon = |p: &std::path::Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    canon(path) == canon(&crucible_home())
}
