//! The golden reply fixtures: the JSON that each `rpc_methods!` reply type
//! writes.
//!
//! Each fixture in `assets/fixtures/golden/replies/` holds the JSON of one
//! reply shape, captured from the daemon handler BEFORE it was rewritten to
//! build the typed struct in this module (part B of step 19, gap 2). A test
//! reads the fixture, deserializes it into the new type, and re-serializes
//! it, so a difference between the fixture and either direction is a wire
//! change.
//!
//! The fixtures hold the JSON of the code before this change, so no test in
//! this module writes them. A fixture that this module's own code produced
//! proves nothing about compatibility.

use super::*;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fixtures/golden/replies")
        .join(format!("{name}.json"))
}

/// Compare the JSON of each case with the fixture `name`, then prove each
/// fixture entry survives a read and a write.
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
fn kiln_open_reply() {
    golden::<KilnOpenReply>("kiln_open");
}

#[test]
fn status_reply() {
    golden::<StatusReply>("status_reply");
}

#[test]
fn kiln_register_reply() {
    golden::<KilnRegisterReply>("kiln_register");
}

#[test]
fn kiln_forget_reply() {
    golden::<KilnForgetReply>("kiln_forget");
}

#[test]
fn llm_register_provider_reply() {
    golden::<LlmRegisterProviderReply>("llm_register_provider");
}

#[test]
fn search_text_reply() {
    golden::<Vec<FtsResult>>("search_text");
}

#[test]
fn search_grep_reply() {
    golden::<GrepSearchResponse>("search_grep");
}

#[test]
fn embed_query_reply() {
    golden::<EmbedQueryReply>("embed_query");
}

#[test]
fn note_upsert_reply() {
    golden::<NoteUpsertReply>("note_upsert");
}

#[test]
fn process_file_reply() {
    golden::<ProcessFileReply>("process_file");
}

#[test]
fn process_batch_reply() {
    golden::<ProcessBatchReply>("process_batch");
}

#[test]
fn project_open_kilns_reply() {
    golden::<ProjectOpenKilnsReply>("project_open_kilns");
}

#[test]
fn scm_clone_response() {
    golden::<ScmCloneResponse>("scm_clone");
}

#[test]
fn fs_list_dir_reply() {
    golden::<FsListing>("fs_list_dir");
}

#[test]
fn fs_move_reply() {
    golden::<FsMoveReply>("fs_move");
}

#[test]
fn fs_mkdir_reply() {
    golden::<FsMkdirReply>("fs_mkdir");
}

#[test]
fn fs_trash_reply() {
    golden::<FsTrashReply>("fs_trash");
}

#[test]
fn note_rename_reply() {
    golden::<NoteRenameReply>("note_rename");
}

#[test]
fn not_implemented_reply() {
    golden::<NotImplementedReply>("not_implemented");
}

#[test]
fn mcp_start_reply() {
    golden::<McpStartReply>("mcp_start");
}

#[test]
fn mcp_stop_reply() {
    golden::<McpStopReply>("mcp_stop");
}

#[test]
fn mcp_status_reply() {
    golden::<McpStatus>("mcp_status");
}

#[test]
fn agents_list_profiles_reply() {
    golden::<AgentProfilesReply>("agents_list_profiles");
}

#[test]
fn agents_list_cards_reply() {
    golden::<AgentCardsListReply>("agents_list_cards");
}

#[test]
fn agents_resolve_profile_reply() {
    golden::<AgentProfileResolved>("agents_resolve_profile");
    // `agents.resolve_profile` answers `null` for a name the daemon has no
    // profile for; the `Option` wrapper carries that, not the fixture above.
    let none: Option<AgentProfileResolved> = serde_json::from_value(Value::Null).unwrap();
    assert!(none.is_none());
}

#[test]
fn models_list_reply() {
    golden::<ModelsListReply>("models_list");
}

#[test]
fn providers_list_reply() {
    golden::<ProvidersListReply>("providers_list");
}

#[test]
fn webhook_receive_reply() {
    golden::<WebhookReceiveReply>("webhook_receive");
}

#[test]
fn suggest_links_reply() {
    golden::<SuggestLinksReply>("suggest_links");
}
