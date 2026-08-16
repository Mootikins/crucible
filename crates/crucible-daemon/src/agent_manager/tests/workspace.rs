use super::*;
use crate::test_support::temp_session_manager;
use crucible_core::session::Session;
use serde_json::json;
use std::path::{Path, PathBuf};

fn create_test_agent_manager_with_workspace_root(
    session_manager: Arc<SessionManager>,
    _workspace_root: &Path,
) -> AgentManager {
    let (event_tx, _) = broadcast::channel(16);
    let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
    AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager,
        background_manager,
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        context_config: None,
        permission_config: None,
        plugin_loader: None,
    })
}

#[tokio::test]
async fn session_workspace_used_for_workspace_tools() {
    let kiln_dir = TempDir::new().unwrap();
    let workspace_dir = TempDir::new().unwrap();

    let session_manager = temp_session_manager();
    let agent_manager = create_test_agent_manager_with_workspace_root(
        session_manager.clone(),
        workspace_dir.path(),
    );

    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![kiln_dir.path().to_path_buf()],
            Some(workspace_dir.path().to_path_buf()),
            None,
        )
        .await
        .unwrap();

    let dispatcher = agent_manager
        .get_or_create_session_dispatcher(&session)
        .await;
    let result = dispatcher
        .dispatch_tool("bash", json!({ "command": "pwd" }), Default::default())
        .await
        .unwrap();

    let pwd = result
        .get("result")
        .and_then(serde_json::Value::as_str)
        .unwrap();

    let workspace_path = workspace_dir.path().to_string_lossy().to_string();
    let kiln_path = kiln_dir.path().to_string_lossy().to_string();
    assert!(
        pwd.contains(&workspace_path),
        "pwd should run in workspace: {pwd}"
    );
    assert!(
        !pwd.contains(&kiln_path),
        "pwd should not run in kiln: {pwd}"
    );
}

#[tokio::test]
async fn session_kiln_used_for_crucible_mcp_server() {
    let kiln_dir = TempDir::new().unwrap();
    let workspace_dir = TempDir::new().unwrap();

    std::fs::write(kiln_dir.path().join("kiln-note.md"), "# kiln\n").unwrap();
    std::fs::write(
        workspace_dir.path().join("workspace-note.md"),
        "# workspace\n",
    )
    .unwrap();

    let session_manager = temp_session_manager();
    let agent_manager = create_test_agent_manager_with_workspace_root(
        session_manager.clone(),
        workspace_dir.path(),
    );

    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![kiln_dir.path().to_path_buf()],
            Some(workspace_dir.path().to_path_buf()),
            None,
        )
        .await
        .unwrap();

    let dispatcher = agent_manager
        .get_or_create_session_dispatcher(&session)
        .await;
    let result = dispatcher
        .dispatch_tool("list_notes", json!({}), Default::default())
        .await
        .unwrap();
    let notes = result
        .get("notes")
        .and_then(serde_json::Value::as_array)
        .unwrap();

    let has_kiln_note = notes.iter().any(|note| {
        note.get("path")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|path| path.ends_with("kiln-note.md"))
    });
    let has_workspace_note = notes.iter().any(|note| {
        note.get("path")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|path| path.ends_with("workspace-note.md"))
    });

    assert!(
        has_kiln_note,
        "list_notes should include kiln note: {result}"
    );
    assert!(
        !has_workspace_note,
        "list_notes should not include workspace-only note: {result}"
    );
}

#[tokio::test]
async fn regression_workspace_equals_kiln_tools_still_work() {
    let shared_dir = TempDir::new().unwrap();
    std::fs::write(shared_dir.path().join("shared-note.md"), "# shared\n").unwrap();

    let session_manager = temp_session_manager();
    let agent_manager =
        create_test_agent_manager_with_workspace_root(session_manager.clone(), shared_dir.path());

    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![shared_dir.path().to_path_buf()],
            Some(shared_dir.path().to_path_buf()),
            None,
        )
        .await
        .unwrap();

    let dispatcher = agent_manager
        .get_or_create_session_dispatcher(&session)
        .await;

    let pwd_result = dispatcher
        .dispatch_tool("bash", json!({ "command": "pwd" }), Default::default())
        .await
        .unwrap();
    let pwd = pwd_result
        .get("result")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    let shared_path = shared_dir.path().to_string_lossy().to_string();
    assert!(
        pwd.contains(&shared_path),
        "pwd should run in shared dir: {pwd}"
    );

    let notes_result = dispatcher
        .dispatch_tool("list_notes", json!({}), Default::default())
        .await
        .unwrap();
    let notes = notes_result
        .get("notes")
        .and_then(serde_json::Value::as_array)
        .unwrap();
    let has_shared_note = notes.iter().any(|note| {
        note.get("path")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|path| path.ends_with("shared-note.md"))
    });
    assert!(
        has_shared_note,
        "list_notes should include shared note when workspace==kiln: {notes_result}"
    );
}

/// A session with no workspace gets a session dispatcher like any other.
///
/// It used to get the daemon-GLOBAL one instead, whose `WorkspaceTools` has no
/// containment at all — so `session.set_workspace` with no `workspace` key
/// (which falls back to `default_kiln()`, and to `""` when there is none)
/// traded a contained tool set for an uncontained one. An absent workspace
/// degrades capabilities; it must not degrade containment. The tools anchor at
/// the session's own storage directory, which is the one place it certainly
/// has, and its kiln stays attached — so the kiln-backed tools stay too.
#[tokio::test]
async fn a_workspace_less_session_still_gets_a_contained_dispatcher() {
    let kiln_dir = TempDir::new().unwrap();
    let default_workspace_root = TempDir::new().unwrap();

    let session_manager = temp_session_manager();
    let agent_manager = create_test_agent_manager_with_workspace_root(
        session_manager.clone(),
        default_workspace_root.path(),
    );

    let session = Session::new(SessionType::Chat, vec![kiln_dir.path().to_path_buf()])
        .with_workspace(PathBuf::new());
    session_manager.register_transient(session.clone());
    let session_dir = session.storage_path(session_manager.sessions_root());
    std::fs::create_dir_all(&session_dir).unwrap();

    let dispatcher = agent_manager
        .get_or_create_session_dispatcher(&session)
        .await;

    assert!(dispatcher.has_tool("bash"));
    assert!(
        dispatcher.has_tool("list_notes"),
        "the session's kiln is still attached, so its tools are still advertised"
    );

    let pwd = dispatcher
        .dispatch_tool("bash", json!({ "command": "pwd" }), Default::default())
        .await
        .unwrap();
    let pwd = pwd
        .get("result")
        .and_then(serde_json::Value::as_str)
        .unwrap()
        .to_string();
    assert!(
        pwd.contains(&session_dir.to_string_lossy().to_string()),
        "a workspace-less session anchors at its own storage dir, not a daemon-wide root: {pwd}"
    );

    // The point of the change: the tool set is contained, not ambient.
    let escape = dispatcher
        .dispatch_tool(
            "read_file",
            json!({ "path": "/etc/passwd" }),
            Default::default(),
        )
        .await;
    assert!(
        escape.is_err(),
        "a workspace-less session must still be contained: {escape:?}"
    );
    let in_kiln = kiln_dir.path().join("note.md");
    std::fs::write(&in_kiln, "KILN-CONTENT").unwrap();
    let read = dispatcher
        .dispatch_tool(
            "read_file",
            json!({ "path": in_kiln.to_string_lossy() }),
            Default::default(),
        )
        .await
        .expect("its kiln is still readable");
    assert!(format!("{read:?}").contains("KILN-CONTENT"));
}
