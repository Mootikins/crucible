//! Permission-related interaction types.
//!
//! Types for requesting and granting permissions for actions like
//! bash commands, file operations, and tool invocations.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::types::acp::FileDiff;
use crate::types::CanonicalToolCall;

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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    /// The canonical call, with its render, its agent and its raw tool
    /// name, so the prompt shows everything that is known. Its `diffs` are
    /// empty: the request holds them once, in `diffs`. `None` for a request
    /// that is not about a tool call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<Box<CanonicalToolCall>>,

    /// The permission layer that asked the user, for example
    /// `permissions config` or `ask mode`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// Plugin whose turn requested this permission, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
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
            call: None,
            layer: None,
            plugin: None,
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
            call: None,
            layer: None,
            plugin: None,
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
            call: None,
            layer: None,
            plugin: None,
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
            call: None,
            layer: None,
            plugin: None,
        }
    }

    /// The prompt for the tool call `call` with the JSON arguments `args`.
    ///
    /// The one builder for every gate: Crucible's own tools and the calls of
    /// an ACP agent get the same prompt. The request holds the diffs of the
    /// call once, in `diffs`, and the call with no diffs.
    pub fn from_call(call: &CanonicalToolCall, args: JsonValue) -> Self {
        Self {
            action: PermAction::Tool {
                name: call.tool.clone(),
                args,
            },
            diffs: call.diffs.clone(),
            call: Some(Box::new(CanonicalToolCall {
                diffs: Vec::new(),
                ..call.clone()
            })),
            layer: None,
            plugin: None,
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

    /// The grant that "always allow" saves for this request, or `None` when
    /// no grant can name the call.
    ///
    /// The suggestion reads the canonical call, the same call that the
    /// daemon stores the grant for and checks it against:
    ///
    /// - a command: the command line;
    /// - a file edit: its one path;
    /// - another call: its exact tool name, never a prefix such as `mcp_*`.
    ///   The user who wants `mcp__github__*` types the `*`.
    ///
    /// A command that Crucible cannot read, an edit of no path or of several
    /// paths, and a call that nothing names (its tool is its kind) get
    /// `None`. A grant for them would cover other calls too.
    ///
    /// A suggestion is the *default* grant, so it may never be wider than the
    /// action the modal displayed. The first token alone was wider: the user
    /// read `rm build/tmp.o` and the offered grant was `rm *`, which covers
    /// `rm -rf /home/user/project` on every project, for as long as the store
    /// file lives. See [`crate::config::PatternStore::matches_bash`].
    pub fn suggested_pattern(&self) -> Option<String> {
        let derived;
        let call = match (&self.call, &self.action) {
            (Some(call), _) => call.as_ref(),
            (None, PermAction::Tool { name, args }) => {
                derived = CanonicalToolCall::crucible_tool(name, args);
                &derived
            }
            (None, PermAction::Bash { tokens }) => {
                return Some(tokens.join(" "))
                    .filter(|c| !c.is_empty())
                    .map(|c| Self::bash_suggestion(&c))
            }
            (None, PermAction::Read { segments } | PermAction::Write { segments }) => {
                return Some(segments.join("/")).filter(|p| !p.is_empty())
            }
        };
        match (call.kind.as_str(), &call.command) {
            ("command", Some(command)) => Some(Self::bash_suggestion(command)),
            ("command", None) => None,
            ("file_edit", _) => match call.paths.as_slice() {
                [path] => Some(path.clone()),
                _ => None,
            },
            _ if call.tool == call.kind => None,
            _ => Some(call.tool.clone()),
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

    /// The one prompt builder holds the diffs once, carries the call with
    /// no diffs, and names the canonical tool. A command of Crucible's shell
    /// and a command of an agent get the same kind of action.
    #[test]
    fn from_call_holds_the_diffs_once_and_names_the_canonical_tool() {
        let args = serde_json::json!({"path": "a.md", "content": "x"});
        let call = CanonicalToolCall {
            diffs: vec![FileDiff {
                path: "a.md".to_string(),
                old_content: None,
                new_content: "x".to_string(),
            }],
            ..CanonicalToolCall::crucible_tool("write_file", &args)
        };
        let request = PermRequest::from_call(&call, args.clone());
        assert_eq!(
            request.action,
            PermAction::Tool {
                name: "write_file".to_string(),
                args
            }
        );
        assert_eq!(request.diffs, call.diffs);
        let held = request.call.expect("the request carries the call");
        assert!(held.diffs.is_empty(), "the request holds the diffs once");

        let command = serde_json::json!({"command": "cargo test"});
        let shell = CanonicalToolCall::crucible_tool("bash", &command);
        let agent = CanonicalToolCall {
            tool: "command".to_string(),
            ..shell.clone()
        };
        for call in [shell, agent] {
            let request = PermRequest::from_call(&call, command.clone());
            assert!(matches!(request.action, PermAction::Tool { .. }));
            assert_eq!(request.suggested_pattern().as_deref(), Some("cargo test"));
        }
    }

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
            let suggestion = request.suggested_pattern().expect("a command has a grant");
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
