//! An agent may read its kilns; it may not read other sessions' transcripts.
//!
//! Before sessions moved out of kilns, `allowed_roots` held the kiln root and
//! nothing subtracted `{kiln}/.crucible/sessions` from it, so `read_file` and
//! `glob` reached every transcript ever recorded in that kiln — every past
//! conversation, from any project, belonging to anyone who shared the corpus.
//! The relocation alone does not close that: a kiln-less session's kiln *is*
//! the data root, which now contains the sessions root, so the subtraction has
//! to be explicit (`with_denied_roots`) and this is what proves it is.
//!
//! Asserted through the real dispatcher rather than by inspecting
//! `allowed_roots`: the vector is a means, and a test that reads it passes
//! whether or not the tool honors it.

use super::{create_test_agent_manager, test_workspace_root};
use crate::session_manager::SessionManager;
use crate::session_storage::FileSessionStorage;
use crucible_core::session::SessionType;
use std::sync::Arc;
use tempfile::TempDir;

const SECRET: &str = r#"{"event":"user_message","data":{"content":"MY-BANK-PASSWORD"}}"#;

/// Put a line in a session's `session.jsonl`. Written straight to the path
/// rather than through the storage trait so the test asserts against the same
/// on-disk location `allowed_roots` is reasoning about.
fn write_transcript(
    session: &crucible_core::session::Session,
    sessions_root: &std::path::Path,
    line: &str,
) {
    let path = session.jsonl_path(sessions_root);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("{line}\n")).unwrap();
}

/// Ask `read_file` for `path` through the session's own dispatcher, exactly as
/// a turn would. `Ok` means the agent got the bytes.
async fn agent_reads(
    manager: &crate::agent_manager::AgentManager,
    session: &crucible_core::session::Session,
    path: &std::path::Path,
) -> Result<serde_json::Value, String> {
    manager
        .get_or_create_session_dispatcher(session)
        .await
        .dispatch_tool(
            "read_file",
            serde_json::json!({ "path": path.to_string_lossy() }),
            Default::default(),
        )
        .await
}

#[tokio::test]
async fn agent_cannot_read_other_sessions_transcripts() {
    let home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let sessions_root = FileSessionStorage::root_for(home.path());
    let sm = Arc::new(SessionManager::new(sessions_root.clone()));

    // Two sessions attached to the SAME kiln — the case the old layout made
    // mutually readable.
    let victim = sm
        .create_session(
            SessionType::Chat,
            vec![kiln.path().to_path_buf()],
            Some(test_workspace_root().to_path_buf()),
            None,
        )
        .await
        .unwrap();
    write_transcript(&victim, &sessions_root, SECRET);

    let snoop = sm
        .create_session(
            SessionType::Chat,
            vec![kiln.path().to_path_buf()],
            Some(test_workspace_root().to_path_buf()),
            None,
        )
        .await
        .unwrap();

    let manager = create_test_agent_manager(sm.clone());
    let victim_log = victim.jsonl_path(&sessions_root);
    assert!(
        victim_log.exists(),
        "precondition: the victim's transcript is on disk at {}",
        victim_log.display()
    );

    let result = agent_reads(&manager, &snoop, &victim_log).await;

    let leaked = format!("{result:?}");
    assert!(
        !leaked.contains("MY-BANK-PASSWORD"),
        "one session's agent read another's transcript: {leaked}"
    );
    assert!(
        result.is_err(),
        "the read must be refused, not silently emptied: {leaked}"
    );
}

/// The other half of the same rule, so the denial cannot be satisfied by
/// refusing everything: a session still reads its *own* transcript, and still
/// reads the kiln it is attached to.
#[tokio::test]
async fn an_agent_still_reads_its_own_transcript_and_its_kiln() {
    let home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    std::fs::write(kiln.path().join("note.md"), "KILN-CONTENT").unwrap();
    let sessions_root = FileSessionStorage::root_for(home.path());
    let sm = Arc::new(SessionManager::new(sessions_root.clone()));

    let session = sm
        .create_session(
            SessionType::Chat,
            vec![kiln.path().to_path_buf()],
            Some(test_workspace_root().to_path_buf()),
            None,
        )
        .await
        .unwrap();
    write_transcript(
        &session,
        &sessions_root,
        r#"{"event":"user_message","data":{"content":"OWN-LOG"}}"#,
    );

    let manager = create_test_agent_manager(sm.clone());

    let own = agent_reads(&manager, &session, &session.jsonl_path(&sessions_root))
        .await
        .expect("a session must still read its own transcript");
    assert!(
        format!("{own:?}").contains("OWN-LOG"),
        "own transcript came back empty: {own:?}"
    );

    let note = agent_reads(&manager, &session, &kiln.path().join("note.md"))
        .await
        .expect("a session must still read the kiln it is attached to");
    assert!(
        format!("{note:?}").contains("KILN-CONTENT"),
        "kiln read came back empty: {note:?}"
    );
}

/// The kiln-less case, which is the one the subtraction exists for: with no
/// kiln of its own a session falls back to the data root, and the data root
/// *contains* the sessions root. Granting the fallback kiln without denying
/// the sessions root under it would re-open every transcript on the machine.
#[tokio::test]
async fn a_session_whose_kiln_is_the_data_root_still_cannot_read_other_transcripts() {
    let home = TempDir::new().unwrap();
    let sessions_root = FileSessionStorage::root_for(home.path());
    let sm = Arc::new(SessionManager::new(sessions_root.clone()));

    let victim = sm
        .create_session(
            SessionType::Chat,
            vec![home.path().to_path_buf()],
            Some(test_workspace_root().to_path_buf()),
            None,
        )
        .await
        .unwrap();
    write_transcript(&victim, &sessions_root, SECRET);

    let snoop = sm
        .create_session(
            SessionType::Chat,
            vec![home.path().to_path_buf()],
            Some(test_workspace_root().to_path_buf()),
            None,
        )
        .await
        .unwrap();

    let manager = create_test_agent_manager(sm.clone());
    let result = agent_reads(&manager, &snoop, &victim.jsonl_path(&sessions_root)).await;

    let leaked = format!("{result:?}");
    assert!(
        !leaked.contains("MY-BANK-PASSWORD"),
        "the data-root kiln re-opened the sessions root: {leaked}"
    );
    assert!(result.is_err(), "the read must be refused: {leaked}");
}
