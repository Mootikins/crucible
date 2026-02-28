//! Unified interaction types and UI primitives.
//!
//! Contains the unified [`InteractionRequest`] and [`InteractionResponse`] enums,
//! popup/panel primitives, and the [`InteractionEvent`] for out-of-band delivery.

use serde::{Deserialize, Serialize};

use crate::types::PopupEntry;

use super::ask::{AskBatch, AskBatchResponse, AskRequest, AskResponse};
use super::edit::{EditRequest, EditResponse, ShowRequest};
use super::permission::{PermRequest, PermResponse};

// ─────────────────────────────────────────────────────────────────────────────
// Popup Request/Response
// ─────────────────────────────────────────────────────────────────────────────

/// A request to show a popup with selectable entries.
///
/// Unlike [`AskRequest`] which uses simple string choices, `PopupRequest` uses
/// [`PopupEntry`] items that can include labels, descriptions, and arbitrary data.
/// This makes it suitable for rich scripted popups from Lua plugins.
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
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            entries: Vec::new(),
            allow_other: false,
        }
    }

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
    pub fn selected(index: usize, entry: PopupEntry) -> Self {
        Self {
            selected_index: Some(index),
            selected_entry: Some(entry),
            other: None,
        }
    }

    pub fn selected_index(index: usize) -> Self {
        Self {
            selected_index: Some(index),
            selected_entry: None,
            other: None,
        }
    }

    pub fn other(text: impl Into<String>) -> Self {
        Self {
            selected_index: None,
            selected_entry: None,
            other: Some(text.into()),
        }
    }

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

    pub fn items<I: IntoIterator<Item = PanelItem>>(mut self, items: I) -> Self {
        self.items = items.into_iter().collect();
        self
    }

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
    pub fn selected<I: IntoIterator<Item = usize>>(indices: I) -> Self {
        Self {
            cancelled: false,
            selected: indices.into_iter().collect(),
            other: None,
        }
    }

    pub fn other(text: impl Into<String>) -> Self {
        Self {
            cancelled: false,
            selected: Vec::new(),
            other: Some(text.into()),
        }
    }

    pub fn cancelled() -> Self {
        Self {
            cancelled: true,
            selected: Vec::new(),
            other: None,
        }
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
    use crate::interaction::ask::{AskQuestion, QuestionAnswer};

    // ─────────────────────────────────────────────────────────────────────────
    // AskBatch interaction tests
    // ─────────────────────────────────────────────────────────────────────────

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
