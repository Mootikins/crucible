//! Interaction modal component for permission and ask requests.
//!
//! Follows Elm-style architecture: Msg → update → Output.

use crucible_oil::node::Node;

use crossterm::event::{KeyEvent, KeyModifiers};
use crucible_core::interaction::{InteractionRequest, PanelState};
#[allow(unused_imports)] // WIP: PopupEntry not yet used
use crucible_core::types::PopupEntry;
use std::collections::HashSet;

mod ask;
mod edit;
mod helpers;
mod panel;
mod perm;
mod popup;
mod show;

#[cfg(test)]
mod tests;

/// Mode for interaction modal input handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InteractionMode {
    /// Navigating/selecting from choices.
    #[default]
    Selecting,
    /// Free-text input (for "Other" option).
    TextInput,
}

/// Messages that can be sent to the interaction modal.
#[derive(Debug, Clone)]
pub enum InteractionModalMsg {
    Key(KeyEvent),
}

/// Output from the interaction modal's update function.
#[derive(Debug, Clone)]
pub enum InteractionModalOutput {
    /// No action needed, continue.
    None,
    /// Close the modal (cancelled).
    Close,
    /// Permission response ready to send.
    PermissionResponse {
        request_id: String,
        response: crucible_core::interaction::PermResponse,
    },
    /// Ask response ready to send.
    AskResponse {
        request_id: String,
        response: crucible_core::interaction::InteractionResponse,
    },
    /// Toggle diff preview visibility.
    ToggleDiff,
    /// Show a notification toast.
    Notify(String),
}

/// State for the interaction modal (Ask, AskBatch, Permission, etc.).
pub struct InteractionModal {
    /// Correlates with response sent back to daemon.
    pub request_id: String,
    /// The request being displayed.
    pub request: InteractionRequest,
    /// Current selection index for choice-based requests.
    pub selected: usize,
    /// Filter text for filterable panels (future use).
    pub filter: String,
    /// Free-text input buffer for "Other" option.
    pub other_text: String,
    /// Current input mode.
    pub mode: InteractionMode,
    /// Checked items for multi-select mode.
    pub checked: HashSet<usize>,
    /// Current question index for multi-question batches.
    pub current_question: usize,
    /// Track if "Other" text was previously entered (for dim rendering when deselected).
    pub other_text_preserved: bool,
    /// Answers per question for AskBatch (Vec of selected indices per question).
    pub batch_answers: Vec<HashSet<usize>>,
    /// Other text per question for AskBatch.
    pub batch_other_texts: Vec<String>,
    /// Whether the diff preview is collapsed (for permission requests with file changes).
    pub diff_collapsed: bool,
    /// Scroll offset for Show and Panel views.
    pub scroll_offset: usize,
    /// Lines of content for Edit interaction.
    pub edit_lines: Vec<String>,
    /// Cursor line position in Edit interaction.
    pub edit_cursor_line: usize,
    /// Cursor column position in Edit interaction.
    pub edit_cursor_col: usize,
    /// Panel tracking state for Panel interaction.
    pub panel_state: Option<PanelState>,
}

impl InteractionModal {
    pub fn new(request_id: String, request: InteractionRequest, show_diff: bool) -> Self {
        let edit_lines = if let InteractionRequest::Edit(ref edit) = request {
            edit.content.lines().map(String::from).collect()
        } else {
            Vec::new()
        };
        let panel_state = if let InteractionRequest::Panel(ref panel) = request {
            Some(PanelState::initial(panel))
        } else {
            None
        };
        let checked = if let InteractionRequest::Panel(ref panel) = request {
            panel.hints.initial_selection.iter().copied().collect()
        } else {
            HashSet::new()
        };
        Self {
            request_id,
            request,
            selected: 0,
            filter: String::new(),
            other_text: String::new(),
            mode: InteractionMode::Selecting,
            checked,
            current_question: 0,
            other_text_preserved: false,
            batch_answers: Vec::new(),
            batch_other_texts: Vec::new(),
            diff_collapsed: !show_diff,
            scroll_offset: 0,
            edit_lines,
            edit_cursor_line: 0,
            edit_cursor_col: 0,
            panel_state,
        }
    }

    /// Process a message and return the output action.
    pub fn update(&mut self, msg: InteractionModalMsg) -> InteractionModalOutput {
        match msg {
            InteractionModalMsg::Key(key) => self.handle_key(key),
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> InteractionModalOutput {
        match &self.request {
            InteractionRequest::Ask(ask) => self.handle_ask_key(key, ask.clone()),
            InteractionRequest::AskBatch(batch) => self.handle_ask_batch_key(key, batch.clone()),
            InteractionRequest::Permission(perm) => self.handle_perm_key(key, perm.clone()),
            InteractionRequest::Show(_) => self.handle_show_key(key),
            InteractionRequest::Popup(popup) => self.handle_popup_key(key, popup.clone()),
            InteractionRequest::Edit(_) => self.handle_edit_key(key),
            InteractionRequest::Panel(panel) => self.handle_panel_key(key, panel.clone()),
        }
    }

    pub(super) fn wrap_selection(selected: usize, delta: isize, total: usize) -> usize {
        if delta < 0 && selected == 0 {
            total - 1
        } else if delta < 0 {
            selected - 1
        } else {
            (selected + 1) % total
        }
    }

    pub(super) fn toggle_checked(set: &mut HashSet<usize>, value: usize) {
        if set.contains(&value) {
            set.remove(&value);
        } else {
            set.insert(value);
        }
    }

    fn is_ctrl_c(key: KeyEvent) -> bool {
        key.modifiers.contains(KeyModifiers::CONTROL)
    }

    pub fn view(&self, term_width: usize, queue_size: usize) -> Node {
        match &self.request {
            InteractionRequest::Permission(perm) => {
                self.render_perm_interaction(perm, term_width, queue_size)
            }
            InteractionRequest::Ask(ask) => self.render_ask_interaction_single(ask, term_width),
            InteractionRequest::AskBatch(batch) => {
                self.render_ask_interaction_batch(batch, term_width)
            }
            InteractionRequest::Show(show) => self.render_show_interaction(show, term_width),
            InteractionRequest::Popup(popup) => self.render_popup_interaction(popup, term_width),
            InteractionRequest::Edit(edit) => self.render_edit_interaction(edit, term_width),
            InteractionRequest::Panel(panel) => self.render_panel_interaction(panel, term_width),
        }
    }
}
