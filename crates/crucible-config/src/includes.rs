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
//! - The env var must be set or config loading will fail
//! - Use this for secrets that shouldn't be in files
//!
//! ## 3. Include Section (legacy)
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

    /// Database configuration file
    #[serde(default)]
    pub database: Option<String>,

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
            && self.database.is_none()
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
        if let Some(path) = &self.database {
            includes.push(("database", path.as_str()));
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

/// Check if a string is a file reference
fn is_file_reference(s: &str) -> bool {
    s.starts_with(FILE_REF_PREFIX) && s.ends_with(FILE_REF_SUFFIX)
}

/// Check if a string is an env reference
fn is_env_reference(s: &str) -> bool {
    s.starts_with(ENV_REF_PREFIX) && s.ends_with(ENV_REF_SUFFIX)
}

/// Extract the path from a file reference
fn extract_file_path(s: &str) -> Option<&str> {
    if is_file_reference(s) {
        Some(&s[FILE_REF_PREFIX.len()..s.len() - FILE_REF_SUFFIX.len()])
    } else {
        None
    }
}

/// Extract the variable name from an env reference
fn extract_env_var(s: &str) -> Option<&str> {
    if is_env_reference(s) {
        Some(&s[ENV_REF_PREFIX.len()..s.len() - ENV_REF_SUFFIX.len()])
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

/// Process all `{file:path}` and `{env:VAR}` references in a TOML value tree
///
/// Walks the entire TOML value tree and replaces:
/// - `{file:path}` with the content of the referenced file
/// - `{env:VAR}` with the value of the environment variable
#[cfg(feature = "toml")]
pub fn process_file_references(
    value: &mut toml::Value,
    base_dir: &Path,
) -> Result<(), Vec<IncludeError>> {
    let mut errors = Vec::new();
    process_refs_recursive(value, base_dir, &mut errors);

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
) {
    match value {
        toml::Value::String(s) => {
            // Handle {file:path} references
            if let Some(file_path) = extract_file_path(s) {
                let resolved = resolve_include_path(file_path, base_dir);
                debug!(
                    "Processing file reference: {} -> {}",
                    file_path,
                    resolved.display()
                );

                match read_file_as_value(&resolved) {
                    Ok(file_value) => {
                        *value = file_value;
                    }
                    Err(e) => {
                        warn!("Failed to load file reference {}: {}", file_path, e);
                        errors.push(e);
                    }
                }
            }
            // Handle {env:VAR} references
            else if let Some(var_name) = extract_env_var(s) {
                debug!("Processing env reference: {}", var_name);

                match std::env::var(var_name) {
                    Ok(env_value) => {
                        *value = toml::Value::String(env_value);
                    }
                    Err(_) => {
                        warn!("Environment variable not found: {}", var_name);
                        errors.push(IncludeError::EnvVarNotFound {
                            var_name: var_name.to_string(),
                        });
                    }
                }
            }
        }
        toml::Value::Array(arr) => {
            for item in arr.iter_mut() {
                process_refs_recursive(item, base_dir, errors);
            }
        }
        toml::Value::Table(table) => {
            for (_key, val) in table.iter_mut() {
                process_refs_recursive(val, base_dir, errors);
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
#[derive(Debug, Clone)]
pub enum IncludeError {
    /// Include file not found
    FileNotFound(PathBuf),

    /// IO error reading include file
    Io {
        /// Path to the file
        path: PathBuf,
        /// Error message
        error: String,
    },

    /// Parse error in include file
    Parse {
        /// Path to the file
        path: PathBuf,
        /// Error message
        error: String,
    },

    /// Environment variable not found
    EnvVarNotFound {
        /// Name of the environment variable
        var_name: String,
    },
}

impl std::fmt::Display for IncludeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IncludeError::FileNotFound(path) => {
                write!(f, "Include file not found: {}", path.display())
            }
            IncludeError::Io { path, error } => {
                write!(f, "IO error reading {}: {}", path.display(), error)
            }
            IncludeError::Parse { path, error } => {
                write!(f, "Parse error in {}: {}", path.display(), error)
            }
            IncludeError::EnvVarNotFound { var_name } => {
                write!(
                    f,
                    "Environment variable not found: {} (referenced as {{env:{}}})",
                    var_name, var_name
                )
            }
        }
    }
}

impl std::error::Error for IncludeError {}

#[cfg(all(test, feature = "toml"))]
mod tests {
    use super::*;
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
    fn test_is_file_reference() {
        assert!(is_file_reference("{file:test.toml}"));
        assert!(is_file_reference("{file:~/secrets/key.txt}"));
        assert!(is_file_reference("{file:/etc/crucible/config.toml}"));

        assert!(!is_file_reference("test.toml"));
        assert!(!is_file_reference("{file:missing-end"));
        assert!(!is_file_reference("file:test.toml}"));
        assert!(!is_file_reference(""));
    }

    #[test]
    fn test_extract_file_path() {
        assert_eq!(extract_file_path("{file:test.toml}"), Some("test.toml"));
        assert_eq!(
            extract_file_path("{file:~/secrets/key.txt}"),
            Some("~/secrets/key.txt")
        );
        assert_eq!(extract_file_path("not a ref"), None);
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

        let result = process_file_references(&mut config, temp.path());
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

        let result = process_file_references(&mut config, temp.path());
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

        let result = process_file_references(&mut config, temp.path());
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

        let result = process_file_references(&mut config, temp.path());
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

        let result = process_file_references(&mut config, temp.path());
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

        let result = process_file_references(&mut config, temp.path());
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
    fn test_is_env_reference() {
        assert!(is_env_reference("{env:OPENAI_API_KEY}"));
        assert!(is_env_reference("{env:MY_VAR}"));
        assert!(is_env_reference("{env:A}"));

        assert!(!is_env_reference("OPENAI_API_KEY"));
        assert!(!is_env_reference("{env:missing-end"));
        assert!(!is_env_reference("env:VAR}"));
        assert!(!is_env_reference(""));
        assert!(!is_env_reference("{file:test.toml}"));
    }

    #[test]
    fn test_extract_env_var() {
        assert_eq!(extract_env_var("{env:OPENAI_API_KEY}"), Some("OPENAI_API_KEY"));
        assert_eq!(extract_env_var("{env:MY_VAR}"), Some("MY_VAR"));
        assert_eq!(extract_env_var("not-a-ref"), None);
    }

    #[test]
    fn test_env_ref_string_value() {
        let temp = TempDir::new().unwrap();

        // Set an env var for testing
        std::env::set_var("CRUCIBLE_TEST_API_KEY", "sk-test-key-12345");

        let config_content = r#"
[embedding]
provider = "openai"
api_key = "{env:CRUCIBLE_TEST_API_KEY}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path());
        assert!(result.is_ok());

        let embedding = config.get("embedding").unwrap();
        assert_eq!(
            embedding.get("api_key").unwrap().as_str().unwrap(),
            "sk-test-key-12345"
        );

        // Cleanup
        std::env::remove_var("CRUCIBLE_TEST_API_KEY");
    }

    #[test]
    fn test_env_ref_not_found() {
        let temp = TempDir::new().unwrap();

        let config_content = r#"
api_key = "{env:CRUCIBLE_NONEXISTENT_VAR_12345}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path());
        assert!(result.is_err());

        let errors = result.unwrap_err();
        assert!(matches!(errors[0], IncludeError::EnvVarNotFound { .. }));

        if let IncludeError::EnvVarNotFound { var_name } = &errors[0] {
            assert_eq!(var_name, "CRUCIBLE_NONEXISTENT_VAR_12345");
        }
    }

    #[test]
    fn test_env_ref_in_array() {
        let temp = TempDir::new().unwrap();

        std::env::set_var("CRUCIBLE_TEST_PATH1", "/opt/tools");
        std::env::set_var("CRUCIBLE_TEST_PATH2", "/usr/local/tools");

        let config_content = r#"
extra_paths = ["{env:CRUCIBLE_TEST_PATH1}", "{env:CRUCIBLE_TEST_PATH2}", "/static/path"]
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path());
        assert!(result.is_ok());

        let paths = config.get("extra_paths").unwrap().as_array().unwrap();
        assert_eq!(paths[0].as_str().unwrap(), "/opt/tools");
        assert_eq!(paths[1].as_str().unwrap(), "/usr/local/tools");
        assert_eq!(paths[2].as_str().unwrap(), "/static/path");

        // Cleanup
        std::env::remove_var("CRUCIBLE_TEST_PATH1");
        std::env::remove_var("CRUCIBLE_TEST_PATH2");
    }

    #[test]
    fn test_mixed_file_and_env_refs() {
        let temp = TempDir::new().unwrap();

        // Create a file
        fs::write(temp.path().join("model.txt"), "gpt-4").unwrap();

        // Set an env var
        std::env::set_var("CRUCIBLE_TEST_MIXED_KEY", "sk-mixed-key");

        let config_content = r#"
[embedding]
provider = "openai"
api_key = "{env:CRUCIBLE_TEST_MIXED_KEY}"
model = "{file:model.txt}"
"#;
        let mut config: toml::Value = toml::from_str(config_content).unwrap();

        let result = process_file_references(&mut config, temp.path());
        assert!(result.is_ok());

        let embedding = config.get("embedding").unwrap();
        assert_eq!(
            embedding.get("api_key").unwrap().as_str().unwrap(),
            "sk-mixed-key"
        );
        assert_eq!(
            embedding.get("model").unwrap().as_str().unwrap(),
            "gpt-4"
        );

        // Cleanup
        std::env::remove_var("CRUCIBLE_TEST_MIXED_KEY");
    }
}
