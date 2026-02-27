//! Permission-related interaction types.
//!
//! Types for requesting and granting permissions for actions like
//! bash commands, file operations, and tool invocations.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ─────────────────────────────────────────────────────────────────────────────
// Permission Request/Response
// ─────────────────────────────────────────────────────────────────────────────

/// Scope for permission grants.
///
/// Determines how long a permission grant remains valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionScope {
    /// Grant permission for this single action only.
    #[default]
    Once,
    /// Grant permission for the current session.
    Session,
    /// Grant permission for the current project/kiln.
    Project,
    /// Grant permission permanently for this user.
    User,
}

/// Types of permission requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PermAction {
    /// Permission to execute a bash command.
    Bash {
        /// Command tokens (e.g., ["npm", "install", "lodash"]).
        tokens: Vec<String>,
    },
    /// Permission to read a file/directory.
    Read {
        /// Path segments (e.g., ["home", "user", "project"]).
        segments: Vec<String>,
    },
    /// Permission to write a file/directory.
    Write {
        /// Path segments.
        segments: Vec<String>,
    },
    /// Permission to call a tool.
    Tool {
        /// Tool name.
        name: String,
        /// Tool arguments.
        args: JsonValue,
    },
}

/// Request permission for an action.
///
/// Supports token-based pattern building for vim-style permission UIs
/// where users can expand/contract the permission scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermRequest {
    /// The action requiring permission.
    pub action: PermAction,
}

impl PermRequest {
    /// Create a bash permission request.
    pub fn bash<I, S>(tokens: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            action: PermAction::Bash {
                tokens: tokens.into_iter().map(Into::into).collect(),
            },
        }
    }

    /// Create a read permission request.
    pub fn read<I, S>(segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            action: PermAction::Read {
                segments: segments.into_iter().map(Into::into).collect(),
            },
        }
    }

    /// Create a write permission request.
    pub fn write<I, S>(segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            action: PermAction::Write {
                segments: segments.into_iter().map(Into::into).collect(),
            },
        }
    }

    /// Create a tool permission request.
    pub fn tool(name: impl Into<String>, args: JsonValue) -> Self {
        Self {
            action: PermAction::Tool {
                name: name.into(),
                args,
            },
        }
    }

    pub fn tokens(&self) -> &[String] {
        match &self.action {
            PermAction::Bash { tokens } => tokens,
            PermAction::Read { segments } | PermAction::Write { segments } => segments,
            PermAction::Tool { .. } => &[],
        }
    }

    /// Suggested pattern for allowlisting this request.
    /// For bash: first token + `*` (e.g., `cargo *`)
    /// For file ops: directory prefix (e.g., `src/`)
    /// For tools: tool name, or MCP prefix + `*` (e.g., `fs_*`)
    pub fn suggested_pattern(&self) -> String {
        match &self.action {
            PermAction::Bash { tokens } => {
                if tokens.is_empty() {
                    "*".to_string()
                } else {
                    format!("{} *", tokens[0])
                }
            }
            PermAction::Read { segments } | PermAction::Write { segments } => {
                if segments.is_empty() {
                    "*".to_string()
                } else {
                    format!("{}/", segments[0])
                }
            }
            PermAction::Tool { name, .. } => {
                if let Some(prefix_end) = name.find('_') {
                    format!("{}_*", &name[..prefix_end])
                } else {
                    name.clone()
                }
            }
        }
    }

    pub fn pattern_at(&self, boundary: usize) -> String {
        let tokens = self.tokens();
        if boundary == 0 {
            "*".to_string()
        } else if boundary >= tokens.len() {
            tokens.join(" ")
        } else {
            format!("{} *", tokens[..boundary].join(" "))
        }
    }
}

/// Response to a permission request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermResponse {
    pub allowed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default)]
    pub scope: PermissionScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl PermResponse {
    pub fn allow() -> Self {
        Self {
            allowed: true,
            pattern: None,
            scope: PermissionScope::Once,
            reason: None,
        }
    }

    pub fn deny() -> Self {
        Self {
            allowed: false,
            pattern: None,
            scope: PermissionScope::Once,
            reason: None,
        }
    }

    pub fn deny_with_reason(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            pattern: None,
            scope: PermissionScope::Once,
            reason: Some(reason.into()),
        }
    }

    pub fn allow_pattern(pattern: impl Into<String>, scope: PermissionScope) -> Self {
        Self {
            allowed: true,
            pattern: Some(pattern.into()),
            scope,
            reason: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perm_request_bash_tokens() {
        let req = PermRequest::bash(["npm", "install", "lodash"]);

        assert_eq!(req.tokens(), &["npm", "install", "lodash"]);
    }

    #[test]
    fn perm_request_pattern_at_boundary() {
        let req = PermRequest::bash(["npm", "install", "lodash"]);

        assert_eq!(req.pattern_at(0), "*");
        assert_eq!(req.pattern_at(1), "npm *");
        assert_eq!(req.pattern_at(2), "npm install *");
        assert_eq!(req.pattern_at(3), "npm install lodash");
        assert_eq!(req.pattern_at(100), "npm install lodash");
    }

    #[test]
    fn perm_request_read_segments() {
        let req = PermRequest::read(["home", "user", "project", "src"]);

        assert_eq!(req.pattern_at(2), "home user *");
    }

    #[test]
    fn perm_response_simple_allow() {
        let resp = PermResponse::allow();

        assert!(resp.allowed);
        assert!(resp.pattern.is_none());
        assert_eq!(resp.scope, PermissionScope::Once);
    }

    #[test]
    fn perm_response_pattern_with_scope() {
        let resp = PermResponse::allow_pattern("npm install *", PermissionScope::Session);

        assert!(resp.allowed);
        assert_eq!(resp.pattern, Some("npm install *".into()));
        assert_eq!(resp.scope, PermissionScope::Session);
    }

    #[test]
    fn perm_response_deny() {
        let resp = PermResponse::deny();

        assert!(!resp.allowed);
    }

    #[test]
    fn perm_request_serialization() {
        let perm = PermRequest::bash(["npm", "install"]);
        let json = serde_json::to_string(&perm).unwrap();
        let restored: PermRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(perm, restored);
    }
}
