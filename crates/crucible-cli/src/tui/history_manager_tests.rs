//! Tests for HistoryManager
//!
//! Tests navigation, boundary conditions, and saved input restoration.

use super::history_manager::HistoryManager;

// =============================================================================
// Construction
// =============================================================================

#[test]
fn new_manager_is_empty() {
    let mgr = HistoryManager::new();
    assert!(mgr.is_empty());
    assert_eq!(mgr.len(), 0);
    assert_eq!(mgr.index(), 0);
}

#[test]
fn default_equals_new() {
    let default = HistoryManager::default();
    let new = HistoryManager::new();
    assert_eq!(default.len(), new.len());
    assert_eq!(default.index(), new.index());
}

// =============================================================================
// Push and basic access
// =============================================================================

#[test]
fn push_adds_entry() {
    let mut mgr = HistoryManager::new();
    mgr.push("first".to_string());

    assert_eq!(mgr.len(), 1);
    assert_eq!(mgr.get(0), Some("first"));
    assert_eq!(mgr.last(), Some("first"));
}

#[test]
fn push_moves_index_to_end() {
    let mut mgr = HistoryManager::new();
    mgr.push("first".to_string());
    mgr.push("second".to_string());

    // Index should be at end (past all entries)
    assert_eq!(mgr.index(), 2);
    assert_eq!(mgr.len(), 2);
}

#[test]
fn get_returns_none_for_invalid_index() {
    let mgr = HistoryManager::new();
    assert_eq!(mgr.get(0), None);
    assert_eq!(mgr.get(100), None);
}

#[test]
fn last_returns_none_when_empty() {
    let mgr = HistoryManager::new();
    assert_eq!(mgr.last(), None);
}

// =============================================================================
// Navigation: prev()
// =============================================================================

#[test]
fn prev_on_empty_returns_none() {
    let mut mgr = HistoryManager::new();
    assert_eq!(mgr.prev("current"), None);
}

#[test]
fn prev_returns_last_entry() {
    let mut mgr = HistoryManager::new();
    mgr.push("first".to_string());
    mgr.push("second".to_string());

    assert_eq!(mgr.prev("current input"), Some("second"));
}

#[test]
fn prev_saves_current_input_on_first_navigation() {
    let mut mgr = HistoryManager::new();
    mgr.push("history".to_string());

    let _ = mgr.prev("my current typing");
    assert_eq!(mgr.saved_input(), "my current typing");
}

#[test]
fn prev_navigates_backwards_through_history() {
    let mut mgr = HistoryManager::new();
    mgr.push("first".to_string());
    mgr.push("second".to_string());
    mgr.push("third".to_string());

    assert_eq!(mgr.prev(""), Some("third"));
    assert_eq!(mgr.prev(""), Some("second"));
    assert_eq!(mgr.prev(""), Some("first"));
}

#[test]
fn prev_stops_at_beginning() {
    let mut mgr = HistoryManager::new();
    mgr.push("only".to_string());

    assert_eq!(mgr.prev(""), Some("only"));
    assert_eq!(mgr.prev(""), None); // Already at start
    assert_eq!(mgr.index(), 0);
}

#[test]
fn prev_does_not_overwrite_saved_input_on_subsequent_calls() {
    let mut mgr = HistoryManager::new();
    mgr.push("first".to_string());
    mgr.push("second".to_string());

    // First navigation saves input
    mgr.prev("original input");
    assert_eq!(mgr.saved_input(), "original input");

    // Second navigation should NOT overwrite
    mgr.prev("should be ignored");
    assert_eq!(mgr.saved_input(), "original input");
}

// =============================================================================
// Navigation: next_entry()
// =============================================================================

#[test]
fn next_at_end_returns_none() {
    let mut mgr = HistoryManager::new();
    mgr.push("entry".to_string());
    // Index is already at end
    assert_eq!(mgr.next_entry(), None);
}

#[test]
fn next_after_prev_returns_next_entry() {
    let mut mgr = HistoryManager::new();
    mgr.push("first".to_string());
    mgr.push("second".to_string());

    mgr.prev(""); // Now at "second"
    mgr.prev(""); // Now at "first"

    assert_eq!(mgr.next_entry(), Some("second"));
}

#[test]
fn next_past_end_returns_saved_input() {
    let mut mgr = HistoryManager::new();
    mgr.push("history".to_string());

    mgr.prev("my typing"); // Saves "my typing", returns "history"

    // Going forward should restore saved input
    assert_eq!(mgr.next_entry(), Some("my typing"));
}

#[test]
fn next_past_saved_input_returns_none() {
    let mut mgr = HistoryManager::new();
    mgr.push("history".to_string());

    mgr.prev("typing");
    mgr.next_entry(); // Returns saved input
    assert_eq!(mgr.next_entry(), None); // Past the end
}

// =============================================================================
// Reset
// =============================================================================

#[test]
fn reset_moves_index_to_end() {
    let mut mgr = HistoryManager::new();
    mgr.push("first".to_string());
    mgr.push("second".to_string());

    mgr.prev(""); // Move to "second"
    mgr.prev(""); // Move to "first"
    assert_eq!(mgr.index(), 0);

    mgr.reset();
    assert_eq!(mgr.index(), 2); // Back at end
}

#[test]
fn reset_on_empty_stays_at_zero() {
    let mut mgr = HistoryManager::new();
    mgr.reset();
    assert_eq!(mgr.index(), 0);
}

// =============================================================================
// Edge cases
// =============================================================================

#[test]
fn rapid_prev_next_preserves_saved_input() {
    let mut mgr = HistoryManager::new();
    mgr.push("a".to_string());
    mgr.push("b".to_string());

    mgr.prev("original");
    mgr.prev("");
    mgr.next_entry();
    mgr.next_entry();

    assert_eq!(mgr.saved_input(), "original");
}

#[test]
fn push_after_navigation_resets_to_end() {
    let mut mgr = HistoryManager::new();
    mgr.push("first".to_string());

    mgr.prev("typing"); // Navigate back
    assert_eq!(mgr.index(), 0);

    mgr.push("new".to_string()); // Add new entry
    assert_eq!(mgr.index(), 2); // Index at end
}

#[test]
fn multiple_push_maintains_order() {
    let mut mgr = HistoryManager::new();
    mgr.push("one".to_string());
    mgr.push("two".to_string());
    mgr.push("three".to_string());

    assert_eq!(mgr.get(0), Some("one"));
    assert_eq!(mgr.get(1), Some("two"));
    assert_eq!(mgr.get(2), Some("three"));
}

#[test]
fn empty_string_can_be_pushed() {
    let mut mgr = HistoryManager::new();
    mgr.push("".to_string());

    assert_eq!(mgr.len(), 1);
    assert_eq!(mgr.get(0), Some(""));
}

#[test]
fn unicode_content_handled_correctly() {
    let mut mgr = HistoryManager::new();
    mgr.push("Hello 世界 🌍".to_string());

    assert_eq!(mgr.get(0), Some("Hello 世界 🌍"));
    assert_eq!(mgr.prev(""), Some("Hello 世界 🌍"));
}

#[test]
fn full_navigation_cycle() {
    let mut mgr = HistoryManager::new();
    mgr.push("cmd1".to_string());
    mgr.push("cmd2".to_string());
    mgr.push("cmd3".to_string());

    // Navigate all the way back
    assert_eq!(mgr.prev("current"), Some("cmd3"));
    assert_eq!(mgr.prev("ignored"), Some("cmd2"));
    assert_eq!(mgr.prev("ignored"), Some("cmd1"));
    assert_eq!(mgr.prev("ignored"), None);

    // Navigate all the way forward
    assert_eq!(mgr.next_entry(), Some("cmd2"));
    assert_eq!(mgr.next_entry(), Some("cmd3"));
    assert_eq!(mgr.next_entry(), Some("current")); // Saved input
    assert_eq!(mgr.next_entry(), None);
}
