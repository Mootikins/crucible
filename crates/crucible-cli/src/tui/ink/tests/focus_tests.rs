use crate::tui::ink::app::{Action, App, ViewContext};
use crate::tui::ink::event::Event;
use crate::tui::ink::focus::{FocusContext, FocusId};
use crate::tui::ink::node::*;
use crate::tui::ink::render::render_to_string;
use crate::tui::ink::style::Style;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

struct FocusableApp {
    items: Vec<String>,
}

impl App for FocusableApp {
    type Msg = ();

    fn init() -> Self {
        Self {
            items: vec!["Item A".into(), "Item B".into(), "Item C".into()],
        }
    }

    fn view(&self, ctx: &ViewContext<'_>) -> Node {
        col(self.items.iter().enumerate().map(|(i, item)| {
            let id = format!("item-{}", i);
            let style = if ctx.is_focused(&id) {
                Style::new().bold()
            } else {
                Style::default()
            };
            focusable(&id, styled(item, style))
        }))
    }

    fn update(&mut self, _event: Event) -> Action<()> {
        Action::Continue
    }
}

#[test]
fn focusable_node_renders_child() {
    let app = FocusableApp::init();
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);

    let rendered = render_to_string(&tree, 80);
    assert!(rendered.contains("Item A"));
    assert!(rendered.contains("Item B"));
    assert!(rendered.contains("Item C"));
}

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

#[test]
fn focusable_node_builder_creates_correct_node() {
    let child = text("content");
    let node = focusable("my-id", child);

    match node {
        Node::Focusable(f) => {
            assert_eq!(f.id.0, "my-id");
            assert!(!f.auto_focus);
        }
        _ => panic!("Expected Focusable node"),
    }
}

#[test]
fn focusable_auto_builder_sets_auto_focus() {
    let child = text("content");
    let node = focusable_auto("my-id", child);

    match node {
        Node::Focusable(f) => {
            assert!(f.auto_focus);
        }
        _ => panic!("Expected Focusable node"),
    }
}
