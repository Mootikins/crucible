//! `client_state.get` / `client_state.set`, through the live RPC method —
//! the daemon's generic store for a client's own display state, proved here
//! exactly as the TUI, a Lua script, or the web server would reach it: a
//! plain RPC call, no HTTP route involved. The web's pane-layout and
//! recents-list routes used to keep this on their own process's disk; a
//! client that is not the web server had no equivalent store at all.

use crucible_core::protocol::RpcMethod;
use crucible_daemon::{DaemonClient, Server};
use serde_json::json;

#[tokio::test]
async fn a_value_set_by_one_client_is_read_back_by_the_same_client() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("daemon.sock");
    let data = dir.path().join("data");

    let server = Server::bind_with_data_home(&socket, data).await.unwrap();
    let shutdown = server.shutdown_handle();
    let task = tokio::spawn(server.run());
    let client = DaemonClient::connect_to(&socket).await.unwrap();

    let set_reply: serde_json::Value = client
        .call(
            RpcMethod::ClientStateSet,
            json!({ "client": "cru-cli", "key": "last-workspace", "value": "/home/u/notes" }),
        )
        .await
        .unwrap();
    assert_eq!(set_reply["status"], "ok", "{set_reply}");

    let get_reply: serde_json::Value = client
        .call(
            RpcMethod::ClientStateGet,
            json!({ "client": "cru-cli", "key": "last-workspace" }),
        )
        .await
        .unwrap();
    assert_eq!(get_reply["value"], json!("/home/u/notes"), "{get_reply}");

    drop(client);
    shutdown.send(()).unwrap();
    task.await.unwrap().unwrap();
}

/// Two different `client` namespaces never see each other's state — the
/// isolation a standalone web instance needs from the production one, proved
/// through the RPC method rather than the daemon's internal function.
#[tokio::test]
async fn two_different_clients_do_not_share_state_through_the_rpc_method() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("daemon.sock");
    let data = dir.path().join("data");

    let server = Server::bind_with_data_home(&socket, data).await.unwrap();
    let shutdown = server.shutdown_handle();
    let task = tokio::spawn(server.run());
    let client = DaemonClient::connect_to(&socket).await.unwrap();

    let _: serde_json::Value = client
        .call(
            RpcMethod::ClientStateSet,
            json!({ "client": "web", "key": "layout", "value": "production" }),
        )
        .await
        .unwrap();
    let _: serde_json::Value = client
        .call(
            RpcMethod::ClientStateSet,
            json!({ "client": "web-standalone", "key": "layout", "value": "debug" }),
        )
        .await
        .unwrap();

    let get_reply: serde_json::Value = client
        .call(
            RpcMethod::ClientStateGet,
            json!({ "client": "web", "key": "layout" }),
        )
        .await
        .unwrap();
    assert_eq!(get_reply["value"], json!("production"), "{get_reply}");

    drop(client);
    shutdown.send(()).unwrap();
    task.await.unwrap().unwrap();
}

/// A `key` that tries to escape the store's own directory is refused, not
/// silently confined — the same rule every path-taking RPC method applies.
#[tokio::test]
async fn a_key_with_a_traversal_sequence_is_refused() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("daemon.sock");
    let data = dir.path().join("data");

    let server = Server::bind_with_data_home(&socket, data).await.unwrap();
    let shutdown = server.shutdown_handle();
    let task = tokio::spawn(server.run());
    let client = DaemonClient::connect_to(&socket).await.unwrap();

    let err = client
        .call::<_, serde_json::Value>(
            RpcMethod::ClientStateSet,
            json!({ "client": "web", "key": "../../etc/passwd", "value": "x" }),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("identifier"), "{err}");

    drop(client);
    shutdown.send(()).unwrap();
    task.await.unwrap().unwrap();
}
