//! Unit tests for OilChatApp.
//!
//! Framework-level tests (mode cycling, labelling). Additional tests live in
//! `tui/oil/tests/` as snapshot and interaction tests.

use super::*;

#[test]
fn mode_cycles_through_the_daemon_s_list_including_a_lua_declared_one() {
    let modes: Vec<String> = ["ask", "plan", "auto", "review"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    assert_eq!(next_mode("ask", &modes).as_deref(), Some("plan"));
    assert_eq!(next_mode("auto", &modes).as_deref(), Some("review"));
    assert_eq!(
        next_mode("review", &modes).as_deref(),
        Some("ask"),
        "the last declared mode wraps to the first"
    );
}

#[test]
fn a_mode_absent_from_the_daemon_s_list_cycles_nowhere() {
    let modes = vec!["ask".to_string(), "plan".to_string()];

    assert_eq!(
        next_mode("review", &modes),
        None,
        "a mode whose declaration is gone must not advance into another one"
    );
    assert_eq!(next_mode("ask", &[]), None, "an empty list cycles nowhere");
}

#[test]
fn mode_label_badges_a_mode_the_tui_has_never_heard_of() {
    // The built-ins keep their exact labels — this is what holds the
    // statusline snapshots still.
    assert_eq!(mode_label("ask"), " ASK ");
    assert_eq!(mode_label("plan"), " PLAN ");
    assert_eq!(mode_label("auto"), " AUTO ");
    assert_eq!(mode_label("review"), " REVIEW ");
}

/// The mode ids an ACP agent declares are its own, not Crucible's. Since
/// `get_modes` reports the agent's set, a delegated session's statusline and
/// mode cycle are driven by ids this crate has never seen — camelCase from
/// claude-agent-acp, hyphenated from codex-acp.
///
/// The badge derives from the id rather than matching a known list, so this
/// is about proving the derivation survives real ACP ids, and that cycling
/// walks the agent's set instead of the shipped one.
#[test]
fn an_acp_agents_own_mode_ids_render_and_cycle() {
    // Exactly what claude-agent-acp 0.73.0 declares.
    let claude: Vec<String> = [
        "default",
        "acceptEdits",
        "plan",
        "auto",
        "bypassPermissions",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    // The badge humanizes the id before upper-casing it, using the one naming
    // rule the rest of the system uses. Raw upper-casing read as
    // ` ACCEPTEDITS `, which is a word no one wrote.
    assert_eq!(mode_label("acceptEdits"), " ACCEPT EDITS ");
    assert_eq!(mode_label("bypassPermissions"), " BYPASS PERMISSIONS ");
    assert_eq!(mode_label("default"), " DEFAULT ");

    assert_eq!(
        next_mode("default", &claude).as_deref(),
        Some("acceptEdits"),
        "cycling must walk the agent's set, not the shipped ask/plan/auto"
    );
    assert_eq!(
        next_mode("bypassPermissions", &claude).as_deref(),
        Some("default"),
        "the last of the agent's modes wraps to its first"
    );

    // codex-acp's ids are hyphenated, and none of them is a Crucible mode.
    let codex: Vec<String> = ["read-only", "auto", "full-access"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(mode_label("full-access"), " FULL ACCESS ");
    assert_eq!(next_mode("read-only", &codex).as_deref(), Some("auto"));

    // A mode Crucible ships but this agent does not offer cycles nowhere,
    // rather than advancing into something `set_mode` would reject.
    assert_eq!(
        next_mode("ask", &claude),
        None,
        "a current mode absent from the agent's set must not cycle"
    );
}

#[test]
fn test_app_init() {
    let app = OilChatApp::default();
    assert!(!app.is_streaming());
    assert_eq!(&*app.mode, "ask");
}

// ─── Task 1.3: setup events populate OilChatApp ─────────────────────

#[test]
fn setup_events_populate_app_progressively() {
    use crucible_core::protocol::session_events::{ContextLimitSource, SessionInitializedPayload};
    use std::path::PathBuf;

    let mut app = OilChatApp::default();
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
    use crucible_core::protocol::session_events::SessionInitializedPayload;
    use std::path::PathBuf;

    let mut app = OilChatApp::default();
    app.set_model("existing-model");

    app.on_message(ChatAppMsg::SessionInitialized(SessionInitializedPayload {
        model: String::new(),
        mode: "ask".into(),
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
    let mut app = OilChatApp::default();
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
    let mut app = OilChatApp::default();
    app.handle_set_command("set show_diffs=0");
    assert!(!app.show_diffs(), "':set show_diffs=0' should disable");
}

#[test]
fn plugins_discovered_raises_notification_for_failed_plugin() {
    use crucible_core::types::PluginStatusEntry;

    let mut app = OilChatApp::default();
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
    let mut app = OilChatApp::default();
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
    let mut app = OilChatApp::default();
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

    let mut app = OilChatApp::default();
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

    let mut app = OilChatApp::default();
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

#[test]
fn precognition_result_renders_as_a_system_line_listing_notes() {
    use crucible_core::traits::chat::PrecognitionNoteInfo;

    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::PrecognitionResult {
        notes_count: 2,
        notes: vec![
            PrecognitionNoteInfo {
                title: "Kilns".into(),
                kiln: Some("docs".parse().unwrap()),
                score: 0.91,
            },
            PrecognitionNoteInfo {
                title: "Wikilinks".into(),
                kiln: None,
                score: 0.72,
            },
        ],
    });

    let nodes = app.container_list().nodes();
    let last = nodes.last().expect("a node was added");
    let focus = crucible_oil::focus::FocusContext::default();
    let ctx = crate::tui::oil::ViewContext::new(&focus);
    let rendered = crucible_oil::render::render_to_plain_text(&last.render(None, &ctx), 120);
    assert!(
        rendered.contains("precognition pulled 2 notes"),
        "count line missing: {rendered}"
    );
    assert!(
        rendered.contains("Kilns (docs, 0.91)"),
        "kiln-labelled entry missing: {rendered}"
    );
    assert!(
        rendered.contains("Wikilinks (0.72)"),
        "unlabelled entry missing: {rendered}"
    );
}

#[test]
fn an_empty_precognition_result_adds_nothing() {
    let mut app = OilChatApp::default();
    let before = app.container_list().nodes().len();
    app.on_message(ChatAppMsg::PrecognitionResult {
        notes_count: 0,
        notes: vec![],
    });
    assert_eq!(app.container_list().nodes().len(), before);
}

/// Render the last transcript node as plain text.
fn last_node_text(app: &OilChatApp, width: usize) -> String {
    let nodes = app.container_list().nodes();
    let last = nodes.last().expect("a node was added");
    let focus = crucible_oil::focus::FocusContext::default();
    let ctx = crate::tui::oil::ViewContext::new(&focus);
    crucible_oil::render::render_to_plain_text(&last.render(None, &ctx), width)
}

#[test]
fn the_startup_banner_names_every_attached_kiln_and_its_path() {
    let mut app = OilChatApp::default();
    app.announce_kilns(
        &[
            KilnSummary {
                name: "crucible".into(),
                path: "/home/u/crucible".into(),
            },
            KilnSummary {
                name: "notes".into(),
                path: "/home/u/notes".into(),
            },
        ],
        0,
    );

    let rendered = last_node_text(&app, 120);
    assert!(
        rendered.contains("2 kilns attached"),
        "count line missing: {rendered}"
    );
    assert!(
        rendered.contains("crucible  /home/u/crucible"),
        "first kiln missing: {rendered}"
    );
    assert!(
        rendered.contains("notes     /home/u/notes"),
        "second kiln missing, or the names are not aligned: {rendered}"
    );
}

#[test]
fn the_startup_banner_says_how_many_proposals_are_pending() {
    let kiln = || KilnSummary {
        name: "notes".into(),
        path: "/k/notes".into(),
    };

    let mut app = OilChatApp::default();
    app.announce_kilns(&[kiln()], 2);
    let rendered = last_node_text(&app, 120);
    assert!(
        rendered.contains("2 proposals pending"),
        "plural count missing: {rendered}"
    );
    assert!(
        rendered.contains("cru proposals list"),
        "the review command is missing: {rendered}"
    );

    let mut app = OilChatApp::default();
    app.announce_kilns(&[kiln()], 1);
    let rendered = last_node_text(&app, 120);
    assert!(
        rendered.contains("1 proposal pending"),
        "singular missing: {rendered}"
    );

    let mut app = OilChatApp::default();
    app.announce_kilns(&[kiln()], 0);
    let rendered = last_node_text(&app, 120);
    assert!(
        !rendered.contains("pending"),
        "no proposal must add no line: {rendered}"
    );
}

#[test]
fn a_session_with_no_kiln_is_told_so() {
    let mut app = OilChatApp::default();
    app.announce_kilns(&[], 0);

    let rendered = last_node_text(&app, 120);
    assert!(
        rendered.contains("No kiln is attached"),
        "an empty attachment must say so: {rendered}"
    );
}

#[test]
fn one_kiln_reads_as_one() {
    let mut app = OilChatApp::default();
    app.announce_kilns(
        &[KilnSummary {
            name: "crucible".into(),
            path: "/home/u/crucible".into(),
        }],
        0,
    );

    let rendered = last_node_text(&app, 120);
    assert!(
        rendered.contains("1 kiln attached"),
        "singular missing: {rendered}"
    );
}

// ─── Frame clock ────────────────────────────────────────────────────────────

fn running_tool_call() -> ChatAppMsg {
    ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: "{}".into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: Vec::new(),
        auto_approved: None,
    }
}

#[test]
fn a_slow_tool_split_follows_the_frame_clock() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("run it".into()));
    app.on_message(running_tool_call());
    let start = app.frame_time();

    assert!(!app.split_slow_tools(), "no time passed on the frame clock");

    app.set_frame_time(start + BACKGROUND_TOOL_SPLIT_THRESHOLD);
    assert!(
        app.split_slow_tools(),
        "the threshold passed on the frame clock"
    );
    assert_eq!(app.container_list().background_task_count(), 1);
}

/// The transcript must not read the wall clock. A replay test feeds a
/// recording through `render_frame` with the frame clock frozen; on a slow CI
/// runner the replay itself outran the split threshold, an incomplete tool
/// left its group above the viewport, and the answer below it scrolled out of
/// the assertion. The sleep here is deliberate: it is the only way to prove
/// that real elapsed time changes nothing.
#[test]
fn the_wall_clock_alone_never_splits_a_tool() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("run it".into()));
    app.on_message(running_tool_call());

    std::thread::sleep(BACKGROUND_TOOL_SPLIT_THRESHOLD + std::time::Duration::from_millis(50));

    assert!(
        !app.split_slow_tools(),
        "the frame clock did not move, so the tool must stay in its group"
    );
    assert_eq!(app.container_list().background_task_count(), 0);
}
