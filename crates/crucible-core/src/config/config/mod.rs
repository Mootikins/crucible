//! Core configuration types and structures.

mod cli_app;
mod errors;
mod kiln_name;
mod provider;
#[cfg(feature = "toml")]
mod registration;
pub mod registry;
mod server;
mod types;

#[cfg(test)]
mod tests;

pub use cli_app::{CliAppConfig, LOCATION_CONFIG_KEYS, SETTINGS_CONFIG_KEYS};
pub use errors::{ConfigError, ConfigValidationError};
pub use kiln_name::{InvalidKilnName, KilnName};
pub use provider::EffectiveLlmConfig;
#[cfg(feature = "toml")]
pub use registration::{
    register_kiln_entry_in_config, register_kiln_in_config, register_llm_provider_in_config,
    register_project_in_config,
};
pub use server::{LoggingConfig, ScmConfig, ServerConfig, WebConfig};
pub use types::{
    parse_duration_string, plugin_name_from_url, PluginEntry, PluginsConfig, ScheduleEntry,
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

/// Where LuaCATS type stubs live: `~/.config/crucible/luals/`.
///
/// One answer for three callers that used to disagree — the daemon
/// auto-generates here on every start, `cru plugin stubs` defaulted to a
/// sibling `stubs/`, and `cru plugin new` writes the path into a scaffolded
/// `.luarc.json`. A scaffold pointing at a directory nothing writes is the
/// same as pointing nowhere.
pub fn lua_stubs_dir() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("crucible").join("luals"))
}
