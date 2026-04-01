//! Unit tests for OilChatApp.
//!
//! TODO(rewrite): Most tests deleted during Phase 0. Rebuild in Phase 7.
//! Only framework-level tests (mode cycling, parsing) survive.

use super::*;

#[test]
fn test_mode_cycle() {
    assert_eq!(ChatMode::Normal.cycle(), ChatMode::Plan);
    assert_eq!(ChatMode::Plan.cycle(), ChatMode::Auto);
    assert_eq!(ChatMode::Auto.cycle(), ChatMode::Normal);
}

#[test]
fn test_mode_from_str() {
    assert_eq!(ChatMode::parse("normal"), ChatMode::Normal);
    assert_eq!(ChatMode::parse("default"), ChatMode::Normal);
    assert_eq!(ChatMode::parse("plan"), ChatMode::Plan);
    assert_eq!(ChatMode::parse("auto"), ChatMode::Auto);
    assert_eq!(ChatMode::parse("unknown"), ChatMode::Normal);
}

#[test]
fn test_app_init() {
    let app = OilChatApp::init();
    assert!(!app.is_streaming());
    assert_eq!(app.mode, ChatMode::Normal);
}
