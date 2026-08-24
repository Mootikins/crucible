use crate::config::io_helpers::read_with_workspace_fallback;
use crate::config::workspace::{KilnAttachment, SecurityConfig};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Top-level project configuration stored in `.crucible/project.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectConfig {
    /// Attached kilns for this project.
    #[serde(default)]
    pub kilns: Vec<KilnAttachment>,
    /// Project-level security policy.
    #[serde(default)]
    pub security: SecurityConfig,
}

/// Read project configuration from `.crucible/project.toml` with workspace fallback.
pub fn read_project_config(dir: &Path) -> Option<ProjectConfig> {
    read_with_workspace_fallback(dir, "project.toml", "project")
}

/// Write project configuration to `.crucible/project.toml`.
pub fn write_project_config(dir: &Path, config: &ProjectConfig) -> Result<()> {
    let crucible_dir = dir.join(".crucible");
    fs::create_dir_all(&crucible_dir)?;
    let config_path = crucible_dir.join("project.toml");
    let toml = toml::to_string_pretty(config)?;
    fs::write(config_path, toml)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::components::DataClassification;
    use crate::config::security::{ProjectFileAccess, ShellPolicy};
    use tempfile::TempDir;

    #[test]
    fn project_config_parses_from_new_format() {
        // `[project]` is a removed section. Old project.toml files still
        // contain it, so the load must ignore it instead of an error.
        let toml = r#"
[project]
name = "Test"

[[kilns]]
path = "."
"#;

        let config: ProjectConfig = toml::from_str(toml).expect("Failed to parse project config");
        assert_eq!(config.kilns.len(), 1);
    }

    #[test]
    fn project_config_backward_compat_workspace_format() {
        let toml = r#"
[workspace]
name = "Old Workspace"

[[kilns]]
path = "./notes"

[security.shell]
whitelist = ["git"]
blacklist = ["rm -rf"]
"#;

        let config: ProjectConfig = toml::from_str(toml).expect("Failed to parse workspace format");
        assert_eq!(config.kilns.len(), 1);
        assert_eq!(config.kilns[0].data_classification, None);
        assert_eq!(config.security.shell.whitelist, vec!["git".to_string()]);
        assert_eq!(config.security.shell.blacklist, vec!["rm -rf".to_string()]);
    }

    #[test]
    fn project_config_roundtrip() {
        let config = ProjectConfig {
            kilns: vec![KilnAttachment {
                path: "./knowledge".into(),
                name: Some("Knowledge".to_string()),
                data_classification: Some(DataClassification::Internal),
            }],
            security: SecurityConfig {
                shell: ShellPolicy {
                    whitelist: vec!["git".to_string()],
                    blacklist: vec!["sudo".to_string()],
                },
                project_files: ProjectFileAccess::ReadOnly,
            },
        };

        let toml = toml::to_string(&config).expect("Failed to serialize");
        let parsed: ProjectConfig = toml::from_str(&toml).expect("Failed to deserialize");

        assert_eq!(config, parsed);
    }

    #[test]
    fn project_config_minimal() {
        let toml = "";
        let config: ProjectConfig = toml::from_str(toml).expect("Failed to parse minimal config");

        assert!(config.kilns.is_empty());
        assert!(config.security.shell.whitelist.is_empty());
        assert!(config.security.shell.blacklist.is_empty());
        // Project files default to read-write when the key is absent.
        assert_eq!(config.security.project_files, ProjectFileAccess::ReadWrite);
    }

    #[test]
    fn project_files_policy_parses_from_toml() {
        for (value, expected) in [
            ("read-write", ProjectFileAccess::ReadWrite),
            ("read-only", ProjectFileAccess::ReadOnly),
            ("off", ProjectFileAccess::Off),
        ] {
            let toml = format!("[security]\nproject_files = \"{value}\"\n");
            let config: ProjectConfig =
                toml::from_str(&toml).expect("Failed to parse project_files policy");
            assert_eq!(config.security.project_files, expected, "value {value}");
        }
    }

    #[test]
    fn read_project_config_tries_project_toml_first() {
        let temp = TempDir::new().expect("Failed to create temp dir");
        let crucible_dir = temp.path().join(".crucible");
        fs::create_dir_all(&crucible_dir).expect("Failed to create .crucible");

        fs::write(
            crucible_dir.join("project.toml"),
            "[[kilns]]\npath = \"from-project\"\n",
        )
        .expect("Failed to write project.toml");
        fs::write(
            crucible_dir.join("workspace.toml"),
            "[[kilns]]\npath = \"from-workspace\"\n",
        )
        .expect("Failed to write workspace.toml");

        let config = read_project_config(temp.path()).expect("Expected project config");
        assert_eq!(
            config.kilns[0].path,
            std::path::PathBuf::from("from-project")
        );
    }

    #[test]
    fn read_project_config_falls_back_to_workspace_toml() {
        let temp = TempDir::new().expect("Failed to create temp dir");
        let crucible_dir = temp.path().join(".crucible");
        fs::create_dir_all(&crucible_dir).expect("Failed to create .crucible");

        fs::write(
            crucible_dir.join("workspace.toml"),
            "[[kilns]]\npath = \"./notes\"\n",
        )
        .expect("Failed to write workspace.toml");

        let config = read_project_config(temp.path()).expect("Expected fallback project config");
        assert_eq!(config.kilns.len(), 1);
    }
}
