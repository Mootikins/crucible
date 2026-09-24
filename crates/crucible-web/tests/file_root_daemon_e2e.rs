//! The web file routes over a real daemon: one enclosing-root rule.
//!
//! The daemon decides which root holds a path. The web routes send the path
//! and show the answer. These tests put a real daemon behind the real web
//! routes, so a web route that decides containment by itself disagrees with
//! the daemon in a way that a mock cannot show.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use crucible_daemon::rpc_client::SessionCreateParams;
use crucible_daemon::{DaemonClient, Server};
use crucible_web::test_support::{build_mock_state, build_test_app};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tower::ServiceExt;

/// A daemon with one registered kiln, and a web app over it.
struct Fixture {
    _home: tempfile::TempDir,
    kiln: PathBuf,
    daemon: DaemonClient,
    app: Router,
}

async fn fixture() -> Fixture {
    let home = tempfile::tempdir().expect("a temp home");
    let kiln = home.path().join("kiln");
    std::fs::create_dir_all(&kiln).expect("the kiln");
    let socket = home.path().join("daemon.sock");
    let server = Server::bind_with_data_home_and_kilns(
        &socket,
        home.path().join("data"),
        &[("kiln", &kiln)],
    )
    .await
    .expect("the daemon binds");
    tokio::spawn(async move {
        let _ = server.run().await;
    });
    let daemon = connect(&socket).await;
    let app = build_test_app(build_mock_state(connect(&socket).await));
    Fixture {
        _home: home,
        kiln,
        daemon,
        app,
    }
}

/// Connect once the daemon accepts, rather than after a fixed sleep.
async fn connect(socket: &Path) -> DaemonClient {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match DaemonClient::connect_to(socket).await {
            Ok(client) => return client,
            Err(e) if tokio::time::Instant::now() >= deadline => {
                panic!("the daemon never accepted a connection: {e}")
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
        }
    }
}

/// Create a session in the fixture kiln. Answers the session's workspace.
async fn session_workspace(daemon: &DaemonClient, workspace: Option<&Path>) -> PathBuf {
    let created = daemon
        .session_create(SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![crucible_daemon::test_support::kiln_name("kiln")],
            workspace: workspace.map(Path::to_path_buf),
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await
        .expect("session.create");
    let id = created["session_id"].as_str().expect("a session id");
    let session = daemon.session_get(id).await.expect("session.get");
    PathBuf::from(
        session["workspace"]
            .as_str()
            .unwrap_or_else(|| panic!("the session has a workspace: {session}")),
    )
}

async fn get(app: &Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn daemon_write(daemon: &DaemonClient, path: &Path, content: &str) -> Value {
    daemon
        .call(
            "fs.write",
            json!({ "path": path, "operation": "put", "content": content }),
        )
        .await
        .expect("fs.write answers")
}

/// The folder that the daemon makes for a session with no workspace is a
/// root for the daemon. Each web route must admit it too. Before the fix, the
/// text read admitted it and the canvas read refused it.
#[tokio::test]
async fn a_session_folder_is_a_root_for_every_file_route() {
    let f = fixture().await;
    let folder = session_workspace(&f.daemon, None).await;
    let note = folder.join("note.md");
    let board = folder.join("board.canvas");

    for (path, content) in [(&note, "hello"), (&board, r#"{"nodes":[],"edges":[]}"#)] {
        let answer = daemon_write(&f.daemon, path, content).await;
        assert_eq!(
            answer["ok"],
            true,
            "the daemon admits {}: {answer}",
            path.display()
        );
    }

    let (status, body) = get(&f.app, &format!("/api/kiln/file?path={}", note.display())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["content"], "hello");

    let (status, body) = get(&f.app, &format!("/api/canvas?path={}", board.display())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        Path::new(body["kiln"].as_str().expect("a root")),
        folder.canonicalize().unwrap(),
        "the canvas root is the session folder: {body}"
    );
}

/// A session workspace that is not a registered project is no root for the
/// daemon. Before the fix, the web admitted the workspace of every session
/// that it listed, so it served a file that the daemon refused to write.
#[tokio::test]
async fn a_session_workspace_that_is_no_project_is_no_root() {
    let f = fixture().await;
    let outside = tempfile::tempdir().expect("a folder outside every root");
    let workspace = session_workspace(&f.daemon, Some(outside.path())).await;
    // `session.create` registers the workspace as a project. Take that back,
    // so that only the session record names the folder.
    f.daemon
        .project_unregister(&workspace)
        .await
        .expect("project.unregister");
    let secret = workspace.join("secret.txt");
    std::fs::write(&secret, "secret").unwrap();

    assert_eq!(
        daemon_write(&f.daemon, &secret, "overwritten").await["failure"],
        "not_found",
        "the daemon refuses the folder"
    );
    for uri in [
        format!("/api/kiln/file?path={}", secret.display()),
        format!("/api/file/raw?path={}", secret.display()),
    ] {
        let (status, body) = get(&f.app, &uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {body}");
    }
    assert_eq!(std::fs::read_to_string(&secret).unwrap(), "secret");
}

/// The fixture kiln is a root for the reads and the writes alike.
#[tokio::test]
async fn a_kiln_file_reads_back_through_the_daemon() {
    let f = fixture().await;
    let note = f.kiln.join("Note.md");
    assert_eq!(daemon_write(&f.daemon, &note, "# Note\n").await["ok"], true);

    let (status, body) = get(&f.app, &format!("/api/kiln/file?path={}", note.display())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["content"], "# Note\n");
    assert_eq!(
        body["content_hash"],
        crucible_core::note_edit::disk_hash("# Note\n")
    );

    let outside = f.kiln.parent().unwrap().join("outside.md");
    std::fs::write(&outside, "outside").unwrap();
    let (status, body) = get(
        &f.app,
        &format!("/api/kiln/file?path={}", outside.display()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}
