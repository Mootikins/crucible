use crate::workspace::{KilnAttachment, SecurityConfig};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Top-level project configuration stored in `.crucible/project.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectConfig {
    /// Optional project metadata section.
    #[serde(default)]
    pub project: Option<ProjectMeta>,
    /// Attached kilns for this project.
    #[serde(default)]
    pub kilns: Vec<KilnAttachment>,
    /// Project-level security policy.
    #[serde(default)]
    pub security: SecurityConfig,
}

/// Optional project metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectMeta {
    /// Optional project display name.
    pub name: Option<String>,
}

/// Read project configuration from `.crucible/project.toml` with workspace fallback.
pub fn read_project_config(dir: &Path) -> Option<ProjectConfig> {
    let crucible_dir = dir.join(".crucible");
    let project_path = crucible_dir.join("project.toml");

    if project_path.exists() {
        let content = fs::read_to_string(&project_path).ok()?;
        return toml::from_str::<ProjectConfig>(&content).ok();
    }

    let workspace_path = crucible_dir.join("workspace.toml");
    let content = fs::read_to_string(&workspace_path).ok()?;
    toml::from_str::<ProjectConfig>(&content).ok()
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
    use crate::components::DataClassification;
    use crate::security::ShellPolicy;
    use tempfile::TempDir;

    #[test]
    fn project_config_parses_from_new_format() {
        let toml = r#"
[project]
name = "Test"

[[kilns]]
path = "."
"#;

        let config: ProjectConfig = toml::from_str(toml).expect("Failed to parse project config");
        assert_eq!(
            config.project.and_then(|p| p.name),
            Some("Test".to_string())
        );
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
        assert_eq!(config.project, None);
        assert_eq!(config.kilns.len(), 1);
        assert_eq!(config.kilns[0].data_classification, None);
        assert_eq!(config.security.shell.whitelist, vec!["git".to_string()]);
        assert_eq!(config.security.shell.blacklist, vec!["rm -rf".to_string()]);
    }

    #[test]
    fn project_config_roundtrip() {
        let config = ProjectConfig {
            project: Some(ProjectMeta {
                name: Some("Project Name".to_string()),
            }),
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

        assert_eq!(config.project, None);
        assert!(config.kilns.is_empty());
        assert!(config.security.shell.whitelist.is_empty());
        assert!(config.security.shell.blacklist.is_empty());
    }

    #[test]
    fn read_project_config_tries_project_toml_first() {
        let temp = TempDir::new().expect("Failed to create temp dir");
        let crucible_dir = temp.path().join(".crucible");
        fs::create_dir_all(&crucible_dir).expect("Failed to create .crucible");

        fs::write(
            crucible_dir.join("project.toml"),
            "[project]\nname = \"New Project\"\n",
        )
        .expect("Failed to write project.toml");
        fs::write(
            crucible_dir.join("workspace.toml"),
            "[workspace]\nname = \"Old Workspace\"\n",
        )
        .expect("Failed to write workspace.toml");

        let config = read_project_config(temp.path()).expect("Expected project config");
        assert_eq!(
            config.project.and_then(|p| p.name),
            Some("New Project".to_string())
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
        assert_eq!(config.project, None);
        assert_eq!(config.kilns.len(), 1);
    }
}
