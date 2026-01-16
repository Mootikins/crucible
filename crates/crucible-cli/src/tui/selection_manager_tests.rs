//! Tests for SelectionManager

use super::selection::SelectionPoint;
use super::selection_manager::SelectionManager;

#[test]
fn new_manager_has_mouse_enabled() {
    let mgr = SelectionManager::new();
    assert!(mgr.is_mouse_capture_enabled());
}

#[test]
fn new_manager_has_no_selection() {
    let mgr = SelectionManager::new();
    assert!(!mgr.has_selection());
    assert!(mgr.selection_range().is_none());
}

#[test]
fn new_manager_has_no_clipboard() {
    let mgr = SelectionManager::new();
    assert!(mgr.clipboard().is_none());
}

#[test]
fn default_equals_new() {
    let default = SelectionManager::default();
    assert!(default.is_mouse_capture_enabled());
    assert!(!default.has_selection());
}

#[test]
fn set_mouse_mode_disables() {
    let mut mgr = SelectionManager::new();
    mgr.set_mouse_mode(false);
    assert!(!mgr.is_mouse_capture_enabled());
}

#[test]
fn set_mouse_mode_enables() {
    let mut mgr = SelectionManager::new();
    mgr.set_mouse_mode(false);
    mgr.set_mouse_mode(true);
    assert!(mgr.is_mouse_capture_enabled());
}

#[test]
fn toggle_mouse_mode_toggles_and_returns_new_state() {
    let mut mgr = SelectionManager::new();
    assert!(mgr.is_mouse_capture_enabled());

    let result = mgr.toggle_mouse_mode();
    assert!(!result);
    assert!(!mgr.is_mouse_capture_enabled());

    let result = mgr.toggle_mouse_mode();
    assert!(result);
    assert!(mgr.is_mouse_capture_enabled());
}

#[test]
fn copy_stores_text() {
    let mut mgr = SelectionManager::new();
    mgr.copy("Hello World".to_string());

    assert_eq!(mgr.clipboard(), Some("Hello World"));
}

#[test]
fn copy_overwrites_previous() {
    let mut mgr = SelectionManager::new();
    mgr.copy("First".to_string());
    mgr.copy("Second".to_string());

    assert_eq!(mgr.clipboard(), Some("Second"));
}

#[test]
fn copy_empty_string() {
    let mut mgr = SelectionManager::new();
    mgr.copy("".to_string());

    assert_eq!(mgr.clipboard(), Some(""));
}

#[test]
fn copy_unicode_content() {
    let mut mgr = SelectionManager::new();
    mgr.copy("Hello 世界 🚀".to_string());

    assert_eq!(mgr.clipboard(), Some("Hello 世界 🚀"));
}

#[test]
fn start_selection_alone_does_not_create_range() {
    let mut mgr = SelectionManager::new();
    let point = SelectionPoint::new(0, 0);

    mgr.start_selection(point);

    assert!(!mgr.has_selection());
}

#[test]
fn update_selection_creates_range() {
    let mut mgr = SelectionManager::new();
    let start = SelectionPoint::new(0, 0);
    let end = SelectionPoint::new(0, 10);

    mgr.start_selection(start);
    mgr.update_selection(end);

    assert!(mgr.has_selection());
}

#[test]
fn selection_range_returns_ordered_points() {
    let mut mgr = SelectionManager::new();
    let start = SelectionPoint::new(1, 10);
    let end = SelectionPoint::new(0, 5);

    mgr.start_selection(start);
    mgr.update_selection(end);

    if let Some((p1, p2)) = mgr.selection_range() {
        assert!(p1 <= p2);
    }
}

#[test]
fn complete_selection_finalizes() {
    let mut mgr = SelectionManager::new();
    let start = SelectionPoint::new(0, 0);
    let end = SelectionPoint::new(0, 5);

    mgr.start_selection(start);
    mgr.update_selection(end);
    mgr.complete_selection();

    assert!(mgr.has_selection());
}

#[test]
fn clear_selection_removes_selection() {
    let mut mgr = SelectionManager::new();
    let start = SelectionPoint::new(0, 0);
    let end = SelectionPoint::new(0, 5);

    mgr.start_selection(start);
    mgr.update_selection(end);
    mgr.complete_selection();
    assert!(mgr.has_selection());

    mgr.clear_selection();
    assert!(!mgr.has_selection());
}

#[test]
fn clear_selection_preserves_clipboard() {
    let mut mgr = SelectionManager::new();
    mgr.copy("preserved".to_string());

    let start = SelectionPoint::new(0, 0);
    let end = SelectionPoint::new(0, 5);
    mgr.start_selection(start);
    mgr.update_selection(end);
    mgr.clear_selection();

    assert_eq!(mgr.clipboard(), Some("preserved"));
}

#[test]
fn selection_accessor_returns_state() {
    let mgr = SelectionManager::new();
    let state = mgr.selection();
    assert!(!state.has_selection());
}

#[test]
fn cache_needs_rebuild_initially() {
    let mgr = SelectionManager::new();
    assert!(mgr.cache_needs_rebuild(80));
}

#[test]
fn update_cache_with_data_then_no_rebuild_needed() {
    use super::selection::RenderedLineInfo;
    let mut mgr = SelectionManager::new();
    let lines = vec![RenderedLineInfo {
        text: "test line".to_string(),
        item_index: 0,
        is_code: false,
    }];
    mgr.update_cache(lines, 80);

    assert!(!mgr.cache_needs_rebuild(80));
}

#[test]
fn update_cache_empty_still_needs_rebuild() {
    let mut mgr = SelectionManager::new();
    mgr.update_cache(vec![], 80);

    assert!(mgr.cache_needs_rebuild(80));
}

#[test]
fn cache_needs_rebuild_on_width_change() {
    let mut mgr = SelectionManager::new();
    mgr.update_cache(vec![], 80);

    assert!(mgr.cache_needs_rebuild(100));
}

#[test]
fn invalidate_cache_marks_for_rebuild() {
    use super::selection::RenderedLineInfo;
    let mut mgr = SelectionManager::new();
    let lines = vec![RenderedLineInfo {
        text: "test".to_string(),
        item_index: 0,
        is_code: false,
    }];
    mgr.update_cache(lines, 80);
    assert!(!mgr.cache_needs_rebuild(80));

    mgr.invalidate_cache();

    assert!(mgr.cache_needs_rebuild(80));
}

#[test]
fn extract_text_from_empty_cache() {
    let mgr = SelectionManager::new();
    let start = SelectionPoint::new(0, 0);
    let end = SelectionPoint::new(0, 10);

    let text = mgr.extract_text(start, end);

    assert_eq!(text, "");
}

#[test]
fn full_selection_workflow() {
    let mut mgr = SelectionManager::new();

    mgr.start_selection(SelectionPoint::new(0, 0));
    mgr.update_selection(SelectionPoint::new(0, 10));
    mgr.complete_selection();
    assert!(mgr.has_selection());

    mgr.copy("Selected text".to_string());
    assert_eq!(mgr.clipboard(), Some("Selected text"));

    mgr.clear_selection();
    assert!(!mgr.has_selection());
    assert_eq!(mgr.clipboard(), Some("Selected text"));
}

#[test]
fn multiple_selection_cycles() {
    let mut mgr = SelectionManager::new();

    mgr.start_selection(SelectionPoint::new(0, 0));
    mgr.update_selection(SelectionPoint::new(0, 5));
    mgr.complete_selection();
    mgr.clear_selection();

    mgr.start_selection(SelectionPoint::new(1, 0));
    mgr.update_selection(SelectionPoint::new(1, 10));
    mgr.complete_selection();

    assert!(mgr.has_selection());
    let range = mgr.selection_range().unwrap();
    assert_eq!(range.0.line, 1);
}

#[test]
fn mouse_mode_independent_of_selection() {
    let mut mgr = SelectionManager::new();

    mgr.start_selection(SelectionPoint::new(0, 0));
    mgr.update_selection(SelectionPoint::new(0, 5));
    mgr.complete_selection();

    mgr.set_mouse_mode(false);

    assert!(mgr.has_selection());
    assert!(!mgr.is_mouse_capture_enabled());
}
