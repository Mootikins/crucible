//! The choice flow that the Ask, AskBatch, Popup and Panel modals share.
//!
//! A choice list has zero or more choices and an optional "Other" slot after
//! them. The cursor moves over the choices and the "Other" slot, and it wraps
//! at each end. Space toggles a choice in a multi-select list. Enter on a
//! choice accepts it. Enter on the "Other" slot opens the text input. In the
//! text input, Enter submits the text and Esc goes back to the list.
//!
//! [`ChoiceList::handle_key`] owns these keys. It returns a [`ChoiceStep`],
//! and each modal turns the step into its own response type.

use super::InteractionMode;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_oil::node::{row, styled, Node};
use crucible_oil::style::Style;
use std::collections::HashSet;

/// The ids of the choices, in screen order.
#[derive(Debug, Clone, Copy)]
enum ChoiceIds<'a> {
    /// The ids `0..count`.
    All(usize),
    /// A filtered subset of ids, for example the visible items of a panel.
    Filtered(&'a [usize]),
}

/// The shape of one choice list.
#[derive(Debug, Clone, Copy)]
pub(super) struct ChoiceList<'a> {
    ids: ChoiceIds<'a>,
    allow_other: bool,
    multi_select: bool,
}

/// The state that [`ChoiceList::handle_key`] reads and writes.
///
/// The fields borrow the state of the modal. A panel keeps its cursor in its
/// `PanelState`, so the cursor is a separate borrow.
pub(super) struct ChoiceInput<'m> {
    /// The screen position of the cursor. The "Other" slot is the position
    /// after the last choice.
    pub cursor: &'m mut usize,
    /// The checked choice ids of a multi-select list.
    pub checked: &'m mut HashSet<usize>,
    /// The text of the "Other" slot.
    pub text: &'m mut String,
    /// The input mode of the modal.
    pub mode: &'m mut InteractionMode,
}

/// The result of one key in the choice flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ChoiceStep {
    /// The flow used the key. The modal has no response to send.
    Handled,
    /// The user accepted the choice with this id (single-select).
    Pick(usize),
    /// The user accepted the checked ids, in ascending order (multi-select).
    PickMany(Vec<usize>),
    /// The user submitted this text in the "Other" slot.
    Other(String),
    /// The user cancelled the modal.
    Cancel,
    /// The flow does not use the key. The modal can use it.
    Ignored,
}

impl<'a> ChoiceList<'a> {
    /// A list of the choices `0..count`.
    pub(super) fn new(count: usize) -> Self {
        Self {
            ids: ChoiceIds::All(count),
            allow_other: false,
            multi_select: false,
        }
    }

    /// A list of these choice ids, in this order.
    pub(super) fn filtered(ids: &'a [usize]) -> Self {
        Self {
            ids: ChoiceIds::Filtered(ids),
            allow_other: false,
            multi_select: false,
        }
    }

    /// Adds the "Other" slot after the choices when `allow` is true.
    #[must_use]
    pub(super) fn allow_other(mut self, allow: bool) -> Self {
        self.allow_other = allow;
        self
    }

    /// Makes Space toggle a choice and Enter accept the checked set.
    #[must_use]
    pub(super) fn multi_select(mut self, multi: bool) -> Self {
        self.multi_select = multi;
        self
    }

    fn choices(&self) -> usize {
        match self.ids {
            ChoiceIds::All(count) => count,
            ChoiceIds::Filtered(ids) => ids.len(),
        }
    }

    /// The number of cursor positions: the choices and the "Other" slot.
    pub(super) fn slots(&self) -> usize {
        self.choices() + usize::from(self.allow_other)
    }

    /// True when `position` is the "Other" slot.
    pub(super) fn is_other(&self, position: usize) -> bool {
        self.allow_other && position == self.choices()
    }

    /// The choice id at a screen position. The "Other" slot has no id.
    fn id(&self, position: usize) -> Option<usize> {
        match self.ids {
            ChoiceIds::All(count) => (position < count).then_some(position),
            ChoiceIds::Filtered(ids) => ids.get(position).copied(),
        }
    }

    /// Moves a cursor that is past the last slot to the last slot.
    pub(super) fn clamp(&self, cursor: usize) -> usize {
        cursor.min(self.slots().saturating_sub(1))
    }

    /// Applies one key to the flow.
    pub(super) fn handle_key(&self, key: KeyEvent, input: ChoiceInput<'_>) -> ChoiceStep {
        match *input.mode {
            InteractionMode::Selecting => self.select_key(key, input),
            InteractionMode::TextInput => Self::text_key(key, input),
            InteractionMode::Filter => ChoiceStep::Ignored,
        }
    }

    fn select_key(&self, key: KeyEvent, input: ChoiceInput<'_>) -> ChoiceStep {
        let total = self.slots().max(1);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                *input.cursor = wrap_selection(*input.cursor, -1, total);
                ChoiceStep::Handled
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                *input.cursor = wrap_selection(*input.cursor, 1, total);
                ChoiceStep::Handled
            }
            KeyCode::Char(' ') if self.multi_select => {
                if let Some(id) = self.id(*input.cursor) {
                    if !input.checked.remove(&id) {
                        input.checked.insert(id);
                    }
                }
                ChoiceStep::Handled
            }
            KeyCode::Enter if self.is_other(*input.cursor) => {
                *input.mode = InteractionMode::TextInput;
                ChoiceStep::Handled
            }
            KeyCode::Enter if self.multi_select => {
                let mut ids: Vec<usize> = input.checked.iter().copied().collect();
                // A HashSet has no order. The requester reads the ids as
                // choice indices, so send them in ascending order.
                ids.sort_unstable();
                ChoiceStep::PickMany(ids)
            }
            KeyCode::Enter => self
                .id(*input.cursor)
                .map_or(ChoiceStep::Handled, ChoiceStep::Pick),
            KeyCode::Esc => ChoiceStep::Cancel,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                ChoiceStep::Cancel
            }
            _ => ChoiceStep::Ignored,
        }
    }

    fn text_key(key: KeyEvent, input: ChoiceInput<'_>) -> ChoiceStep {
        match key.code {
            KeyCode::Enter => ChoiceStep::Other(input.text.clone()),
            KeyCode::Esc => {
                *input.mode = InteractionMode::Selecting;
                ChoiceStep::Handled
            }
            KeyCode::Backspace => {
                input.text.pop();
                ChoiceStep::Handled
            }
            KeyCode::Char(c) => {
                input.text.push(c);
                ChoiceStep::Handled
            }
            _ => ChoiceStep::Handled,
        }
    }
}

/// Moves `selected` one step up (`delta < 0`) or down, and wraps at each end.
pub(super) fn wrap_selection(selected: usize, delta: isize, total: usize) -> usize {
    if delta < 0 && selected == 0 {
        total - 1
    } else if delta < 0 {
        selected - 1
    } else {
        (selected + 1) % total
    }
}

/// The "Other..." row. `marker` is the prefix when the cursor is on the row.
pub(super) fn other_row(is_cursor: bool, marker: &str) -> Node {
    let t = crate::tui::oil::theme::active();
    let (prefix, style) = if is_cursor {
        (
            marker.to_string(),
            Style::new().fg(t.resolve_color(t.colors.primary)).bold(),
        )
    } else {
        (
            " ".repeat(marker.chars().count()),
            Style::new()
                .fg(t.resolve_color(t.colors.text_muted))
                .italic(),
        )
    };
    styled(format!("{prefix}Other..."), style)
}

/// The text input row of the "Other" slot, with `indent` before the label.
pub(super) fn other_text_row(indent: &str, text: &str) -> Node {
    let t = crate::tui::oil::theme::active();
    row([
        styled(
            format!("{indent}Enter text: "),
            Style::new().fg(t.resolve_color(t.colors.text_muted)),
        ),
        styled(text, Style::new().fg(t.resolve_color(t.colors.text))),
        styled("_", Style::new().fg(t.resolve_color(t.colors.primary))),
    ])
}
