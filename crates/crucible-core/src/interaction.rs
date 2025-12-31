//! Interaction protocol primitives for agent-user communication.
//!
//! This module defines request/response types for structured interactions between
//! agents and users. These primitives are renderer-agnostic and can be used by
//! TUI, web, or other frontends.
//!
//! # Request Types
//!
//! - [`AskRequest`] - Questions with optional choices (single/multi-select)
//! - [`PermRequest`] - Permission requests with token-based pattern building
//! - [`EditRequest`] - Artifact editing with format hints
//! - [`ShowRequest`] - Display content (no response needed)
//!
//! # Example
//!
//! ```
//! use crucible_core::interaction::{AskRequest, AskResponse, PermRequest, PermissionScope};
//!
//! // Create a question with choices
//! let ask = AskRequest::new("Which option?")
//!     .choices(["Option A", "Option B", "Option C"])
//!     .allow_other();
//!
//! // Create a permission request
//! let perm = PermRequest::bash(["npm", "install", "lodash"]);
//! assert_eq!(perm.pattern_at(2), "npm install *");
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ─────────────────────────────────────────────────────────────────────────────
// Ask Request/Response
// ─────────────────────────────────────────────────────────────────────────────

/// A question to ask the user.
///
/// Supports single-select, multi-select, and free-text input modes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskRequest {
    /// The question text to display.
    pub question: String,
    /// Optional list of choices. If None, expects free-text input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<String>>,
    /// Allow selecting multiple choices.
    #[serde(default)]
    pub multi_select: bool,
    /// Allow free-text input in addition to choices.
    #[serde(default)]
    pub allow_other: bool,
}

impl AskRequest {
    /// Create a new question.
    pub fn new(question: impl Into<String>) -> Self {
        Self {
            question: question.into(),
            choices: None,
            multi_select: false,
            allow_other: false,
        }
    }

    /// Add choices to the question.
    pub fn choices<I, S>(mut self, choices: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.choices = Some(choices.into_iter().map(Into::into).collect());
        self
    }

    /// Enable multi-select mode.
    pub fn multi_select(mut self) -> Self {
        self.multi_select = true;
        self
    }

    /// Allow free-text "other" input.
    pub fn allow_other(mut self) -> Self {
        self.allow_other = true;
        self
    }
}

/// Response to an [`AskRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskResponse {
    /// Indices of selected choices (empty if using "other").
    #[serde(default)]
    pub selected: Vec<usize>,
    /// Free-text input if "other" was chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
}

impl AskResponse {
    /// Create a response with a single selection.
    pub fn selected(index: usize) -> Self {
        Self {
            selected: vec![index],
            other: None,
        }
    }

    /// Create a response with multiple selections.
    pub fn selected_many<I: IntoIterator<Item = usize>>(indices: I) -> Self {
        Self {
            selected: indices.into_iter().collect(),
            other: None,
        }
    }

    /// Create a response with free-text input.
    pub fn other(text: impl Into<String>) -> Self {
        Self {
            selected: Vec::new(),
            other: Some(text.into()),
        }
    }
}

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

    /// Get tokens/segments for pattern building.
    pub fn tokens(&self) -> &[String] {
        match &self.action {
            PermAction::Bash { tokens } => tokens,
            PermAction::Read { segments } | PermAction::Write { segments } => segments,
            PermAction::Tool { .. } => &[],
        }
    }

    /// Build a pattern from token boundary.
    ///
    /// - `boundary=0` → `"*"` (allow anything)
    /// - `boundary=1` → `"npm *"` (allow npm with any args)
    /// - `boundary=len` → exact match
    ///
    /// This enables vim-motion-style UIs where users can use `h`/`l` to
    /// expand or contract the permission scope.
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
    /// Whether permission was granted.
    pub allowed: bool,
    /// Optional pattern for the grant (e.g., "npm install *").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// Scope of the permission grant.
    #[serde(default)]
    pub scope: PermissionScope,
}

impl PermResponse {
    /// Create an allow-once response.
    pub fn allow() -> Self {
        Self {
            allowed: true,
            pattern: None,
            scope: PermissionScope::Once,
        }
    }

    /// Create a deny response.
    pub fn deny() -> Self {
        Self {
            allowed: false,
            pattern: None,
            scope: PermissionScope::Once,
        }
    }

    /// Create an allow response with a pattern and scope.
    pub fn allow_pattern(pattern: impl Into<String>, scope: PermissionScope) -> Self {
        Self {
            allowed: true,
            pattern: Some(pattern.into()),
            scope,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Edit Request/Response
// ─────────────────────────────────────────────────────────────────────────────

/// Format hint for artifact content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactFormat {
    /// Markdown content.
    #[default]
    Markdown,
    /// Source code.
    Code,
    /// JSON data.
    Json,
    /// Plain text.
    Plain,
}

/// Request to edit an artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditRequest {
    /// The content to edit.
    pub content: String,
    /// Format hint for the editor.
    #[serde(default)]
    pub format: ArtifactFormat,
    /// Optional hint for what to focus on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl EditRequest {
    /// Create a new edit request.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            format: ArtifactFormat::default(),
            hint: None,
        }
    }

    /// Set the format hint.
    pub fn format(mut self, format: ArtifactFormat) -> Self {
        self.format = format;
        self
    }

    /// Set the editing hint.
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Response from editing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditResponse {
    /// The modified content.
    pub modified: String,
}

impl EditResponse {
    /// Create a new edit response.
    pub fn new(modified: impl Into<String>) -> Self {
        Self {
            modified: modified.into(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Show Request (no response)
// ─────────────────────────────────────────────────────────────────────────────

/// Request to show content to the user (display only, no response).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShowRequest {
    /// The content to display.
    pub content: String,
    /// Format hint for rendering.
    #[serde(default)]
    pub format: ArtifactFormat,
    /// Optional title for the display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl ShowRequest {
    /// Create a new show request.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            format: ArtifactFormat::default(),
            title: None,
        }
    }

    /// Set the format hint.
    pub fn format(mut self, format: ArtifactFormat) -> Self {
        self.format = format;
        self
    }

    /// Set the title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unified Request/Response Enums
// ─────────────────────────────────────────────────────────────────────────────

/// Unified interaction request type.
///
/// This enum wraps all interaction primitives for use in event systems
/// and channels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InteractionRequest {
    /// Question with optional choices.
    Ask(AskRequest),
    /// Artifact editing.
    Edit(EditRequest),
    /// Display content (no response).
    Show(ShowRequest),
    /// Permission request.
    Permission(PermRequest),
}

impl InteractionRequest {
    /// Get the request kind for pattern matching.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Ask(_) => "ask",
            Self::Edit(_) => "edit",
            Self::Show(_) => "show",
            Self::Permission(_) => "permission",
        }
    }

    /// Check if this request expects a response.
    pub fn expects_response(&self) -> bool {
        !matches!(self, Self::Show(_))
    }
}

impl From<AskRequest> for InteractionRequest {
    fn from(req: AskRequest) -> Self {
        InteractionRequest::Ask(req)
    }
}

impl From<EditRequest> for InteractionRequest {
    fn from(req: EditRequest) -> Self {
        InteractionRequest::Edit(req)
    }
}

impl From<ShowRequest> for InteractionRequest {
    fn from(req: ShowRequest) -> Self {
        InteractionRequest::Show(req)
    }
}

impl From<PermRequest> for InteractionRequest {
    fn from(req: PermRequest) -> Self {
        InteractionRequest::Permission(req)
    }
}

/// Unified interaction response type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InteractionResponse {
    /// Response to an ask request.
    Ask(AskResponse),
    /// Response to an edit request.
    Edit(EditResponse),
    /// Response to a permission request.
    Permission(PermResponse),
    /// Request was cancelled by the user.
    Cancelled,
}

impl From<AskResponse> for InteractionResponse {
    fn from(resp: AskResponse) -> Self {
        InteractionResponse::Ask(resp)
    }
}

impl From<EditResponse> for InteractionResponse {
    fn from(resp: EditResponse) -> Self {
        InteractionResponse::Edit(resp)
    }
}

impl From<PermResponse> for InteractionResponse {
    fn from(resp: PermResponse) -> Self {
        InteractionResponse::Permission(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─────────────────────────────────────────────────────────────────────────
    // AskRequest tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn ask_request_with_choices() {
        let ask = AskRequest::new("Which option?").choices(["Option A", "Option B", "Option C"]);

        assert_eq!(ask.question, "Which option?");
        assert_eq!(
            ask.choices,
            Some(vec![
                "Option A".into(),
                "Option B".into(),
                "Option C".into()
            ])
        );
        assert!(!ask.multi_select);
        assert!(!ask.allow_other);
    }

    #[test]
    fn ask_request_multi_select() {
        let ask = AskRequest::new("Select all that apply")
            .choices(["A", "B", "C"])
            .multi_select();

        assert!(ask.multi_select);
    }

    #[test]
    fn ask_request_allows_free_text() {
        let ask = AskRequest::new("Pick or type custom")
            .choices(["Preset 1", "Preset 2"])
            .allow_other();

        assert!(ask.allow_other);
    }

    #[test]
    fn ask_response_single_selection() {
        let response = AskResponse::selected(1);

        assert_eq!(response.selected, vec![1]);
        assert!(response.other.is_none());
    }

    #[test]
    fn ask_response_multi_selection() {
        let response = AskResponse::selected_many([0, 2]);

        assert_eq!(response.selected, vec![0, 2]);
    }

    #[test]
    fn ask_response_custom_text() {
        let response = AskResponse::other("Custom input");

        assert!(response.selected.is_empty());
        assert_eq!(response.other, Some("Custom input".into()));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // PermRequest tests
    // ─────────────────────────────────────────────────────────────────────────

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

    // ─────────────────────────────────────────────────────────────────────────
    // EditRequest tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn edit_request_with_format() {
        let edit = EditRequest::new("# Plan\n\n1. Do thing")
            .format(ArtifactFormat::Markdown)
            .hint("Focus on the steps");

        assert_eq!(edit.format, ArtifactFormat::Markdown);
        assert_eq!(edit.hint, Some("Focus on the steps".into()));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // ShowRequest tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn show_request_with_title() {
        let show = ShowRequest::new("Some content")
            .title("Important Notice")
            .format(ArtifactFormat::Plain);

        assert_eq!(show.title, Some("Important Notice".into()));
        assert_eq!(show.format, ArtifactFormat::Plain);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // InteractionRequest tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn interaction_request_from_ask() {
        let ask = AskRequest::new("Question?");
        let req: InteractionRequest = ask.into();

        assert!(matches!(req, InteractionRequest::Ask(_)));
        assert_eq!(req.kind(), "ask");
        assert!(req.expects_response());
    }

    #[test]
    fn interaction_request_from_permission() {
        let perm = PermRequest::bash(["npm", "install"]);
        let req: InteractionRequest = perm.into();

        assert!(matches!(req, InteractionRequest::Permission(_)));
        assert_eq!(req.kind(), "permission");
        assert!(req.expects_response());
    }

    #[test]
    fn interaction_request_show_no_response() {
        let show = ShowRequest::new("Display this");
        let req: InteractionRequest = show.into();

        assert!(matches!(req, InteractionRequest::Show(_)));
        assert_eq!(req.kind(), "show");
        assert!(!req.expects_response());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Serialization tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn ask_request_serialization() {
        let ask = AskRequest::new("Question?").choices(["A", "B"]);
        let json = serde_json::to_string(&ask).unwrap();
        let restored: AskRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(ask, restored);
    }

    #[test]
    fn perm_request_serialization() {
        let perm = PermRequest::bash(["npm", "install"]);
        let json = serde_json::to_string(&perm).unwrap();
        let restored: PermRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(perm, restored);
    }

    #[test]
    fn interaction_request_serialization() {
        let req = InteractionRequest::Ask(AskRequest::new("Test?"));
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"kind\":\"ask\""));
        let restored: InteractionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, restored);
    }

    #[test]
    fn interaction_response_cancelled() {
        let resp = InteractionResponse::Cancelled;
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"kind\":\"cancelled\""));
        let restored: InteractionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, restored);
    }
}
