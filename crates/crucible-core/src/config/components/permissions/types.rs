use serde::{Deserialize, Serialize};

/// Scope for writing permission rules to config files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionScope {
    /// Project-level config: `crucible.toml` in the project directory.
    Project,
    /// User-level config: `~/.config/crucible/config.toml` (or platform equivalent).
    User,
}

/// Permission mode for tool access control
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionMode {
    /// Allow tool execution
    Allow,
    /// Deny tool execution
    Deny,
    /// Ask user for permission
    #[default]
    Ask,
}

impl std::str::FromStr for PermissionMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "allow" => Ok(PermissionMode::Allow),
            "deny" => Ok(PermissionMode::Deny),
            "ask" => Ok(PermissionMode::Ask),
            other => Err(format!(
                "Invalid permission mode: '{}'. Must be allow, deny, or ask",
                other
            )),
        }
    }
}

impl std::fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionMode::Allow => write!(f, "allow"),
            PermissionMode::Deny => write!(f, "deny"),
            PermissionMode::Ask => write!(f, "ask"),
        }
    }
}

/// Parsed permission rule with tool, optional server, and pattern
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRule {
    /// Tool name (bash, read, edit, write, delete, mcp, plugin, *)
    pub tool: String,
    /// Server name for MCP/plugin rules (e.g., "github" in "mcp:github:*")
    pub server: Option<String>,
    /// Pattern for matching (e.g., "cargo test *", "src/**")
    pub pattern: String,
}

/// Permission configuration for tool access control
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionConfig {
    /// Default permission mode when no rule matches
    #[serde(default)]
    pub default: PermissionMode,
    /// Rules that allow tool execution
    #[serde(default)]
    pub allow: Vec<String>,
    /// Rules that deny tool execution
    #[serde(default)]
    pub deny: Vec<String>,
    /// Rules that ask user for permission
    #[serde(default)]
    pub ask: Vec<String>,
}

impl Default for PermissionConfig {
    fn default() -> Self {
        Self {
            default: PermissionMode::Ask,
            allow: Vec::new(),
            deny: Vec::new(),
            ask: Vec::new(),
        }
    }
}

impl PermissionConfig {
    /// Create a new permission configuration with default settings
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum PermissionDecision {
    Allow,
    Deny { reason: String },
    Ask,
}
