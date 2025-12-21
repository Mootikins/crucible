//! Unified path discovery for Crucible plugins
//!
//! This module provides a consistent pattern for discovering plugin scripts
//! across multiple directories with support for:
//! - Global defaults (uses platform-appropriate directory: ~/.config/crucible/plugins/ on Linux,
//!   %APPDATA%\crucible\plugins\ on Windows (Roaming, syncs), ~/Library/Application Support/crucible/plugins/ on macOS)
//! - Kiln-specific paths (KILN/.crucible/plugins/ and KILN/plugins/)
//! - Additional paths from configuration
//! - Option to disable defaults
//!
//! ## Plugin Discovery Order
//!
//! Plugins are discovered in priority order (later sources override earlier by name):
//! 1. Global personal: `~/.config/crucible/plugins/`
//! 2. Kiln personal: `KILN/.crucible/plugins/` (gitignored)
//! 3. Kiln shared: `KILN/plugins/` (version-controlled)
//!
//! ## Usage
//!
//! ```rust,ignore
//! // Create discovery paths for plugins
//! let paths = DiscoveryPaths::new("plugins", Some(kiln_path));
//!
//! // Get all paths to search
//! for path in paths.all_paths() {
//!     println!("Searching: {}", path.display());
//! }
//!
//! // Add additional paths from config
//! let paths = paths.with_additional(vec!["/custom/plugins".into()]);
//! ```

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Unified path discovery configuration for Crucible plugins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryPaths {
    /// Type of resource being discovered (typically "plugins")
    type_name: String,

    /// Default paths (~/.config/crucible/<type>/, KILN/.crucible/<type>/)
    defaults: Vec<PathBuf>,

    /// Additional paths from configuration
    additional: Vec<PathBuf>,

    /// Whether to include default paths (true by default)
    use_defaults: bool,
}

impl DiscoveryPaths {
    /// Create new discovery paths for a resource type
    ///
    /// Default paths are:
    /// - Global: `~/.config/crucible/<type_name>/` (platform-appropriate config directory)
    /// - Kiln personal: `KILN/.crucible/<type_name>/` (gitignored)
    /// - Kiln shared: `KILN/<type_name>/` (version-controlled, if provided)
    ///
    /// Platform-specific global paths:
    /// - Linux: `~/.config/crucible/<type_name>/` (XDG config directory)
    /// - macOS: `~/Library/Application Support/crucible/<type_name>/`
    /// - Windows: `%APPDATA%\crucible\<type_name>\` (Roaming AppData, syncs across machines)
    ///
    /// # Arguments
    /// * `type_name` - The resource type (typically "plugins")
    /// * `kiln_path` - Optional path to the kiln directory
    pub fn new(type_name: impl Into<String>, kiln_path: Option<&Path>) -> Self {
        let type_name = type_name.into();
        let mut defaults = vec![];

        // Global default: Use platform-appropriate directory
        // Tools/hooks/events are treated as configuration (runtime concerns, not data)
        // On Windows: Use Roaming AppData (%APPDATA%) so user scripts sync across machines
        // On Linux: Use XDG config directory (~/.config/) - these are configuration, not data
        // On macOS: Use Application Support (current convention)
        #[cfg(target_os = "windows")]
        {
            // Windows: Use Roaming AppData for user-created scripts (they should sync)
            if let Some(config_dir) = dirs::config_dir() {
                defaults.push(config_dir.join("crucible").join(&type_name));
            } else if let Some(home) = dirs::home_dir() {
                defaults.push(home.join(".crucible").join(&type_name));
            }
        }

        #[cfg(target_os = "linux")]
        {
            // Linux: Use XDG config directory (~/.config/) - these are configuration, not data
            if let Some(config_dir) = dirs::config_dir() {
                defaults.push(config_dir.join("crucible").join(&type_name));
            } else if let Some(home) = dirs::home_dir() {
                defaults.push(home.join(".crucible").join(&type_name));
            }
        }

        #[cfg(target_os = "macos")]
        {
            // macOS: Use Application Support (keep as-is)
            if let Some(data_dir) = dirs::data_dir() {
                defaults.push(data_dir.join("crucible").join(&type_name));
            } else if let Some(home) = dirs::home_dir() {
                defaults.push(home.join(".crucible").join(&type_name));
            }
        }

        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            // Fallback for other platforms: Use config directory
            if let Some(config_dir) = dirs::config_dir() {
                defaults.push(config_dir.join("crucible").join(&type_name));
            } else if let Some(home) = dirs::home_dir() {
                defaults.push(home.join(".crucible").join(&type_name));
            }
        }

        // Kiln-specific paths (in priority order)
        if let Some(kiln) = kiln_path {
            // Kiln personal: KILN/.crucible/<type_name>/ (gitignored)
            defaults.push(kiln.join(".crucible").join(&type_name));
            // Kiln shared: KILN/<type_name>/ (version-controlled)
            defaults.push(kiln.join(&type_name));
        }

        Self {
            type_name,
            defaults,
            additional: vec![],
            use_defaults: true,
        }
    }

    /// Create discovery paths with no defaults (explicit paths only)
    pub fn empty(type_name: impl Into<String>) -> Self {
        Self {
            type_name: type_name.into(),
            defaults: vec![],
            additional: vec![],
            use_defaults: false,
        }
    }

    /// Get the resource type name
    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    /// Get all paths to search, combining defaults and additional
    ///
    /// Returns paths in priority order (additional first, then defaults)
    pub fn all_paths(&self) -> Vec<PathBuf> {
        let mut paths = self.additional.clone();

        if self.use_defaults {
            for default in &self.defaults {
                if !paths.contains(default) {
                    paths.push(default.clone());
                }
            }
        }

        paths
    }

    /// Get only the default paths
    pub fn default_paths(&self) -> &[PathBuf] {
        &self.defaults
    }

    /// Get only the additional paths
    pub fn additional_paths(&self) -> &[PathBuf] {
        &self.additional
    }

    /// Check if defaults are enabled
    pub fn uses_defaults(&self) -> bool {
        self.use_defaults
    }

    /// Add additional paths (builder pattern)
    pub fn with_additional(mut self, paths: Vec<PathBuf>) -> Self {
        self.additional.extend(paths);
        self
    }

    /// Add a single additional path (builder pattern)
    pub fn with_path(mut self, path: PathBuf) -> Self {
        if !self.additional.contains(&path) {
            self.additional.push(path);
        }
        self
    }

    /// Disable default paths (builder pattern)
    pub fn without_defaults(mut self) -> Self {
        self.use_defaults = false;
        self
    }

    /// Enable default paths (builder pattern)
    pub fn with_defaults(mut self) -> Self {
        self.use_defaults = true;
        self
    }

    /// Filter to only existing directories
    pub fn existing_paths(&self) -> Vec<PathBuf> {
        self.all_paths()
            .into_iter()
            .filter(|p| p.is_dir())
            .collect()
    }

    /// Get subdirectory paths for a nested type
    ///
    /// For example, `paths.subdir("events", "recipe_discovered")` returns
    /// paths like `~/.crucible/events/recipe_discovered/`
    pub fn subdir(&self, subdir_name: &str) -> Vec<PathBuf> {
        self.all_paths()
            .into_iter()
            .map(|p| p.join(subdir_name))
            .collect()
    }

    /// Get existing subdirectory paths
    pub fn existing_subdir(&self, subdir_name: &str) -> Vec<PathBuf> {
        self.subdir(subdir_name)
            .into_iter()
            .filter(|p| p.is_dir())
            .collect()
    }
}

/// Configuration section for discovery paths in TOML
///
/// ```toml
/// [discovery.tools]
/// additional_paths = ["/custom/tools"]
/// use_defaults = true
///
/// [discovery.hooks]
/// additional_paths = []
/// use_defaults = true
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Additional paths to search (beyond defaults)
    #[serde(default)]
    pub additional_paths: Vec<PathBuf>,

    /// Whether to include default paths (default: true)
    #[serde(default = "default_true")]
    pub use_defaults: bool,
}

fn default_true() -> bool {
    true
}

impl DiscoveryPaths {
    /// Apply configuration from TOML
    pub fn with_config(mut self, config: &DiscoveryConfig) -> Self {
        self.additional.extend(config.additional_paths.clone());
        self.use_defaults = config.use_defaults;
        self
    }

    /// Create discovery paths from configuration
    ///
    /// This method creates a `DiscoveryPaths` instance based on configuration,
    /// expanding tilde paths and respecting the `use_defaults` setting.
    ///
    /// # Arguments
    /// * `type_name` - The resource type (e.g., "tools", "hooks", "events")
    /// * `kiln_path` - Optional path to the kiln directory
    /// * `config` - Configuration for this discovery type
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use crucible_rune::discovery_paths::{DiscoveryPaths, DiscoveryConfig};
    ///
    /// let config = DiscoveryConfig {
    ///     additional_paths: vec!["~/.config/crucible/hooks".into()],
    ///     use_defaults: true,
    /// };
    ///
    /// let paths = DiscoveryPaths::from_config("hooks", Some(kiln_path), &config);
    /// ```
    pub fn from_config(
        type_name: &str,
        kiln_path: Option<&Path>,
        config: &DiscoveryConfig,
    ) -> Self {
        // Expand tilde paths
        let expanded_paths: Vec<PathBuf> = config
            .additional_paths
            .iter()
            .map(|p| {
                let path_str = p.to_string_lossy();
                shellexpand::tilde(&path_str).into_owned().into()
            })
            .collect();

        // Create base paths
        let mut paths = if config.use_defaults {
            Self::new(type_name, kiln_path)
        } else {
            Self::empty(type_name)
        };

        // Add expanded additional paths
        paths.additional = expanded_paths;

        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Cross-platform test path helper
    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("crucible_test_{}", name))
    }

    #[test]
    fn test_new_without_kiln() {
        let paths = DiscoveryPaths::new("tools", None);

        assert_eq!(paths.type_name(), "tools");
        assert!(paths.uses_defaults());

        // Should have the platform global default if a base dir exists.
        let expected = {
            #[cfg(target_os = "windows")]
            {
                dirs::config_dir()
                    .map(|d| d.join("crucible").join("tools"))
                    .or_else(|| dirs::home_dir().map(|h| h.join(".crucible").join("tools")))
            }

            #[cfg(target_os = "linux")]
            {
                dirs::config_dir()
                    .map(|d| d.join("crucible").join("tools"))
                    .or_else(|| dirs::home_dir().map(|h| h.join(".crucible").join("tools")))
            }

            #[cfg(target_os = "macos")]
            {
                dirs::data_dir()
                    .map(|d| d.join("crucible").join("tools"))
                    .or_else(|| dirs::home_dir().map(|h| h.join(".crucible").join("tools")))
            }

            #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
            {
                dirs::config_dir()
                    .map(|d| d.join("crucible").join("tools"))
                    .or_else(|| dirs::home_dir().map(|h| h.join(".crucible").join("tools")))
            }
        };

        if let Some(expected) = expected {
            assert!(!paths.default_paths().is_empty());
            assert_eq!(&paths.default_paths()[0], &expected);
        }
    }

    #[test]
    fn test_new_with_kiln() {
        let kiln = test_path("test-kiln");
        let paths = DiscoveryPaths::new("hooks", Some(&kiln));

        assert_eq!(paths.type_name(), "hooks");

        // Should have kiln path in defaults
        let kiln_path = kiln.join(".crucible").join("hooks");
        assert!(paths.default_paths().contains(&kiln_path));
    }

    #[test]
    fn test_empty() {
        let paths = DiscoveryPaths::empty("tools");

        assert!(!paths.uses_defaults());
        assert!(paths.default_paths().is_empty());
        assert!(paths.all_paths().is_empty());
    }

    #[test]
    fn test_with_additional() {
        let paths = DiscoveryPaths::empty("tools")
            .with_path("/custom/path1".into())
            .with_path("/custom/path2".into());

        assert_eq!(paths.additional_paths().len(), 2);
        assert_eq!(paths.all_paths().len(), 2);
    }

    #[test]
    fn test_without_defaults() {
        let paths = DiscoveryPaths::new("tools", None)
            .without_defaults()
            .with_path("/custom/only".into());

        // Should only return additional paths
        assert_eq!(paths.all_paths().len(), 1);
        assert_eq!(paths.all_paths()[0], PathBuf::from("/custom/only"));
    }

    #[test]
    fn test_all_paths_no_duplicates() {
        let kiln = test_path("kiln");
        let paths = DiscoveryPaths::new("tools", Some(&kiln))
            .with_path(kiln.join(".crucible").join("tools")); // Add same as default

        // Should not duplicate
        let all = paths.all_paths();
        let unique_count = all.iter().collect::<std::collections::HashSet<_>>().len();
        assert_eq!(all.len(), unique_count);
    }

    #[test]
    fn test_existing_paths() {
        let temp = TempDir::new().unwrap();
        let existing = temp.path().join("exists");
        fs::create_dir(&existing).unwrap();

        let paths = DiscoveryPaths::empty("tools")
            .with_path(existing.clone())
            .with_path(temp.path().join("nonexistent"));

        let existing_paths = paths.existing_paths();
        assert_eq!(existing_paths.len(), 1);
        assert_eq!(existing_paths[0], existing);
    }

    #[test]
    fn test_subdir() {
        let paths = DiscoveryPaths::empty("events")
            .with_path("/home/user/.crucible/events".into())
            .with_path("/kiln/events".into());

        let subdirs = paths.subdir("recipe_discovered");
        assert_eq!(subdirs.len(), 2);
        assert_eq!(
            subdirs[0],
            PathBuf::from("/home/user/.crucible/events/recipe_discovered")
        );
        assert_eq!(subdirs[1], PathBuf::from("/kiln/events/recipe_discovered"));
    }

    #[test]
    fn test_existing_subdir() {
        let temp = TempDir::new().unwrap();
        let base = temp.path().join("events");
        let subdir = base.join("test_event");
        fs::create_dir_all(&subdir).unwrap();

        let paths = DiscoveryPaths::empty("events").with_path(base);
        let existing = paths.existing_subdir("test_event");

        assert_eq!(existing.len(), 1);
        assert_eq!(existing[0], subdir);

        // Non-existent subdir should return empty
        let none = paths.existing_subdir("nonexistent");
        assert!(none.is_empty());
    }

    #[test]
    fn test_with_config() {
        let config = DiscoveryConfig {
            additional_paths: vec!["/extra/path".into()],
            use_defaults: false,
        };

        let paths = DiscoveryPaths::new("tools", None).with_config(&config);

        assert!(!paths.uses_defaults());
        assert!(paths
            .additional_paths()
            .contains(&PathBuf::from("/extra/path")));
    }

    #[test]
    fn test_priority_order() {
        let kiln = PathBuf::from("/kiln");
        let paths = DiscoveryPaths::new("tools", Some(&kiln)).with_path("/priority/first".into());

        let all = paths.all_paths();
        // Additional paths should come first
        assert_eq!(all[0], PathBuf::from("/priority/first"));
    }

    #[test]
    fn test_from_config_with_defaults() {
        let kiln = test_path("test-kiln");
        let config = DiscoveryConfig {
            additional_paths: vec!["/custom/path".into()],
            use_defaults: true,
        };

        let paths = DiscoveryPaths::from_config("tools", Some(&kiln), &config);

        assert_eq!(paths.type_name(), "tools");
        assert!(paths.uses_defaults());
        assert!(paths
            .additional_paths()
            .contains(&PathBuf::from("/custom/path")));
        // Should also have default paths
        assert!(!paths.default_paths().is_empty());
    }

    #[test]
    fn test_from_config_without_defaults() {
        let kiln = test_path("test-kiln");
        let config = DiscoveryConfig {
            additional_paths: vec!["/custom/path".into()],
            use_defaults: false,
        };

        let paths = DiscoveryPaths::from_config("hooks", Some(&kiln), &config);

        assert_eq!(paths.type_name(), "hooks");
        assert!(!paths.uses_defaults());
        assert_eq!(paths.additional_paths().len(), 1);
        assert_eq!(paths.additional_paths()[0], PathBuf::from("/custom/path"));
        // Should not include defaults in all_paths
        assert_eq!(paths.all_paths().len(), 1);
    }

    #[test]
    fn test_from_config_tilde_expansion() {
        let config = DiscoveryConfig {
            additional_paths: vec!["~/.config/crucible/tools".into()],
            use_defaults: false,
        };

        let paths = DiscoveryPaths::from_config("tools", None, &config);

        let expanded = paths.additional_paths();
        assert_eq!(expanded.len(), 1);
        // Should expand tilde to home directory
        if let Some(home) = dirs::home_dir() {
            let expected = home.join(".config/crucible/tools");
            assert_eq!(expanded[0], expected);
        }
    }

    #[test]
    fn test_from_config_multiple_paths() {
        let config = DiscoveryConfig {
            additional_paths: vec!["/path1".into(), "~/.config/path2".into(), "/path3".into()],
            use_defaults: true,
        };

        let paths = DiscoveryPaths::from_config("events", None, &config);

        assert_eq!(paths.additional_paths().len(), 3);
        assert!(paths
            .additional_paths()
            .iter()
            .any(|p| p.ends_with("path1")));
        assert!(paths
            .additional_paths()
            .iter()
            .any(|p| p.ends_with("path3")));
    }
}
