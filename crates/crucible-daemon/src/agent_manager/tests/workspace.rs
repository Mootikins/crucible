use super::*;
use crucible_core::session::Session;
use serde_json::json;
use std::path::{Path, PathBuf};

fn create_test_agent_manager_with_workspace_root(
    session_manager: Arc<SessionManager>,
    workspace_root: &Path,
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
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(workspace_root.to_path_buf())),
    })
}

#[tokio::test]
async fn session_workspace_used_for_workspace_tools() {
    let kiln_dir = TempDir::new().unwrap();
    let workspace_dir = TempDir::new().unwrap();

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let agent_manager =
        create_test_agent_manager_with_workspace_root(session_manager.clone(), workspace_dir.path());

    let session = session_manager
        .create_session(
            SessionType::Chat,
            kiln_dir.path().to_path_buf(),
            Some(workspace_dir.path().to_path_buf()),
            vec![],
            None,
        )
        .await
        .unwrap();

    let dispatcher = agent_manager.get_or_create_session_dispatcher(&session);
    let result = dispatcher
        .dispatch_tool("bash", json!({ "command": "pwd" }))
        .await
        .unwrap();

    let pwd = result
        .get("result")
        .and_then(serde_json::Value::as_str)
        .unwrap();

    let workspace_path = workspace_dir.path().to_string_lossy().to_string();
    let kiln_path = kiln_dir.path().to_string_lossy().to_string();
    assert!(pwd.contains(&workspace_path), "pwd should run in workspace: {pwd}");
    assert!(!pwd.contains(&kiln_path), "pwd should not run in kiln: {pwd}");
}

#[tokio::test]
async fn session_kiln_used_for_crucible_mcp_server() {
    let kiln_dir = TempDir::new().unwrap();
    let workspace_dir = TempDir::new().unwrap();

    std::fs::write(kiln_dir.path().join("kiln-note.md"), "# kiln\n").unwrap();
    std::fs::write(workspace_dir.path().join("workspace-note.md"), "# workspace\n").unwrap();

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let agent_manager =
        create_test_agent_manager_with_workspace_root(session_manager.clone(), workspace_dir.path());

    let session = session_manager
        .create_session(
            SessionType::Chat,
            kiln_dir.path().to_path_buf(),
            Some(workspace_dir.path().to_path_buf()),
            vec![],
            None,
        )
        .await
        .unwrap();

    let dispatcher = agent_manager.get_or_create_session_dispatcher(&session);
    let result = dispatcher.dispatch_tool("list_notes", json!({})).await.unwrap();
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

    assert!(has_kiln_note, "list_notes should include kiln note: {result}");
    assert!(
        !has_workspace_note,
        "list_notes should not include workspace-only note: {result}"
    );
}

#[tokio::test]
async fn regression_workspace_equals_kiln_tools_still_work() {
    let shared_dir = TempDir::new().unwrap();
    std::fs::write(shared_dir.path().join("shared-note.md"), "# shared\n").unwrap();

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let agent_manager =
        create_test_agent_manager_with_workspace_root(session_manager.clone(), shared_dir.path());

    let session = session_manager
        .create_session(
            SessionType::Chat,
            shared_dir.path().to_path_buf(),
            Some(shared_dir.path().to_path_buf()),
            vec![],
            None,
        )
        .await
        .unwrap();

    let dispatcher = agent_manager.get_or_create_session_dispatcher(&session);

    let pwd_result = dispatcher
        .dispatch_tool("bash", json!({ "command": "pwd" }))
        .await
        .unwrap();
    let pwd = pwd_result
        .get("result")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    let shared_path = shared_dir.path().to_string_lossy().to_string();
    assert!(pwd.contains(&shared_path), "pwd should run in shared dir: {pwd}");

    let notes_result = dispatcher.dispatch_tool("list_notes", json!({})).await.unwrap();
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

#[tokio::test]
async fn empty_workspace_uses_default_dispatcher_without_panic() {
    let kiln_dir = TempDir::new().unwrap();
    let default_workspace_root = TempDir::new().unwrap();

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let agent_manager = create_test_agent_manager_with_workspace_root(
        session_manager.clone(),
        default_workspace_root.path(),
    );

    let session = Session::new(SessionType::Chat, kiln_dir.path().to_path_buf())
        .with_workspace(PathBuf::new());
    session_manager.register_transient(session.clone());

    let dispatcher = agent_manager.get_or_create_session_dispatcher(&session);
    let result = dispatcher
        .dispatch_tool("bash", json!({ "command": "pwd" }))
        .await
        .unwrap();

    let pwd = result
        .get("result")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    let default_root = default_workspace_root.path().to_string_lossy().to_string();

    assert!(dispatcher.has_tool("bash"));
    assert!(!dispatcher.has_tool("list_notes"));
    assert!(
        pwd.contains(&default_root),
        "empty workspace should use default workspace dispatcher root: {pwd}"
    );
}
