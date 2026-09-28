//! End-to-end tests for mid-session scope mutations:
//! session.connect_kiln, session.disconnect_kiln, session.set_workspace.

mod common;

use common::InProcessDaemonBuilder;
use crucible_daemon::DaemonClient;

/// The three registered kilns this suite needs: the one every session is
/// created with, a second to attach mid-session, and one classified
/// Confidential so the trust gate has something to refuse.
///
/// `classified` is registered LAZY on purpose. Boot opens every eager
/// registered kiln, so an eager entry here would be open before the test ran
/// and the "a refused attach opened nothing" assertion would be reading the
/// daemon's own startup rather than the attach.
async fn start_server() -> common::InProcessDaemon {
    let builder = InProcessDaemonBuilder::new().expect("a test daemon builder");
    let classified = builder.data_home().join("kilns").join("classified");
    std::fs::create_dir_all(classified.join(".crucible")).expect("classified kiln dir");
    std::fs::write(
        classified.join(".crucible").join("project.toml"),
        "[[kilns]]\npath = \".\"\ndata_classification = \"confidential\"\n",
    )
    .expect("classified kiln project.toml");

    builder
        .with_kiln("kiln")
        .with_kiln("extra-kiln")
        .with_lazy_kiln_at("classified", classified)
        .start()
        .await
        .expect("Failed to start server")
}

fn kiln_name(name: &str) -> crucible_core::config::KilnName {
    crucible_core::config::KilnName::parse(name).expect("a valid test kiln name")
}

async fn create_session(client: &DaemonClient) -> String {
    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![crucible_daemon::test_support::kiln_name("kiln")],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await
        .expect("session_create failed");

    result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string()
}

#[tokio::test]
async fn connect_then_disconnect_kiln_roundtrips() {
    let server = start_server().await;

    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");
    let session_id = create_session(&client).await;

    let created_with = vec![serde_json::json!("kiln")];
    let with_extra = vec![serde_json::json!("kiln"), serde_json::json!("extra-kiln")];

    let scope = client
        .session_connect_kiln(&session_id, &kiln_name("extra-kiln"))
        .await
        .expect("connect_kiln failed");
    assert_eq!(scope["kilns"].as_array(), Some(&with_extra));

    // Idempotent: connecting again doesn't duplicate.
    let scope = client
        .session_connect_kiln(&session_id, &kiln_name("extra-kiln"))
        .await
        .expect("second connect_kiln failed");
    assert_eq!(scope["kilns"].as_array(), Some(&with_extra));

    let scope = client
        .session_disconnect_kiln(&session_id, &kiln_name("extra-kiln"))
        .await
        .expect("disconnect_kiln failed");
    assert_eq!(scope["kilns"].as_array(), Some(&created_with));

    // Persisted: session.get reflects the final set.
    let session = client.session_get(&session_id).await.unwrap();
    assert_eq!(session["kilns"].as_array(), Some(&created_with));

    server.shutdown().await;
}

/// No kiln in the set is privileged any more: the one a session was created
/// with detaches like any other, and re-attaching it is an ordinary idempotent
/// connect rather than an "already primary" error. Flattening removed the
/// distinction, and this is the test that used to assert it.
#[tokio::test]
async fn the_kiln_a_session_was_created_with_detaches_like_any_other() {
    let server = start_server().await;
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");
    let session_id = create_session(&client).await;

    let scope = client
        .session_disconnect_kiln(&session_id, &kiln_name("kiln"))
        .await
        .expect("detaching the create-time kiln is allowed");
    assert_eq!(
        scope["kilns"].as_array().map(Vec::len),
        Some(0),
        "the set must be empty after detaching its only member: {:?}",
        scope["kilns"]
    );

    let scope = client
        .session_connect_kiln(&session_id, &kiln_name("kiln"))
        .await
        .expect("re-attaching it is an ordinary connect");
    assert_eq!(
        scope["kilns"].as_array(),
        Some(&vec![serde_json::json!("kiln")])
    );

    server.shutdown().await;
}

/// A session's workspace is fixed at creation. `session.set_workspace` stays
/// on the wire so an older client gets a refusal it can show, but the daemon
/// changes nothing: not to another directory, and not to none.
#[tokio::test]
async fn set_workspace_is_refused_and_the_session_keeps_its_workspace() {
    let server = start_server().await;
    let created_in = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();

    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");
    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![crucible_daemon::test_support::kiln_name("kiln")],
            workspace: Some(created_in.path().to_path_buf()),
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await
        .expect("session_create failed");
    let session_id = result["session_id"].as_str().unwrap().to_string();

    let err = client
        .session_set_workspace(&session_id, Some(other.path()))
        .await
        .expect_err("a workspace change on an existing session must be refused");
    assert!(
        err.to_string().contains("created in"),
        "the refusal must say the workspace is fixed at creation: {err}"
    );

    let err = client
        .session_set_workspace(&session_id, None)
        .await
        .expect_err("detaching the workspace must be refused too");
    assert!(
        err.to_string().contains("created in"),
        "the refusal must say the workspace is fixed at creation: {err}"
    );

    let session = client.session_get(&session_id).await.unwrap();
    assert_eq!(
        session["workspace"].as_str().unwrap(),
        created_in.path().to_string_lossy(),
        "the session must keep the workspace it was created with"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn connect_kiln_rejected_by_trust_leaves_kiln_unopened() {
    let server = start_server().await;
    // A kiln classified Confidential (requires Local trust), registered by the
    // fixture. The session below has no agent, so its provider trust resolves
    // to Cloud, which cannot satisfy Confidential — the attach must be refused.

    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");
    let session_id = create_session(&client).await;

    let err = client
        .session_connect_kiln(&session_id, &kiln_name("classified"))
        .await
        .expect_err("trust-rejected attach must fail");
    assert!(
        err.to_string().contains("insufficient"),
        "unexpected error: {err}"
    );

    // The refusal must leave no side effect. `kiln.list` names every REGISTERED
    // kiln now, so the claim is not that the row is absent — it is that the row
    // is still CLOSED. An opened kiln is one the daemon indexes and serves
    // files from, and the trust floor refused exactly that.
    let listed = client.kiln_list().await.expect("kiln.list failed");
    let row = listed
        .iter()
        .find(|row| row["name"] == serde_json::json!("classified"))
        .unwrap_or_else(|| panic!("the registered kiln is listed: {listed:?}"));
    assert_eq!(
        row["open"],
        serde_json::json!(false),
        "a trust-refused attach must not open the kiln: {listed:?}"
    );

    // Session scope is unchanged — the rejected kiln was never added.
    let session = client.session_get(&session_id).await.unwrap();
    assert_eq!(
        session["kilns"].as_array(),
        Some(&vec![serde_json::json!("kiln")])
    );

    server.shutdown().await;
}
