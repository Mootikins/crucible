//! Unit tests for OilChatApp.
//!
//! Framework-level tests (mode cycling, parsing). Additional tests live in
//! `tui/oil/tests/` as snapshot and interaction tests.

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

// ─── Task 1.3: setup events populate OilChatApp ─────────────────────

#[test]
fn setup_events_populate_app_progressively() {
    use crate::tui::oil::app::App;
    use crucible_core::protocol::session_events::{ContextLimitSource, SessionInitializedPayload};
    use std::path::PathBuf;

    let mut app = OilChatApp::init();
    app.set_status("Loading...");
    assert_eq!(app.status_text(), "Loading...");

    // session_initialized: model + mode update; agent_name is informational.
    app.on_message(ChatAppMsg::SessionInitialized(SessionInitializedPayload {
        model: "glm-5".into(),
        mode: "plan".into(),
        agent_name: Some("claude".into()),
        kiln_path: PathBuf::from("/k"),
        workspace_path: PathBuf::from("/w"),
    }));
    assert_eq!(app.current_model(), "glm-5");
    assert_eq!(app.mode, ChatMode::Plan);

    // workspace_indexed / kiln_notes_indexed: Loading... stays.
    app.on_message(ChatAppMsg::WorkspaceIndexed(vec!["src/lib.rs".into()]));
    app.on_message(ChatAppMsg::KilnNotesIndexed(vec!["note:Daily.md".into()]));
    assert_eq!(app.status_text(), "Loading...");

    // context_limit_resolved: context_total updates.
    app.on_message(ChatAppMsg::ContextLimitResolved {
        limit: 128_000,
        source: ContextLimitSource::ProviderApi,
    });
    assert_eq!(app.context_usage(), (0, 128_000));

    // mcp_servers_ready: flips status to Ready.
    app.on_message(ChatAppMsg::McpServersReady(vec![]));
    assert_eq!(app.status_text(), "Ready");
}

#[test]
fn session_initialized_preserves_model_when_empty_string() {
    use crate::tui::oil::app::App;
    use crucible_core::protocol::session_events::SessionInitializedPayload;
    use std::path::PathBuf;

    let mut app = OilChatApp::init();
    app.set_model("existing-model");

    app.on_message(ChatAppMsg::SessionInitialized(SessionInitializedPayload {
        model: String::new(),
        mode: "normal".into(),
        agent_name: None,
        kiln_path: PathBuf::from("/k"),
        workspace_path: PathBuf::from("/w"),
    }));

    // Empty model does NOT clobber the existing display value.
    assert_eq!(app.current_model(), "existing-model");
}

#[test]
fn plugins_discovered_raises_notification_for_failed_plugin() {
    use crate::tui::oil::app::App;
    use crucible_core::types::PluginStatusEntry;

    let mut app = OilChatApp::init();
    assert!(!app.has_notifications());

    app.on_message(ChatAppMsg::PluginsDiscovered(vec![PluginStatusEntry {
        name: "broken".into(),
        version: "0.1.0".into(),
        state: "failed".into(),
        error: Some("bad Lua".into()),
    }]));

    assert!(app.has_notifications());
}
