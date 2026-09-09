//! Plugin manifest: what the host knows about a plugin before it runs it.
//!
//! There is no manifest FILE. A plugin is a directory that holds an entry
//! file (`init.luau`, else `init.lua`), and `plugin.yaml` is gone. The host
//! synthesizes this struct from the directory — the directory name is the
//! identity, because it is the only name the host knows without running Lua
//! — and the spec table the entry file returns declares the rest.
//!
//! ## Example entry file
//!
//! ```lua
//! return {
//!     name = "my-plugin",
//!     version = "1.0.0",
//!     description = "A sample plugin",
//!     author = "Your Name",
//!     license = "MIT",
//!
//!     setup = function(opts) end,
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("Failed to read manifest: {0}")]
    Io(#[from] std::io::Error),

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

    /// The version the plugin's spec table declares, once one has been read.
    ///
    /// `None` until then. Discovery walks directories and never runs Lua, so
    /// between discovery and load the host knows no version at all. This
    /// used to hold a synthesized `"0.0.0"`, which `plugin.list` and the
    /// session-setup event both reported as if a release had said so.
    #[serde(default)]
    pub version: Option<String>,

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

    /// The name the plugin's spec table declares, when it differs from the
    /// directory it lives in.
    ///
    /// Identity is the DIRECTORY name — the only name knowable without running
    /// Lua. This is honoured for `[plugins.<name>]` lookup so a repo cloned as
    /// `crucible-discord` whose plugin declares `name = "discord"` still gets
    /// its config section.
    #[serde(skip)]
    pub declared_name: Option<String>,
}

impl PluginManifest {
    /// Create a manifest from a directory path alone, with no Lua run.
    ///
    /// Uses the directory stem as the plugin name, and NO version: the
    /// version is the plugin's own claim, and the spec table that carries it
    /// is only read at load.
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
            version: None,
            description: String::new(),
            author: String::new(),
            license: None,
            intercepts_tools: false,
            declared_name: None,
        })
    }

    pub fn validate(&self) -> ManifestResult<()> {
        if self.name.is_empty() {
            return Err(ManifestError::MissingField("name".to_string()));
        }

        if !is_valid_plugin_name(&self.name) {
            return Err(ManifestError::Validation(format!(
                "Invalid plugin name '{}': must be lowercase alphanumeric with hyphens",
                self.name
            )));
        }

        // A version is optional — an unloaded plugin has none — but a
        // version that IS stated has to parse, or the plugin is claiming
        // something no reader can compare.
        if let Some(version) = &self.version {
            if version.is_empty() {
                return Err(ManifestError::MissingField("version".to_string()));
            }
            if !is_valid_version(version) {
                return Err(ManifestError::InvalidVersion(version.clone()));
            }
        }

        Ok(())
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

    /// The version the plugin declared, or `None` while it is unread.
    pub fn version(&self) -> Option<&str> {
        self.manifest.version.as_deref()
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
    fn test_from_directory_defaults() {
        let manifest =
            PluginManifest::from_directory_defaults(Path::new("/plugins/my-plugin")).unwrap();
        assert_eq!(manifest.name, "my-plugin");
        assert_eq!(manifest.version, None);
        assert!(!manifest.intercepts_tools);
    }

    #[test]
    fn test_from_directory_defaults_invalid_name() {
        let result = PluginManifest::from_directory_defaults(Path::new("/plugins/My Plugin!"));
        assert!(result.is_err());
    }
}
