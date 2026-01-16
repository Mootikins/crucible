//! Tests for InputModeManager

use super::input_mode_manager::InputModeManager;
use std::time::Instant;

#[test]
fn new_manager_not_in_rapid_mode() {
    let mgr = InputModeManager::new();
    assert!(!mgr.is_rapid_input());
}

#[test]
fn new_manager_has_empty_buffer() {
    let mgr = InputModeManager::new();
    assert_eq!(mgr.rapid_buffer(), "");
}

#[test]
fn new_manager_has_no_last_key_time() {
    let mgr = InputModeManager::new();
    assert!(mgr.last_key_time().is_none());
}

#[test]
fn default_equals_new() {
    let default = InputModeManager::default();
    assert!(!default.is_rapid_input());
    assert_eq!(default.rapid_buffer(), "");
}

#[test]
fn start_rapid_input_enables_mode() {
    let mut mgr = InputModeManager::new();
    mgr.start_rapid_input();

    assert!(mgr.is_rapid_input());
}

#[test]
fn start_rapid_input_clears_buffer() {
    let mut mgr = InputModeManager::new();
    mgr.push_char('a');
    assert_eq!(mgr.rapid_buffer(), "a");

    mgr.start_rapid_input();

    assert_eq!(mgr.rapid_buffer(), "");
}

#[test]
fn end_rapid_input_disables_mode() {
    let mut mgr = InputModeManager::new();
    mgr.start_rapid_input();
    assert!(mgr.is_rapid_input());

    mgr.end_rapid_input();

    assert!(!mgr.is_rapid_input());
}

#[test]
fn end_rapid_input_clears_buffer() {
    let mut mgr = InputModeManager::new();
    mgr.start_rapid_input();
    mgr.push_char('x');

    mgr.end_rapid_input();

    assert_eq!(mgr.rapid_buffer(), "");
}

#[test]
fn push_char_accumulates() {
    let mut mgr = InputModeManager::new();
    mgr.push_char('h');
    mgr.push_char('e');
    mgr.push_char('l');
    mgr.push_char('l');
    mgr.push_char('o');

    assert_eq!(mgr.rapid_buffer(), "hello");
}

#[test]
fn push_char_handles_unicode() {
    let mut mgr = InputModeManager::new();
    mgr.push_char('世');
    mgr.push_char('界');
    mgr.push_char('🚀');

    assert_eq!(mgr.rapid_buffer(), "世界🚀");
}

#[test]
fn clear_rapid_buffer_empties_buffer() {
    let mut mgr = InputModeManager::new();
    mgr.push_char('t');
    mgr.push_char('e');
    mgr.push_char('s');
    mgr.push_char('t');

    mgr.clear_rapid_buffer();

    assert_eq!(mgr.rapid_buffer(), "");
}

#[test]
fn clear_rapid_buffer_preserves_mode() {
    let mut mgr = InputModeManager::new();
    mgr.start_rapid_input();
    mgr.push_char('x');

    mgr.clear_rapid_buffer();

    assert!(mgr.is_rapid_input());
}

#[test]
fn set_last_key_time_stores_instant() {
    let mut mgr = InputModeManager::new();
    let now = Instant::now();

    mgr.set_last_key_time(now);

    assert!(mgr.last_key_time().is_some());
}

#[test]
fn clear_last_key_time_removes_instant() {
    let mut mgr = InputModeManager::new();
    mgr.set_last_key_time(Instant::now());
    assert!(mgr.last_key_time().is_some());

    mgr.clear_last_key_time();

    assert!(mgr.last_key_time().is_none());
}

#[test]
fn rapid_input_full_cycle() {
    let mut mgr = InputModeManager::new();

    mgr.start_rapid_input();
    assert!(mgr.is_rapid_input());

    mgr.push_char('p');
    mgr.push_char('a');
    mgr.push_char('s');
    mgr.push_char('t');
    mgr.push_char('e');
    assert_eq!(mgr.rapid_buffer(), "paste");

    mgr.end_rapid_input();
    assert!(!mgr.is_rapid_input());
    assert_eq!(mgr.rapid_buffer(), "");
}

#[test]
fn multiple_rapid_input_cycles() {
    let mut mgr = InputModeManager::new();

    mgr.start_rapid_input();
    mgr.push_char('a');
    mgr.end_rapid_input();

    mgr.start_rapid_input();
    mgr.push_char('b');
    assert_eq!(mgr.rapid_buffer(), "b");

    mgr.end_rapid_input();
    assert_eq!(mgr.rapid_buffer(), "");
}

#[test]
fn push_char_works_outside_rapid_mode() {
    let mut mgr = InputModeManager::new();
    assert!(!mgr.is_rapid_input());

    mgr.push_char('x');

    assert_eq!(mgr.rapid_buffer(), "x");
}

#[test]
fn last_key_time_independent_of_rapid_mode() {
    let mut mgr = InputModeManager::new();
    let now = Instant::now();

    mgr.set_last_key_time(now);
    mgr.start_rapid_input();
    mgr.end_rapid_input();

    assert!(mgr.last_key_time().is_some());
}
