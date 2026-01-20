use crate::tui::oil::node::*;
use crate::tui::oil::render::render_to_string;
use crate::tui::oil::style::Style;

#[test]
fn error_boundary_renders_child_when_no_error() {
    let node = error_boundary(text("Child content"), text("Fallback"));

    let rendered = render_to_string(&node, 80);
    assert!(rendered.contains("Child content"));
    assert!(!rendered.contains("Fallback"));
}

#[test]
fn error_boundary_builder_creates_correct_structure() {
    let child = text("child");
    let fallback = text("fallback");
    let node = error_boundary(child, fallback);

    match node {
        Node::ErrorBoundary(eb) => {
            assert!(matches!(*eb.child, Node::Text(_)));
            assert!(matches!(*eb.fallback, Node::Text(_)));
        }
        _ => panic!("Expected ErrorBoundary node"),
    }
}

#[test]
fn nested_error_boundaries_work() {
    let inner = error_boundary(text("Inner"), text("Inner fallback"));
    let outer = error_boundary(inner, text("Outer fallback"));

    let rendered = render_to_string(&outer, 80);
    assert!(rendered.contains("Inner"));
    assert!(!rendered.contains("fallback"));
}

#[test]
fn error_boundary_in_col_layout() {
    let node = col([
        text("Header"),
        error_boundary(text("Content"), text("Error")),
        text("Footer"),
    ]);

    let rendered = render_to_string(&node, 80);
    assert!(rendered.contains("Header"));
    assert!(rendered.contains("Content"));
    assert!(rendered.contains("Footer"));
    assert!(!rendered.contains("Error"));
}
