//! The note tools in a turn whose write mode is `propose`.

use super::super::{CreateNoteParams, NotePathParams, NoteWrites, TurnWriteMode, UpdateNoteParams};
use crate::proposals::{proposals_root, ProposalStore};
use crucible_core::file_write::ExpectedBase;
use crucible_core::note_edit::disk_hash;
use crucible_core::proposal::{ProposalAuthor, ProposalState};
use crucible_core::session::{Session, SessionType};
use crucible_core::types::WriteMode;
use rmcp::handler::server::wrapper::Parameters;
use std::sync::Arc;
use tempfile::TempDir;

struct Fixture {
    kiln: TempDir,
    _data: TempDir,
    store: Arc<ProposalStore>,
    mode: TurnWriteMode,
    tools: super::super::NoteTools,
}

fn fixture(session: &Session) -> Fixture {
    let kiln = TempDir::new().unwrap();
    let data = TempDir::new().unwrap();
    let store = Arc::new(ProposalStore::new(proposals_root(data.path())));
    let mode = TurnWriteMode::default();
    mode.set(WriteMode::Propose);
    let tools = super::unindexed(kiln.path().to_string_lossy().to_string())
        .with_writes(NoteWrites::new(mode.clone(), store.clone(), session));
    Fixture {
        kiln,
        _data: data,
        store,
        mode,
        tools,
    }
}

fn answer(result: rmcp::model::CallToolResult) -> serde_json::Value {
    let text = result
        .content
        .first()
        .unwrap()
        .as_text()
        .unwrap()
        .text
        .clone();
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn create_note_in_propose_mode_leaves_the_disk_unchanged() {
    let session = Session::new(SessionType::Chat, vec![]);
    let f = fixture(&session);

    let result = f
        .tools
        .create_note(Parameters(CreateNoteParams {
            path: "Ideas/new.md".to_string(),
            content: "# New".to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();

    let answer = answer(result);
    assert_eq!(answer["status"], "proposed");
    assert!(!f.kiln.path().join("Ideas/new.md").exists());

    let listed = f.store.list(false).unwrap();
    assert_eq!(listed.len(), 1);
    let proposal = &listed[0];
    assert_eq!(answer["proposal"], serde_json::json!(proposal.id));
    assert_eq!(proposal.state, ProposalState::Open);
    assert_eq!(
        proposal.author,
        ProposalAuthor::Session {
            id: session.id.clone()
        }
    );
    let write = &proposal.writes[0];
    assert_eq!(write.path, "Ideas/new.md");
    assert_eq!(write.base, ExpectedBase::Absent);
    assert_eq!(write.new_text, "# New");
    assert_eq!(
        write.root.as_path(),
        f.kiln.path().canonicalize().unwrap().as_path()
    );

    // The same tools apply the next write when the turn mode is `apply`.
    f.mode.set(WriteMode::Apply);
    let result = f
        .tools
        .create_note(Parameters(CreateNoteParams {
            path: "applied.md".to_string(),
            content: "# Applied".to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();
    assert_eq!(answer_status(result), "created");
    assert!(f.kiln.path().join("applied.md").exists());
}

fn answer_status(result: rmcp::model::CallToolResult) -> String {
    answer(result)["status"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn update_note_in_propose_mode_records_the_base() {
    let session = Session::new(SessionType::Chat, vec![]);
    let f = fixture(&session);
    let before = "# Old\n\nText.";
    std::fs::write(f.kiln.path().join("note.md"), before).unwrap();

    let result = f
        .tools
        .update_note(Parameters(UpdateNoteParams {
            path: "note.md".to_string(),
            content: Some("# New\n\nText.".to_string()),
            frontmatter: None,
        }))
        .await
        .unwrap();

    assert_eq!(answer_status(result), "proposed");
    assert_eq!(
        std::fs::read_to_string(f.kiln.path().join("note.md")).unwrap(),
        before
    );
    let proposal = &f.store.list(false).unwrap()[0];
    let write = &proposal.writes[0];
    assert_eq!(write.path, "note.md");
    assert_eq!(write.new_text, "# New\n\nText.");
    assert_eq!(
        write.base,
        ExpectedBase::Text {
            text: before.to_string(),
            hash: disk_hash(before),
        }
    );
}

#[tokio::test]
async fn a_plugin_session_proposal_names_the_plugin() {
    let session =
        Session::new(SessionType::Plugin, vec![]).with_plugin(Some("consolidation".to_string()));
    let f = fixture(&session);

    f.tools
        .create_note(Parameters(CreateNoteParams {
            path: "merged.md".to_string(),
            content: "# Merged".to_string(),
            frontmatter: None,
        }))
        .await
        .unwrap();

    let proposal = &f.store.list(false).unwrap()[0];
    assert_eq!(
        proposal.author,
        ProposalAuthor::Plugin {
            name: "consolidation".to_string()
        }
    );
    assert_eq!(proposal.session.as_ref(), Some(&session.id));

    // A plugin that created a session of another type is not the author of
    // its proposals: that session is a conversation, and it names itself.
    let chat = Session::new(SessionType::Chat, vec![]).with_plugin(Some("discord".into()));
    assert_eq!(
        super::super::author_of(&chat),
        ProposalAuthor::Session {
            id: chat.id.clone()
        }
    );

    // A plugin session with no plugin name names the session instead.
    let unnamed = Session::new(SessionType::Plugin, vec![]);
    assert_eq!(
        super::super::author_of(&unnamed),
        ProposalAuthor::Session {
            id: unnamed.id.clone()
        }
    );
}

#[tokio::test]
async fn a_second_update_in_one_turn_builds_on_the_proposed_text() {
    let session = Session::new(SessionType::Chat, vec![]);
    let f = fixture(&session);
    let before = "# Old\n\nText.";
    std::fs::write(f.kiln.path().join("note.md"), before).unwrap();

    // The first call changes the frontmatter only. The second call changes
    // the content only, so it must keep the frontmatter of the first call.
    f.tools
        .update_note(Parameters(UpdateNoteParams {
            path: "note.md".to_string(),
            content: None,
            frontmatter: Some(serde_json::json!({ "status": "draft" })),
        }))
        .await
        .unwrap();
    f.tools
        .update_note(Parameters(UpdateNoteParams {
            path: "note.md".to_string(),
            content: Some("# New\n\nText.".to_string()),
            frontmatter: None,
        }))
        .await
        .unwrap();

    assert_eq!(
        std::fs::read_to_string(f.kiln.path().join("note.md")).unwrap(),
        before
    );
    let listed = f.store.list(false).unwrap();
    assert_eq!(listed.len(), 1);
    let writes = &listed[0].writes;
    assert_eq!(writes.len(), 1);
    let write = &writes[0];
    assert!(
        write.new_text.contains("status: draft"),
        "the first edit is lost: {:?}",
        write.new_text
    );
    assert!(
        write.new_text.contains("# New"),
        "the second edit is lost: {:?}",
        write.new_text
    );
    assert_eq!(
        write.base,
        ExpectedBase::Text {
            text: before.to_string(),
            hash: disk_hash(before),
        }
    );
}

#[tokio::test]
async fn an_update_after_a_proposed_create_builds_on_the_proposed_note() {
    let session = Session::new(SessionType::Chat, vec![]);
    let f = fixture(&session);

    f.tools
        .create_note(Parameters(CreateNoteParams {
            path: "new.md".to_string(),
            content: "# New".to_string(),
            frontmatter: Some(serde_json::json!({ "status": "draft" })),
        }))
        .await
        .unwrap();
    // The file is not on the disk, but the turn proposes it, so the update
    // finds it.
    let result = f
        .tools
        .update_note(Parameters(UpdateNoteParams {
            path: "new.md".to_string(),
            content: Some("# Newer".to_string()),
            frontmatter: None,
        }))
        .await
        .unwrap();

    assert_eq!(answer_status(result), "proposed");
    assert!(!f.kiln.path().join("new.md").exists());
    let listed = f.store.list(false).unwrap();
    assert_eq!(listed.len(), 1);
    let write = &listed[0].writes[0];
    assert_eq!(listed[0].writes.len(), 1);
    assert!(
        write.new_text.contains("status: draft"),
        "{:?}",
        write.new_text
    );
    assert!(write.new_text.contains("# Newer"), "{:?}", write.new_text);
    assert_eq!(write.base, ExpectedBase::Absent);
}

#[tokio::test]
async fn delete_note_in_propose_mode_refuses_and_keeps_the_file() {
    let session = Session::new(SessionType::Chat, vec![]);
    let f = fixture(&session);
    std::fs::write(f.kiln.path().join("note.md"), "# Keep").unwrap();

    let error = f
        .tools
        .delete_note(Parameters(NotePathParams {
            path: "note.md".to_string(),
        }))
        .await
        .unwrap_err();

    assert!(
        error.message.contains("not available in propose mode"),
        "{error:?}"
    );
    assert!(f.kiln.path().join("note.md").exists());
}
