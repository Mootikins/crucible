//! Configuration loading utilities for various file formats.

use crate::{Config, ConfigError};
use std::path::{Path, PathBuf};
use tokio::fs;

#[cfg(feature = "toml")]
use crate::includes::{merge_includes, process_file_references, ResolveMode};

/// Configuration loader supporting multiple file formats.
#[derive(Debug)]
pub struct ConfigLoader {
    search_paths: Vec<PathBuf>,
    format: ConfigFormat,
}

impl ConfigLoader {
    /// Create a new configuration loader.
    ///
    /// Uses platform-appropriate directories:
    /// - Linux: `~/.config/crucible/` (XDG Base Directory)
    /// - macOS: `~/Library/Application Support/crucible/`
    /// - Windows: `%APPDATA%\crucible\` (Roaming AppData)
    pub fn new() -> Self {
        // Use platform-appropriate config directory
        let config_dir = if let Ok(override_dir) = std::env::var("CRUCIBLE_CONFIG_DIR") {
            PathBuf::from(override_dir)
        } else if let Some(config_dir) = dirs::config_dir() {
            config_dir.join("crucible")
        } else {
            // Fallback: Use home directory with .config subdirectory
            if let Some(home) = dirs::home_dir() {
                home.join(".config").join("crucible")
            } else {
                PathBuf::from("~/.config/crucible") // Last resort fallback
            }
        };

        Self {
            search_paths: vec![PathBuf::from("./config"), PathBuf::from("./"), config_dir],
            format: ConfigFormat::Auto,
        }
    }

    /// Create a loader with specific search paths.
    pub fn with_search_paths(paths: Vec<PathBuf>) -> Self {
        Self {
            search_paths: paths,
            format: ConfigFormat::Auto,
        }
    }

    /// Set the configuration format.
    pub fn with_format(mut self, format: ConfigFormat) -> Self {
        self.format = format;
        self
    }

    /// Add a search path.
    pub fn add_search_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.search_paths.push(path.as_ref().to_path_buf());
        self
    }

    /// Load configuration from a file.
    ///
    /// This method also processes `[include]` directives to load external
    /// configuration files and merge them into the main config.
    pub async fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Config, ConfigError> {
        let path = path.as_ref();
        let content = fs::read_to_string(path).await?;

        let format = ConfigFormat::from_path(path)?;
        Self::parse_from_string_with_includes(&content, format, path)
    }

    /// Load configuration from a file (synchronous).
    ///
    /// This method also processes `[include]` directives to load external
    /// configuration files and merge them into the main config.
    pub fn load_from_file_sync<P: AsRef<Path>>(path: P) -> Result<Config, ConfigError> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)?;

        let format = ConfigFormat::from_path(path)?;
        Self::parse_from_string_with_includes(&content, format, path)
    }

    /// Load configuration from a string.
    pub fn load_from_str(content: &str, format: ConfigFormat) -> Result<Config, ConfigError> {
        Self::parse_from_string(content, format)
    }

    /// Search for and load configuration from standard locations.
    pub async fn load_from_search_paths(&self, filename: &str) -> Result<Config, ConfigError> {
        // First try exact filename
        for search_path in &self.search_paths {
            let config_path = search_path.join(filename);

            if config_path.exists() {
                tracing::debug!("Loading config from: {}", config_path.display());
                return Self::load_from_file(&config_path).await;
            }
        }

        // Try with different extensions if not specified
        if !filename.contains('.') {
            for search_path in &self.search_paths {
                for extension in ["yaml", "yml", "toml", "json"] {
                    let config_path = search_path.join(format!("{}.{}", filename, extension));

                    if config_path.exists() {
                        tracing::debug!("Loading config from: {}", config_path.display());
                        return Self::load_from_file(&config_path).await;
                    }
                }
            }
        }

        Err(ConfigError::MissingValue {
            field: format!("config file: {}", filename),
        })
    }

    /// Search for and load configuration from standard locations (synchronous).
    pub fn load_from_search_paths_sync(&self, filename: &str) -> Result<Config, ConfigError> {
        // First try exact filename
        for search_path in &self.search_paths {
            let config_path = search_path.join(filename);

            if config_path.exists() {
                tracing::debug!("Loading config from: {}", config_path.display());
                return Self::load_from_file_sync(&config_path);
            }
        }

        // Try with different extensions if not specified
        if !filename.contains('.') {
            for search_path in &self.search_paths {
                for extension in ["yaml", "yml", "toml", "json"] {
                    let config_path = search_path.join(format!("{}.{}", filename, extension));

                    if config_path.exists() {
                        tracing::debug!("Loading config from: {}", config_path.display());
                        return Self::load_from_file_sync(&config_path);
                    }
                }
            }
        }

        Err(ConfigError::MissingValue {
            field: format!("config file: {}", filename),
        })
    }

    /// Parse configuration from a string with specified format.
    fn parse_from_string(content: &str, format: ConfigFormat) -> Result<Config, ConfigError> {
        match format {
            #[cfg(feature = "yaml")]
            ConfigFormat::Yaml => {
                let config: Config = serde_yaml::from_str(content)?;
                Ok(config)
            }
            #[cfg(feature = "toml")]
            ConfigFormat::Toml => {
                let config: Config = toml::from_str(content)?;
                Ok(config)
            }
            ConfigFormat::Json => {
                let config: Config = serde_json::from_str(content)?;
                Ok(config)
            }
            #[cfg(not(feature = "yaml"))]
            ConfigFormat::Yaml => Err(ConfigError::Io(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "YAML support not enabled",
            ))),
            #[cfg(not(feature = "toml"))]
            ConfigFormat::Toml => Err(ConfigError::Io(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "TOML support not enabled",
            ))),
            ConfigFormat::Auto => {
                // Try to detect format and parse
                if let Ok(config) = Self::parse_from_string(content, ConfigFormat::Yaml) {
                    return Ok(config);
                }
                if let Ok(config) = Self::parse_from_string(content, ConfigFormat::Toml) {
                    return Ok(config);
                }
                if let Ok(config) = Self::parse_from_string(content, ConfigFormat::Json) {
                    return Ok(config);
                }
                Err(ConfigError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Unable to parse configuration in any supported format",
                )))
            }
        }
    }

    /// Parse configuration from a string with include processing.
    ///
    /// For TOML format, this processes:
    /// 1. `{file:path}` references anywhere in the config
    /// 2. `[include]` directives to merge external files into sections
    fn parse_from_string_with_includes(
        content: &str,
        format: ConfigFormat,
        config_path: &Path,
    ) -> Result<Config, ConfigError> {
        #[cfg(feature = "toml")]
        if matches!(format, ConfigFormat::Toml | ConfigFormat::Auto) {
            // Try TOML with includes first
            if let Ok(mut toml_value) = toml::from_str::<toml::Value>(content) {
                // Get the directory containing the config file
                let base_dir = config_path.parent().unwrap_or(Path::new("."));

                // Process {file:path} references first (can be anywhere)
                if let Err(errors) = process_file_references(&mut toml_value, base_dir, ResolveMode::BestEffort) {
                    for error in errors {
                        tracing::warn!("File reference error: {}", error);
                    }
                }

                // Then process [include] section (legacy, merges into top-level)
                if let Err(errors) = merge_includes(&mut toml_value, base_dir) {
                    for error in errors {
                        tracing::warn!("Include error: {}", error);
                    }
                }

                // Now parse the merged config
                let config: Config = toml_value
                    .try_into()
                    .map_err(|e: toml::de::Error| ConfigError::Toml(e))?;

                return Ok(config);
            }
        }

        // Fall back to standard parsing (no includes support for YAML/JSON)
        Self::parse_from_string(content, format)
    }

    /// Save configuration to a file.
    pub async fn save_to_file<P: AsRef<Path>>(config: &Config, path: P) -> Result<(), ConfigError> {
        let path = path.as_ref();
        let format = ConfigFormat::from_path(path)?;
        let content = Self::serialize_to_string(config, format)?;

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::write(path, content).await?;
        Ok(())
    }

    /// Save configuration to a file (synchronous).
    pub fn save_to_file_sync<P: AsRef<Path>>(config: &Config, path: P) -> Result<(), ConfigError> {
        let path = path.as_ref();
        let format = ConfigFormat::from_path(path)?;
        let content = Self::serialize_to_string(config, format)?;

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(path, content)?;
        Ok(())
    }

    /// Serialize configuration to a string with specified format.
    fn serialize_to_string(config: &Config, format: ConfigFormat) -> Result<String, ConfigError> {
        match format {
            #[cfg(feature = "yaml")]
            ConfigFormat::Yaml => {
                let content = serde_yaml::to_string(config)?;
                Ok(content)
            }
            #[cfg(feature = "toml")]
            ConfigFormat::Toml => {
                let content = toml::to_string_pretty(config)
                    .map_err(|e| crate::ConfigError::TomlSer(format!("{}", e)))?;
                Ok(content)
            }
            ConfigFormat::Json => {
                let content = serde_json::to_string_pretty(config)?;
                Ok(content)
            }
            #[cfg(not(feature = "yaml"))]
            ConfigFormat::Yaml => Err(ConfigError::Io(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "YAML support not enabled",
            ))),
            #[cfg(not(feature = "toml"))]
            ConfigFormat::Toml => Err(ConfigError::Io(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "TOML support not enabled",
            ))),
            ConfigFormat::Auto => {
                // Default to YAML for auto format
                Self::serialize_to_string(config, ConfigFormat::Yaml)
            }
        }
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Supported configuration file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    /// YAML format.
    Yaml,
    /// TOML format.
    Toml,
    /// JSON format.
    Json,
    /// Auto-detect format from file extension.
    Auto,
}

impl ConfigFormat {
    /// Detect format from file path.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let path = path.as_ref();

        // Handle temporary files without extensions by trying to detect from content
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            match extension.to_lowercase().as_str() {
                "yaml" | "yml" => Ok(Self::Yaml),
                "toml" => Ok(Self::Toml),
                "json" => Ok(Self::Json),
                _ => Err(ConfigError::MissingValue {
                    field: format!("unsupported file extension: {}", extension),
                }),
            }
        } else {
            // For files without extensions (like temporary files), default to YAML
            // The actual format detection will happen during parsing
            Ok(Self::Auto)
        }
    }

    /// Get the file extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Json => "json",
            Self::Auto => "yaml",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // NOTE: Platform-specific paths (macOS/Windows) not tested — Linux CI only.
    // ConfigLoader::new() resolves dirs::config_dir() which is platform-dependent.

    // =========================================================================
    // Constructor and builder tests
    // =========================================================================

    #[test]
    fn new_includes_default_search_paths() {
        let loader = ConfigLoader::new();
        // Should have at least ./config, ./, and a config dir
        assert!(
            loader.search_paths.len() >= 3,
            "expected at least 3 default search paths, got {}",
            loader.search_paths.len()
        );
        assert_eq!(loader.search_paths[0], PathBuf::from("./config"));
        assert_eq!(loader.search_paths[1], PathBuf::from("./"));
        assert_eq!(loader.format, ConfigFormat::Auto);
    }

    #[test]
    fn with_search_paths_replaces_defaults() {
        let custom_paths = vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")];
        let loader = ConfigLoader::with_search_paths(custom_paths.clone());
        assert_eq!(loader.search_paths, custom_paths);
        assert_eq!(loader.format, ConfigFormat::Auto);
    }

    #[test]
    fn add_search_path_appends() {
        let loader =
            ConfigLoader::with_search_paths(vec![PathBuf::from("/base")]).add_search_path("/extra");
        assert_eq!(loader.search_paths.len(), 2);
        assert_eq!(loader.search_paths[1], PathBuf::from("/extra"));
    }

    #[test]
    fn with_format_sets_format() {
        let loader = ConfigLoader::new().with_format(ConfigFormat::Json);
        assert_eq!(loader.format, ConfigFormat::Json);
    }

    #[test]
    fn default_delegates_to_new() {
        let default_loader = ConfigLoader::default();
        let new_loader = ConfigLoader::new();
        assert_eq!(default_loader.search_paths, new_loader.search_paths);
        assert_eq!(default_loader.format, new_loader.format);
    }

    // =========================================================================
    // load_from_str tests
    // =========================================================================

    #[test]
    fn load_from_str_valid_toml() {
        let toml_content = r#"
            [chat]
            show_thinking = true
        "#;
        let result = ConfigLoader::load_from_str(toml_content, ConfigFormat::Toml);
        assert!(
            result.is_ok(),
            "valid TOML should parse: {:?}",
            result.err()
        );
        let config = result.unwrap();
        // Chat section should have been parsed
        assert!(config.chat.is_some());
    }

    #[test]
    fn load_from_str_valid_json() {
        let json_content = r#"{"profile": "test"}"#;
        let result = ConfigLoader::load_from_str(json_content, ConfigFormat::Json);
        assert!(
            result.is_ok(),
            "valid JSON should parse: {:?}",
            result.err()
        );
        let config = result.unwrap();
        assert_eq!(config.profile, Some("test".to_string()));
    }

    #[test]
    fn load_from_str_invalid_toml_returns_err() {
        let bad_toml = "invalid = [[[";
        let result = ConfigLoader::load_from_str(bad_toml, ConfigFormat::Toml);
        assert!(result.is_err(), "invalid TOML must return Err, not panic");
    }

    #[test]
    fn load_from_str_invalid_json_returns_err() {
        let bad_json = "{not json at all";
        let result = ConfigLoader::load_from_str(bad_json, ConfigFormat::Json);
        assert!(result.is_err(), "invalid JSON must return Err, not panic");
    }

    #[test]
    fn load_from_str_auto_detects_toml() {
        let toml_content = r#"
            profile = "auto-detected"
        "#;
        let result = ConfigLoader::load_from_str(toml_content, ConfigFormat::Auto);
        assert!(
            result.is_ok(),
            "Auto format should detect TOML: {:?}",
            result.err()
        );
        let config = result.unwrap();
        assert_eq!(config.profile, Some("auto-detected".to_string()));
    }

    #[test]
    fn load_from_str_auto_unparseable_returns_err() {
        let garbage = "<<<not any known format>>>";
        let result = ConfigLoader::load_from_str(garbage, ConfigFormat::Auto);
        assert!(
            result.is_err(),
            "completely unparseable input must return Err"
        );
    }

    // =========================================================================
    // load_from_file_sync / save_to_file_sync tests
    // =========================================================================

    #[test]
    fn load_from_file_sync_missing_file_returns_err() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does_not_exist.toml");
        let result = ConfigLoader::load_from_file_sync(&missing);
        assert!(result.is_err(), "missing file must return Err, not panic");
    }

    #[test]
    fn load_from_file_sync_valid_toml() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("test.toml");
        std::fs::write(&config_path, r#"profile = "from-file""#).unwrap();

        let result = ConfigLoader::load_from_file_sync(&config_path);
        assert!(
            result.is_ok(),
            "valid TOML file should load: {:?}",
            result.err()
        );
        let config = result.unwrap();
        assert_eq!(config.profile, Some("from-file".to_string()));
    }

    #[test]
    fn load_from_file_sync_invalid_extension_returns_err() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.xml");
        std::fs::write(&config_path, "<xml/>").unwrap();

        let result = ConfigLoader::load_from_file_sync(&config_path);
        assert!(result.is_err(), "unsupported extension must return Err");
    }

    #[test]
    fn save_and_load_roundtrip_toml() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("roundtrip.toml");

        // Create a config with a known value
        let original_toml = r#"profile = "roundtrip-test""#;
        let original = ConfigLoader::load_from_str(original_toml, ConfigFormat::Toml).unwrap();

        // Save it
        ConfigLoader::save_to_file_sync(&original, &config_path).unwrap();

        // Load it back
        let loaded = ConfigLoader::load_from_file_sync(&config_path).unwrap();
        assert_eq!(loaded.profile, original.profile);
    }

    #[test]
    fn save_to_file_sync_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let nested_path = tmp.path().join("a").join("b").join("c").join("config.toml");

        let config =
            ConfigLoader::load_from_str("profile = \"nested\"", ConfigFormat::Toml).unwrap();
        let result = ConfigLoader::save_to_file_sync(&config, &nested_path);
        assert!(
            result.is_ok(),
            "should create parent dirs: {:?}",
            result.err()
        );
        assert!(nested_path.exists());
    }

    #[test]
    fn save_and_load_roundtrip_json() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("roundtrip.json");

        let original =
            ConfigLoader::load_from_str(r#"{"profile": "json-rt"}"#, ConfigFormat::Json).unwrap();
        ConfigLoader::save_to_file_sync(&original, &config_path).unwrap();

        let loaded = ConfigLoader::load_from_file_sync(&config_path).unwrap();
        assert_eq!(loaded.profile, Some("json-rt".to_string()));
    }

    // =========================================================================
    // ConfigFormat tests
    // =========================================================================

    #[test]
    fn config_format_from_path_known_extensions() {
        assert_eq!(
            ConfigFormat::from_path("config.toml").unwrap(),
            ConfigFormat::Toml
        );
        assert_eq!(
            ConfigFormat::from_path("config.yaml").unwrap(),
            ConfigFormat::Yaml
        );
        assert_eq!(
            ConfigFormat::from_path("config.yml").unwrap(),
            ConfigFormat::Yaml
        );
        assert_eq!(
            ConfigFormat::from_path("config.json").unwrap(),
            ConfigFormat::Json
        );
    }

    #[test]
    fn config_format_from_path_unsupported_extension_returns_err() {
        let result = ConfigFormat::from_path("config.xml");
        assert!(result.is_err());
    }

    #[test]
    fn config_format_from_path_no_extension_returns_auto() {
        let result = ConfigFormat::from_path("config");
        assert_eq!(result.unwrap(), ConfigFormat::Auto);
    }

    #[test]
    fn config_format_extension_returns_correct_strings() {
        assert_eq!(ConfigFormat::Toml.extension(), "toml");
        assert_eq!(ConfigFormat::Yaml.extension(), "yaml");
        assert_eq!(ConfigFormat::Json.extension(), "json");
        assert_eq!(ConfigFormat::Auto.extension(), "yaml");
    }

    // =========================================================================
    // search_paths_sync tests
    // =========================================================================

    #[test]
    fn load_from_search_paths_sync_finds_file() {
        let tmp = TempDir::new().unwrap();
        let config_dir = tmp.path().join("config");
        std::fs::create_dir(&config_dir).unwrap();
        std::fs::write(config_dir.join("crucible.toml"), r#"profile = "found-it""#).unwrap();

        let loader = ConfigLoader::with_search_paths(vec![config_dir]);
        let result = loader.load_from_search_paths_sync("crucible.toml");
        assert!(
            result.is_ok(),
            "should find config in search path: {:?}",
            result.err()
        );
        assert_eq!(result.unwrap().profile, Some("found-it".to_string()));
    }

    #[test]
    fn load_from_search_paths_sync_tries_extensions() {
        let tmp = TempDir::new().unwrap();
        // Place a .toml file but search without extension
        std::fs::write(tmp.path().join("crucible.toml"), r#"profile = "auto-ext""#).unwrap();

        let loader = ConfigLoader::with_search_paths(vec![tmp.path().to_path_buf()]);
        let result = loader.load_from_search_paths_sync("crucible");
        assert!(
            result.is_ok(),
            "should auto-discover .toml extension: {:?}",
            result.err()
        );
        assert_eq!(result.unwrap().profile, Some("auto-ext".to_string()));
    }

    #[test]
    fn load_from_search_paths_sync_not_found_returns_err() {
        let tmp = TempDir::new().unwrap();
        let loader = ConfigLoader::with_search_paths(vec![tmp.path().to_path_buf()]);
        let result = loader.load_from_search_paths_sync("nonexistent");
        assert!(result.is_err(), "missing config must return Err");
    }
}
