//! Global configuration types
//!
//! Defines the `~/.config/crucible/config.toml` format for user-level configuration.
//! Provides defaults that apply to all workspaces unless overridden.

use crate::{includes::IncludeConfig, workspace::SecurityConfig, ConfigError};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

/// Global user configuration
///
/// Loaded from `~/.config/crucible/config.toml` (or equivalent platform config dir).
/// Provides user-level defaults for all workspaces.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct GlobalConfig {
    /// Include/exclude patterns for file processing
    pub include: Option<IncludeConfig>,
    /// Security configuration defaults
    pub security: SecurityConfig,
}

impl GlobalConfig {
    /// Load global configuration from default location
    ///
    /// Returns default config if file doesn't exist.
    /// Fails if file exists but cannot be parsed.
    ///
    /// # Platform paths
    ///
    /// - Linux: `~/.config/crucible/config.toml`
    /// - macOS: `~/Library/Application Support/crucible/config.toml`
    /// - Windows: `%APPDATA%\crucible\config.toml`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crucible_config::GlobalConfig;
    ///
    /// let config = GlobalConfig::load()?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::default_path()?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(&path).map_err(|e| ConfigError::InvalidValue {
            field: format!("global config at {}", path.display()),
            value: format!("Failed to read: {}", e),
        })?;

        toml::from_str(&contents).map_err(|e| ConfigError::InvalidValue {
            field: format!("global config at {}", path.display()),
            value: format!("Failed to parse: {}", e),
        })
    }

    /// Get default config file path for current platform
    fn default_path() -> Result<PathBuf, ConfigError> {
        let config_dir = dirs::config_dir().ok_or_else(|| ConfigError::MissingValue {
            field: "config directory".to_string(),
        })?;

        Ok(config_dir.join("crucible").join("config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ShellPolicy;

    #[test]
    fn global_config_parses_from_toml() {
        let toml = r#"
[security.shell]
whitelist = ["git", "cargo", "just"]
blacklist = ["sudo", "rm -rf"]

[include]
gateway = "mcps.toml"
embedding = "secrets/api.toml"
"#;

        let config: GlobalConfig = toml::from_str(toml).expect("Failed to parse");

        // Verify security config
        assert_eq!(config.security.shell.whitelist.len(), 3);
        assert!(config.security.shell.whitelist.contains(&"git".to_string()));
        assert!(config
            .security
            .shell
            .whitelist
            .contains(&"cargo".to_string()));
        assert!(config
            .security
            .shell
            .whitelist
            .contains(&"just".to_string()));

        assert_eq!(config.security.shell.blacklist.len(), 2);
        assert!(config
            .security
            .shell
            .blacklist
            .contains(&"sudo".to_string()));
        assert!(config
            .security
            .shell
            .blacklist
            .contains(&"rm -rf".to_string()));

        // Verify include config
        let include = config.include.expect("Include config missing");
        assert_eq!(include.gateway, Some("mcps.toml".to_string()));
        assert_eq!(include.embedding, Some("secrets/api.toml".to_string()));
    }

    #[test]
    fn global_config_parses_minimal() {
        let toml = "";

        let config: GlobalConfig = toml::from_str(toml).expect("Failed to parse");

        assert_eq!(config.security.shell.whitelist.len(), 0);
        assert_eq!(config.security.shell.blacklist.len(), 0);
        assert!(config.include.is_none());
    }

    #[test]
    fn global_config_load_returns_default_when_missing() {
        // Since we can't override dirs::config_dir(), we test the default() behavior
        let config = GlobalConfig::default();

        assert_eq!(config.security.shell.whitelist.len(), 0);
        assert_eq!(config.security.shell.blacklist.len(), 0);
        assert!(config.include.is_none());
    }

    #[test]
    fn global_config_serializes_to_toml() {
        let config = GlobalConfig {
            security: SecurityConfig {
                shell: ShellPolicy {
                    whitelist: vec!["git".to_string(), "cargo".to_string()],
                    blacklist: vec!["sudo".to_string()],
                },
            },
            include: None,
        };

        let toml = toml::to_string(&config).expect("Failed to serialize");
        let parsed: GlobalConfig = toml::from_str(&toml).expect("Failed to re-parse");

        assert_eq!(config, parsed);
    }

    #[test]
    fn global_config_with_include_patterns() {
        let toml = r#"
[include]
gateway = "mcps.toml"
enrichment = "enrichment.toml"

[security.shell]
whitelist = ["git"]
"#;

        let config: GlobalConfig = toml::from_str(toml).expect("Failed to parse");

        let include = config.include.expect("Include config missing");
        assert_eq!(include.gateway, Some("mcps.toml".to_string()));
        assert_eq!(include.enrichment, Some("enrichment.toml".to_string()));
    }

    #[test]
    fn global_config_default_path_is_reasonable() {
        // Just verify it doesn't panic and returns something
        let path = GlobalConfig::default_path();
        assert!(path.is_ok());

        let path = path.unwrap();
        assert!(path.to_string_lossy().contains("crucible"));
        assert!(path.to_string_lossy().contains("config.toml"));
    }
}
