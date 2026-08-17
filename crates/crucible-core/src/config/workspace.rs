//! Shared configuration components for kiln attachments and security overrides.
//!
//! These types are embedded in [`ProjectConfig`](crate::config::ProjectConfig),
//! which is what `.crucible/project.toml` (and the legacy `workspace.toml`
//! fallback) deserializes into.

use crate::config::components::DataClassification;
use crate::config::security::{ProjectFileAccess, ShellPolicy};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    /// Optional data classification for the kiln
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_classification: Option<DataClassification>,
}

impl KilnAttachment {
    /// Get the effective data classification for this kiln.
    ///
    /// Returns the configured classification, or `DataClassification::Public` if not set.
    pub fn effective_classification(&self) -> DataClassification {
        self.data_classification
            .unwrap_or(DataClassification::Public)
    }
}

/// Security configuration for workspace
///
/// Allows workspace-level security policy overrides.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct SecurityConfig {
    /// Shell command execution policy
    pub shell: ShellPolicy,
    /// Web UI access to project files outside any kiln. Defaults to
    /// read-write; see [`ProjectFileAccess`].
    pub project_files: ProjectFileAccess,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kiln_attachment_parses_with_name() {
        let attachment: KilnAttachment =
            toml::from_str("path = \"./notes\"\nname = \"Main Notes\"\n")
                .expect("Failed to parse");

        assert_eq!(attachment.path, PathBuf::from("./notes"));
        assert_eq!(attachment.name, Some("Main Notes".to_string()));
    }

    #[test]
    fn kiln_attachment_without_name() {
        let attachment: KilnAttachment =
            toml::from_str("path = \"./docs\"\n").expect("Failed to parse");

        assert_eq!(attachment.path, PathBuf::from("./docs"));
        assert_eq!(attachment.name, None);
    }

    #[test]
    fn security_config_parses_shell_policy() {
        let toml = r#"
[shell]
whitelist = ["git", "cargo"]
blacklist = ["rm -rf"]
"#;

        let security: SecurityConfig = toml::from_str(toml).expect("Failed to parse");

        assert_eq!(security.shell.whitelist, vec!["git", "cargo"]);
        assert_eq!(security.shell.blacklist, vec!["rm -rf"]);
    }

    #[test]
    fn security_config_defaults_to_empty() {
        let security: SecurityConfig = toml::from_str("").expect("Failed to parse");

        // Default shell policy should be empty (deny-all).
        assert_eq!(security.shell.whitelist.len(), 0);
        assert_eq!(security.shell.blacklist.len(), 0);
    }

    #[test]
    fn kiln_attachment_with_data_classification_confidential() {
        let toml = r#"
path = "./docs"
data_classification = "confidential"
"#;

        let attachment: KilnAttachment = toml::from_str(toml).expect("Failed to parse");

        assert_eq!(attachment.path, PathBuf::from("./docs"));
        assert_eq!(
            attachment.data_classification,
            Some(DataClassification::Confidential)
        );
        assert_eq!(
            attachment.effective_classification(),
            DataClassification::Confidential
        );
    }

    #[test]
    fn kiln_attachment_with_data_classification_internal() {
        let toml = r#"
path = "./docs"
data_classification = "internal"
"#;

        let attachment: KilnAttachment = toml::from_str(toml).expect("Failed to parse");

        assert_eq!(attachment.path, PathBuf::from("./docs"));
        assert_eq!(
            attachment.data_classification,
            Some(DataClassification::Internal)
        );
        assert_eq!(
            attachment.effective_classification(),
            DataClassification::Internal
        );
    }

    #[test]
    fn kiln_attachment_without_data_classification_defaults_to_public() {
        let attachment: KilnAttachment =
            toml::from_str("path = \"./docs\"\n").expect("Failed to parse");

        assert_eq!(attachment.path, PathBuf::from("./docs"));
        assert_eq!(attachment.data_classification, None);
        assert_eq!(
            attachment.effective_classification(),
            DataClassification::Public
        );
    }

    #[test]
    fn kiln_attachment_roundtrip_with_classification() {
        let attachment = KilnAttachment {
            path: PathBuf::from("./notes"),
            name: Some("Notes".to_string()),
            data_classification: Some(DataClassification::Confidential),
        };

        let toml = toml::to_string(&attachment).expect("Failed to serialize");
        let parsed: KilnAttachment = toml::from_str(&toml).expect("Failed to re-parse");

        assert_eq!(attachment, parsed);
        assert_eq!(
            parsed.effective_classification(),
            DataClassification::Confidential
        );
    }
}
