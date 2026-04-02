use std::collections::VecDeque;

use crucible_core::interaction::PermRequest;
use crucible_core::traits::chat::PrecognitionNoteInfo;

use super::state::AutocompleteKind;
use super::MAX_SHELL_HISTORY;

/// Autocomplete popup state — purely local UI chrome.
///
/// Groups the five tightly-coupled fields that together describe
/// whether a popup is visible, what kind it is, which item is
/// highlighted, and how the list is filtered.
#[derive(Debug, Default)]
pub struct PopupState {
    /// Whether the popup overlay is currently visible
    pub show: bool,
    /// Index of the currently highlighted item
    pub selected: usize,
    /// What the popup is completing (command, file, model, …)
    pub kind: AutocompleteKind,
    /// User-typed text used to narrow the item list
    pub filter: String,
    /// Cursor position in the input buffer where the trigger character was typed
    pub trigger_pos: usize,
}

/// Permission request state — queue, display settings, and auto-confirm flag
pub(crate) struct PermissionState {
    /// Queue of pending permission requests (request_id, request) when multiple arrive rapidly
    pub permission_queue: VecDeque<(String, PermRequest)>,
    /// Whether to show diff by default in permission prompts (session-scoped)
    pub perm_show_diff: bool,
    /// Whether to auto-allow all permission prompts for this session
    pub perm_autoconfirm_session: bool,
}

impl Default for PermissionState {
    fn default() -> Self {
        Self {
            permission_queue: VecDeque::new(),
            perm_show_diff: true,
            perm_autoconfirm_session: false,
        }
    }
}

/// Shell command history state — recent commands and recall index
pub(crate) struct ShellHistoryState {
    /// Recent shell commands (for !-history recall)
    pub shell_history: VecDeque<String>,
    /// Current index into shell_history during recall
    pub shell_history_index: Option<usize>,
}

impl Default for ShellHistoryState {
    fn default() -> Self {
        Self {
            shell_history: VecDeque::with_capacity(MAX_SHELL_HISTORY),
            shell_history_index: None,
        }
    }
}
/// Precognition state — auto-RAG settings and last result cache
pub(crate) struct PrecognitionState {
    /// Whether to auto-enrich user messages with knowledge base context (precognition / auto-RAG)
    pub precognition: bool,
    /// Number of context results to inject per precognition query (1-20)
    pub precognition_results: usize,
    /// Count of notes injected in the last precognition result
    pub last_notes_count: Option<usize>,
    /// Notes returned by the last precognition query
    pub last_notes: Vec<PrecognitionNoteInfo>,
}

impl Default for PrecognitionState {
    fn default() -> Self {
        Self {
            precognition: true,
            precognition_results: 5,
            last_notes_count: None,
            last_notes: Vec::new(),
        }
    }
}
