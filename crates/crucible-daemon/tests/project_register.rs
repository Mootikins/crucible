//! `project.register`'s untrusted-root rule, proved through the real RPC
//! path — the same path the TUI and a Lua script use, not the web's HTTP
//! route. Before this rule moved into the daemon, only the web route's own
//! pre-check refused a credential store; a raw RPC caller who set no
//! `untrusted` flag at all (the only shape `PathRequest` could send) skipped
//! it entirely, because the daemon method never looked for one.
use crucible_core::protocol::RpcMethod;
use crucible_daemon::{DaemonClient, Server};
use serde_json::json;

/// A local caller (the CLI, the TUI, a Lua script) may register its own
/// dotfiles repo: `register`'s floor is `forbidden_root_reason` alone.
#[tokio::test]
async fn a_trusted_caller_may_register_a_directory_holding_a_credential_store() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let dotfiles = dir.path().join("dotfiles");
    std::fs::create_dir_all(dotfiles.join(".ssh")).unwrap();
    let socket = dir.path().join("daemon.sock");
    let data = dir.path().join("data");

    let server = Server::bind_with_data_home(&socket, data).await.unwrap();
    let shutdown = server.shutdown_handle();
    let task = tokio::spawn(server.run());
    let client = DaemonClient::connect_to(&socket).await.unwrap();

    let reply: serde_json::Value = client
        .call(RpcMethod::ProjectRegister, json!({ "path": dotfiles }))
        .await
        .unwrap();
    assert_eq!(reply["path"], json!(dotfiles.canonicalize().unwrap()));

    drop(client);
    shutdown.send(()).unwrap();
    task.await.unwrap().unwrap();
}

/// The same request, with `untrusted: true` — the field the web API sets and
/// that `PathRequest` never carried — is refused through the RPC method
/// itself. No web route is involved: this is the daemon's own decision, so a
/// caller that skips the web's axum layer entirely cannot skip the rule too.
#[tokio::test]
async fn an_untrusted_caller_is_refused_a_directory_holding_a_credential_store() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let dotfiles = dir.path().join("dotfiles");
    std::fs::create_dir_all(dotfiles.join(".ssh")).unwrap();
    let socket = dir.path().join("daemon.sock");
    let data = dir.path().join("data");

    let server = Server::bind_with_data_home(&socket, data).await.unwrap();
    let shutdown = server.shutdown_handle();
    let task = tokio::spawn(server.run());
    let client = DaemonClient::connect_to(&socket).await.unwrap();

    let err = client
        .call::<_, serde_json::Value>(
            RpcMethod::ProjectRegister,
            json!({ "path": dotfiles, "untrusted": true }),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("credential store"), "{err}");

    drop(client);
    shutdown.send(()).unwrap();
    task.await.unwrap().unwrap();
}

/// An untrusted caller may still register an ordinary directory: the extra
/// rule narrows the floor, it does not replace registration with an
/// allowlist.
#[tokio::test]
async fn an_untrusted_caller_may_register_an_ordinary_directory() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("app");
    std::fs::create_dir_all(&project).unwrap();
    let socket = dir.path().join("daemon.sock");
    let data = dir.path().join("data");

    let server = Server::bind_with_data_home(&socket, data).await.unwrap();
    let shutdown = server.shutdown_handle();
    let task = tokio::spawn(server.run());
    let client = DaemonClient::connect_to(&socket).await.unwrap();

    let reply: serde_json::Value = client
        .call(
            RpcMethod::ProjectRegister,
            json!({ "path": project, "untrusted": true }),
        )
        .await
        .unwrap();
    assert_eq!(reply["path"], json!(project.canonicalize().unwrap()));

    drop(client);
    shutdown.send(()).unwrap();
    task.await.unwrap().unwrap();
}
