use std::collections::VecDeque;

use crucible_core::interaction::PermRequest;

use super::state::AutocompleteKind;

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
    /// Whether permission prompts show the full command/args wrapped across
    /// lines instead of a single truncated line (session-scoped)
    pub perm_full_commands: bool,
}

impl Default for PermissionState {
    fn default() -> Self {
        Self {
            permission_queue: VecDeque::new(),
            perm_show_diff: true,
            perm_autoconfirm_session: false,
            perm_full_commands: true,
        }
    }
}

/// Precognition state — auto-RAG settings and last result cache
pub(crate) struct PrecognitionState {
    /// Whether to auto-enrich user messages with knowledge base context (precognition / auto-RAG)
    pub precognition: bool,
}

impl Default for PrecognitionState {
    fn default() -> Self {
        Self { precognition: true }
    }
}
