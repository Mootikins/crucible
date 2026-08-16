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

// ===================== RE-ATTACK: containment probes =====================

/// `session.set_workspace` with no `workspace` detaches: the workspace falls
/// back to `default_kiln()`, which on a kiln-less session is `None` and
/// therefore `PathBuf::default()` — the empty path. An empty kiln set must
/// degrade capabilities, never containment.
#[tokio::test]
async fn detaching_the_workspace_of_a_kilnless_session_does_not_uncontain_it() {
    let home = TempDir::new().unwrap();
    let sessions_root = FileSessionStorage::root_for(home.path());
    let sm = Arc::new(SessionManager::new(sessions_root.clone()));

    let victim = sm
        .create_session(
            SessionType::Chat,
            vec![],
            Some(test_workspace_root().to_path_buf()),
            None,
        )
        .await
        .unwrap();
    write_transcript(&victim, &sessions_root, SECRET);

    let snoop = sm
        .create_session(
            SessionType::Chat,
            vec![],
            Some(test_workspace_root().to_path_buf()),
            None,
        )
        .await
        .unwrap();

    let manager = create_test_agent_manager(sm.clone());
    // Exactly what `session.set_workspace {"session_id": ...}` (no workspace
    // key) does.
    let snoop = manager.set_workspace(&snoop.id, None, None).await.unwrap();
    assert_eq!(snoop.workspace, std::path::PathBuf::new());

    let result = agent_reads(&manager, &snoop, &victim.jsonl_path(&sessions_root)).await;

    let leaked = format!("{result:?}");
    assert!(
        !leaked.contains("MY-BANK-PASSWORD"),
        "a workspace-less, kiln-less session read another transcript: {leaked}"
    );
    assert!(
        result.is_err(),
        "with no kiln and no workspace the allowlist is the session's own dir, \
         so this must be refused rather than silently emptied: {leaked}"
    );
}

/// The legacy in-kiln transcript directory is denied per *kiln*. The workspace
/// is an allowed root too, and a project directory that used to be someone's
/// kiln still holds `{dir}/.crucible/sessions/*`.
#[tokio::test]
async fn a_workspace_that_used_to_be_a_kiln_does_not_expose_its_legacy_transcripts() {
    let home = TempDir::new().unwrap();
    let repo = TempDir::new().unwrap();
    let sessions_root = FileSessionStorage::root_for(home.path());
    let sm = Arc::new(SessionManager::new(sessions_root.clone()));

    // A pre-relocation transcript that migration never reached (the directory
    // is not a registered project, and no config names it).
    let legacy = repo
        .path()
        .join(".crucible")
        .join("sessions")
        .join("chat-victim");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join("session.jsonl"), format!("{SECRET}\n")).unwrap();

    let snoop = sm
        .create_session(
            SessionType::Chat,
            vec![],
            Some(repo.path().to_path_buf()),
            None,
        )
        .await
        .unwrap();

    let manager = create_test_agent_manager(sm.clone());
    let result = agent_reads(&manager, &snoop, &legacy.join("session.jsonl")).await;

    let leaked = format!("{result:?}");
    assert!(
        !leaked.contains("MY-BANK-PASSWORD"),
        "the workspace's legacy in-kiln transcripts stayed readable: {leaked}"
    );
}

/// `read_file` is not the only file-reading tool. `read_note` resolves a
/// caller path against the kiln with `validate_path_within_kiln`, which knows
/// nothing about `denied_roots` — so the legacy in-kiln transcript directory
/// that `read_file` refuses is wide open through the note tools.
#[tokio::test]
async fn read_note_cannot_reach_the_legacy_in_kiln_transcript_directory() {
    let home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let sessions_root = FileSessionStorage::root_for(home.path());
    let sm = Arc::new(SessionManager::new(sessions_root.clone()));

    let legacy = kiln
        .path()
        .join(".crucible")
        .join("sessions")
        .join("chat-victim");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join("session.jsonl"), format!("{SECRET}\n")).unwrap();

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

    // Same file `read_file` refuses, through the other door.
    let via_read_file = agent_reads(&manager, &snoop, &legacy.join("session.jsonl")).await;
    assert!(
        !format!("{via_read_file:?}").contains("MY-BANK-PASSWORD"),
        "precondition: read_file must already refuse this"
    );

    let result = manager
        .get_or_create_session_dispatcher(&snoop)
        .await
        .dispatch_tool(
            "read_note",
            serde_json::json!({ "path": ".crucible/sessions/chat-victim/session.jsonl" }),
            Default::default(),
        )
        .await;

    let leaked = format!("{result:?}");
    assert!(
        !leaked.contains("MY-BANK-PASSWORD"),
        "read_note handed over a transcript read_file refuses: {leaked}"
    );
}

/// `grep_notes` takes a `folder`, validated only by
/// `validate_folder_within_kiln`. The ripgrep walk skips hidden files, but not
/// when the hidden directory is the walk root the caller named.
#[tokio::test]
async fn grep_notes_cannot_be_pointed_at_the_legacy_in_kiln_transcript_directory() {
    let home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();
    let sessions_root = FileSessionStorage::root_for(home.path());
    let sm = Arc::new(SessionManager::new(sessions_root.clone()));

    let legacy = kiln
        .path()
        .join(".crucible")
        .join("sessions")
        .join("chat-victim");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(legacy.join("session.md"), format!("{SECRET}\n")).unwrap();

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
    let result = manager
        .get_or_create_session_dispatcher(&snoop)
        .await
        .dispatch_tool(
            "grep_notes",
            serde_json::json!({
                "query": "MY-BANK-PASSWORD",
                "folder": ".crucible/sessions",
            }),
            Default::default(),
        )
        .await;

    let leaked = format!("{result:?}");
    assert!(
        !leaked.contains("MY-BANK-PASSWORD"),
        "grep_notes walked the legacy transcript directory: {leaked}"
    );
}

/// End-to-end of the `refuse_forbidden_scope` dodge: a kiln path whose `..`
/// traverses a directory that does not exist yet is accepted as scope (the
/// gate compares an un-normalized path), and the agent's own `write_file` then
/// materializes the missing component — at which point the same path
/// canonicalizes *inside* the sessions root, one level deeper than the deny
/// root, and out-ranks it.
#[tokio::test]
async fn a_kiln_that_traverses_a_missing_directory_cannot_reach_the_sessions_root() {
    let home = TempDir::new().unwrap();
    let sessions_root = FileSessionStorage::root_for(home.path());
    let sm = Arc::new(SessionManager::new(sessions_root.clone()));

    let victim = sm
        .create_session(
            SessionType::Chat,
            vec![],
            Some(test_workspace_root().to_path_buf()),
            None,
        )
        .await
        .unwrap();
    write_transcript(&victim, &sessions_root, SECRET);

    // What `session.create {"kilns": [...]}` / `session.connect_kiln` accept.
    let dodge = home
        .path()
        .join("not-yet")
        .join("..")
        .join("sessions")
        .join(&*victim.id);
    assert!(
        crate::server::session::scope::refuse_forbidden_scope("kiln", &dodge, &sessions_root)
            .is_err(),
        "the scope gate must refuse a kiln whose `..` lands in the sessions root"
    );

    let snoop = sm
        .create_session(
            SessionType::Chat,
            vec![dodge.clone()],
            Some(test_workspace_root().to_path_buf()),
            None,
        )
        .await
        .unwrap();

    let manager = create_test_agent_manager(sm.clone());
    let dispatcher = manager.get_or_create_session_dispatcher(&snoop).await;

    // Step 1: the agent materializes the missing component (write_file mkdir -p).
    let _seeded = dispatcher
        .dispatch_tool(
            "write_file",
            serde_json::json!({
                "path": dodge.join("junk.txt").to_string_lossy(),
                "content": "x",
            }),
            Default::default(),
        )
        .await;

    // Step 2: the same lexical path now resolves inside the sessions root.
    let result = dispatcher
        .dispatch_tool(
            "read_file",
            serde_json::json!({ "path": dodge.join("session.jsonl").to_string_lossy() }),
            Default::default(),
        )
        .await;

    let leaked = format!("{result:?}");
    assert!(
        !leaked.contains("MY-BANK-PASSWORD"),
        "a `..`-through-a-missing-directory kiln reached the sessions root: {leaked}"
    );
}

// ============ RE-ATTACK: the doors that are not `read_file` ============
//
// The workspace tools consult the session's root set. The note, search and
// kiln tools consulted a *different* rule — "inside the kiln, and not under
// `{kiln}/.crucible`" — which is the same containment-per-tool-family shape as
// Claude Code's CVE-2026-39861 and Gemini CLI's #27767. That rule is blind to
// the one case it most needs to see: a kiln that ENCLOSES the flat sessions
// root, where the transcripts sit at `{kiln}/sessions/...` with no `.crucible`
// component anywhere in the name.
//
// A kiln-less session's kiln *is* the data root, so this is the default shape
// of a `cru chat` with no kiln, not a contrived one.

/// Build the enclosing-kiln setup: a victim whose transcript lives under the
/// sessions root, and a snoop whose kiln is the data root above it.
async fn enclosing_kiln_setup(
    home: &TempDir,
) -> (
    Arc<SessionManager>,
    crucible_core::session::Session,
    crucible_core::session::Session,
) {
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
    // The legacy `.md` spelling of the same transcript, which is what the
    // note-shaped tools (`*.md` globs, `is_indexable_file`) actually walk.
    std::fs::write(
        victim.storage_path(&sessions_root).join("session.md"),
        format!("---\ntags:\n  - session\n---\n\n{SECRET}\n"),
    )
    .unwrap();

    let snoop = sm
        .create_session(
            SessionType::Chat,
            vec![home.path().to_path_buf()],
            Some(test_workspace_root().to_path_buf()),
            None,
        )
        .await
        .unwrap();

    (sm, victim, snoop)
}

#[tokio::test]
async fn read_note_cannot_reach_a_transcript_the_kiln_encloses() {
    let home = TempDir::new().unwrap();
    let (sm, victim, snoop) = enclosing_kiln_setup(&home).await;
    let manager = create_test_agent_manager(sm.clone());

    let result = manager
        .get_or_create_session_dispatcher(&snoop)
        .await
        .dispatch_tool(
            "read_note",
            serde_json::json!({ "path": format!("sessions/{}/session.jsonl", victim.id) }),
            Default::default(),
        )
        .await;

    let leaked = format!("{result:?}");
    assert!(
        !leaked.contains("MY-BANK-PASSWORD"),
        "read_note reached a transcript through a kiln that encloses the sessions root: {leaked}"
    );
}

/// The same file, reached without ever naming the directory it is in.
/// `read_note` falls back to a recursive `read_dir` walk for a bare filename,
/// so containment has to hold on what the walk YIELDS, not only on what the
/// caller wrote.
#[tokio::test]
async fn read_note_cannot_find_a_transcript_by_bare_filename() {
    let home = TempDir::new().unwrap();
    let (sm, victim, snoop) = enclosing_kiln_setup(&home).await;
    let manager = create_test_agent_manager(sm.clone());

    let result = manager
        .get_or_create_session_dispatcher(&snoop)
        .await
        .dispatch_tool(
            "read_note",
            serde_json::json!({ "path": "session.jsonl" }),
            Default::default(),
        )
        .await;

    let leaked = format!("{result:?}");
    assert!(
        !leaked.contains("MY-BANK-PASSWORD"),
        "the note-name walk found a transcript by filename alone: {leaked}"
    );
    // A walk that finds it and *then* has the read refused is still a leak:
    // the refusal names the path it resolved, which confirms the file exists
    // and hands back the session id it belongs to. Containment has to apply to
    // what the walk YIELDS, so the lookup comes back empty-handed instead.
    assert!(
        !leaked.contains(&*victim.id),
        "the refusal named the session whose transcript it found — an \
         enumeration oracle for every session on the machine: {leaked}"
    );
}

/// The folder is where a walk STARTS, not where it ends. `grep_notes` with no
/// folder at all walks the whole kiln — under a kiln that encloses the sessions
/// root that is every transcript on the machine, and no `folder` was ever named
/// for a check on the caller's string to catch.
#[tokio::test]
async fn grep_notes_without_a_folder_does_not_walk_into_the_sessions_root() {
    let home = TempDir::new().unwrap();
    let (sm, _victim, snoop) = enclosing_kiln_setup(&home).await;
    std::fs::write(home.path().join("real-note.md"), "an ordinary note").unwrap();
    let manager = create_test_agent_manager(sm.clone());

    let result = manager
        .get_or_create_session_dispatcher(&snoop)
        .await
        .dispatch_tool(
            "grep_notes",
            serde_json::json!({ "query": "MY-BANK-PASSWORD" }),
            Default::default(),
        )
        .await;

    // The query is echoed back in the success shape, so the leak has to be read
    // off the MATCHES rather than off the response text.
    let matches = result
        .as_ref()
        .ok()
        .and_then(|v| v.get("matches"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(
        matches.is_empty(),
        "the folderless walk reached the sessions root: {matches:?}"
    );
}

#[tokio::test]
async fn grep_notes_cannot_be_pointed_at_the_sessions_root() {
    let home = TempDir::new().unwrap();
    let (sm, _victim, snoop) = enclosing_kiln_setup(&home).await;
    let manager = create_test_agent_manager(sm.clone());

    let result = manager
        .get_or_create_session_dispatcher(&snoop)
        .await
        .dispatch_tool(
            "grep_notes",
            serde_json::json!({ "query": "MY-BANK-PASSWORD", "folder": "sessions" }),
            Default::default(),
        )
        .await;

    let leaked = format!("{result:?}");
    assert!(
        !leaked.contains("MY-BANK-PASSWORD"),
        "grep_notes walked the sessions root: {leaked}"
    );
}

/// Enumeration is a leak of its own: `list_notes` reports every walked file's
/// path, which names every session that has ever run on this machine, and with
/// `include_frontmatter` it reads them as well.
#[tokio::test]
async fn list_notes_does_not_enumerate_transcripts_the_kiln_encloses() {
    let home = TempDir::new().unwrap();
    let (sm, victim, snoop) = enclosing_kiln_setup(&home).await;
    let manager = create_test_agent_manager(sm.clone());

    let result = manager
        .get_or_create_session_dispatcher(&snoop)
        .await
        .dispatch_tool(
            "list_notes",
            serde_json::json!({ "recursive": true, "include_frontmatter": true }),
            Default::default(),
        )
        .await;

    let leaked = format!("{result:?}");
    assert!(
        !leaked.contains(&*victim.id),
        "list_notes enumerated another session's transcript directory: {leaked}"
    );
    assert!(
        !leaked.contains("MY-BANK-PASSWORD"),
        "and read its frontmatter besides: {leaked}"
    );
}

/// `property_search`'s filesystem fallback walks the kiln with no filter at
/// all — not even the hidden-directory skip the other walkers have.
#[tokio::test]
async fn property_search_does_not_reach_transcripts_the_kiln_encloses() {
    let home = TempDir::new().unwrap();
    let (sm, victim, snoop) = enclosing_kiln_setup(&home).await;
    let manager = create_test_agent_manager(sm.clone());

    let result = manager
        .get_or_create_session_dispatcher(&snoop)
        .await
        .dispatch_tool(
            "property_search",
            serde_json::json!({ "properties": { "tags": ["session"] } }),
            Default::default(),
        )
        .await;

    let leaked = format!("{result:?}");
    assert!(
        !leaked.contains(&*victim.id),
        "property_search matched inside another session's transcript directory: {leaked}"
    );
}

/// `get_kiln_info` reports counts rather than content, and a count is still an
/// oracle: it says how many sessions the machine has recorded. It skips hidden
/// directories, which is exactly the filter that does nothing when the
/// transcripts are at `{kiln}/sessions/` rather than `{kiln}/.crucible/`.
#[tokio::test]
async fn get_kiln_info_does_not_count_transcripts_the_kiln_encloses() {
    let home = TempDir::new().unwrap();
    let (sm, _victim, snoop) = enclosing_kiln_setup(&home).await;
    std::fs::write(home.path().join("real-note.md"), "# Note").unwrap();
    let manager = create_test_agent_manager(sm.clone());

    let result = manager
        .get_or_create_session_dispatcher(&snoop)
        .await
        .dispatch_tool("get_kiln_info", serde_json::json!({}), Default::default())
        .await
        .expect("get_kiln_info must still work for the session's own kiln");

    assert_eq!(
        result
            .get("total_files")
            .and_then(serde_json::Value::as_u64),
        Some(1),
        "only the kiln's own note may be counted, not the transcripts under it: {result:?}"
    );
}
