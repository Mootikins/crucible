use super::key_event;
use crate::tui::oil::components::interaction_modal::{
    InteractionModal, InteractionModalMsg, InteractionModalOutput, InteractionMode,
};
use crossterm::event::KeyCode;
use crucible_core::interaction::{
    InteractionRequest, InteractionResponse, InteractivePanel, PanelHints, PanelItem,
};

fn make_panel_modal(items: Vec<&str>, hints: PanelHints) -> InteractionModal {
    let panel = InteractivePanel::new("Select")
        .items(items.into_iter().map(PanelItem::new))
        .hints(hints);
    InteractionModal::new("panel-1".into(), InteractionRequest::Panel(panel), false)
}

#[test]
fn panel_navigation_wraps() {
    let mut modal = make_panel_modal(vec!["A", "B", "C"], PanelHints::new());

    assert_eq!(modal.panel_state.as_ref().unwrap().cursor, 0);
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    assert_eq!(modal.panel_state.as_ref().unwrap().cursor, 1);
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    assert_eq!(modal.panel_state.as_ref().unwrap().cursor, 2);
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    assert_eq!(modal.panel_state.as_ref().unwrap().cursor, 0);
}

#[test]
fn panel_single_select() {
    let mut modal = make_panel_modal(vec!["X", "Y"], PanelHints::new());
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));

    let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
    match output {
        InteractionModalOutput::AskResponse { response, .. } => match response {
            InteractionResponse::Panel(result) => {
                assert!(!result.cancelled);
                assert_eq!(result.selected, vec![1]);
            }
            _ => panic!("Expected Panel response"),
        },
        _ => panic!("Expected AskResponse"),
    }
}

#[test]
fn panel_multi_select_toggle() {
    let mut modal = make_panel_modal(vec!["A", "B", "C"], PanelHints::new().multi_select());

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char(' '))));
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char(' '))));

    assert!(modal.checked.contains(&0));
    assert!(modal.checked.contains(&2));
    assert!(!modal.checked.contains(&1));

    let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
    match output {
        InteractionModalOutput::AskResponse {
            response: InteractionResponse::Panel(result),
            ..
        } => assert_eq!(result.selected, vec![0, 2]),
        other => panic!("Expected a Panel response, got {other:?}"),
    }
}

#[test]
fn panel_cancel() {
    let mut modal = make_panel_modal(vec!["A"], PanelHints::new());
    let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Esc)));
    match output {
        InteractionModalOutput::AskResponse { response, .. } => match response {
            InteractionResponse::Panel(result) => assert!(result.cancelled),
            _ => panic!("Expected Panel response"),
        },
        _ => panic!("Expected AskResponse"),
    }
}

#[test]
fn panel_filter_narrows_visible() {
    let mut modal = make_panel_modal(
        vec!["Apple", "Banana", "Avocado"],
        PanelHints::new().filterable(),
    );

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('/'))));
    assert_eq!(modal.mode, InteractionMode::Filter);

    for c in "a".chars() {
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char(c))));
    }

    let state = modal.panel_state.as_ref().unwrap();
    assert_eq!(state.visible.len(), 3);

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('p'))));
    let state = modal.panel_state.as_ref().unwrap();
    assert_eq!(state.visible.len(), 1);
    assert_eq!(state.visible[0], 0);
}

#[test]
fn panel_initial_selection_applied() {
    let modal = make_panel_modal(
        vec!["A", "B", "C"],
        PanelHints::new().multi_select().initial_selection([1, 2]),
    );
    assert!(modal.checked.contains(&1));
    assert!(modal.checked.contains(&2));
    assert!(!modal.checked.contains(&0));
}

fn other_text(output: InteractionModalOutput) -> Option<String> {
    match output {
        InteractionModalOutput::AskResponse {
            response: InteractionResponse::Panel(result),
            ..
        } => result.other,
        other => panic!("Expected a Panel response, got {other:?}"),
    }
}

/// The panel draws an "Other..." row after the items. The cursor reaches it,
/// and Enter on it opens the text input.
#[test]
fn panel_other_slot_takes_text() {
    let mut modal = make_panel_modal(vec!["A", "B"], PanelHints::new().allow_other());

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Up)));
    assert_eq!(modal.panel_state.as_ref().unwrap().cursor, 2);
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
    assert_eq!(modal.mode, InteractionMode::TextInput);

    for c in "mine".chars() {
        modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char(c))));
    }
    let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
    assert_eq!(other_text(output), Some("mine".to_string()));
}

/// In a panel that filters and allows "Other", the filter and the text of
/// the "Other" slot are separate inputs.
#[test]
fn panel_filter_and_other_text_stay_apart() {
    let mut modal = make_panel_modal(
        vec!["Apple", "Banana"],
        PanelHints::new().filterable().allow_other(),
    );

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('/'))));
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('b'))));
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
    assert_eq!(modal.mode, InteractionMode::Selecting);
    assert_eq!(modal.panel_state.as_ref().unwrap().visible, vec![1]);

    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Down)));
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
    assert_eq!(modal.mode, InteractionMode::TextInput);
    modal.update(InteractionModalMsg::Key(key_event(KeyCode::Char('z'))));

    assert_eq!(modal.panel_state.as_ref().unwrap().filter, "b");
    let output = modal.update(InteractionModalMsg::Key(key_event(KeyCode::Enter)));
    assert_eq!(other_text(output), Some("z".to_string()));
}
