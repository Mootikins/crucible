use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
