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

use crate::types::PopupEntry;

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
// Batched Ask Request/Response (for multi-question interactions)
// ─────────────────────────────────────────────────────────────────────────────

/// A batch of questions to ask the user.
///
/// Supports 1-4 questions shown together. Each question has choices,
/// and an "Other" free-text option is always implicitly available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskBatch {
    /// Unique ID for correlating request with response.
    pub id: uuid::Uuid,
    /// Questions to ask (1-4).
    pub questions: Vec<AskQuestion>,
}

impl AskBatch {
    /// Create a new empty batch with a generated ID.
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            questions: Vec::new(),
        }
    }

    /// Create a batch with a specific ID.
    pub fn with_id(id: uuid::Uuid) -> Self {
        Self {
            id,
            questions: Vec::new(),
        }
    }

    /// Add a question to the batch.
    pub fn question(mut self, q: AskQuestion) -> Self {
        self.questions.push(q);
        self
    }
}

impl Default for AskBatch {
    fn default() -> Self {
        Self::new()
    }
}

/// A single question in an [`AskBatch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskQuestion {
    /// Short label (max 12 chars) displayed as header.
    pub header: String,
    /// Full question text.
    pub question: String,
    /// Available choices.
    pub choices: Vec<String>,
    /// Allow multiple selections.
    #[serde(default)]
    pub multi_select: bool,
    /// Allow free-text "other" input.
    #[serde(default)]
    pub allow_other: bool,
}

impl AskQuestion {
    /// Create a new question.
    pub fn new(header: impl Into<String>, question: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            question: question.into(),
            choices: Vec::new(),
            multi_select: false,
            allow_other: false,
        }
    }

    /// Add a choice.
    pub fn choice(mut self, c: impl Into<String>) -> Self {
        self.choices.push(c.into());
        self
    }

    /// Add multiple choices at once.
    pub fn choices<I, S>(mut self, choices: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.choices.extend(choices.into_iter().map(Into::into));
        self
    }

    /// Enable multi-select mode.
    pub fn multi_select(mut self) -> Self {
        self.multi_select = true;
        self
    }
}

/// Response to an [`AskBatch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskBatchResponse {
    /// The request ID this responds to.
    pub id: uuid::Uuid,
    /// One answer per question, in order.
    pub answers: Vec<QuestionAnswer>,
    /// True if user cancelled the whole interaction.
    #[serde(default)]
    pub cancelled: bool,
}

impl AskBatchResponse {
    /// Create a new response for a request ID.
    pub fn new(id: uuid::Uuid) -> Self {
        Self {
            id,
            answers: Vec::new(),
            cancelled: false,
        }
    }

    /// Add an answer.
    pub fn answer(mut self, a: QuestionAnswer) -> Self {
        self.answers.push(a);
        self
    }

    /// Mark as cancelled.
    pub fn cancelled(id: uuid::Uuid) -> Self {
        Self {
            id,
            answers: Vec::new(),
            cancelled: true,
        }
    }
}

/// Answer to a single question in an [`AskBatch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionAnswer {
    /// Selected choice indices (empty if "Other" was chosen).
    #[serde(default)]
    pub selected: Vec<usize>,
    /// Free-text input if "Other" was chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
}

impl QuestionAnswer {
    /// Create answer with a single choice selection.
    pub fn choice(index: usize) -> Self {
        Self {
            selected: vec![index],
            other: None,
        }
    }

    /// Create answer with multiple choice selections.
    pub fn choices<I: IntoIterator<Item = usize>>(indices: I) -> Self {
        Self {
            selected: indices.into_iter().collect(),
            other: None,
        }
    }

    /// Create answer with free-text "Other" input.
    pub fn other(text: impl Into<String>) -> Self {
        Self {
            selected: Vec::new(),
            other: Some(text.into()),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Popup Request/Response
// ─────────────────────────────────────────────────────────────────────────────

/// A request to show a popup with selectable entries.
///
/// Unlike [`AskRequest`] which uses simple string choices, `PopupRequest` uses
/// [`PopupEntry`] items that can include labels, descriptions, and arbitrary data.
/// This makes it suitable for rich scripted popups from Rune/Lua plugins.
///
/// # Example
///
/// ```
/// use crucible_core::interaction::PopupRequest;
/// use crucible_core::types::PopupEntry;
///
/// let popup = PopupRequest::new("Select a note")
///     .entries([
///         PopupEntry::new("Daily Note").with_description("Today's journal"),
///         PopupEntry::new("Todo List").with_description("Tasks for the week"),
///     ]);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopupRequest {
    /// Title/prompt to display above the popup.
    pub title: String,
    /// Entries to display in the popup.
    #[serde(default)]
    pub entries: Vec<PopupEntry>,
    /// Allow free-text input if no entry is selected.
    #[serde(default)]
    pub allow_other: bool,
}

impl PopupRequest {
    /// Create a new popup request with a title.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            entries: Vec::new(),
            allow_other: false,
        }
    }

    /// Set the popup entries.
    pub fn entries<I>(mut self, entries: I) -> Self
    where
        I: IntoIterator<Item = PopupEntry>,
    {
        self.entries = entries.into_iter().collect();
        self
    }

    /// Add a single entry to the popup.
    pub fn entry(mut self, entry: PopupEntry) -> Self {
        self.entries.push(entry);
        self
    }

    /// Allow free-text "other" input.
    pub fn allow_other(mut self) -> Self {
        self.allow_other = true;
        self
    }
}

/// Response to a [`PopupRequest`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopupResponse {
    /// Index of the selected entry (if any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_index: Option<usize>,
    /// The selected entry (if any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_entry: Option<PopupEntry>,
    /// Free-text input if "other" was chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
}

impl PopupResponse {
    /// Create a response with a selection.
    pub fn selected(index: usize, entry: PopupEntry) -> Self {
        Self {
            selected_index: Some(index),
            selected_entry: Some(entry),
            other: None,
        }
    }

    /// Create a response with a selection index only.
    pub fn selected_index(index: usize) -> Self {
        Self {
            selected_index: Some(index),
            selected_entry: None,
            other: None,
        }
    }

    /// Create a response with free-text input.
    pub fn other(text: impl Into<String>) -> Self {
        Self {
            selected_index: None,
            selected_entry: None,
            other: Some(text.into()),
        }
    }

    /// Create an empty response (popup dismissed without selection).
    pub fn none() -> Self {
        Self {
            selected_index: None,
            selected_entry: None,
            other: None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Interactive Panel (primitive for scripted UI flows)
// ─────────────────────────────────────────────────────────────────────────────

/// An item in an interactive panel.
///
/// Each item has a label, optional description, and optional arbitrary data
/// that can be returned when selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelItem {
    /// Display label for this item.
    pub label: String,
    /// Optional description shown below or beside the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Arbitrary data associated with this item (returned on selection).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl PanelItem {
    /// Create a new panel item with just a label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: None,
            data: None,
        }
    }

    /// Add a description to this item.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add arbitrary data to this item.
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// Render/behavior hints for an interactive panel.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelHints {
    /// Show filter/search input for fuzzy matching.
    #[serde(default)]
    pub filterable: bool,
    /// Allow selecting multiple items (toggle with space/tab).
    #[serde(default)]
    pub multi_select: bool,
    /// Show "Other..." option for free-text input.
    #[serde(default)]
    pub allow_other: bool,
    /// Pre-select these indices when panel opens.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initial_selection: Vec<usize>,
    /// Initial filter text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_filter: Option<String>,
}

impl PanelHints {
    /// Create default hints.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable filtering/search.
    pub fn filterable(mut self) -> Self {
        self.filterable = true;
        self
    }

    /// Enable multi-select mode.
    pub fn multi_select(mut self) -> Self {
        self.multi_select = true;
        self
    }

    /// Enable "Other..." free-text option.
    pub fn allow_other(mut self) -> Self {
        self.allow_other = true;
        self
    }

    /// Set initial selection.
    pub fn initial_selection<I: IntoIterator<Item = usize>>(mut self, indices: I) -> Self {
        self.initial_selection = indices.into_iter().collect();
        self
    }
}

/// An interactive panel request.
///
/// This is the core primitive for scripted UI flows. Scripts provide items
/// and hints; the TUI renders an interactive list with filtering, selection,
/// and optional key handler callbacks.
///
/// Higher-level patterns (question sequences, fuzzy search, wizards) are
/// built on top of this primitive in script-land.
///
/// # Example
///
/// ```
/// use crucible_core::interaction::{InteractivePanel, PanelItem, PanelHints};
///
/// let panel = InteractivePanel::new("Select database")
///     .item(PanelItem::new("PostgreSQL").with_description("Full-featured RDBMS"))
///     .item(PanelItem::new("SQLite").with_description("Embedded, single-file"))
///     .hints(PanelHints::new().filterable());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractivePanel {
    /// Header/prompt text displayed above the panel.
    pub header: String,
    /// Items to display.
    #[serde(default)]
    pub items: Vec<PanelItem>,
    /// Render/behavior hints.
    #[serde(default)]
    pub hints: PanelHints,
}

impl InteractivePanel {
    /// Create a new interactive panel with a header.
    pub fn new(header: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            items: Vec::new(),
            hints: PanelHints::default(),
        }
    }

    /// Add an item to the panel.
    pub fn item(mut self, item: PanelItem) -> Self {
        self.items.push(item);
        self
    }

    /// Set all items at once.
    pub fn items<I: IntoIterator<Item = PanelItem>>(mut self, items: I) -> Self {
        self.items = items.into_iter().collect();
        self
    }

    /// Set render/behavior hints.
    pub fn hints(mut self, hints: PanelHints) -> Self {
        self.hints = hints;
        self
    }
}

/// Current state of an interactive panel (exposed to key handlers).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelState {
    /// Current cursor position (index in visible items).
    pub cursor: usize,
    /// Selected indices (in original items list).
    #[serde(default)]
    pub selected: Vec<usize>,
    /// Current filter text.
    #[serde(default)]
    pub filter: String,
    /// Indices of items currently visible after filtering.
    #[serde(default)]
    pub visible: Vec<usize>,
}

impl PanelState {
    /// Create initial state for a panel.
    pub fn initial(panel: &InteractivePanel) -> Self {
        Self {
            cursor: 0,
            selected: panel.hints.initial_selection.clone(),
            filter: panel.hints.initial_filter.clone().unwrap_or_default(),
            visible: (0..panel.items.len()).collect(),
        }
    }
}

/// Action returned by a key handler to control panel behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PanelAction {
    /// Continue with default key handling.
    Continue,
    /// Accept current selection and close panel.
    Accept,
    /// Accept with specific selection (overrides current).
    AcceptWith {
        selected: Vec<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        other: Option<String>,
    },
    /// Cancel and close panel.
    Cancel,
    /// Toggle selection at specified index.
    ToggleSelect { index: usize },
    /// Move cursor by delta (positive = down, negative = up).
    MoveCursor { delta: i32 },
    /// Set filter text.
    SetFilter { text: String },
}

/// Result when an interactive panel closes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelResult {
    /// Whether the user cancelled (Escape).
    #[serde(default)]
    pub cancelled: bool,
    /// Selected item indices (in original items list).
    #[serde(default)]
    pub selected: Vec<usize>,
    /// Free-text "other" input if used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
}

impl PanelResult {
    /// Create a result with selected indices.
    pub fn selected<I: IntoIterator<Item = usize>>(indices: I) -> Self {
        Self {
            cancelled: false,
            selected: indices.into_iter().collect(),
            other: None,
        }
    }

    /// Create a result with free-text input.
    pub fn other(text: impl Into<String>) -> Self {
        Self {
            cancelled: false,
            selected: Vec::new(),
            other: Some(text.into()),
        }
    }

    /// Create a cancelled result.
    pub fn cancelled() -> Self {
        Self {
            cancelled: true,
            selected: Vec::new(),
            other: None,
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
    /// Batched questions with "Other" text input option.
    AskBatch(AskBatch),
    /// Artifact editing.
    Edit(EditRequest),
    /// Display content (no response).
    Show(ShowRequest),
    /// Permission request.
    Permission(PermRequest),
    /// Popup with rich entries (for scripted popups).
    Popup(PopupRequest),
    /// Interactive panel (core primitive for scripted UI flows).
    Panel(InteractivePanel),
}

impl InteractionRequest {
    /// Get the request kind for pattern matching.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Ask(_) => "ask",
            Self::AskBatch(_) => "ask_batch",
            Self::Edit(_) => "edit",
            Self::Show(_) => "show",
            Self::Permission(_) => "permission",
            Self::Popup(_) => "popup",
            Self::Panel(_) => "panel",
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

impl From<AskBatch> for InteractionRequest {
    fn from(batch: AskBatch) -> Self {
        InteractionRequest::AskBatch(batch)
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

impl From<PopupRequest> for InteractionRequest {
    fn from(req: PopupRequest) -> Self {
        InteractionRequest::Popup(req)
    }
}

impl From<InteractivePanel> for InteractionRequest {
    fn from(panel: InteractivePanel) -> Self {
        InteractionRequest::Panel(panel)
    }
}

/// Unified interaction response type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InteractionResponse {
    /// Response to an ask request.
    Ask(AskResponse),
    /// Response to a batched ask request.
    AskBatch(AskBatchResponse),
    /// Response to an edit request.
    Edit(EditResponse),
    /// Response to a permission request.
    Permission(PermResponse),
    /// Response to a popup request.
    Popup(PopupResponse),
    /// Response to an interactive panel.
    Panel(PanelResult),
    /// Request was cancelled by the user.
    Cancelled,
}

impl From<AskResponse> for InteractionResponse {
    fn from(resp: AskResponse) -> Self {
        InteractionResponse::Ask(resp)
    }
}

impl From<AskBatchResponse> for InteractionResponse {
    fn from(resp: AskBatchResponse) -> Self {
        InteractionResponse::AskBatch(resp)
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

impl From<PopupResponse> for InteractionResponse {
    fn from(resp: PopupResponse) -> Self {
        InteractionResponse::Popup(resp)
    }
}

impl From<PanelResult> for InteractionResponse {
    fn from(result: PanelResult) -> Self {
        InteractionResponse::Panel(result)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Interaction Event (for out-of-band delivery)
// ─────────────────────────────────────────────────────────────────────────────

/// Event carrying an interaction request with its correlation ID.
///
/// Used for delivering interactions through channels outside of streaming.
#[derive(Debug, Clone)]
pub struct InteractionEvent {
    pub request_id: String,
    pub request: InteractionRequest,
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
    // AskBatch tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn ask_batch_creation() {
        let batch = AskBatch::new()
            .question(
                AskQuestion::new("Auth", "Which authentication?")
                    .choice("JWT")
                    .choice("Session"),
            )
            .question(
                AskQuestion::new("DB", "Which database?")
                    .choice("Postgres")
                    .choice("SQLite"),
            );

        assert_eq!(batch.questions.len(), 2);
        assert_eq!(batch.questions[0].header, "Auth");
        assert_eq!(batch.questions[0].choices.len(), 2);
    }

    #[test]
    fn ask_batch_response_creation() {
        let response = AskBatchResponse::new(uuid::Uuid::new_v4())
            .answer(QuestionAnswer::choice(1))
            .answer(QuestionAnswer::other("Custom DB"));

        assert_eq!(response.answers.len(), 2);
        assert_eq!(response.answers[0].selected, vec![1]);
        assert_eq!(response.answers[1].other, Some("Custom DB".into()));
    }

    #[test]
    fn ask_question_multi_select() {
        let q = AskQuestion::new("Features", "Select features")
            .choices(["A", "B", "C"])
            .multi_select();

        assert!(q.multi_select);
        assert_eq!(q.choices.len(), 3);
    }

    #[test]
    fn question_answer_multi_choices() {
        let answer = QuestionAnswer::choices([0, 2]);

        assert_eq!(answer.selected, vec![0, 2]);
        assert!(answer.other.is_none());
    }

    #[test]
    fn ask_batch_cancelled() {
        let id = uuid::Uuid::new_v4();
        let response = AskBatchResponse::cancelled(id);

        assert!(response.cancelled);
        assert!(response.answers.is_empty());
        assert_eq!(response.id, id);
    }

    #[test]
    fn interaction_request_from_ask_batch() {
        let batch = AskBatch::new().question(AskQuestion::new("Test", "Question?").choice("A"));
        let req: InteractionRequest = batch.into();

        assert!(matches!(req, InteractionRequest::AskBatch(_)));
        assert_eq!(req.kind(), "ask_batch");
        assert!(req.expects_response());
    }

    #[test]
    fn interaction_response_from_ask_batch() {
        let resp: InteractionResponse = AskBatchResponse::new(uuid::Uuid::new_v4())
            .answer(QuestionAnswer::choice(0))
            .into();

        assert!(matches!(resp, InteractionResponse::AskBatch(_)));
    }

    #[test]
    fn ask_batch_serialization() {
        let batch = AskBatch::new().question(
            AskQuestion::new("Test", "Question?")
                .choice("A")
                .choice("B"),
        );
        let json = serde_json::to_string(&batch).unwrap();
        let restored: AskBatch = serde_json::from_str(&json).unwrap();

        assert_eq!(batch.questions.len(), restored.questions.len());
        assert_eq!(batch.questions[0].header, restored.questions[0].header);
    }

    #[test]
    fn ask_batch_response_serialization() {
        let response = AskBatchResponse::new(uuid::Uuid::new_v4())
            .answer(QuestionAnswer::choice(0))
            .answer(QuestionAnswer::other("Custom"));
        let json = serde_json::to_string(&response).unwrap();
        let restored: AskBatchResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(response.answers.len(), restored.answers.len());
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

    // ─────────────────────────────────────────────────────────────────────────
    // PopupRequest tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn popup_request_with_entries() {
        let popup = PopupRequest::new("Select a note").entries([
            PopupEntry::new("Daily Note").with_description("Today's journal"),
            PopupEntry::new("Todo List"),
        ]);

        assert_eq!(popup.title, "Select a note");
        assert_eq!(popup.entries.len(), 2);
        assert_eq!(popup.entries[0].label, "Daily Note");
        assert_eq!(
            popup.entries[0].description,
            Some("Today's journal".to_string())
        );
        assert!(!popup.allow_other);
    }

    #[test]
    fn popup_request_builder_pattern() {
        let popup = PopupRequest::new("Choose action")
            .entry(PopupEntry::new("Save"))
            .entry(PopupEntry::new("Discard"))
            .allow_other();

        assert_eq!(popup.entries.len(), 2);
        assert!(popup.allow_other);
    }

    #[test]
    fn popup_response_with_selection() {
        let entry = PopupEntry::new("Selected Item");
        let resp = PopupResponse::selected(1, entry.clone());

        assert_eq!(resp.selected_index, Some(1));
        assert_eq!(resp.selected_entry, Some(entry));
        assert!(resp.other.is_none());
    }

    #[test]
    fn popup_response_with_other() {
        let resp = PopupResponse::other("Custom text");

        assert!(resp.selected_index.is_none());
        assert!(resp.selected_entry.is_none());
        assert_eq!(resp.other, Some("Custom text".to_string()));
    }

    #[test]
    fn popup_response_none() {
        let resp = PopupResponse::none();

        assert!(resp.selected_index.is_none());
        assert!(resp.selected_entry.is_none());
        assert!(resp.other.is_none());
    }

    #[test]
    fn interaction_request_from_popup() {
        let popup = PopupRequest::new("Test");
        let req: InteractionRequest = popup.into();

        assert!(matches!(req, InteractionRequest::Popup(_)));
        assert_eq!(req.kind(), "popup");
        assert!(req.expects_response());
    }

    #[test]
    fn popup_request_serialization() {
        let popup = PopupRequest::new("Select").entries([PopupEntry::new("Option A")]);
        let json = serde_json::to_string(&popup).unwrap();
        let restored: PopupRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(popup.title, restored.title);
        assert_eq!(popup.entries.len(), restored.entries.len());
    }

    #[test]
    fn popup_response_serialization() {
        let entry = PopupEntry::new("Item");
        let resp = PopupResponse::selected(0, entry);
        let json = serde_json::to_string(&resp).unwrap();
        let restored: PopupResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp.selected_index, restored.selected_index);
    }

    #[test]
    fn interaction_response_popup_serialization() {
        let entry = PopupEntry::new("Test");
        let resp: InteractionResponse = PopupResponse::selected(0, entry).into();
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"kind\":\"popup\""));
        let restored: InteractionResponse = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, InteractionResponse::Popup(_)));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // InteractivePanel tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn panel_item_builder() {
        let item = PanelItem::new("PostgreSQL")
            .with_description("Full-featured RDBMS")
            .with_data(serde_json::json!({"type": "sql"}));

        assert_eq!(item.label, "PostgreSQL");
        assert_eq!(item.description, Some("Full-featured RDBMS".into()));
        assert!(item.data.is_some());
    }

    #[test]
    fn panel_hints_builder() {
        let hints = PanelHints::new()
            .filterable()
            .multi_select()
            .allow_other()
            .initial_selection([0, 2]);

        assert!(hints.filterable);
        assert!(hints.multi_select);
        assert!(hints.allow_other);
        assert_eq!(hints.initial_selection, vec![0, 2]);
    }

    #[test]
    fn interactive_panel_builder() {
        let panel = InteractivePanel::new("Select database")
            .item(PanelItem::new("PostgreSQL"))
            .item(PanelItem::new("SQLite").with_description("Embedded"))
            .hints(PanelHints::new().filterable());

        assert_eq!(panel.header, "Select database");
        assert_eq!(panel.items.len(), 2);
        assert_eq!(panel.items[0].label, "PostgreSQL");
        assert_eq!(panel.items[1].description, Some("Embedded".into()));
        assert!(panel.hints.filterable);
    }

    #[test]
    fn panel_state_initial() {
        let panel = InteractivePanel::new("Test")
            .items([
                PanelItem::new("A"),
                PanelItem::new("B"),
                PanelItem::new("C"),
            ])
            .hints(PanelHints::new().initial_selection([1]));

        let state = PanelState::initial(&panel);

        assert_eq!(state.cursor, 0);
        assert_eq!(state.selected, vec![1]);
        assert_eq!(state.filter, "");
        assert_eq!(state.visible, vec![0, 1, 2]);
    }

    #[test]
    fn panel_result_variants() {
        let selected = PanelResult::selected([0, 2]);
        assert!(!selected.cancelled);
        assert_eq!(selected.selected, vec![0, 2]);
        assert!(selected.other.is_none());

        let other = PanelResult::other("Custom choice");
        assert!(!other.cancelled);
        assert!(other.selected.is_empty());
        assert_eq!(other.other, Some("Custom choice".into()));

        let cancelled = PanelResult::cancelled();
        assert!(cancelled.cancelled);
        assert!(cancelled.selected.is_empty());
    }

    #[test]
    fn interaction_request_from_panel() {
        let panel = InteractivePanel::new("Test").item(PanelItem::new("A"));
        let req: InteractionRequest = panel.into();

        assert!(matches!(req, InteractionRequest::Panel(_)));
        assert_eq!(req.kind(), "panel");
        assert!(req.expects_response());
    }

    #[test]
    fn interaction_response_from_panel_result() {
        let resp: InteractionResponse = PanelResult::selected([0]).into();

        assert!(matches!(resp, InteractionResponse::Panel(_)));
    }

    #[test]
    fn panel_serialization() {
        let panel = InteractivePanel::new("Test")
            .item(PanelItem::new("A").with_description("Option A"))
            .hints(PanelHints::new().filterable().multi_select());

        let json = serde_json::to_string(&panel).unwrap();
        let restored: InteractivePanel = serde_json::from_str(&json).unwrap();

        assert_eq!(panel.header, restored.header);
        assert_eq!(panel.items.len(), restored.items.len());
        assert_eq!(panel.hints.filterable, restored.hints.filterable);
        assert_eq!(panel.hints.multi_select, restored.hints.multi_select);
    }

    #[test]
    fn panel_result_serialization() {
        let result = PanelResult::selected([0, 1]);
        let json = serde_json::to_string(&result).unwrap();
        let restored: PanelResult = serde_json::from_str(&json).unwrap();

        assert_eq!(result.selected, restored.selected);
        assert_eq!(result.cancelled, restored.cancelled);
    }

    #[test]
    fn interaction_request_panel_serialization() {
        let panel = InteractivePanel::new("Test").item(PanelItem::new("A"));
        let req: InteractionRequest = panel.into();
        let json = serde_json::to_string(&req).unwrap();

        assert!(json.contains("\"kind\":\"panel\""));
        let restored: InteractionRequest = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, InteractionRequest::Panel(_)));
    }

    #[test]
    fn interaction_response_panel_serialization() {
        let resp: InteractionResponse = PanelResult::selected([0]).into();
        let json = serde_json::to_string(&resp).unwrap();

        assert!(json.contains("\"kind\":\"panel\""));
        let restored: InteractionResponse = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, InteractionResponse::Panel(_)));
    }

    #[test]
    fn panel_action_serialization() {
        let action = PanelAction::AcceptWith {
            selected: vec![0, 1],
            other: Some("custom".into()),
        };
        let json = serde_json::to_string(&action).unwrap();
        let restored: PanelAction = serde_json::from_str(&json).unwrap();

        assert_eq!(action, restored);
    }
}
