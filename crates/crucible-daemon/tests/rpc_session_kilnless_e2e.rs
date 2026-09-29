//! End-to-end tests for kiln-less session creation — the branch's headline
//! feature: `session.create` with `kilns` omitted yields a session with NO
//! kiln, a tools-only agent that still composes with the mid-session scope
//! RPCs.
//!
//! Zero kilns is a legitimate state (§4.1), not a request for a default. The
//! daemon used to substitute its own data root, which is the PARENT of the
//! sessions root — so every kiln-less session carried an allowed root
//! enclosing every transcript the daemon had ever written, and `grep` walked
//! straight into it. An empty kiln set degrades capabilities (no note or kiln
//! tools, no precognition, no semantic search); it must never degrade
//! containment.
//!
//! HERMETICITY: `bind_with_data_home` injects the daemon's data root as a
//! *value* threaded through `Server`/`RpcContext` (no `CRUCIBLE_HOME` env
//! mutation), and every data-root-aware handler reads that value —
//! `handle_session_list`, `handle_kiln_list`, the archive sweep, and
//! `handle_session_create`. Nothing here touches the developer's real
//! `~/.crucible`.
//!
//! No test in this file is `#[ignore]`d: they run in `just test ci`. The
//! 22-line header that used to explain why they all were described the
//! `crucible_home()` fallback bug, which was fixed and the attributes removed
//! without anyone updating the prose.

mod common;

use common::{InProcessDaemon, InProcessDaemonBuilder};
use crucible_core::protocol::requests::SessionCreateRequest;
use crucible_daemon::DaemonClient;

/// One registered kiln, so the scope mutations below have a NAME to attach.
/// Its directory is outside the data root the registration floor refuses.
async fn start_server() -> InProcessDaemon {
    let builder = InProcessDaemonBuilder::new().expect("a test daemon builder");
    let extra = builder.data_home().join("extra-kiln");
    builder
        .with_kiln_at("extra-kiln", extra)
        .start()
        .await
        .expect("Failed to start server")
}

/// Create a session with an empty kiln set — the tools-only path.
async fn create_kilnless_session(client: &DaemonClient) -> crucible_core::session::SessionSummary {
    client
        .session_create(SessionCreateRequest {
            session_type: "chat".to_string(),
            ..Default::default()
        })
        .await
        .expect("kiln-less session_create failed")
}

#[tokio::test]
async fn kilnless_create_succeeds_and_returns_active_session() {
    let server = start_server().await;
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");

    let created = create_kilnless_session(&client).await;

    let session_id = created.id.as_str();
    assert!(!session_id.is_empty(), "session_id must be non-empty");
    assert_eq!(
        created.session_type,
        crucible_core::session::SessionType::Chat,
        "kiln-less session should keep its requested type"
    );

    // The response echoes the resolved kiln set, and for a kiln-less create
    // it is empty — the data root is emphatically not substituted in.
    assert!(
        created.kilns.is_empty(),
        "a kiln-less create must attach no kiln at all"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn kilnless_session_persists_an_empty_kiln_set() {
    let server = start_server().await;
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");

    let created = create_kilnless_session(&client).await;
    let session_id = created.id.to_string();

    // Re-read via session.get: empty on the wire must be empty on disk too.
    // The data root in particular must not have crept back in — it encloses
    // the sessions root, and an allowed root there is the transcript leak.
    let session = client.session_get(&session_id).await.unwrap();
    assert!(
        session.kilns.is_empty(),
        "a kiln-less session must persist an empty kiln set, not {}",
        server.data_home().display()
    );

    server.shutdown().await;
}

#[tokio::test]
async fn kilnless_no_workspace_gets_session_scratch_dir() {
    let server = start_server().await;
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");

    let created = create_kilnless_session(&client).await;
    let session_id = created.id.to_string();

    // With no workspace provided, the session gets its own session-unique
    // scratch workspace under `<data_home>/workspaces/<session_id>` — NOT the
    // kiln path. This is the session's filesystem containment boundary.
    let session = client.session_get(&session_id).await.unwrap();
    assert!(session.kilns.is_empty());
    let workspace = session.workspace.as_deref().expect("workspace present");

    let expected = server.data_home().join("workspaces").join(&session_id);
    assert_eq!(
        workspace,
        expected.as_path(),
        "kiln-less workspace should be a session-unique scratch dir under <data_home>/workspaces"
    );
    assert!(
        expected.is_dir(),
        "the scratch workspace directory should have been created"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn kilnless_session_composes_with_scope_mutations() {
    let server = start_server().await;
    let extra_kiln = crucible_core::config::KilnName::parse("extra-kiln").unwrap();

    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");

    let created = create_kilnless_session(&client).await;
    let session_id = created.id.to_string();

    // A kiln-less session must still accept the mid-session scope RPCs: connect
    // an extra kiln, then disconnect it, round-tripping back to empty.
    let scope = client
        .session_connect_kiln(&session_id, &extra_kiln)
        .await
        .expect("connect_kiln on a kiln-less session failed");
    let attached = scope["kilns"].as_array().unwrap();
    assert!(
        attached
            .iter()
            .any(|k| k.as_str() == Some(extra_kiln.as_str())),
        "extra kiln should be attached: {attached:?}"
    );

    let scope = client
        .session_disconnect_kiln(&session_id, &extra_kiln)
        .await
        .expect("disconnect_kiln on a kiln-less session failed");
    assert!(
        !scope["kilns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k.as_str() == Some(extra_kiln.as_str())),
        "the extra kiln should be gone after disconnect: {:?}",
        scope["kilns"]
    );

    // Persisted: attach then detach round-trips back to the empty set the
    // session was created with, rather than leaving a substituted default
    // behind.
    let session = client.session_get(&session_id).await.unwrap();
    assert!(
        session.kilns.is_empty(),
        "scope mutations must round-trip back to the empty kiln set"
    );

    server.shutdown().await;
}
