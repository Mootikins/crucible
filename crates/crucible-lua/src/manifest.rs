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
//! ```

use serde::{Deserialize, Serialize};
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

    /// Whether this plugin takes tool calls over.
    ///
    /// The one declaration the host checks. NOT a sandboxing claim — plugin
    /// Lua runs in the daemon VM with `io` and `os`, so a declaration checked
    /// by the host is advisory against a non-adversarial author, and
    /// installation is the real boundary. It is a COMPOSITION claim: a handler
    /// returning `handled = true` takes another component's tool call and
    /// returns BEFORE the permission gate. A plugin that does that by accident
    /// should be refused; one that means it should say so.
    ///
    /// It replaced a ten-name `Capability` enum in which nine names had no
    /// call site outside the parser's own tests, and could not have had one:
    /// `lifecycle/mod.rs` installs `register_stdlib_compat` unconditionally,
    /// so every plugin holds `io` and `os.remove` whether or not it declared
    /// `filesystem`.
    #[serde(default, rename = "intercept_tools", alias = "intercept-tools")]
    pub intercepts_tools: bool,

    #[serde(default)]
    pub dependencies: Vec<PluginDependency>,

    #[serde(default)]
    pub enabled: Option<bool>,

    /// True when no `plugin.yaml` was found and this manifest was synthesized
    /// from the directory.
    ///
    /// It decides whether the Lua spec's `name`, `version` and `description`
    /// override it: a manifest the author actually wrote is the more specific
    /// statement and wins; a synthesized one is a placeholder and yields.
    ///
    /// This used to be inferred from `version == "0.0.0"`, which is the
    /// synthesized default — so the spec's NAME was taken only when the
    /// VERSION happened to still be the placeholder, and a real manifest
    /// pinned at `0.0.0` would have had its name silently replaced.
    #[serde(skip)]
    pub synthesized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginDependency {
    pub name: String,

    #[serde(default)]
    pub optional: bool,
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
            intercepts_tools: false,
            dependencies: Vec::new(),
            enabled: None,
            synthesized: true,
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

    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
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

/// Where a plugin was discovered from, ordered by priority (highest first).
///
/// There is no `Kiln` variant. The daemon's `daemon_plugin_paths` emits only
/// these three, and `PluginManager::with_standard_paths` reads the first two
/// from `crucible_core::paths` (`env_plugin_paths`, `user_plugins_dir`), so
/// the two path lists share one definition. Plugins are user-scoped; a kiln's
/// tree is opted into via `runtimepath`, which makes it `Runtime` like any
/// other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginSource {
    /// `CRUCIBLE_PLUGIN_PATH` env var
    EnvPath,
    /// `~/.config/crucible/plugins/`
    User,
    /// A `runtimepath` entry's `plugins/`, `$CRUCIBLE_RUNTIME/plugins/`, or the
    /// exe-relative runtime path
    Runtime,
}

impl std::fmt::Display for PluginSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EnvPath => f.write_str("path"),
            Self::User => f.write_str("user"),
            Self::Runtime => f.write_str("runtime"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub dir: PathBuf,
    pub state: PluginState,
    pub last_error: Option<String>,
    pub source: PluginSource,
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
            source: PluginSource::User,
        }
    }

    pub fn with_source(manifest: PluginManifest, dir: PathBuf, source: PluginSource) -> Self {
        Self {
            manifest,
            dir,
            state: PluginState::Discovered,
            last_error: None,
            source,
        }
    }

    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    pub fn version(&self) -> &str {
        &self.manifest.version
    }

    /// The plugin's entry file: `init.luau`, else `init.lua`.
    ///
    /// The manifest used to name it in a `main:` field, defaulting to
    /// `init.lua`. That field could name a file that was not there, and did:
    /// the shipped `crucible-help` said `main: init.lua` beside an
    /// `init.luau` and silently failed to load, because the sweep that
    /// renamed the other eleven walked `runtime/plugins/` and it sat outside.
    /// A field that can name the wrong file eventually does, so there is no
    /// field.
    ///
    /// Falls back to the preferred name when neither exists, so the loader's
    /// "Main file not found" error names something a user can create. A
    /// directory holding BOTH spellings is refused at discovery, not here.
    pub fn main_path(&self) -> PathBuf {
        crate::source_files::init_file(&self.dir)
            .ok()
            .flatten()
            .unwrap_or_else(|| {
                self.dir
                    .join(format!("init.{}", crate::source_files::PREFERRED_EXTENSION))
            })
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
        assert!(!manifest.intercepts_tools);
    }

    #[test]
    fn test_parse_full_manifest() {
        let yaml = r#"
name: my-plugin
version: "1.0.0"
description: A sample plugin
author: Test Author
license: MIT
main: lua/init.lua
init: setup

intercept_tools: true

dependencies:
  - name: other-plugin
  - name: optional-dep
    optional: true

exports:
  tools:
    - search
    - create
  commands:
    - /my-command
  auto_discover: true
"#;
        let manifest = PluginManifest::from_yaml(yaml).unwrap();
        assert_eq!(manifest.name, "my-plugin");
        assert_eq!(manifest.description, "A sample plugin");
        assert_eq!(manifest.author, "Test Author");
        assert_eq!(manifest.license, Some("MIT".to_string()));
        assert!(manifest.intercepts_tools);
        assert_eq!(manifest.dependencies.len(), 2);
        assert_eq!(manifest.required_dependencies().count(), 1);
    }

    #[test]
    fn exports_block_is_ignored_for_backward_compat() {
        // `exports` (tools/commands/views/handlers/auto_discover) was parsed
        // but never consumed, so the field was deleted. Existing plugin.yaml
        // files that still declare it must keep parsing (no
        // deny_unknown_fields here).
        let yaml = r#"
name: my-plugin
version: "1.0.0"
exports:
  tools:
    - search
  auto_discover: true
"#;
        let manifest = PluginManifest::from_yaml(yaml).unwrap();
        assert_eq!(manifest.name, "my-plugin");
    }

    #[test]
    fn dependency_version_field_is_ignored_for_backward_compat() {
        // The dependency-level `version` constraint was parsed but never
        // compared, so the field was deleted. Existing plugin.yaml files that
        // still set it must keep parsing (no deny_unknown_fields here).
        let yaml = r#"
name: my-plugin
version: "1.0.0"
dependencies:
  - name: other-plugin
    version: ">=1.0"
"#;
        let manifest = PluginManifest::from_yaml(yaml).unwrap();
        assert_eq!(
            manifest.dependencies,
            vec![PluginDependency {
                name: "other-plugin".to_string(),
                optional: false,
            }]
        );
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
"#;
        let manifest = PluginManifest::from_yaml(yaml).unwrap();
        let plugin = LoadedPlugin::new(manifest, PathBuf::from("/plugins/test"));

        assert_eq!(plugin.name(), "test-plugin");
        assert_eq!(plugin.version(), "1.0.0");
        // No entry file on disk at that path, so the resolver falls back to
        // the preferred name — which is what the loader's "not found" error
        // should name.
        assert_eq!(plugin.main_path(), PathBuf::from("/plugins/test/init.luau"));
        assert_eq!(plugin.state, PluginState::Discovered);
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
        assert!(!manifest.intercepts_tools);
        assert!(manifest.dependencies.is_empty());
    }

    #[test]
    fn test_from_directory_defaults_invalid_name() {
        let result = PluginManifest::from_directory_defaults(Path::new("/plugins/My Plugin!"));
        assert!(result.is_err());
    }

    #[test]
    fn test_vault_capability_alias() {
        let yaml = r#"
name: hermit
version: "0.1.0"
intercept_tools: true
"#;
        let manifest = PluginManifest::from_yaml(yaml).unwrap();
        assert!(manifest.intercepts_tools);
    }
}
