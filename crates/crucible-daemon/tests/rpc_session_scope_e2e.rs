//! End-to-end tests for mid-session scope mutations:
//! session.connect_kiln, session.disconnect_kiln, session.set_workspace.

use anyhow::Result;
use crucible_daemon::DaemonClient;
use crucible_daemon::Server;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::task::JoinHandle;

struct TestServer {
    _temp_dir: TempDir,
    socket_path: PathBuf,
    _server_handle: JoinHandle<()>,
    shutdown_handle: tokio::sync::broadcast::Sender<()>,
}

fn ensure_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

impl TestServer {
    async fn start() -> Result<Self> {
        ensure_crypto_provider();
        let temp_dir = tempfile::tempdir()?;
        let socket_path = temp_dir.path().join("daemon.sock");

        // Three registered kilns: the one every session is created with, a
        // second to attach mid-session, and one classified Confidential so the
        // trust gate has something to refuse. All outside the data root, which
        // the registration floor denies.
        let kiln = temp_dir.path().join("kilns").join("kiln");
        let extra = temp_dir.path().join("kilns").join("extra-kiln");
        let classified = temp_dir.path().join("kilns").join("classified");
        for dir in [&kiln, &extra, &classified] {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::create_dir_all(classified.join(".crucible"))?;
        std::fs::write(
            classified.join(".crucible").join("project.toml"),
            "[[kilns]]\npath = \".\"\ndata_classification = \"confidential\"\n",
        )?;

        let server = Server::bind_with_data_home_and_kilns(
            &socket_path,
            temp_dir.path().to_path_buf(),
            &[
                ("kiln", &kiln),
                ("extra-kiln", &extra),
                ("classified", &classified),
            ],
        )
        .await?;
        let shutdown_handle = server.shutdown_handle();

        let server_handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        // Poll for readiness rather than sleeping a fixed interval. Under a
        // loaded box the socket may not be accepting when a fixed timer
        // elapses, which is this suite's intermittent-failure source.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if DaemonClient::connect_to(&socket_path).await.is_ok() {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "daemon did not start accepting connections within 5s"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        Ok(Self {
            _temp_dir: temp_dir,
            socket_path,
            _server_handle: server_handle,
            shutdown_handle,
        })
    }

    async fn shutdown(self) {
        let _ = self.shutdown_handle.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
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
    let server = TestServer::start().await.expect("Failed to start server");

    let client = DaemonClient::connect_to(&server.socket_path)
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
    let server = TestServer::start().await.expect("Failed to start server");
    let client = DaemonClient::connect_to(&server.socket_path)
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

#[tokio::test]
async fn set_workspace_attaches_and_detach_leaves_the_session_with_none() {
    let server = TestServer::start().await.expect("Failed to start server");
    let project_dir = tempfile::tempdir().unwrap();

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");
    let session_id = create_session(&client).await;

    let scope = client
        .session_set_workspace(&session_id, Some(project_dir.path()))
        .await
        .expect("set_workspace failed");
    assert_eq!(
        scope["workspace"].as_str().unwrap(),
        project_dir.path().to_string_lossy()
    );

    // Detach: the session then has NO workspace, and says so on the wire.
    // It used to be handed back the kiln path, which left every client
    // re-deriving `workspace == kilns[0]` to tell "no project" from "the
    // project is the kiln" — and the web UI had its own copy of that rule.
    let scope = client
        .session_set_workspace(&session_id, None)
        .await
        .expect("workspace detach failed");
    assert!(
        scope["workspace"].is_null(),
        "detach must report no workspace, got {}",
        scope["workspace"]
    );
    assert!(
        !scope["workspace"]
            .as_str()
            .is_some_and(|w| w.ends_with("kilns/kiln")),
        "and specifically not the kiln path: {}",
        scope["workspace"]
    );

    server.shutdown().await;
}

#[tokio::test]
async fn connect_kiln_rejected_by_trust_leaves_kiln_unopened() {
    let server = TestServer::start().await.expect("Failed to start server");
    // A kiln classified Confidential (requires Local trust), registered by the
    // fixture. The session below has no agent, so its provider trust resolves
    // to Cloud, which cannot satisfy Confidential — the attach must be refused.

    let client = DaemonClient::connect_to(&server.socket_path)
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

    // The refusal must leave no side effect: the rejected kiln was never opened,
    // so it must not surface in kiln.list (where it would otherwise be indexed).
    let listed = serde_json::to_string(&client.kiln_list().await.expect("kiln.list failed"))
        .expect("serialize kiln list");
    assert!(
        !listed.contains("classified"),
        "rejected kiln leaked into kiln.list: {listed}"
    );

    // Session scope is unchanged — the rejected kiln was never added.
    let session = client.session_get(&session_id).await.unwrap();
    assert_eq!(
        session["kilns"].as_array(),
        Some(&vec![serde_json::json!("kiln")])
    );

    server.shutdown().await;
}

#[tokio::test]
async fn set_workspace_rejects_nonexistent_directory() {
    let server = TestServer::start().await.expect("Failed to start server");
    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");
    let session_id = create_session(&client).await;

    let err = client
        .session_set_workspace(
            &session_id,
            Some(std::path::Path::new("/definitely/not/a/real/dir")),
        )
        .await
        .expect_err("nonexistent workspace must be rejected");
    assert!(
        err.to_string().contains("not a directory"),
        "unexpected error: {err}"
    );

    server.shutdown().await;
}
