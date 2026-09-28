use super::{ctrl_c, key_event};
use crate::tui::oil::components::interaction_modal::choice::{
    wrap_selection, ChoiceInput, ChoiceList, ChoiceStep,
};
use crate::tui::oil::components::interaction_modal::InteractionMode;
use crossterm::event::{KeyCode, KeyEvent};
use std::collections::HashSet;

/// The state that a modal lends to the flow.
#[derive(Default)]
struct State {
    cursor: usize,
    checked: HashSet<usize>,
    text: String,
    mode: InteractionMode,
}

impl State {
    fn key(&mut self, list: &ChoiceList<'_>, key: KeyEvent) -> ChoiceStep {
        list.handle_key(
            key,
            ChoiceInput {
                cursor: &mut self.cursor,
                checked: &mut self.checked,
                text: &mut self.text,
                mode: &mut self.mode,
            },
        )
    }

    fn press(&mut self, list: &ChoiceList<'_>, code: KeyCode) -> ChoiceStep {
        self.key(list, key_event(code))
    }
}

#[test]
fn the_cursor_wraps_over_the_choices_and_the_other_slot() {
    let list = ChoiceList::new(2).allow_other(true);
    let mut state = State::default();

    state.press(&list, KeyCode::Up);
    assert_eq!(
        state.cursor, 2,
        "Up from the first row goes to the Other slot"
    );
    state.press(&list, KeyCode::Down);
    assert_eq!(
        state.cursor, 0,
        "Down from the Other slot goes to the first row"
    );
    state.press(&list, KeyCode::Char('j'));
    assert_eq!(state.cursor, 1);
    state.press(&list, KeyCode::Char('K'));
    assert_eq!(state.cursor, 0);
}

#[test]
fn an_empty_list_keeps_the_cursor_and_enter_picks_nothing() {
    let list = ChoiceList::new(0);
    let mut state = State::default();

    assert_eq!(state.press(&list, KeyCode::Down), ChoiceStep::Handled);
    assert_eq!(state.cursor, 0);
    assert_eq!(state.press(&list, KeyCode::Enter), ChoiceStep::Handled);
}

#[test]
fn enter_picks_the_id_of_a_filtered_row() {
    let ids = [4, 7, 9];
    let list = ChoiceList::filtered(&ids);
    let mut state = State::default();

    state.press(&list, KeyCode::Down);
    assert_eq!(state.press(&list, KeyCode::Enter), ChoiceStep::Pick(7));
}

#[test]
fn space_toggles_the_id_and_enter_sends_the_checked_ids_in_order() {
    let ids = [5, 2, 8];
    let list = ChoiceList::filtered(&ids).multi_select(true);
    let mut state = State::default();

    state.press(&list, KeyCode::Char(' '));
    state.press(&list, KeyCode::Down);
    state.press(&list, KeyCode::Char(' '));
    state.press(&list, KeyCode::Down);
    state.press(&list, KeyCode::Char(' '));
    state.press(&list, KeyCode::Char(' '));

    assert_eq!(state.checked, HashSet::from([5, 2]));
    assert_eq!(
        state.press(&list, KeyCode::Enter),
        ChoiceStep::PickMany(vec![2, 5])
    );
}

#[test]
fn space_on_the_other_slot_checks_nothing() {
    let list = ChoiceList::new(1).allow_other(true).multi_select(true);
    let mut state = State {
        cursor: 1,
        ..State::default()
    };

    state.press(&list, KeyCode::Char(' '));
    assert!(state.checked.is_empty());
}

#[test]
fn space_in_a_single_select_list_is_left_to_the_modal() {
    let list = ChoiceList::new(2);
    let mut state = State::default();

    assert_eq!(state.press(&list, KeyCode::Char(' ')), ChoiceStep::Ignored);
    assert!(state.checked.is_empty());
}

#[test]
fn the_other_slot_opens_the_text_input_and_submits_the_text() {
    let list = ChoiceList::new(1).allow_other(true).multi_select(true);
    let mut state = State {
        cursor: 1,
        ..State::default()
    };

    assert_eq!(state.press(&list, KeyCode::Enter), ChoiceStep::Handled);
    assert_eq!(state.mode, InteractionMode::TextInput);

    for c in "abx".chars() {
        state.press(&list, KeyCode::Char(c));
    }
    state.press(&list, KeyCode::Backspace);
    assert_eq!(
        state.press(&list, KeyCode::Enter),
        ChoiceStep::Other("ab".to_string())
    );
}

#[test]
fn esc_in_the_text_input_goes_back_to_the_list() {
    let list = ChoiceList::new(1).allow_other(true);
    let mut state = State {
        mode: InteractionMode::TextInput,
        ..State::default()
    };

    assert_eq!(state.press(&list, KeyCode::Esc), ChoiceStep::Handled);
    assert_eq!(state.mode, InteractionMode::Selecting);
}

#[test]
fn esc_and_ctrl_c_cancel_the_list() {
    let list = ChoiceList::new(2);
    let mut state = State::default();

    assert_eq!(state.press(&list, KeyCode::Esc), ChoiceStep::Cancel);
    assert_eq!(state.key(&list, ctrl_c()), ChoiceStep::Cancel);
}

#[test]
fn the_flow_leaves_the_filter_input_to_the_modal() {
    let list = ChoiceList::new(2);
    let mut state = State {
        mode: InteractionMode::Filter,
        ..State::default()
    };

    assert_eq!(state.press(&list, KeyCode::Char('a')), ChoiceStep::Ignored);
    assert!(state.text.is_empty());
}

#[test]
fn clamp_keeps_the_cursor_on_a_slot() {
    let ids = [3];
    let list = ChoiceList::filtered(&ids).allow_other(true);
    assert_eq!(list.clamp(5), 1);
    assert_eq!(list.clamp(0), 0);
    assert_eq!(ChoiceList::filtered(&[]).clamp(3), 0);
}

#[test]
fn wrap_selection_wraps_at_each_end() {
    assert_eq!(wrap_selection(0, -1, 3), 2);
    assert_eq!(wrap_selection(2, 1, 3), 0);
    assert_eq!(wrap_selection(1, -1, 3), 0);
    assert_eq!(wrap_selection(1, 1, 3), 2);
}
