//! Declarative interaction protocol
//!
//! Defines request/response primitives for agent-user interaction.
//! Renderer-agnostic - can be rendered in TUI, web, or via FFI.

/// A question to ask the user
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskRequest {
    pub question: String,
    pub choices: Option<Vec<String>>,
    pub multi_select: bool,
    pub allow_other: bool,
}

impl AskRequest {
    pub fn new(question: impl Into<String>) -> Self {
        Self {
            question: question.into(),
            choices: None,
            multi_select: false,
            allow_other: false,
        }
    }

    pub fn choices<I, S>(mut self, choices: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.choices = Some(choices.into_iter().map(Into::into).collect());
        self
    }

    pub fn multi_select(mut self) -> Self {
        self.multi_select = true;
        self
    }

    pub fn allow_other(mut self) -> Self {
        self.allow_other = true;
        self
    }
}

/// Response to an AskRequest
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskResponse {
    pub selected: Vec<usize>,
    pub other: Option<String>,
}

impl AskResponse {
    pub fn selected(index: usize) -> Self {
        Self {
            selected: vec![index],
            other: None,
        }
    }

    pub fn selected_many<I: IntoIterator<Item = usize>>(indices: I) -> Self {
        Self {
            selected: indices.into_iter().collect(),
            other: None,
        }
    }

    pub fn other(text: impl Into<String>) -> Self {
        Self {
            selected: Vec::new(),
            other: Some(text.into()),
        }
    }
}

/// Scope for permission grants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionScope {
    #[default]
    Once,
    Session,
    Project,
    User,
}

/// Types of permission requests
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermAction {
    Bash {
        tokens: Vec<String>,
    },
    Read {
        segments: Vec<String>,
    },
    Write {
        segments: Vec<String>,
    },
    Tool {
        name: String,
        args: serde_json::Value,
    },
}

/// Request permission for an action
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermRequest {
    pub action: PermAction,
}

impl PermRequest {
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

    /// Get tokens/segments for pattern building
    pub fn tokens(&self) -> &[String] {
        match &self.action {
            PermAction::Bash { tokens } => tokens,
            PermAction::Read { segments } | PermAction::Write { segments } => segments,
            PermAction::Tool { .. } => &[],
        }
    }

    /// Build a pattern from token boundary
    /// boundary=0 means "*" (any), boundary=len means exact match
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

/// Response to a permission request
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermResponse {
    pub allowed: bool,
    pub pattern: Option<String>,
    pub scope: PermissionScope,
}

impl PermResponse {
    pub fn allow() -> Self {
        Self {
            allowed: true,
            pattern: None,
            scope: PermissionScope::Once,
        }
    }

    pub fn deny() -> Self {
        Self {
            allowed: false,
            pattern: None,
            scope: PermissionScope::Once,
        }
    }

    pub fn allow_pattern(pattern: impl Into<String>, scope: PermissionScope) -> Self {
        Self {
            allowed: true,
            pattern: Some(pattern.into()),
            scope,
        }
    }
}

/// Format for artifacts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArtifactFormat {
    #[default]
    Markdown,
    Code,
    Json,
    Plain,
}

/// Request to edit an artifact
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditRequest {
    pub content: String,
    pub format: ArtifactFormat,
    pub hint: Option<String>,
}

impl EditRequest {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            format: ArtifactFormat::default(),
            hint: None,
        }
    }

    pub fn format(mut self, format: ArtifactFormat) -> Self {
        self.format = format;
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Response from editing - includes the modified content
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditResponse {
    pub modified: String,
}

/// Request to show content (no response needed)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowRequest {
    pub content: String,
    pub format: ArtifactFormat,
    pub title: Option<String>,
}

impl ShowRequest {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            format: ArtifactFormat::default(),
            title: None,
        }
    }

    pub fn format(mut self, format: ArtifactFormat) -> Self {
        self.format = format;
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// Unified interaction request type
#[derive(Debug, Clone, PartialEq)]
pub enum InteractionRequest {
    Ask(AskRequest),
    Edit(EditRequest),
    Show(ShowRequest),
    Permission(PermRequest),
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

#[cfg(test)]
mod tests {
    use super::*;

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

    // Permission tests

    #[test]
    fn perm_request_bash_tokens() {
        let req = PermRequest::bash(["npm", "install", "lodash"]);

        assert_eq!(req.tokens(), &["npm", "install", "lodash"]);
    }

    #[test]
    fn perm_request_pattern_at_boundary() {
        let req = PermRequest::bash(["npm", "install", "lodash"]);

        // boundary=0: allow anything
        assert_eq!(req.pattern_at(0), "*");
        // boundary=1: "npm *"
        assert_eq!(req.pattern_at(1), "npm *");
        // boundary=2: "npm install *"
        assert_eq!(req.pattern_at(2), "npm install *");
        // boundary=3 (full): exact match
        assert_eq!(req.pattern_at(3), "npm install lodash");
        // boundary > len: still exact
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

    // Edit/Show tests

    #[test]
    fn edit_request_with_format() {
        let edit = EditRequest::new("# Plan\n\n1. Do thing")
            .format(ArtifactFormat::Markdown)
            .hint("Focus on the steps");

        assert_eq!(edit.format, ArtifactFormat::Markdown);
        assert_eq!(edit.hint, Some("Focus on the steps".into()));
    }

    #[test]
    fn show_request_with_title() {
        let show = ShowRequest::new("Some content")
            .title("Important Notice")
            .format(ArtifactFormat::Plain);

        assert_eq!(show.title, Some("Important Notice".into()));
        assert_eq!(show.format, ArtifactFormat::Plain);
    }

    #[test]
    fn interaction_request_from_ask() {
        let ask = AskRequest::new("Question?");
        let req: InteractionRequest = ask.into();

        assert!(matches!(req, InteractionRequest::Ask(_)));
    }

    #[test]
    fn interaction_request_from_permission() {
        let perm = PermRequest::bash(["npm", "install"]);
        let req: InteractionRequest = perm.into();

        assert!(matches!(req, InteractionRequest::Permission(_)));
    }
}
