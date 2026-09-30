//! Wire-compatibility proof for step 19 gap 2A.
//!
//! Each fixture in `assets/fixtures/golden/replies/` holds the JSON of one
//! reply shape, captured from the pre-change `crucible-daemon` handler — the
//! commit just before gap 2A gave the method a named reply type (see the
//! Simplification Plan, step 19) — by running the real daemon over a real
//! socket (or, for the two cases that need to seed internal registry state,
//! the real `RpcDispatcher` in-process) and recording its actual reply.
//! Session ids, generated request ids and timestamps are normalized to fixed
//! placeholders; everything else is what the old code actually produced.
//!
//! A test reads the fixture, deserializes each case into the new type, and
//! re-serializes it, so a difference between the fixture and either
//! direction is a wire change.
//!
//! A change here on purpose (the wire moving) means editing the fixture by
//! hand in the same commit as the code change, and saying why.

use super::*;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fixtures/golden/replies")
        .join(format!("{name}.json"))
}

/// Compare the JSON of each case in the fixture `name` with the reply type
/// `T`, then prove each entry survives a read and a write.
fn golden<T: Serialize + DeserializeOwned>(name: &str) {
    let path = fixture_path(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let expected: Value = serde_json::from_str(&text).expect("the fixture is JSON");
    let cases = expected.as_array().expect("a fixture is an array");
    assert!(!cases.is_empty(), "{name}: the fixture has no cases");
    for case in cases {
        let read: T = serde_json::from_value(case.clone())
            .unwrap_or_else(|e| panic!("{name}: the fixture does not read as the reply type: {e}"));
        assert_eq!(
            &serde_json::to_value(&read).expect("a reply writes JSON"),
            case,
            "{name}: the reply JSON differs from {}. The wire changed",
            path.display()
        );
    }
}

#[test]
fn session_pause_and_resume() {
    golden::<SessionTransitionReply>("session_transition");
}

#[test]
fn session_history_and_resume_from_storage() {
    golden::<SessionHistoryReply>("session_history");
}

#[test]
fn session_end() {
    golden::<SessionEndReply>("session_end");
}

#[test]
fn session_delete() {
    golden::<SessionDeleteReply>("session_delete");
}

#[test]
fn session_archive_and_unarchive() {
    golden::<SessionArchiveReply>("session_archive");
}

#[test]
fn session_replay() {
    golden::<SessionReplayStartedReply>("session_replay_started");
}

#[test]
fn session_compact() {
    golden::<SessionCompactReply>("session_compact");
}

#[test]
fn session_subscribe_and_unsubscribe() {
    golden::<SessionSubscribeReply>("session_subscribe");
    golden::<SessionUnsubscribeReply>("session_unsubscribe");
}

#[test]
fn session_set_title_and_generate_title() {
    // `session.set_title` and `session.generate_title` share
    // `SessionTitleReply`. `generate_title` needs a real LLM turn to produce
    // a title, so this fixture is captured from `set_title` alone; the
    // shared type is what the wire compatibility claim is about.
    golden::<SessionTitleReply>("session_title");
}

#[test]
fn session_configure_agent() {
    golden::<SessionConfigureAgentReply>("session_configure_agent");
}

#[test]
fn session_inject_context() {
    golden::<SessionInjectContextReply>("session_inject_context");
}

#[test]
fn session_clear() {
    golden::<SessionClearReply>("session_clear");
}

#[test]
fn session_connect_disconnect_and_set_workspace_scope() {
    // `session.set_workspace` shares `SessionScopeReply` but never reaches
    // it: `server/session/scope.rs::handle_session_set_workspace` always
    // answers `AgentError::WorkspaceFixed` (the workspace is fixed at
    // creation; the method stays on the wire only to give an older client a
    // named refusal instead of `METHOD_NOT_FOUND`). The fixture is captured
    // from `session.connect_kiln` and `session.disconnect_kiln`, which do
    // produce it.
    golden::<SessionScopeReply>("session_scope");
}

#[test]
fn session_list_models() {
    golden::<SessionListModelsReply>("session_list_models");
}

#[test]
fn session_commands() {
    golden::<SessionCommandsReply>("session_commands");
}

#[test]
fn session_list_agent_options() {
    golden::<SessionListAgentOptionsReply>("session_list_agent_options");
}

#[test]
fn session_knob_set() {
    golden::<SessionKnobSetReply>("session_knob_set");
}

#[test]
fn session_add_list_and_dismiss_notification() {
    golden::<SessionAddNotificationReply>("session_add_notification");
    golden::<SessionListNotificationsReply>("session_list_notifications");
    golden::<SessionDismissNotificationReply>("session_dismiss_notification");
}

#[test]
fn session_pending_interactions_and_respond() {
    golden::<SessionPendingInteractionsReply>("session_pending_interactions");
    golden::<SessionInteractionRespondReply>("session_interaction_respond");
}

#[test]
fn session_plugin_approval() {
    golden::<PluginApprovalReply>("plugin_approval");
    golden::<SessionListPluginApprovalsReply>("session_list_plugin_approvals");
}

#[test]
fn session_test_interaction() {
    golden::<SessionTestInteractionReply>("session_test_interaction");
}

#[test]
fn session_fork() {
    golden::<SessionForkReply>("session_fork");
}

#[test]
fn session_cache_stats() {
    golden::<SessionCacheStatsReply>("session_cache_stats");
}

#[test]
fn session_undo_can_undo_and_undo_depth() {
    golden::<SessionUndoReply>("session_undo");
    golden::<SessionCanUndoReply>("session_can_undo");
    golden::<SessionUndoDepthReply>("session_undo_depth");
}

#[test]
fn session_status() {
    golden::<SessionStatusReply>("session_status");
}

#[test]
fn session_cleanup() {
    golden::<SessionCleanupReply>("session_cleanup");
}

#[test]
fn session_render_markdown_and_export_to_file() {
    golden::<SessionRenderMarkdownResponse>("session_render_markdown");
    golden::<SessionExportToFileResponse>("session_export_to_file");
}

#[test]
fn lua_register_commands() {
    golden::<LuaRegisterCommandsReply>("lua_register_commands");
}

#[test]
fn config_set_and_save() {
    golden::<ConfigSetReply>("config_set");
    golden::<ConfigSaveReply>("config_save");
}

#[test]
fn ui_set_theme() {
    golden::<UiSetThemeReply>("ui_set_theme");
}

#[test]
fn workflow_start_approve_gate_status_and_cancel() {
    golden::<WorkflowRunReply>("workflow_run");
    golden::<WorkflowStatusReply>("workflow_status");
    golden::<WorkflowCancelReply>("workflow_cancel");
}
