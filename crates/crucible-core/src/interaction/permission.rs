//! Permission-related interaction types.
//!
//! Types for requesting and granting permissions for actions like
//! bash commands, file operations, and tool invocations.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::types::acp::FileDiff;
use crate::types::{ToolDisplay, ToolDisplayKind};

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

    /// File diffs for this permission. Empty for non-file actions or when
    /// the diff couldn't be synthesized at the daemon (e.g. file too large
    /// or `old_string` not found in disk content).
    ///
    /// `#[serde(default)]` keeps older clients/servers wire-compatible:
    /// requests without this field deserialize as `vec![]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diffs: Vec<FileDiff>,
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
            diffs: Vec::new(),
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
            diffs: Vec::new(),
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
            diffs: Vec::new(),
        }
    }

    /// Create a tool permission request.
    pub fn tool(name: impl Into<String>, args: JsonValue) -> Self {
        Self {
            action: PermAction::Tool {
                name: name.into(),
                args,
            },
            diffs: Vec::new(),
        }
    }

    /// Builder: attach a set of file diffs to this permission.
    #[must_use]
    pub fn with_diffs(mut self, diffs: Vec<FileDiff>) -> Self {
        self.diffs = diffs;
        self
    }

    pub fn tokens(&self) -> &[String] {
        match &self.action {
            PermAction::Bash { tokens } => tokens,
            PermAction::Read { segments } | PermAction::Write { segments } => segments,
            PermAction::Tool { .. } => &[],
        }
    }

    /// Suggested pattern for allowlisting this request.
    ///
    /// For bash: the command line itself (e.g., `cargo build --release`).
    /// For file ops: directory prefix (e.g., `src/`).
    /// For tools: tool name, or MCP prefix + `*` (e.g., `fs_*`).
    ///
    /// A suggestion is the *default* grant, so it may never be wider than the
    /// action the modal displayed. The first token alone was wider: the user
    /// read `rm build/tmp.o` and the offered grant was `rm *`, which covers
    /// `rm -rf /home/user/project` on every project, for as long as the store
    /// file lives. A shell tool call answers with its command for the same
    /// reason — the tool name `bash` is a bash pattern, and as a prefix it
    /// read `bash -c <anything>`. The user who wants a wider grant edits the
    /// suggestion and types the `*`; see
    /// [`crate::config::PatternStore::matches_bash`].
    pub fn suggested_pattern(&self) -> String {
        match &self.action {
            PermAction::Bash { tokens } => {
                if tokens.is_empty() {
                    "*".to_string()
                } else {
                    Self::bash_suggestion(&tokens.join(" "))
                }
            }
            PermAction::Read { segments } | PermAction::Write { segments } => {
                if segments.is_empty() {
                    "*".to_string()
                } else {
                    format!("{}/", segments[0])
                }
            }
            PermAction::Tool { name, args } => {
                let display = ToolDisplay::of(name, args);
                match (display.kind, display.primary) {
                    (ToolDisplayKind::Command, Some(command)) => Self::bash_suggestion(&command),
                    _ => match name.find('_') {
                        Some(prefix_end) => format!("{}_*", &name[..prefix_end]),
                        None => name.clone(),
                    },
                }
            }
        }
    }

    /// The bash rule to suggest for the command line `command`.
    ///
    /// The suggestion drops a trailing wildcard, because a trailing `*` is
    /// what widens a stored rule and only the user may type it. The shell
    /// expands `rm *` before `rm` runs, so the user reads a command that
    /// acts on the files of one directory; the same text stored as a rule
    /// reads `rm <anything>`, which the user never approved and which
    /// [`crate::config::PatternStore::load_file`] refuses when it reads the
    /// store back.
    /// A suggestion that is narrower than the command costs one more prompt.
    /// A suggestion that is wider costs the project.
    fn bash_suggestion(command: &str) -> String {
        let narrowed = command.trim_end_matches(|c: char| c == '*' || c.is_whitespace());
        if narrowed.is_empty() {
            // A command line of only globs has no rule that means it. Keep
            // the pattern the store refuses, so the click reports the refusal
            // instead of saving a grant the user cannot read.
            "*".to_string()
        } else {
            narrowed.to_string()
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

    /// A `*` is what widens a stored bash rule, so the suggestion never ends
    /// with one - not even when the command the model ran ends with a glob.
    /// The shell expands `rm *` before `rm` sees it, so the user reads a
    /// command that acts on the files in one directory; the same text stored
    /// as a rule reads `rm <anything>`, which is not what the user approved.
    #[test]
    fn a_bash_suggestion_never_ends_with_a_wildcard() {
        use crate::config::PatternStore;

        let requests = [
            PermRequest::bash(["rm", "*"]),
            PermRequest::bash(["git", "add", "*"]),
            PermRequest::tool("bash", serde_json::json!({"command": "rm *"})),
            PermRequest::tool("bash", serde_json::json!({"command": "git add *"})),
        ];

        for request in requests {
            let suggestion = request.suggested_pattern();
            assert!(
                !suggestion.ends_with('*'),
                "suggested {suggestion:?} for {request:?}"
            );

            // The prompt never suggests a rule the loader refuses, so the
            // suggestion survives the write and the read.
            let mut store = PatternStore::new();
            store.add_bash_pattern(&suggestion).unwrap();
            let dir = tempfile::TempDir::new().unwrap();
            let file = PatternStore::user_file_in(&dir.path().join("whitelists.d"));
            store.save_file(&file).unwrap();
            assert_eq!(
                PatternStore::load_file(&file).unwrap(),
                store,
                "the loader refused the suggestion {suggestion:?}"
            );

            // And it never reaches a command the user did not read.
            assert!(
                !store.matches_bash("rm -rf /home/u/proj"),
                "the suggestion {suggestion:?} reaches an unapproved command"
            );
        }
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
