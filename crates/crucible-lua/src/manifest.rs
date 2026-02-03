//! Plugin manifest parsing and validation
//!
//! Plugins declare metadata, dependencies, and capabilities in a `plugin.yaml` manifest.
//!
//! ## Example Manifest
//!
//! ```yaml
//! name: my-plugin
//! version: "1.0.0"
//! description: A sample plugin
//! author: Your Name
//!
//! main: lua/init.lua
//!
//! capabilities:
//!   - filesystem
//!   - shell
//!
//! dependencies:
//!   - name: other-plugin
//!     version: ">=1.0"
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("Failed to read manifest: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to parse YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Validation failed: {0}")]
    Validation(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid version format: {0}")]
    InvalidVersion(String),
}

pub type ManifestResult<T> = Result<T, ManifestError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,

    #[serde(default)]
    pub description: String,

    #[serde(default)]
    pub author: String,

    #[serde(default)]
    pub license: Option<String>,

    #[serde(default)]
    pub homepage: Option<String>,

    #[serde(default)]
    pub repository: Option<String>,

    #[serde(default = "default_main")]
    pub main: String,

    #[serde(default)]
    pub init: Option<String>,

    #[serde(default)]
    pub capabilities: Vec<Capability>,

    #[serde(default)]
    pub dependencies: Vec<PluginDependency>,

    #[serde(default)]
    pub exports: ExportDeclarations,

    #[serde(default)]
    pub config: Option<ConfigSchema>,

    #[serde(default)]
    pub keywords: Vec<String>,

    #[serde(default)]
    pub enabled: Option<bool>,
}

fn default_main() -> String {
    "init.lua".to_string()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Filesystem,
    Network,
    Shell,
    Vault,
    Agent,
    Ui,
    Config,
    System,
    WebSocket,
}

impl Capability {
    pub fn all() -> Vec<Self> {
        vec![
            Self::Filesystem,
            Self::Network,
            Self::Shell,
            Self::Vault,
            Self::Agent,
            Self::Ui,
            Self::Config,
            Self::System,
            Self::WebSocket,
        ]
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Filesystem => "Read/write files outside the kiln",
            Self::Network => "Make HTTP requests",
            Self::Shell => "Execute shell commands",
            Self::Vault => "Access the knowledge vault",
            Self::Agent => "Interact with AI agents",
            Self::Ui => "Create custom UI views",
            Self::Config => "Access user configuration",
            Self::System => "Access system information",
            Self::WebSocket => "Establish persistent WebSocket connections",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginDependency {
    pub name: String,

    #[serde(default)]
    pub version: Option<String>,

    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExportDeclarations {
    #[serde(default)]
    pub tools: Vec<String>,

    #[serde(default)]
    pub commands: Vec<String>,

    #[serde(default)]
    pub views: Vec<String>,

    #[serde(default)]
    pub handlers: Vec<String>,

    #[serde(default)]
    pub auto_discover: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigSchema {
    #[serde(default)]
    pub properties: HashMap<String, ConfigProperty>,

    #[serde(default)]
    pub required: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigProperty {
    #[serde(rename = "type")]
    pub prop_type: ConfigType,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub default: Option<serde_yaml::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigType {
    String,
    Number,
    Boolean,
    Array,
    Object,
}

impl PluginManifest {
    pub fn from_yaml(yaml: &str) -> ManifestResult<Self> {
        let manifest: Self = serde_yaml::from_str(yaml)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn from_file(path: &Path) -> ManifestResult<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_yaml(&content)
    }

    pub fn discover(plugin_dir: &Path) -> ManifestResult<Option<Self>> {
        let candidates = ["plugin.yaml", "plugin.yml", "manifest.yaml", "manifest.yml"];

        for name in candidates {
            let path = plugin_dir.join(name);
            if path.exists() {
                return Self::from_file(&path).map(Some);
            }
        }

        Ok(None)
    }

    /// Create a default manifest from a directory path (no plugin.yaml required).
    ///
    /// Uses the directory stem as the plugin name with version "0.0.0".
    /// Sets `auto_discover: true` so Lua files are scanned for exports.
    pub fn from_directory_defaults(dir: &Path) -> ManifestResult<Self> {
        let name = dir
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| ManifestError::Validation("Cannot derive name from directory".into()))?
            .to_string();

        if !is_valid_plugin_name(&name) {
            return Err(ManifestError::Validation(format!(
                "Directory name '{}' is not a valid plugin name",
                name
            )));
        }

        Ok(Self {
            name,
            version: "0.0.0".to_string(),
            description: String::new(),
            author: String::new(),
            license: None,
            homepage: None,
            repository: None,
            main: "init.lua".to_string(),
            init: None,
            capabilities: Vec::new(),
            dependencies: Vec::new(),
            exports: ExportDeclarations {
                auto_discover: true,
                ..Default::default()
            },
            config: None,
            keywords: Vec::new(),
            enabled: None,
        })
    }

    pub fn validate(&self) -> ManifestResult<()> {
        if self.name.is_empty() {
            return Err(ManifestError::MissingField("name".to_string()));
        }

        if self.version.is_empty() {
            return Err(ManifestError::MissingField("version".to_string()));
        }

        if !is_valid_plugin_name(&self.name) {
            return Err(ManifestError::Validation(format!(
                "Invalid plugin name '{}': must be lowercase alphanumeric with hyphens",
                self.name
            )));
        }

        if !is_valid_version(&self.version) {
            return Err(ManifestError::InvalidVersion(self.version.clone()));
        }

        Ok(())
    }

    pub fn main_path(&self, plugin_dir: &Path) -> PathBuf {
        plugin_dir.join(&self.main)
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }

    pub fn has_capability(&self, cap: Capability) -> bool {
        self.capabilities.contains(&cap)
    }

    pub fn required_dependencies(&self) -> impl Iterator<Item = &PluginDependency> {
        self.dependencies.iter().filter(|d| !d.optional)
    }
}

fn is_valid_plugin_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }

    let mut chars = name.chars().peekable();

    if !chars.peek().is_some_and(|c| c.is_ascii_lowercase()) {
        return false;
    }

    for c in chars {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' && c != '_' {
            return false;
        }
    }

    !name.ends_with('-') && !name.ends_with('_')
}

fn is_valid_version(version: &str) -> bool {
    if version.is_empty() {
        return false;
    }

    let parts: Vec<&str> = version.split('.').collect();

    if parts.is_empty() || parts.len() > 4 {
        return false;
    }

    for (i, part) in parts.iter().enumerate() {
        if i < parts.len() - 1 {
            if part.parse::<u32>().is_err() {
                return false;
            }
        } else if part.parse::<u32>().is_err()
            && !part.chars().all(|c| c.is_alphanumeric() || c == '-')
        {
            return false;
        }
    }

    true
}

#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub dir: PathBuf,
    pub state: PluginState,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
    Discovered,
    Loaded,
    Active,
    Disabled,
    Error,
}

impl std::fmt::Display for PluginState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Discovered => f.write_str("Discovered"),
            Self::Loaded => f.write_str("Loaded"),
            Self::Active => f.write_str("Active"),
            Self::Disabled => f.write_str("Disabled"),
            Self::Error => f.write_str("Error"),
        }
    }
}

impl LoadedPlugin {
    pub fn new(manifest: PluginManifest, dir: PathBuf) -> Self {
        Self {
            manifest,
            dir,
            state: PluginState::Discovered,
            last_error: None,
        }
    }

    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    pub fn version(&self) -> &str {
        &self.manifest.version
    }

    pub fn main_path(&self) -> PathBuf {
        self.manifest.main_path(&self.dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_manifest() {
        let yaml = r#"
name: my-plugin
version: "1.0.0"
"#;
        let manifest = PluginManifest::from_yaml(yaml).unwrap();
        assert_eq!(manifest.name, "my-plugin");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.main, "init.lua");
        assert!(manifest.capabilities.is_empty());
    }

    #[test]
    fn test_parse_full_manifest() {
        let yaml = r#"
name: my-plugin
version: "1.0.0"
description: A sample plugin
author: Test Author
license: MIT
homepage: https://example.com
main: lua/init.lua
init: setup

capabilities:
  - filesystem
  - shell
  - vault

dependencies:
  - name: other-plugin
    version: ">=1.0"
  - name: optional-dep
    optional: true

exports:
  tools:
    - search
    - create
  commands:
    - /my-command
  auto_discover: true

keywords:
  - productivity
  - notes
"#;
        let manifest = PluginManifest::from_yaml(yaml).unwrap();
        assert_eq!(manifest.name, "my-plugin");
        assert_eq!(manifest.description, "A sample plugin");
        assert_eq!(manifest.author, "Test Author");
        assert_eq!(manifest.license, Some("MIT".to_string()));
        assert_eq!(manifest.main, "lua/init.lua");
        assert_eq!(manifest.init, Some("setup".to_string()));
        assert_eq!(manifest.capabilities.len(), 3);
        assert!(manifest.has_capability(Capability::Filesystem));
        assert!(manifest.has_capability(Capability::Shell));
        assert!(manifest.has_capability(Capability::Vault));
        assert!(!manifest.has_capability(Capability::Network));
        assert_eq!(manifest.dependencies.len(), 2);
        assert_eq!(manifest.required_dependencies().count(), 1);
        assert_eq!(manifest.exports.tools.len(), 2);
        assert!(manifest.exports.auto_discover);
    }

    #[test]
    fn test_validate_missing_name() {
        let yaml = r#"
version: "1.0.0"
"#;
        let result = PluginManifest::from_yaml(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_missing_version() {
        let yaml = r#"
name: my-plugin
"#;
        let result = PluginManifest::from_yaml(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_invalid_name() {
        let yaml = r#"
name: My Plugin!
version: "1.0.0"
"#;
        let result = PluginManifest::from_yaml(yaml);
        assert!(matches!(result, Err(ManifestError::Validation(_))));
    }

    #[test]
    fn test_valid_plugin_names() {
        assert!(is_valid_plugin_name("my-plugin"));
        assert!(is_valid_plugin_name("plugin123"));
        assert!(is_valid_plugin_name("a"));
        assert!(is_valid_plugin_name("my_plugin"));
        assert!(is_valid_plugin_name("my-plugin-v2"));
    }

    #[test]
    fn test_invalid_plugin_names() {
        assert!(!is_valid_plugin_name(""));
        assert!(!is_valid_plugin_name("My-Plugin"));
        assert!(!is_valid_plugin_name("my plugin"));
        assert!(!is_valid_plugin_name("-plugin"));
        assert!(!is_valid_plugin_name("plugin-"));
        assert!(!is_valid_plugin_name("123plugin"));
        assert!(!is_valid_plugin_name("my.plugin"));
    }

    #[test]
    fn test_valid_versions() {
        assert!(is_valid_version("1"));
        assert!(is_valid_version("1.0"));
        assert!(is_valid_version("1.0.0"));
        assert!(is_valid_version("1.0.0-beta"));
        assert!(is_valid_version("0.1.0"));
        assert!(is_valid_version("10.20.30"));
    }

    #[test]
    fn test_invalid_versions() {
        assert!(!is_valid_version(""));
        assert!(!is_valid_version("v1.0.0"));
        assert!(!is_valid_version("1.0.0.0.0"));
        assert!(!is_valid_version("a.b.c"));
    }

    #[test]
    fn test_loaded_plugin() {
        let yaml = r#"
name: test-plugin
version: "1.0.0"
main: lua/main.lua
"#;
        let manifest = PluginManifest::from_yaml(yaml).unwrap();
        let plugin = LoadedPlugin::new(manifest, PathBuf::from("/plugins/test"));

        assert_eq!(plugin.name(), "test-plugin");
        assert_eq!(plugin.version(), "1.0.0");
        assert_eq!(
            plugin.main_path(),
            PathBuf::from("/plugins/test/lua/main.lua")
        );
        assert_eq!(plugin.state, PluginState::Discovered);
    }

    #[test]
    fn test_config_schema() {
        let yaml = r#"
name: config-plugin
version: "1.0.0"

config:
  properties:
    api_key:
      type: string
      description: API key for service
    max_results:
      type: number
      default: 10
    enabled:
      type: boolean
      default: true
  required:
    - api_key
"#;
        let manifest = PluginManifest::from_yaml(yaml).unwrap();
        let config = manifest.config.unwrap();
        assert_eq!(config.properties.len(), 3);
        assert_eq!(config.required, vec!["api_key"]);
        assert_eq!(config.properties["api_key"].prop_type, ConfigType::String);
        assert_eq!(
            config.properties["max_results"].prop_type,
            ConfigType::Number
        );
    }

    #[test]
    fn test_manifest_enabled_default() {
        let yaml = r#"
name: test
version: "1.0.0"
"#;
        let manifest = PluginManifest::from_yaml(yaml).unwrap();
        assert!(manifest.is_enabled());
    }

    #[test]
    fn test_manifest_explicitly_disabled() {
        let yaml = r#"
name: test
version: "1.0.0"
enabled: false
"#;
        let manifest = PluginManifest::from_yaml(yaml).unwrap();
        assert!(!manifest.is_enabled());
    }

    #[test]
    fn test_from_directory_defaults() {
        let manifest =
            PluginManifest::from_directory_defaults(Path::new("/plugins/my-plugin")).unwrap();
        assert_eq!(manifest.name, "my-plugin");
        assert_eq!(manifest.version, "0.0.0");
        assert_eq!(manifest.main, "init.lua");
        assert!(manifest.exports.auto_discover);
        assert!(manifest.capabilities.is_empty());
        assert!(manifest.dependencies.is_empty());
    }

    #[test]
    fn test_from_directory_defaults_invalid_name() {
        let result = PluginManifest::from_directory_defaults(Path::new("/plugins/My Plugin!"));
        assert!(result.is_err());
    }
}
