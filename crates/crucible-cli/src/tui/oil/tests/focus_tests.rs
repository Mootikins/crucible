use crate::tui::oil::app::ViewContext;
use crate::tui::oil::focus::{FocusContext, FocusId};

#[test]
fn view_context_is_focused_returns_correct_state() {
    let mut focus = FocusContext::new();
    focus.register(FocusId::new("a"), false);
    focus.register(FocusId::new("b"), false);
    focus.focus("a");

    let ctx = ViewContext::new(&focus);
    assert!(ctx.is_focused("a"));
    assert!(!ctx.is_focused("b"));
    assert!(!ctx.is_focused("nonexistent"));
}

#[test]
fn focus_cycles_through_registered_items() {
    let mut focus = FocusContext::new();
    focus.register(FocusId::new("first"), false);
    focus.register(FocusId::new("second"), false);
    focus.register(FocusId::new("third"), false);

    assert!(focus.active_id().is_none());

    focus.focus_next();
    assert!(focus.is_focused("first"));

    focus.focus_next();
    assert!(focus.is_focused("second"));

    focus.focus_next();
    assert!(focus.is_focused("third"));

    focus.focus_next();
    assert!(focus.is_focused("first"));
}

#[test]
fn focus_prev_cycles_backwards() {
    let mut focus = FocusContext::new();
    focus.register(FocusId::new("first"), false);
    focus.register(FocusId::new("second"), false);
    focus.register(FocusId::new("third"), false);
    focus.focus("third");

    focus.focus_prev();
    assert!(focus.is_focused("second"));

    focus.focus_prev();
    assert!(focus.is_focused("first"));

    focus.focus_prev();
    assert!(focus.is_focused("third"));
}

#[test]
fn auto_focus_sets_initial_focus() {
    let mut focus = FocusContext::new();
    focus.register(FocusId::new("first"), true);
    focus.register(FocusId::new("second"), false);

    focus.apply_auto_focus();

    assert!(focus.is_focused("first"));
}

#[test]
fn clear_registrations_removes_all() {
    let mut focus = FocusContext::new();
    focus.register(FocusId::new("a"), false);
    focus.register(FocusId::new("b"), false);
    focus.focus("a");

    focus.clear_registrations();

    assert!(focus.focus_order().is_empty());
    assert!(focus.active_id().is_none());
}
