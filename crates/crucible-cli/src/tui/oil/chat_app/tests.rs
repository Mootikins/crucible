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
fn set_show_diffs_disable_then_enable_round_trips_field() {
    // The :set show_diffs command flows through runtime_config.set + sync_runtime_to_fields;
    // this test locks in that the cli-visible field actually flips. Without coverage,
    // the cross-layer plumbing could regress silently.
    let mut app = OilChatApp::init();
    assert!(app.show_diffs(), "show_diffs default expected to be true");

    app.handle_set_command("set show_diffs false");
    assert!(
        !app.show_diffs(),
        "show_diffs should be false after :set show_diffs false"
    );

    app.handle_set_command("set show_diffs true");
    assert!(
        app.show_diffs(),
        "show_diffs should flip back to true on :set show_diffs true"
    );
}

#[test]
fn set_show_diffs_disable_via_short_form() {
    // `:set disable show_diffs` and `:set show_diffs=0` are alternate forms;
    // the runtime config layer normalizes both into a bool. Smoke-test one.
    let mut app = OilChatApp::init();
    app.handle_set_command("set show_diffs=0");
    assert!(!app.show_diffs(), "':set show_diffs=0' should disable");
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

// ─── US-602: shell command history storage ──────────────────────────

#[test]
fn shell_history_stores_commands_in_arrival_order() {
    let mut app = OilChatApp::init();
    app.push_shell_history("ls -la".into());
    app.push_shell_history("git status".into());
    app.push_shell_history("cargo test".into());

    let hist = &app.shell_history.shell_history;
    assert_eq!(hist.len(), 3);
    assert_eq!(hist.front().unwrap(), "ls -la");
    assert_eq!(hist.back().unwrap(), "cargo test");
}

#[test]
fn shell_history_caps_at_max_and_evicts_oldest() {
    let mut app = OilChatApp::init();
    for i in 0..(MAX_SHELL_HISTORY + 10) {
        app.push_shell_history(format!("cmd{i}"));
    }

    let hist = &app.shell_history.shell_history;
    assert_eq!(
        hist.len(),
        MAX_SHELL_HISTORY,
        "history is bounded to the last {MAX_SHELL_HISTORY} commands"
    );
    // FIFO eviction: the earliest commands drop off the front.
    assert!(
        !hist.contains(&"cmd0".to_string()),
        "the oldest command should be evicted"
    );
    assert_eq!(
        hist.back().unwrap(),
        &format!("cmd{}", MAX_SHELL_HISTORY + 9),
        "the newest command is retained"
    );
}
