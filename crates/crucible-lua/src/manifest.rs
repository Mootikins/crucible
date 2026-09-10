//! Plugin manifest parsing and validation
//!
//! Plugins declare metadata, dependencies, and capabilities in a `plugin.yaml`
//! manifest.
//!
//! **A capability is a declaration, not a sandbox.** [`PluginManifest::grants`]
//! is stamped into the VM's plugin context at load
//! ([`crate::plugin_context`]) so the running plugin's declaration is always
//! readable, and `intercept_tools` is enforced from it at the tool-call seam.
//! The rest state what the plugin touches. Restricting the `cru.*` API by them
//! is out of scope on purpose: a plugin is code the operator installed, and it
//! gets the API the way an editor plugin gets the editor.
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

    #[serde(default = "default_main")]
    pub main: String,

    #[serde(default)]
    pub capabilities: Vec<Capability>,

    #[serde(default)]
    pub dependencies: Vec<PluginDependency>,

    #[serde(default)]
    pub enabled: Option<bool>,
}

fn default_main() -> String {
    "init.lua".to_string()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Filesystem,
    Network,
    Shell,
    #[serde(alias = "vault")]
    Kiln,
    Agent,
    Ui,
    Config,
    System,
    #[serde(rename = "websocket", alias = "web_socket")]
    WebSocket,
    /// Take over a tool call: return `{ handled = true, result = … }` to
    /// replace execution, or rewrite its arguments before dispatch.
    ///
    /// Separate from [`Self::Agent`] because it is not observation. A handler
    /// returning `handled` returns BEFORE the permission gate
    /// (`agent_manager/messaging/tool_call.rs`), so without this an ordinary
    /// plugin held the power the container sandbox needs — the sandbox is the
    /// one legitimate holder, since taking the call over *is* the sandbox.
    /// `cancel` needs no capability: refusing a call can only narrow.
    #[serde(rename = "intercept_tools", alias = "intercept-tools")]
    InterceptTools,
}

/// What one plugin's installation declared.
///
/// A set rather than a `Vec<Capability>`, because the only question asked of
/// it is "does this hold X". An ABSENT set and an EMPTY one mean different
/// things: absent means no plugin is running at all, which
/// [`crate::plugin_context`] states by having no context, not by an empty set.
///
/// Only `intercept_tools` is READ as authority. The others are what the
/// manifest says the plugin touches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilitySet(std::collections::BTreeSet<Capability>);

impl CapabilitySet {
    /// A plugin that declared nothing.
    pub fn none() -> Self {
        Self::default()
    }

    /// Whether the grant is held.
    pub fn holds(&self, cap: Capability) -> bool {
        self.0.contains(&cap)
    }

    /// The same grants without `cap`.
    ///
    /// Two callers: a plugin COMMAND and a plugin TOOL run under their
    /// plugin's grants minus `intercept_tools`, because neither is a
    /// tool-call hook and neither has interception to do. See
    /// `plugin_tools.rs`.
    pub fn without(&self, cap: Capability) -> Self {
        let mut narrowed = self.clone();
        narrowed.0.remove(&cap);
        narrowed
    }
}

impl FromIterator<Capability> for CapabilitySet {
    fn from_iter<I: IntoIterator<Item = Capability>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl PluginManifest {
    /// What this installation declared.
    pub fn grants(&self) -> CapabilitySet {
        self.capabilities.iter().copied().collect()
    }
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
            // The file that is really there, preferred extension first. A
            // manifest-less plugin has no `main` field to read, so guessing one
            // name meant a `init.luau` plugin resolved to a path that does not
            // exist.
            main: crate::source_files::init_file(dir)
                .ok()
                .flatten()
                .and_then(|path| {
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.to_string())
                })
                .unwrap_or_else(|| "init.lua".to_string()),
            capabilities: Vec::new(),
            dependencies: Vec::new(),
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
main: lua/init.lua
init: setup

capabilities:
  - filesystem
  - shell
  - kiln

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
        assert_eq!(manifest.main, "lua/init.lua");
        assert_eq!(manifest.capabilities.len(), 3);
        assert!(manifest.has_capability(Capability::Filesystem));
        assert!(manifest.has_capability(Capability::Shell));
        assert!(manifest.has_capability(Capability::Kiln));
        assert!(!manifest.has_capability(Capability::Network));
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
        assert!(manifest.capabilities.is_empty());
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
capabilities:
  - vault
  - ui
"#;
        let manifest = PluginManifest::from_yaml(yaml).unwrap();
        assert!(manifest.has_capability(Capability::Kiln));
        assert!(manifest.has_capability(Capability::Ui));
    }
}
