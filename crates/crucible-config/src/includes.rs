//! Simple TOML include mechanism
//!
//! This module provides ways to include external content into configuration:
//!
//! ## 1. File References: `{file:path}`
//!
//! Use `{file:path}` anywhere in your config to pull in external content:
//!
//! ```toml
//! # Include a whole section from a TOML file
//! gateway = "{file:mcps.toml}"
//!
//! # Include just a secret value
//! [embedding]
//! provider = "openai"
//! api_key = "{file:~/.secrets/openai.key}"
//!
//! # Works in arrays too
//! extra_paths = ["{file:paths.toml}"]
//! ```
//!
//! - If the file is `.toml`, it's parsed and merged as structured data
//! - Otherwise, the file content is used as a string value (trimmed)
//! - Paths can be relative, absolute, or use `~` for home directory
//!
//! ## 2. Environment Variables: `{env:VAR}`
//!
//! Use `{env:VAR}` to read values from environment variables:
//!
//! ```toml
//! [embedding]
//! provider = "openai"
//! api_key = "{env:OPENAI_API_KEY}"
//!
//! [providers.anthropic]
//! api_key = "{env:ANTHROPIC_API_KEY}"
//! ```
//!
//! - In `BestEffort` mode (default), missing env vars log a warning and continue
//! - In `Strict` mode, missing env vars are treated as hard errors
//! - Use this for secrets that shouldn't be in files
//!
//! ## 3. Directory References: `{dir:path}` (config.d style)
//!
//! Use `{dir:path}` to merge all `.toml` files from a directory:
//!
//! ```toml
//! # Include all provider configs from a directory
//! providers = "{dir:~/.config/crucible/providers.d/}"
//! ```
//!
//! Files in the directory are processed in sorted order (alphabetically),
//! allowing predictable override behavior with numeric prefixes:
//! - `00-base.toml` - processed first
//! - `10-cloud.toml` - processed second
//! - `99-override.toml` - processed last, overrides earlier values
//!
//! Non-`.toml` files and hidden files (starting with `.`) are ignored.
//!
//! ## 4. Include Section (legacy)
//!
//! The `[include]` section merges files into specific top-level sections:
//!
//! ```toml
//! [include]
//! gateway = "mcps.toml"
//! ```
//!
//! This merges `mcps.toml` into the `gateway` section.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Include configuration specifying external files to load
///
/// Each key corresponds to a section in the config, and the value
/// is the path to the file containing that section's configuration.
///
/// # Example
///
/// ```toml
/// [include]
/// gateway = "mcps.toml"           # MCP server configurations
/// embedding = "~/secrets/api.toml" # API keys (keep secure!)
/// profiles = "profiles.toml"       # Environment profiles
/// ```
///
/// Any section name not explicitly listed here can still be used
/// via the catch-all `custom` field.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct IncludeConfig {
    /// Gateway/MCP servers configuration file
    #[serde(default)]
    pub gateway: Option<String>,

    /// Discovery paths configuration file
    #[serde(default)]
    pub discovery: Option<String>,

    /// Hooks configuration file
    #[serde(default)]
    pub hooks: Option<String>,

    /// Enrichment configuration file
    #[serde(default)]
    pub enrichment: Option<String>,

    /// Embedding provider configuration file
    ///
    /// Useful for keeping API keys separate:
    /// ```toml
    /// # embedding.toml
    /// provider = "openai"
    /// model = "text-embedding-3-small"
    /// api_key = "sk-..."
    /// ```
    #[serde(default)]
    pub embedding: Option<String>,

    /// ACP (Agent Client Protocol) configuration file
    #[serde(default)]
    pub acp: Option<String>,

    /// Profiles configuration file
    ///
    /// Define multiple environment profiles:
    /// ```toml
    /// # profiles.toml
    /// [development]
    /// kiln_path = "~/dev-vault"
    ///
    /// [production]
    /// kiln_path = "/data/vault"
    /// ```
    #[serde(default)]
    pub profiles: Option<String>,

    /// Additional named includes (for custom sections)
    ///
    /// Any key not matching the explicit fields above will be
    /// captured here, allowing arbitrary section includes.
    #[serde(flatten)]
    pub custom: HashMap<String, String>,
}

impl IncludeConfig {
    /// Check if there are any includes to process
    pub fn is_empty(&self) -> bool {
        self.gateway.is_none()
            && self.discovery.is_none()
            && self.hooks.is_none()
            && self.enrichment.is_none()
            && self.embedding.is_none()
            && self.acp.is_none()
            && self.profiles.is_none()
            && self.custom.is_empty()
    }

    /// Get all include paths as (section_name, path) pairs
    pub fn all_includes(&self) -> Vec<(&str, &str)> {
        let mut includes = Vec::new();

        if let Some(path) = &self.gateway {
            includes.push(("gateway", path.as_str()));
        }
        if let Some(path) = &self.discovery {
            includes.push(("discovery", path.as_str()));
        }
        if let Some(path) = &self.hooks {
            includes.push(("hooks", path.as_str()));
        }
        if let Some(path) = &self.enrichment {
            includes.push(("enrichment", path.as_str()));
        }
        if let Some(path) = &self.embedding {
            includes.push(("embedding", path.as_str()));
        }
        if let Some(path) = &self.acp {
            includes.push(("acp", path.as_str()));
        }
        if let Some(path) = &self.profiles {
            includes.push(("profiles", path.as_str()));
        }

        for (section, path) in &self.custom {
            includes.push((section.as_str(), path.as_str()));
        }

        includes
    }
}

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

/// Read an include file and return its content as a TOML value
#[cfg(feature = "toml")]
pub fn read_include_file(path: &Path) -> Result<toml::Value, IncludeError> {
    if !path.exists() {
        return Err(IncludeError::FileNotFound(path.to_path_buf()));
    }

    let content = std::fs::read_to_string(path).map_err(|e| IncludeError::Io {
        path: path.to_path_buf(),
        error: e.to_string(),
    })?;

    let value: toml::Value = toml::from_str(&content).map_err(|e| IncludeError::Parse {
        path: path.to_path_buf(),
        error: e.to_string(),
    })?;

    Ok(value)
}

/// Pattern for file references: {file:path}
const FILE_REF_PREFIX: &str = "{file:";
const FILE_REF_SUFFIX: &str = "}";

/// Pattern for env references: {env:VAR}
const ENV_REF_PREFIX: &str = "{env:";
const ENV_REF_SUFFIX: &str = "}";

/// Pattern for directory references: {dir:path}
const DIR_REF_PREFIX: &str = "{dir:";
const DIR_REF_SUFFIX: &str = "}";

/// Reference kind enum for template resolution
#[derive(Debug, Clone, PartialEq, Eq)]
enum RefKind {
    /// File reference: {file:path}
    File(PathBuf),
    /// Environment variable reference: {env:VAR}
    Env(String),
    /// Directory reference: {dir:path}
    Dir(PathBuf),
}

/// Controls how template resolution handles missing references (e.g., env vars).
///
/// - `BestEffort` (default): logs warnings and continues, collecting errors.
/// - `Strict`: treats missing references as hard errors (logs at error level).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResolveMode {
    /// Treat missing references as hard errors.
    Strict,
    /// Log warnings and continue (default). Current callers use this.
    #[default]
    BestEffort,
}

/// Parse a reference string into RefKind if it matches any pattern
fn parse_ref_kind(s: &str) -> Option<RefKind> {
    if s.starts_with(FILE_REF_PREFIX) && s.ends_with(FILE_REF_SUFFIX) {
        let path_str = &s[FILE_REF_PREFIX.len()..s.len() - FILE_REF_SUFFIX.len()];
        Some(RefKind::File(PathBuf::from(path_str)))
    } else if s.starts_with(ENV_REF_PREFIX) && s.ends_with(ENV_REF_SUFFIX) {
        let var_name = &s[ENV_REF_PREFIX.len()..s.len() - ENV_REF_SUFFIX.len()];
        Some(RefKind::Env(var_name.to_string()))
    } else if s.starts_with(DIR_REF_PREFIX) && s.ends_with(DIR_REF_SUFFIX) {
        let path_str = &s[DIR_REF_PREFIX.len()..s.len() - DIR_REF_SUFFIX.len()];
        Some(RefKind::Dir(PathBuf::from(path_str)))
    } else {
        None
    }
}

/// Read a file and return its content as a TOML value
///
/// - If the file has a `.toml` extension, parse it as TOML
/// - Otherwise, return the content as a trimmed string
#[cfg(feature = "toml")]
fn read_file_as_value(path: &Path) -> Result<toml::Value, IncludeError> {
    if !path.exists() {
        return Err(IncludeError::FileNotFound(path.to_path_buf()));
    }

    let content = std::fs::read_to_string(path).map_err(|e| IncludeError::Io {
        path: path.to_path_buf(),
        error: e.to_string(),
    })?;

    // If it's a TOML file, parse it as structured data
    if path.extension().is_some_and(|ext| ext == "toml") {
        let value: toml::Value = toml::from_str(&content).map_err(|e| IncludeError::Parse {
            path: path.to_path_buf(),
            error: e.to_string(),
        })?;
        Ok(value)
    } else {
        // Otherwise, return as a trimmed string (useful for secrets)
        Ok(toml::Value::String(content.trim().to_string()))
    }
}

/// Read all .toml files from a directory and merge them
///
/// Files are processed in sorted order (alphabetically by filename),
/// allowing predictable override behavior with numeric prefixes:
/// - `00-base.toml` is processed first
/// - `99-override.toml` is processed last and overrides earlier values
///
/// Non-.toml files are ignored.
#[cfg(feature = "toml")]
fn read_dir_as_value(
    dir_path: &Path,
    base_dir: &Path,
    errors: &mut Vec<IncludeError>,
    mode: ResolveMode,
) -> Result<toml::Value, IncludeError> {
    if !dir_path.exists() {
        return Err(IncludeError::DirNotFound(dir_path.to_path_buf()));
    }

    if !dir_path.is_dir() {
        return Err(IncludeError::NotADirectory(dir_path.to_path_buf()));
    }

    // Collect and sort .toml files
    let mut toml_files: Vec<PathBuf> = std::fs::read_dir(dir_path)
        .map_err(|e| IncludeError::Io {
            path: dir_path.to_path_buf(),
            error: e.to_string(),
        })?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path.extension().is_some_and(|ext| ext == "toml")
                && !path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.'))
        })
        .collect();

    toml_files.sort();

    debug!(
        "Reading {} .toml files from {}",
        toml_files.len(),
        dir_path.display()
    );

    // Start with an empty table
    let mut result = toml::Value::Table(toml::map::Map::new());

    // Merge each file in order
    for file_path in toml_files {
        debug!("Processing config fragment: {}", file_path.display());

        match read_file_as_value(&file_path) {
            Ok(mut file_value) => {
                // Recursively process any refs in this file
                process_refs_recursive(&mut file_value, base_dir, errors, mode);

                // Merge into result
                merge_toml_values(&mut result, &file_value);
            }
            Err(e) => {
                warn!(
                    "Failed to read config fragment {}: {}",
                    file_path.display(),
                    e
                );
                errors.push(e);
            }
        }
    }

    Ok(result)
}

/// Process all `{file:path}`, `{env:VAR}`, and `{dir:path}` references in a TOML value tree
///
/// Walks the entire TOML value tree and replaces:
/// - `{file:path}` with the content of the referenced file
/// - `{env:VAR}` with the value of the environment variable
/// - `{dir:path}` with merged content of all .toml files in the directory (config.d style)
#[cfg(feature = "toml")]
pub fn process_file_references(
    value: &mut toml::Value,
    base_dir: &Path,
    mode: ResolveMode,
) -> Result<(), Vec<IncludeError>> {
    let mut errors = Vec::new();
    process_refs_recursive(value, base_dir, &mut errors, mode);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(feature = "toml")]
fn process_refs_recursive(
    value: &mut toml::Value,
    base_dir: &Path,
    errors: &mut Vec<IncludeError>,
    mode: ResolveMode,
) {
    match value {
        toml::Value::String(s) => {
            // Parse the reference and handle accordingly
            if let Some(ref_kind) = parse_ref_kind(s) {
                match ref_kind {
                    RefKind::File(file_path) => {
                        let resolved = resolve_include_path(file_path.to_str().unwrap(), base_dir);
                        debug!(
                            "Processing file reference: {} -> {}",
                            file_path.display(),
                            resolved.display()
                        );

                        match read_file_as_value(&resolved) {
                            Ok(file_value) => {
                                *value = file_value;
                            }
                            Err(e) => {
                                match mode {
                                    ResolveMode::Strict => {
                                        tracing::error!(
                                            "Failed to load file reference {}: {}",
                                            file_path.display(),
                                            e
                                        );
                                    }
                                    ResolveMode::BestEffort => {
                                        warn!(
                                            "Failed to load file reference {}: {}",
                                            file_path.display(),
                                            e
                                        );
                                    }
                                }
                                errors.push(e);
                            }
                        }
                    }
                    RefKind::Env(var_name) => {
                        debug!("Processing env reference: {}", var_name);

                        match std::env::var(&var_name) {
                            Ok(env_value) => {
                                *value = toml::Value::String(env_value);
                            }
                            Err(_) => {
                                match mode {
                                    ResolveMode::Strict => {
                                        tracing::error!(
                                            "Environment variable not found: {}",
                                            var_name
                                        );
                                    }
                                    ResolveMode::BestEffort => {
                                        warn!("Environment variable not found: {}", var_name);
                                    }
                                }
                                errors.push(IncludeError::EnvVarNotFound {
                                    var_name: var_name.clone(),
                                });
                            }
                        }
                    }
                    RefKind::Dir(dir_path) => {
                        let resolved = resolve_include_path(dir_path.to_str().unwrap(), base_dir);
                        debug!(
                            "Processing dir reference: {} -> {}",
                            dir_path.display(),
                            resolved.display()
                        );

                        match read_dir_as_value(&resolved, base_dir, errors, mode) {
                            Ok(dir_value) => {
                                *value = dir_value;
                            }
                            Err(e) => {
                                match mode {
                                    ResolveMode::Strict => {
                                        tracing::error!(
                                            "Failed to load dir reference {}: {}",
                                            dir_path.display(),
                                            e
                                        );
                                    }
                                    ResolveMode::BestEffort => {
                                        warn!(
                                            "Failed to load dir reference {}: {}",
                                            dir_path.display(),
                                            e
                                        );
                                    }
                                }
                                errors.push(e);
                            }
                        }
                    }
                }
            }
        }
        toml::Value::Array(arr) => {
            for item in arr.iter_mut() {
                process_refs_recursive(item, base_dir, errors, mode);
            }
        }
        toml::Value::Table(table) => {
            for (_key, val) in table.iter_mut() {
                process_refs_recursive(val, base_dir, errors, mode);
            }
        }
        // Other types (Integer, Float, Boolean, Datetime) don't contain refs
        _ => {}
    }
}

/// Merge include files into a TOML value
///
/// This function takes the main config as a TOML Value and merges
/// included files into the appropriate sections.
#[cfg(feature = "toml")]
pub fn merge_includes(
    main_config: &mut toml::Value,
    base_dir: &Path,
) -> Result<(), Vec<IncludeError>> {
    let mut errors = Vec::new();

    // Extract the include section if present
    let includes = if let Some(toml::Value::Table(table)) = main_config.get("include") {
        match toml::Value::Table(table.clone()).try_into::<IncludeConfig>() {
            Ok(inc) => inc,
            Err(e) => {
                errors.push(IncludeError::Parse {
                    path: PathBuf::from("[include section]"),
                    error: e.to_string(),
                });
                return Err(errors);
            }
        }
    } else {
        return Ok(()); // No includes to process
    };

    if includes.is_empty() {
        return Ok(());
    }

    // Process each include
    for (section, include_path) in includes.all_includes() {
        let resolved_path = resolve_include_path(include_path, base_dir);
        debug!(
            "Processing include: {} -> {}",
            section,
            resolved_path.display()
        );

        match read_include_file(&resolved_path) {
            Ok(included_value) => {
                // Merge into the main config at the specified section
                if let toml::Value::Table(ref mut main_table) = main_config {
                    // If the section already exists, merge; otherwise set
                    if let Some(existing) = main_table.get_mut(section) {
                        merge_toml_values(existing, &included_value);
                    } else {
                        main_table.insert(section.to_string(), included_value);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to include {}: {}", include_path, e);
                errors.push(e);
            }
        }
    }

    // Remove the include section from the final config
    if let toml::Value::Table(ref mut main_table) = main_config {
        main_table.remove("include");
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Merge two TOML values, with source overriding target for conflicts
#[cfg(feature = "toml")]
fn merge_toml_values(target: &mut toml::Value, source: &toml::Value) {
    match (target, source) {
        (toml::Value::Table(target_table), toml::Value::Table(source_table)) => {
            // Deep merge tables
            for (key, source_value) in source_table {
                if let Some(target_value) = target_table.get_mut(key) {
                    merge_toml_values(target_value, source_value);
                } else {
                    target_table.insert(key.clone(), source_value.clone());
                }
            }
        }
        (toml::Value::Array(target_array), toml::Value::Array(source_array)) => {
            // Append source array items to target
            target_array.extend(source_array.iter().cloned());
        }
        (target, source) => {
            // For other types, source overwrites target
            *target = source.clone();
        }
    }
}

/// Errors that can occur during include processing
#[derive(Debug, Clone, thiserror::Error)]
pub enum IncludeError {
    /// Include file not found
    #[error("Include file not found: {}", .0.display())]
    FileNotFound(PathBuf),

    /// Include directory not found
    #[error("Include directory not found: {}", .0.display())]
    DirNotFound(PathBuf),

    /// Path is not a directory
    #[error("Path is not a directory: {}", .0.display())]
    NotADirectory(PathBuf),

    /// IO error reading include file
    #[error("IO error reading {}: {}", path.display(), error)]
    Io {
        /// Path to the file
        path: PathBuf,
        /// Error message
        error: String,
    },

    /// Parse error in include file
    #[error("Parse error in {}: {}", path.display(), error)]
    Parse {
        /// Path to the file
        path: PathBuf,
        /// Error message
        error: String,
    },

    /// Environment variable not found
    #[error("Environment variable not found: {var_name} (referenced as {{env:{var_name}}})")]
    EnvVarNotFound {
        /// Name of the environment variable
        var_name: String,
    },
}

#[cfg(all(test, feature = "toml"))]
mod tests {
    use super::*;
    use crucible_core::test_support::EnvVarGuard;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_include_config_empty() {
        let config = IncludeConfig::default();
        assert!(config.is_empty());
    }

    #[test]
    fn test_include_config_with_gateway() {
        let toml_content = r#"
gateway = "mcps.toml"
"#;
        let config: IncludeConfig = toml::from_str(toml_content).unwrap();

        assert!(!config.is_empty());
        assert_eq!(config.gateway, Some("mcps.toml".to_string()));
    }

    #[test]
    fn test_include_config_all_includes() {
        let toml_content = r#"
gateway = "mcps.toml"
discovery = "discovery.toml"
hooks = "hooks.toml"
enrichment = "enrichment.toml"
custom_section = "custom.toml"
"#;
        let config: IncludeConfig = toml::from_str(toml_content).unwrap();

        let includes = config.all_includes();
        assert_eq!(includes.len(), 5);

        let section_names: Vec<&str> = includes.iter().map(|(s, _)| *s).collect();
        assert!(section_names.contains(&"gateway"));
        assert!(section_names.contains(&"discovery"));
        assert!(section_names.contains(&"hooks"));
        assert!(section_names.contains(&"enrichment"));
        assert!(section_names.contains(&"custom_section"));
    }

    #[test]
    fn test_resolve_include_path_relative() {
        let base = PathBuf::from("/home/user/.config/crucible");
        let resolved = resolve_include_path("mcps.toml", &base);
        assert_eq!(
            resolved,
            PathBuf::from("/home/user/.config/crucible/mcps.toml")
        );
    }

    #[test]
    fn test_resolve_include_path_absolute() {
        let base = PathBuf::from("/home/user/.config/crucible");
        let resolved = resolve_include_path("/etc/crucible/mcps.toml", &base);
        assert_eq!(resolved, PathBuf::from("/etc/crucible/mcps.toml"));
    }

    #[test]
    fn test_resolve_include_path_home() {
        let base = PathBuf::from("/some/path");
        let resolved = resolve_include_path("~/crucible/mcps.toml", &base);

        // Should start with home directory
        if let Some(home) = dirs::home_dir() {
            assert_eq!(resolved, home.join("crucible/mcps.toml"));
        }
    }

    #[test]
    fn test_merge_includes_gateway() {
        let temp = TempDir::new().unwrap();

        // Create the include file
        let mcps_content = r#"
[[servers]]
name = "github"
prefix = "gh_"

[servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
"#;
        fs::write(temp.path().join("mcps.toml"), mcps_content).unwrap();

        // Create main config with include
        let main_content = r#"
profile = "default"

[include]
gateway = "mcps.toml"

[cli]
verbose = true
"#;
        let mut main_config: toml::Value = toml::from_str(main_content).unwrap();

        // Merge includes
        let result = merge_includes(&mut main_config, temp.path());
        assert!(result.is_ok(), "Merge should succeed");

        // Verify the gateway section was added
        let gateway = main_config
            .get("gateway")
            .expect("gateway section should exist");
        let servers = gateway.get("servers").expect("servers array should exist");
        assert!(servers.is_array());

        let servers_array = servers.as_array().unwrap();
        assert_eq!(servers_array.len(), 1);

        let first_server = &servers_array[0];
        assert_eq!(
            first_server.get("name").and_then(|v| v.as_str()),
            Some("github")
        );

        // Verify include section was removed
        assert!(main_config.get("include").is_none());

        // Verify other sections remain
        assert!(main_config.get("profile").is_some());
        assert!(main_config.get("cli").is_some());
    }

    #[test]
    fn test_merge_includes_appends_arrays() {
        let temp = TempDir::new().unwrap();

        // Create include file with one server
        let include_content = r#"
[[servers]]
name = "included-server"

[servers.transport]
type = "stdio"
command = "included-cmd"
"#;
        fs::write(temp.path().join("extra.toml"), include_content).unwrap();

        // Main config already has a server
        let main_content = r#"
[include]
gateway = "extra.toml"

[[gateway.servers]]
name = "original-server"

[gateway.servers.transport]
type = "stdio"
command = "original-cmd"
"#;
        let mut main_config: toml::Value = toml::from_str(main_content).unwrap();

        let result = merge_includes(&mut main_config, temp.path());
        assert!(result.is_ok());

        // Should have both servers
        let servers = main_config
            .get("gateway")
            .and_then(|g| g.get("servers"))
            .and_then(|s| s.as_array())
            .expect("servers array");

        assert_eq!(servers.len(), 2, "Should have original + included server");
    }

    #[test]
    fn test_merge_includes_file_not_found() {
        let temp = TempDir::new().unwrap();

        let main_content = r#"
[include]
gateway = "nonexistent.toml"
"#;
        let mut main_config: toml::Value = toml::from_str(main_content).unwrap();

        let result = merge_includes(&mut main_config, temp.path());
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], IncludeError::FileNotFound(_)));
    }

    #[test]
    fn test_merge_includes_parse_error() {
        let temp = TempDir::new().unwrap();

        // Create invalid TOML
        fs::write(temp.path().join("bad.toml"), "invalid = [[[").unwrap();

        let main_content = r#"
[include]
gateway = "bad.toml"
"#;
        let mut main_config: toml::Value = toml::from_str(main_content).unwrap();

        let result = merge_includes(&mut main_config, temp.path());
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], IncludeError::Parse { .. }));
    }

    #[test]
    fn test_merge_includes_no_includes() {
        let main_content = r#"
profile = "default"

[cli]
verbose = true
"#;
        let mut main_config: toml::Value = toml::from_str(main_content).unwrap();

        // Use temp dir even though it's not accessed (no includes to process)
        let base_dir = std::env::temp_dir().join("crucible_test_no_includes");
        let result = merge_includes(&mut main_config, &base_dir);
        assert!(result.is_ok());

        // Config should be unchanged
        assert!(main_config.get("profile").is_some());
        assert!(main_config.get("cli").is_some());
    }

    // =========================================================================
    // File Reference Tests
    // =========================================================================

    #[test]
    fn test_parse_ref_kind_file() {
        assert_eq!(
            parse_ref_kind("{file:test.toml}"),
            Some(RefKind::File(PathBuf::from("test.toml")))
        );
        assert_eq!(
            parse_ref_kind("{file:~/secrets/key.txt}"),
            Some(RefKind::File(PathBuf::from("~/secrets/key.txt")))
        );
        assert_eq!(
            parse_ref_kind("{file:/etc/crucible/config.toml}"),
            Some(RefKind::File(PathBuf::from("/etc/crucible/config.toml")))
        );

        assert_eq!(parse_ref_kind("test.toml"), None);
        assert_eq!(parse_ref_kind("{file:missing-end"), None);
        assert_eq!(parse_ref_kind("file:test.toml}"), None);
        assert_eq!(parse_ref_kind(""), None);
    }

    #[test]
    #[test]
    fn test_parse_ref_kind_env() {
        assert_eq!(
            parse_ref_kind("{env:OPENAI_API_KEY}"),
            Some(RefKind::Env("OPENAI_API_KEY".to_string()))
        );
        assert_eq!(
            parse_ref_kind("{env:MY_VAR}"),
            Some(RefKind::Env("MY_VAR".to_string()))
        );
        assert_eq!(
            parse_ref_kind("{env:A}"),
            Some(RefKind::Env("A".to_string()))
        );

        assert_eq!(parse_ref_kind("OPENAI_API_KEY"), None);
        assert_eq!(parse_ref_kind("{env:missing-end"), None);
        assert_eq!(parse_ref_kind("env:VAR}"), None);
        assert_eq!(parse_ref_kind(""), None);
    }

    #[test]
    fn test_parse_ref_kind_dir() {
        assert_eq!(
            parse_ref_kind("{dir:~/.config/crucible/providers.d/}"),
            Some(RefKind::Dir(PathBuf::from(
                "~/.config/crucible/providers.d/"
            )))
        );
        assert_eq!(
            parse_ref_kind("{dir:providers.d}"),
            Some(RefKind::Dir(PathBuf::from("providers.d")))
        );
        assert_eq!(
            parse_ref_kind("{dir:/etc/crucible/conf.d}"),
            Some(RefKind::Dir(PathBuf::from("/etc/crucible/conf.d")))
        );

        assert_eq!(parse_ref_kind("providers.d"), None);
        assert_eq!(parse_ref_kind("{dir:missing-end"), None);
        assert_eq!(parse_ref_kind("dir:path}"), None);
        assert_eq!(parse_ref_kind(""), None);
    }

    #[test]
    fn test_file_ref_string_value() {
        let temp = TempDir::new().unwrap();

        // Create a plain text file (like a secret key)
        fs::write(temp.path().join("api.key"), "sk-secret-key-12345\n").unwrap();

        let config_content = r#"
[embedding]
provider = "openai"
api_key = "{file:api.key}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        assert!(result.is_ok());

        // The api_key should now be the file content (trimmed)
        let api_key = config
            .get("embedding")
            .and_then(|e| e.get("api_key"))
            .and_then(|k| k.as_str())
            .expect("api_key should exist");

        assert_eq!(api_key, "sk-secret-key-12345");
    }

    #[test]
    fn test_file_ref_toml_value() {
        let temp = TempDir::new().unwrap();

        // Create a TOML file to include
        let gateway_content = r#"
[[servers]]
name = "github"
prefix = "gh_"

[servers.transport]
type = "stdio"
command = "npx"
"#;
        fs::write(temp.path().join("gateway.toml"), gateway_content).unwrap();

        let config_content = r#"
profile = "default"
gateway = "{file:gateway.toml}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        assert!(result.is_ok());

        // The gateway should now be the parsed TOML content
        let gateway = config.get("gateway").expect("gateway should exist");
        assert!(gateway.is_table());

        let servers = gateway.get("servers").expect("servers should exist");
        assert!(servers.is_array());

        let first_server = servers.as_array().unwrap().first().unwrap();
        assert_eq!(
            first_server.get("name").and_then(|n| n.as_str()),
            Some("github")
        );
    }

    #[test]
    fn test_file_ref_in_array() {
        let temp = TempDir::new().unwrap();

        // Create files with paths
        fs::write(temp.path().join("path1.txt"), "/opt/tools").unwrap();
        fs::write(temp.path().join("path2.txt"), "/usr/local/tools").unwrap();

        let config_content = r#"
extra_paths = ["{file:path1.txt}", "{file:path2.txt}", "/static/path"]
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        assert!(result.is_ok());

        let paths = config
            .get("extra_paths")
            .and_then(|p| p.as_array())
            .expect("extra_paths should be an array");

        assert_eq!(paths.len(), 3);
        assert_eq!(paths[0].as_str(), Some("/opt/tools"));
        assert_eq!(paths[1].as_str(), Some("/usr/local/tools"));
        assert_eq!(paths[2].as_str(), Some("/static/path"));
    }

    #[test]
    fn test_file_ref_nested() {
        let temp = TempDir::new().unwrap();

        fs::write(temp.path().join("secret.txt"), "super-secret").unwrap();

        let config_content = r#"
[level1]
[level1.level2]
[level1.level2.level3]
secret = "{file:secret.txt}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        assert!(result.is_ok());

        let secret = config
            .get("level1")
            .and_then(|l1| l1.get("level2"))
            .and_then(|l2| l2.get("level3"))
            .and_then(|l3| l3.get("secret"))
            .and_then(|s| s.as_str())
            .expect("secret should exist");

        assert_eq!(secret, "super-secret");
    }

    #[test]
    fn test_file_ref_not_found() {
        let temp = TempDir::new().unwrap();

        let config_content = r#"
api_key = "{file:nonexistent.key}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], IncludeError::FileNotFound(_)));
    }

    #[test]
    fn test_file_ref_with_home_path() {
        // This test just verifies the path is resolved correctly
        // (actual file won't exist, so we check the error path)
        let temp = TempDir::new().unwrap();

        let config_content = r#"
api_key = "{file:~/.secrets/test.key}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        // Should fail with FileNotFound (not a parse error)
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert!(matches!(errors[0], IncludeError::FileNotFound(_)));

        // Verify the path was resolved to home directory
        if let IncludeError::FileNotFound(path) = &errors[0] {
            if let Some(home) = dirs::home_dir() {
                assert!(path.starts_with(home));
            }
        }
    }

    // ========================================================================
    // Environment variable reference tests
    // ========================================================================

    #[test]
    fn test_dir_ref_merges_toml_files() {
        let temp = TempDir::new().unwrap();

        // Create a directory with config fragments
        let providers_dir = temp.path().join("providers.d");
        fs::create_dir(&providers_dir).unwrap();

        // Files are sorted alphabetically, so use numeric prefixes
        fs::write(
            providers_dir.join("00-local.toml"),
            r#"
[local]
backend = "ollama"
endpoint = "http://localhost:11434"
"#,
        )
        .unwrap();

        fs::write(
            providers_dir.join("10-cloud.toml"),
            r#"
[cloud]
backend = "openai"
api_key = "sk-test"
"#,
        )
        .unwrap();

        let config_content = r#"
providers = "{dir:providers.d}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        assert!(result.is_ok(), "Should succeed: {:?}", result);

        // Should have merged both files
        let providers = config.get("providers").expect("providers should exist");
        assert!(providers.is_table());

        let local = providers.get("local").expect("local should exist");
        assert_eq!(local.get("backend").unwrap().as_str(), Some("ollama"));

        let cloud = providers.get("cloud").expect("cloud should exist");
        assert_eq!(cloud.get("backend").unwrap().as_str(), Some("openai"));
    }

    #[test]
    fn test_dir_ref_sorted_order() {
        let temp = TempDir::new().unwrap();

        let conf_dir = temp.path().join("conf.d");
        fs::create_dir(&conf_dir).unwrap();

        // Same key in multiple files - later files should override
        fs::write(
            conf_dir.join("00-base.toml"),
            r#"
name = "base"
timeout = 30
"#,
        )
        .unwrap();

        fs::write(
            conf_dir.join("99-override.toml"),
            r#"
name = "override"
"#,
        )
        .unwrap();

        let config_content = r#"
settings = "{dir:conf.d}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        assert!(result.is_ok());

        let settings = config.get("settings").unwrap();
        // 99-override.toml should override 00-base.toml
        assert_eq!(settings.get("name").unwrap().as_str(), Some("override"));
        // But timeout from 00-base.toml should remain
        assert_eq!(settings.get("timeout").unwrap().as_integer(), Some(30));
    }

    #[test]
    fn test_dir_ref_ignores_non_toml() {
        let temp = TempDir::new().unwrap();

        let conf_dir = temp.path().join("conf.d");
        fs::create_dir(&conf_dir).unwrap();

        fs::write(
            conf_dir.join("config.toml"),
            r#"
key = "value"
"#,
        )
        .unwrap();

        // These should be ignored
        fs::write(conf_dir.join("README.md"), "# Documentation").unwrap();
        fs::write(conf_dir.join(".hidden"), "hidden file").unwrap();
        fs::write(conf_dir.join("backup.toml.bak"), "backup").unwrap();

        let config_content = r#"
settings = "{dir:conf.d}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        assert!(result.is_ok());

        let settings = config.get("settings").unwrap();
        assert_eq!(settings.get("key").unwrap().as_str(), Some("value"));
        // Only 1 key from the one .toml file
        assert_eq!(settings.as_table().unwrap().len(), 1);
    }

    #[test]
    fn test_dir_ref_empty_directory() {
        let temp = TempDir::new().unwrap();

        let empty_dir = temp.path().join("empty.d");
        fs::create_dir(&empty_dir).unwrap();

        let config_content = r#"
settings = "{dir:empty.d}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        assert!(result.is_ok());

        // Should be an empty table
        let settings = config.get("settings").unwrap();
        assert!(settings.is_table());
        assert!(settings.as_table().unwrap().is_empty());
    }

    #[test]
    fn test_dir_ref_not_found() {
        let temp = TempDir::new().unwrap();

        let config_content = r#"
settings = "{dir:nonexistent.d}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert!(matches!(errors[0], IncludeError::DirNotFound(_)));
    }

    #[test]
    fn test_dir_ref_with_nested_refs() {
        let temp = TempDir::new().unwrap();

        // Set up env var for nested ref
        let _guard = EnvVarGuard::set("CRUCIBLE_TEST_DIR_KEY", "nested-secret".to_string());

        let conf_dir = temp.path().join("conf.d");
        fs::create_dir(&conf_dir).unwrap();

        // File with {env:} reference inside
        fs::write(
            conf_dir.join("secrets.toml"),
            r#"
api_key = "{env:CRUCIBLE_TEST_DIR_KEY}"
"#,
        )
        .unwrap();

        let config_content = r#"
settings = "{dir:conf.d}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        assert!(result.is_ok());

        let settings = config.get("settings").unwrap();
        assert_eq!(
            settings.get("api_key").unwrap().as_str(),
            Some("nested-secret")
        );
    }

    #[test]
    fn test_dir_ref_with_home_path() {
        // Test that ~ paths are resolved (will fail with DirNotFound since dir doesn't exist)
        let temp = TempDir::new().unwrap();

        let config_content = r#"
settings = "{dir:~/.config/crucible/nonexistent.d/}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert!(matches!(errors[0], IncludeError::DirNotFound(_)));

        // Verify path was resolved to home directory
        if let IncludeError::DirNotFound(path) = &errors[0] {
            if let Some(home) = dirs::home_dir() {
                assert!(path.starts_with(home), "Path should start with home dir");
            }
        }
    }

    #[test]
    fn test_dir_ref_ignores_subdirectories() {
        let temp = TempDir::new().unwrap();

        let conf_dir = temp.path().join("conf.d");
        fs::create_dir(&conf_dir).unwrap();

        // Create a toml file
        fs::write(
            conf_dir.join("config.toml"),
            r#"
key = "value"
"#,
        )
        .unwrap();

        // Create a subdirectory with toml files (should be ignored)
        let sub_dir = conf_dir.join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        fs::write(
            sub_dir.join("nested.toml"),
            r#"
nested_key = "nested_value"
"#,
        )
        .unwrap();

        let config_content = r#"
settings = "{dir:conf.d}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        assert!(result.is_ok());

        let settings = config.get("settings").unwrap();
        // Should only have the top-level key, not nested
        assert_eq!(settings.get("key").unwrap().as_str(), Some("value"));
        assert!(
            settings.get("nested_key").is_none(),
            "Subdirs should be ignored"
        );
    }

    #[test]
    fn test_dir_ref_parse_error_continues() {
        let temp = TempDir::new().unwrap();

        let conf_dir = temp.path().join("conf.d");
        fs::create_dir(&conf_dir).unwrap();

        // Valid file
        fs::write(
            conf_dir.join("00-valid.toml"),
            r#"
valid_key = "valid_value"
"#,
        )
        .unwrap();

        // Invalid TOML file
        fs::write(conf_dir.join("50-invalid.toml"), "invalid = [[[").unwrap();

        // Another valid file
        fs::write(
            conf_dir.join("99-also-valid.toml"),
            r#"
another_key = "another_value"
"#,
        )
        .unwrap();

        let config_content = r#"
settings = "{dir:conf.d}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        // Should have errors from the invalid file
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], IncludeError::Parse { .. }));
    }

    #[test]
    fn test_dir_ref_deep_merge_tables() {
        let temp = TempDir::new().unwrap();

        let conf_dir = temp.path().join("conf.d");
        fs::create_dir(&conf_dir).unwrap();

        // First file with nested table
        fs::write(
            conf_dir.join("00-base.toml"),
            r#"
[server]
host = "localhost"
port = 8080

[server.tls]
enabled = false
"#,
        )
        .unwrap();

        // Second file adds to nested table
        fs::write(
            conf_dir.join("10-tls.toml"),
            r#"
[server.tls]
enabled = true
cert = "/path/to/cert.pem"
"#,
        )
        .unwrap();

        let config_content = r#"
settings = "{dir:conf.d}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        assert!(result.is_ok(), "Should succeed: {:?}", result);

        let settings = config.get("settings").unwrap();
        let server = settings.get("server").unwrap();

        // Original values preserved
        assert_eq!(server.get("host").unwrap().as_str(), Some("localhost"));
        assert_eq!(server.get("port").unwrap().as_integer(), Some(8080));

        // TLS section was deep-merged
        let tls = server.get("tls").unwrap();
        assert_eq!(tls.get("enabled").unwrap().as_bool(), Some(true)); // Overridden
        assert_eq!(tls.get("cert").unwrap().as_str(), Some("/path/to/cert.pem"));
        // Added
    }

    #[test]
    fn test_dir_ref_appends_arrays() {
        let temp = TempDir::new().unwrap();

        let conf_dir = temp.path().join("mcps.d");
        fs::create_dir(&conf_dir).unwrap();

        // First file with servers array
        fs::write(
            conf_dir.join("00-github.toml"),
            r#"
[[servers]]
name = "github"
prefix = "gh_"
"#,
        )
        .unwrap();

        // Second file adds more servers
        fs::write(
            conf_dir.join("10-gitlab.toml"),
            r#"
[[servers]]
name = "gitlab"
prefix = "gl_"
"#,
        )
        .unwrap();

        let config_content = r#"
gateway = "{dir:mcps.d}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path(), ResolveMode::BestEffort);
        assert!(result.is_ok(), "Should succeed: {:?}", result);

        let gateway = config.get("gateway").unwrap();
        let servers = gateway.get("servers").unwrap().as_array().unwrap();

        // Both servers should be present (arrays appended)
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].get("name").unwrap().as_str(), Some("github"));
        assert_eq!(servers[1].get("name").unwrap().as_str(), Some("gitlab"));
    }
}
