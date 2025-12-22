//! Workspace configuration types
//!
//! Defines the `.crucible/workspace.toml` format for workspace-level configuration.
//! A workspace can contain multiple kiln attachments and security overrides.

use crate::security::ShellPolicy;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Complete workspace configuration
///
/// Loaded from `.crucible/workspace.toml` in the workspace root.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceConfig {
    /// Workspace metadata
    pub workspace: WorkspaceMeta,
    /// Attached kilns (knowledge bases)
    #[serde(default)]
    pub kilns: Vec<KilnAttachment>,
    /// Security configuration overrides
    #[serde(default)]
    pub security: SecurityConfig,
}

/// Workspace metadata
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceMeta {
    /// Human-readable workspace name
    pub name: String,
}

/// Kiln attachment configuration
///
/// Defines a knowledge base (kiln) that is part of this workspace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KilnAttachment {
    /// Absolute or relative path to kiln directory
    pub path: PathBuf,
    /// Optional display name for the kiln
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Security configuration for workspace
///
/// Allows workspace-level security policy overrides.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct SecurityConfig {
    /// Shell command execution policy
    pub shell: ShellPolicy,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_config_parses_from_toml() {
        let toml = r#"
[workspace]
name = "My Research Project"

[[kilns]]
path = "./notes"
name = "Main Notes"

[[kilns]]
path = "./references"

[security.shell]
whitelist = ["git", "cargo"]
blacklist = ["rm -rf"]
"#;

        let config: WorkspaceConfig = toml::from_str(toml).expect("Failed to parse");

        assert_eq!(config.workspace.name, "My Research Project");
        assert_eq!(config.kilns.len(), 2);
        assert_eq!(config.kilns[0].path, PathBuf::from("./notes"));
        assert_eq!(config.kilns[0].name, Some("Main Notes".to_string()));
        assert_eq!(config.kilns[1].path, PathBuf::from("./references"));
        assert_eq!(config.kilns[1].name, None);

        assert_eq!(config.security.shell.whitelist.len(), 2);
        assert!(config.security.shell.whitelist.contains(&"git".to_string()));
        assert!(config
            .security
            .shell
            .whitelist
            .contains(&"cargo".to_string()));
        assert_eq!(config.security.shell.blacklist.len(), 1);
        assert!(config
            .security
            .shell
            .blacklist
            .contains(&"rm -rf".to_string()));
    }

    #[test]
    fn workspace_config_parses_minimal() {
        let toml = r#"
[workspace]
name = "Minimal"
"#;

        let config: WorkspaceConfig = toml::from_str(toml).expect("Failed to parse");

        assert_eq!(config.workspace.name, "Minimal");
        assert_eq!(config.kilns.len(), 0);
        assert_eq!(config.security.shell.whitelist.len(), 0);
        assert_eq!(config.security.shell.blacklist.len(), 0);
    }

    #[test]
    fn workspace_config_serializes_to_toml() {
        let config = WorkspaceConfig {
            workspace: WorkspaceMeta {
                name: "Test Workspace".to_string(),
            },
            kilns: vec![KilnAttachment {
                path: PathBuf::from("./notes"),
                name: Some("Notes".to_string()),
            }],
            security: SecurityConfig {
                shell: ShellPolicy {
                    whitelist: vec!["git".to_string()],
                    blacklist: vec!["sudo".to_string()],
                },
            },
        };

        let toml = toml::to_string(&config).expect("Failed to serialize");
        let parsed: WorkspaceConfig = toml::from_str(&toml).expect("Failed to re-parse");

        assert_eq!(config, parsed);
    }

    #[test]
    fn kiln_attachment_without_name() {
        let toml = r#"
[workspace]
name = "Test"

[[kilns]]
path = "./docs"
"#;

        let config: WorkspaceConfig = toml::from_str(toml).expect("Failed to parse");

        assert_eq!(config.kilns[0].path, PathBuf::from("./docs"));
        assert_eq!(config.kilns[0].name, None);
    }

    #[test]
    fn security_config_defaults_to_empty() {
        let toml = r#"
[workspace]
name = "Test"
"#;

        let config: WorkspaceConfig = toml::from_str(toml).expect("Failed to parse");

        // Default security config should be empty (deny-all)
        assert_eq!(config.security.shell.whitelist.len(), 0);
        assert_eq!(config.security.shell.blacklist.len(), 0);
    }
}
