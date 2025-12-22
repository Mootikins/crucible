//! Configuration resolution and merging
//!
//! Implements three-tier configuration hierarchy:
//! 1. System defaults (hardcoded safe defaults)
//! 2. Global config (`~/.config/crucible/config.toml`)
//! 3. Workspace config (`.crucible/workspace.toml`)
//!
//! Lower tiers can extend or restrict higher tiers.

use crate::{global::GlobalConfig, security::ShellPolicy, workspace::WorkspaceConfig, ConfigError};
use std::{fs, path::Path};

/// Resolves configuration from multiple tiers
///
/// Merges system defaults, global config, and workspace config
/// according to the three-tier hierarchy.
#[derive(Debug, Clone)]
pub struct ConfigResolver {
    /// Global user configuration
    global: GlobalConfig,
    /// Workspace-specific configuration (if in a workspace)
    workspace: Option<WorkspaceConfig>,
}

impl ConfigResolver {
    /// Create resolver for a workspace
    ///
    /// Loads global config, then workspace config from `.crucible/workspace.toml`.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to workspace root (containing `.crucible/` directory)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crucible_config::ConfigResolver;
    ///
    /// let resolver = ConfigResolver::for_workspace("/path/to/workspace")?;
    /// let policy = resolver.shell_policy();
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn for_workspace(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let global = GlobalConfig::load()?;

        let workspace_config_path = path.as_ref().join(".crucible").join("workspace.toml");
        let workspace = if workspace_config_path.exists() {
            let contents = fs::read_to_string(&workspace_config_path).map_err(|e| {
                ConfigError::InvalidValue {
                    field: format!("workspace config at {}", workspace_config_path.display()),
                    value: format!("Failed to read: {}", e),
                }
            })?;

            let config: WorkspaceConfig =
                toml::from_str(&contents).map_err(|e| ConfigError::InvalidValue {
                    field: format!("workspace config at {}", workspace_config_path.display()),
                    value: format!("Failed to parse: {}", e),
                })?;

            Some(config)
        } else {
            None
        };

        Ok(Self { global, workspace })
    }

    /// Get merged shell policy
    ///
    /// Merges policies in order:
    /// 1. Start with system defaults (safe development commands)
    /// 2. Merge global config (user preferences)
    /// 3. Merge workspace config (project-specific overrides)
    ///
    /// Lower tiers can both extend (add to whitelist) and restrict (add to blacklist).
    /// Blacklist always takes precedence over whitelist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use crucible_config::ConfigResolver;
    ///
    /// let resolver = ConfigResolver::for_workspace(".")?;
    /// let policy = resolver.shell_policy();
    ///
    /// if policy.is_allowed("git", &["status"]) {
    ///     println!("Git is allowed");
    /// }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn shell_policy(&self) -> ShellPolicy {
        // Start with safe defaults
        let mut policy = ShellPolicy::with_defaults();

        // Merge global config
        policy = policy.merge(&self.global.security.shell);

        // Merge workspace config if present
        if let Some(workspace) = &self.workspace {
            policy = policy.merge(&workspace.security.shell);
        }

        policy
    }

    /// Get workspace config if present
    ///
    /// Returns `None` if not in a workspace context.
    pub fn workspace_config(&self) -> Option<&WorkspaceConfig> {
        self.workspace.as_ref()
    }

    /// Get global config
    pub fn global_config(&self) -> &GlobalConfig {
        &self.global
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_workspace(workspace_toml: Option<&str>) -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let workspace_path = temp_dir.path();

        // Create .crucible directory
        let crucible_dir = workspace_path.join(".crucible");
        fs::create_dir(&crucible_dir).expect("Failed to create .crucible dir");

        // Write workspace config if provided
        if let Some(toml) = workspace_toml {
            let workspace_config_path = crucible_dir.join("workspace.toml");
            let mut file = fs::File::create(workspace_config_path).expect("Failed to create file");
            file.write_all(toml.as_bytes())
                .expect("Failed to write file");
        }

        temp_dir
    }

    #[test]
    fn resolver_loads_workspace_config() {
        let workspace_toml = r#"
[workspace]
name = "Test Workspace"

[[kilns]]
path = "./notes"
"#;

        let temp_dir = create_test_workspace(Some(workspace_toml));
        let resolver =
            ConfigResolver::for_workspace(temp_dir.path()).expect("Failed to load resolver");

        let workspace = resolver
            .workspace_config()
            .expect("Workspace config missing");
        assert_eq!(workspace.workspace.name, "Test Workspace");
        assert_eq!(workspace.kilns.len(), 1);
    }

    #[test]
    fn resolver_handles_missing_workspace_config() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let workspace_path = temp_dir.path();

        // Create .crucible directory but no config file
        let crucible_dir = workspace_path.join(".crucible");
        fs::create_dir(&crucible_dir).expect("Failed to create .crucible dir");

        let resolver =
            ConfigResolver::for_workspace(workspace_path).expect("Failed to load resolver");

        assert!(resolver.workspace_config().is_none());
    }

    #[test]
    fn resolver_merges_global_and_workspace() {
        // Create workspace with custom security config
        let workspace_toml = r#"
[workspace]
name = "Secure Project"

[security.shell]
whitelist = ["custom-tool"]
blacklist = ["dangerous-cmd"]
"#;

        let temp_dir = create_test_workspace(Some(workspace_toml));
        let resolver =
            ConfigResolver::for_workspace(temp_dir.path()).expect("Failed to load resolver");

        let policy = resolver.shell_policy();

        // Should have defaults + workspace additions
        assert!(policy.is_allowed("git", &["status"])); // from defaults
        assert!(policy.is_allowed("cargo", &["build"])); // from defaults
        assert!(policy.is_allowed("custom-tool", &[])); // from workspace

        // Workspace blacklist should work
        assert!(!policy.is_allowed("dangerous-cmd", &[]));

        // Default blacklist should still work
        assert!(!policy.is_allowed("sudo", &["rm"]));
    }

    #[test]
    fn lower_tier_can_restrict() {
        // Workspace blacklists something that's in default whitelist
        let workspace_toml = r#"
[workspace]
name = "Restricted Project"

[security.shell]
blacklist = ["git push", "docker"]
"#;

        let temp_dir = create_test_workspace(Some(workspace_toml));
        let resolver =
            ConfigResolver::for_workspace(temp_dir.path()).expect("Failed to load resolver");

        let policy = resolver.shell_policy();

        // git is in defaults, but git push is blacklisted by workspace
        assert!(policy.is_allowed("git", &["status"]));
        assert!(policy.is_allowed("git", &["commit"]));
        assert!(!policy.is_allowed("git", &["push"])); // blocked by workspace

        // docker is in defaults, but blocked by workspace
        assert!(!policy.is_allowed("docker", &["run"]));
    }

    #[test]
    fn shell_policy_starts_with_defaults() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let workspace_path = temp_dir.path();

        // Create minimal workspace with no security overrides
        let crucible_dir = workspace_path.join(".crucible");
        fs::create_dir(&crucible_dir).expect("Failed to create .crucible dir");

        let workspace_toml = r#"
[workspace]
name = "Default Security"
"#;

        let workspace_config_path = crucible_dir.join("workspace.toml");
        let mut file = fs::File::create(workspace_config_path).expect("Failed to create file");
        file.write_all(workspace_toml.as_bytes())
            .expect("Failed to write file");

        let resolver =
            ConfigResolver::for_workspace(workspace_path).expect("Failed to load resolver");

        let policy = resolver.shell_policy();

        // Should have all default safe commands
        assert!(policy.is_allowed("git", &["status"]));
        assert!(policy.is_allowed("cargo", &["build"]));
        assert!(policy.is_allowed("npm", &["install"]));
        assert!(policy.is_allowed("docker", &["ps"]));

        // Should block default dangerous commands
        assert!(!policy.is_allowed("sudo", &["rm"]));
        assert!(!policy.is_allowed("rm", &["-rf", "/"]));
    }

    #[test]
    fn resolver_exposes_configs() {
        let workspace_toml = r#"
[workspace]
name = "Test"

[[kilns]]
path = "./notes"
"#;

        let temp_dir = create_test_workspace(Some(workspace_toml));
        let resolver =
            ConfigResolver::for_workspace(temp_dir.path()).expect("Failed to load resolver");

        // Can access global config
        let _global = resolver.global_config();

        // Can access workspace config
        let workspace = resolver.workspace_config().expect("Workspace missing");
        assert_eq!(workspace.workspace.name, "Test");
    }

    #[test]
    fn resolver_fails_on_invalid_workspace_toml() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let workspace_path = temp_dir.path();

        let crucible_dir = workspace_path.join(".crucible");
        fs::create_dir(&crucible_dir).expect("Failed to create .crucible dir");

        // Write invalid TOML
        let workspace_config_path = crucible_dir.join("workspace.toml");
        let mut file = fs::File::create(workspace_config_path).expect("Failed to create file");
        file.write_all(b"this is not valid toml {[}")
            .expect("Failed to write file");

        let result = ConfigResolver::for_workspace(workspace_path);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, ConfigError::InvalidValue { .. }));
    }
}
