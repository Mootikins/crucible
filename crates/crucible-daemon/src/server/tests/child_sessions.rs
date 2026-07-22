//! Child-session visibility and lifecycle-cascade behavior.
//!
//! Delegated children are full sessions but not first-class: hidden from
//! default `session.list`, revealed with `include_children`, and archived/
//! deleted together with their parent.

use super::*;
use crucible_core::session::{Session, SessionType};

/// Persist a fabricated child session (parent-linked) directly into the
/// kiln's session storage, the same shape `create_child_session` writes.
fn write_child_session(kiln: &std::path::Path, parent_id: &str) -> String {
    let child = Session::new(SessionType::Agent, kiln.to_path_buf())
        .with_parent(parent_id.to_string())
        .with_title("delegated task");
    let dir = kiln.join(".crucible").join("sessions").join(&child.id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("meta.json"),
        serde_json::to_string_pretty(&child).unwrap(),
    )
    .unwrap();
    child.id
}

#[tokio::test]
async fn session_list_hides_children_unless_requested() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let parent_id = create_chat_session(&mut client, &server.kiln_path, 700).await;
    let child_id = write_child_session(&server.kiln_path, &parent_id);

    // Default listing: parent visible, child hidden.
    let response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 701,
            "method": "session.list",
            "params": { "kiln": server.kiln_path.to_string_lossy() }
        }),
    )
    .await;
    let sessions = response["result"]["sessions"].as_array().unwrap();
    assert!(
        sessions
            .iter()
            .any(|s| s["session_id"] == parent_id.as_str()),
        "parent must be listed: {sessions:?}"
    );
    assert!(
        !sessions
            .iter()
            .any(|s| s["session_id"] == child_id.as_str()),
        "child must be hidden by default: {sessions:?}"
    );

    // include_children reveals it, with its parent link.
    let response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 702,
            "method": "session.list",
            "params": {
                "kiln": server.kiln_path.to_string_lossy(),
                "include_children": true
            }
        }),
    )
    .await;
    let sessions = response["result"]["sessions"].as_array().unwrap();
    let child = sessions
        .iter()
        .find(|s| s["session_id"] == child_id.as_str())
        .unwrap_or_else(|| panic!("child must appear with include_children: {sessions:?}"));
    assert_eq!(child["parent_session_id"], parent_id.as_str());

    server.shutdown().await;
}

#[tokio::test]
async fn session_archive_cascades_to_children() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let parent_id = create_chat_session(&mut client, &server.kiln_path, 710).await;
    let child_id = write_child_session(&server.kiln_path, &parent_id);

    let response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 711,
            "method": "session.archive",
            "params": {
                "session_id": parent_id,
                "kiln": server.kiln_path.to_string_lossy()
            }
        }),
    )
    .await;
    assert!(response["error"].is_null(), "archive failed: {response:?}");

    let child_meta = server
        .kiln_path
        .join(".crucible")
        .join("sessions")
        .join(&child_id)
        .join("meta.json");
    let child: Session =
        serde_json::from_str(&std::fs::read_to_string(&child_meta).unwrap()).unwrap();
    assert!(
        child.archived,
        "archiving the parent must archive its children"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn session_delete_cascades_to_children() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let parent_id = create_chat_session(&mut client, &server.kiln_path, 720).await;
    let child_id = write_child_session(&server.kiln_path, &parent_id);

    let response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 721,
            "method": "session.delete",
            "params": {
                "session_id": parent_id,
                "kiln": server.kiln_path.to_string_lossy()
            }
        }),
    )
    .await;
    assert!(response["error"].is_null(), "delete failed: {response:?}");

    let child_dir = server
        .kiln_path
        .join(".crucible")
        .join("sessions")
        .join(&child_id);
    assert!(
        !child_dir.exists(),
        "deleting the parent must delete its children"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn session_get_reports_parent_link() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let parent_id = create_chat_session(&mut client, &server.kiln_path, 730).await;

    let response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 731,
            "method": "session.get",
            "params": { "session_id": parent_id }
        }),
    )
    .await;
    assert!(response["error"].is_null(), "get failed: {response:?}");
    assert!(
        response["result"]["parent_session_id"].is_null(),
        "top-level session has no parent link"
    );

    server.shutdown().await;
}
