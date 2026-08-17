//! Unit tests for OilChatApp.
//!
//! Framework-level tests (mode cycling, labelling). Additional tests live in
//! `tui/oil/tests/` as snapshot and interaction tests.

use super::*;

#[test]
fn mode_cycles_through_the_daemon_s_list_including_a_lua_declared_one() {
    let modes: Vec<String> = ["normal", "plan", "auto", "review"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    assert_eq!(next_mode("normal", &modes).as_deref(), Some("plan"));
    assert_eq!(next_mode("auto", &modes).as_deref(), Some("review"));
    assert_eq!(
        next_mode("review", &modes).as_deref(),
        Some("normal"),
        "the last declared mode wraps to the first"
    );
}

#[test]
fn a_mode_absent_from_the_daemon_s_list_cycles_nowhere() {
    let modes = vec!["normal".to_string(), "plan".to_string()];

    assert_eq!(
        next_mode("review", &modes),
        None,
        "a mode whose declaration is gone must not advance into another one"
    );
    assert_eq!(
        next_mode("normal", &[]),
        None,
        "an empty list cycles nowhere"
    );
}

#[test]
fn mode_label_badges_a_mode_the_tui_has_never_heard_of() {
    // The built-ins keep their exact labels — this is what holds the
    // statusline snapshots still.
    assert_eq!(mode_label("normal"), " NORMAL ");
    assert_eq!(mode_label("plan"), " PLAN ");
    assert_eq!(mode_label("auto"), " AUTO ");
    assert_eq!(mode_label("review"), " REVIEW ");
}

#[test]
fn test_app_init() {
    let app = OilChatApp::init();
    assert!(!app.is_streaming());
    assert_eq!(&*app.mode, "normal");
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
        kilns: Vec::new(),
        workspace_path: PathBuf::from("/w"),
    }));
    assert_eq!(app.current_model(), "glm-5");
    assert_eq!(&*app.mode, "plan");

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
        kilns: Vec::new(),
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

/// T1 — a finished shell command must be recorded in the transcript.
///
/// `update_shell_modal` took the `ShellHistoryItem` and did
/// `let _ = &history_item;`, so `ContainerList::add_shell_execution`,
/// `ChatNode::ShellExecution` and `render_shell_execution` were all complete
/// and all unreachable: you ran `!cargo build`, closed the modal, and the
/// conversation showed no trace of it. The rendered half is
/// `a_closed_shell_command_appears_in_the_frame`.
#[test]
fn a_closed_shell_command_is_recorded_in_the_transcript() {
    use crate::tui::oil::components::{ShellHistoryItem, ShellModalOutput};

    let mut app = OilChatApp::init();
    assert_eq!(app.container_list().nodes().len(), 0);

    app.handle_shell_modal_output(ShellModalOutput::Close {
        history_item: ShellHistoryItem {
            command: "cargo build --release".to_string(),
            exit_code: 101,
            output_tail: vec!["error: could not compile".to_string()],
            output_path: None,
        },
        insert: None,
    });

    assert_eq!(
        app.container_list().nodes().len(),
        1,
        "the command should be in the transcript"
    );
}

/// `i` fills the composer *and* records the command — both halves of one key
/// press, and it used to be zero for two: the insert was dropped by a `Tick`
/// that never came, the transcript entry by the discarded history item.
#[test]
fn inserting_shell_output_fills_the_composer_and_the_transcript() {
    use crate::tui::oil::components::{InsertedOutput, ShellHistoryItem, ShellModalOutput};

    let mut app = OilChatApp::init();
    app.handle_shell_modal_output(ShellModalOutput::Close {
        history_item: ShellHistoryItem {
            command: "echo hi".to_string(),
            exit_code: 0,
            output_tail: vec!["hi".to_string()],
            output_path: None,
        },
        insert: Some(InsertedOutput {
            content: "$ echo hi\nhi".to_string(),
            truncated: false,
        }),
    });

    assert!(
        app.input_content().contains("hi"),
        "the output should land in the composer, got: {:?}",
        app.input_content()
    );
    assert_eq!(
        app.container_list().nodes().len(),
        1,
        "and the command should still be recorded in the transcript"
    );
}
