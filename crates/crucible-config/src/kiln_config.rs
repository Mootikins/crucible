use crate::io_helpers::read_with_workspace_fallback;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Top-level kiln configuration stored in `.crucible/kiln.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KilnConfig {
    /// Kiln metadata section.
    #[serde(alias = "workspace")]
    pub kiln: KilnMeta,
}

/// Human-readable kiln metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KilnMeta {
    /// Kiln display name.
    pub name: String,
}

/// Read kiln configuration from `.crucible/kiln.toml` with workspace fallback.
pub fn read_kiln_config(dir: &Path) -> Option<KilnConfig> {
    read_with_workspace_fallback(dir, "kiln.toml", "kiln")
}

/// Write kiln configuration to `.crucible/kiln.toml`.
pub fn write_kiln_config(dir: &Path, config: &KilnConfig) -> Result<()> {
    let crucible_dir = dir.join(".crucible");
    fs::create_dir_all(&crucible_dir)?;
    let config_path = crucible_dir.join("kiln.toml");
    let toml = toml::to_string_pretty(config)?;
    fs::write(config_path, toml)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn kiln_config_parses_from_new_format() {
        let toml = r#"
[kiln]
name = "Test"
"#;

        let config: KilnConfig = toml::from_str(toml).expect("Failed to parse kiln config");
        assert_eq!(config.kiln.name, "Test");
    }

    #[test]
    fn kiln_config_backward_compat_workspace_section() {
        let toml = r#"
[workspace]
name = "Test"
"#;

        let config: KilnConfig = toml::from_str(toml).expect("Failed to parse workspace alias");
        assert_eq!(config.kiln.name, "Test");
    }

    #[test]
    fn kiln_config_roundtrip() {
        let config = KilnConfig {
            kiln: KilnMeta {
                name: "Test Kiln".to_string(),
            },
        };

        let toml = toml::to_string(&config).expect("Failed to serialize");
        let parsed: KilnConfig = toml::from_str(&toml).expect("Failed to deserialize");

        assert_eq!(config, parsed);
    }

    #[test]
    fn read_kiln_config_tries_kiln_toml_first() {
        let temp = TempDir::new().expect("Failed to create temp dir");
        let crucible_dir = temp.path().join(".crucible");
        fs::create_dir_all(&crucible_dir).expect("Failed to create .crucible");

        fs::write(
            crucible_dir.join("kiln.toml"),
            "[kiln]\nname = \"Kiln First\"\n",
        )
        .expect("Failed to write kiln.toml");
        fs::write(
            crucible_dir.join("workspace.toml"),
            "[workspace]\nname = \"Workspace Fallback\"\n",
        )
        .expect("Failed to write workspace.toml");

        let config = read_kiln_config(temp.path()).expect("Expected kiln config");
        assert_eq!(config.kiln.name, "Kiln First");
    }

    #[test]
    fn read_kiln_config_falls_back_to_workspace_toml() {
        let temp = TempDir::new().expect("Failed to create temp dir");
        let crucible_dir = temp.path().join(".crucible");
        fs::create_dir_all(&crucible_dir).expect("Failed to create .crucible");

        fs::write(
            crucible_dir.join("workspace.toml"),
            "[workspace]\nname = \"Workspace Fallback\"\n",
        )
        .expect("Failed to write workspace.toml");

        let config = read_kiln_config(temp.path()).expect("Expected workspace fallback config");
        assert_eq!(config.kiln.name, "Workspace Fallback");
    }

    #[test]
    fn read_kiln_config_returns_none_when_no_file() {
        let temp = TempDir::new().expect("Failed to create temp dir");
        assert!(read_kiln_config(temp.path()).is_none());
    }
}
