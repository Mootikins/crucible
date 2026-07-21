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

        let server =
            Server::bind_with_data_home(&socket_path, temp_dir.path().to_path_buf()).await?;
        let shutdown_handle = server.shutdown_handle();

        let server_handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

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

async fn create_session(client: &DaemonClient, kiln: &std::path::Path) -> String {
    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: Some(kiln.to_path_buf()),
            workspace: None,
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: None,
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
    let kiln_dir = tempfile::tempdir().unwrap();
    let extra_kiln = tempfile::tempdir().unwrap();

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");
    let session_id = create_session(&client, kiln_dir.path()).await;

    let scope = client
        .session_connect_kiln(&session_id, extra_kiln.path())
        .await
        .expect("connect_kiln failed");
    let connected = scope["connected_kilns"].as_array().unwrap();
    assert_eq!(connected.len(), 1);
    assert_eq!(
        connected[0].as_str().unwrap(),
        extra_kiln.path().to_string_lossy()
    );

    // Idempotent: connecting again doesn't duplicate.
    let scope = client
        .session_connect_kiln(&session_id, extra_kiln.path())
        .await
        .expect("second connect_kiln failed");
    assert_eq!(scope["connected_kilns"].as_array().unwrap().len(), 1);

    let scope = client
        .session_disconnect_kiln(&session_id, extra_kiln.path())
        .await
        .expect("disconnect_kiln failed");
    assert!(scope["connected_kilns"]
        .as_array()
        .map(|a| a.is_empty())
        .unwrap_or(true));

    // Persisted: session.get reflects the final (empty) connected set.
    let session = client.session_get(&session_id).await.unwrap();
    assert!(session["connected_kilns"]
        .as_array()
        .map(|a| a.is_empty())
        .unwrap_or(true));

    server.shutdown().await;
}

#[tokio::test]
async fn primary_kiln_cannot_be_detached_or_reattached() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().unwrap();

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");
    let session_id = create_session(&client, kiln_dir.path()).await;

    let err = client
        .session_disconnect_kiln(&session_id, kiln_dir.path())
        .await
        .expect_err("detaching the primary kiln must fail");
    assert!(
        err.to_string().contains("primary kiln"),
        "unexpected error: {err}"
    );

    let err = client
        .session_connect_kiln(&session_id, kiln_dir.path())
        .await
        .expect_err("attaching the primary kiln must fail");
    assert!(
        err.to_string().contains("primary kiln"),
        "unexpected error: {err}"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn set_workspace_attaches_and_detach_falls_back_to_kiln() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().unwrap();
    let project_dir = tempfile::tempdir().unwrap();

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");
    let session_id = create_session(&client, kiln_dir.path()).await;

    let scope = client
        .session_set_workspace(&session_id, Some(project_dir.path()))
        .await
        .expect("set_workspace failed");
    assert_eq!(
        scope["workspace"].as_str().unwrap(),
        project_dir.path().to_string_lossy()
    );

    // Detach: workspace falls back to the kiln path (the workspace-less state).
    let scope = client
        .session_set_workspace(&session_id, None)
        .await
        .expect("workspace detach failed");
    assert_eq!(
        scope["workspace"].as_str().unwrap(),
        kiln_dir.path().to_string_lossy()
    );

    server.shutdown().await;
}

#[tokio::test]
async fn set_workspace_rejects_nonexistent_directory() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().unwrap();

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");
    let session_id = create_session(&client, kiln_dir.path()).await;

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
