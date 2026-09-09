//! The idle timer, end to end through a real `Server`.
//!
//! `super::idle` proves the POLICY against a fake clock. These two prove the
//! WIRING: that `run()` actually leaves its accept loop when the window
//! expires, and that a connected client actually holds it open. The window is
//! sub-second here, which the probe floor (`PROBE_MIN`) turns into a one-second
//! check.

use super::*;

/// Bind a server with the idle timer armed to `window`, on an isolated root.
async fn bind_idle_server(tmp: &TempDir, window: std::time::Duration) -> (PathBuf, Server) {
    let sock_path = tmp.path().join("idle.sock");
    let kiln_path = tmp.path().join("kiln");
    std::fs::create_dir_all(&kiln_path).unwrap();

    let server = Server::bind_with_plugin_config(BindWithPluginConfigParams {
        path: sock_path.clone(),
        config_home: Some(tmp.path().join("config")),
        data_home: Some(tmp.path().to_path_buf()),
        idle_shutdown: Some(window),
        ..Default::default()
    })
    .await
    .unwrap();

    (sock_path, server)
}

/// The leak this exists for: an auto-spawned daemon whose client never came
/// back. Nothing signals it, so it has to end itself.
#[tokio::test]
async fn a_daemon_nobody_uses_ends_itself() {
    let tmp = TempDir::new().unwrap();
    let (_sock, server) = bind_idle_server(&tmp, std::time::Duration::from_millis(200)).await;

    let task = tokio::spawn(server.run());

    let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), task).await;
    assert!(
        outcome.is_ok(),
        "an idle daemon must leave its accept loop on its own"
    );
}

/// And it must not end itself while somebody is holding the socket, however
/// long that client sits there saying nothing.
#[tokio::test]
async fn a_connected_client_holds_the_daemon_open() {
    let tmp = TempDir::new().unwrap();
    let (sock_path, server) = bind_idle_server(&tmp, std::time::Duration::from_millis(200)).await;

    let mut task = tokio::spawn(server.run());
    let client = UnixStream::connect(&sock_path).await.unwrap();

    // Several probe periods with the connection open and idle. The daemon has
    // no traffic to serve; only the connection itself keeps it here.
    let while_connected = tokio::time::timeout(std::time::Duration::from_secs(3), &mut task).await;
    assert!(
        while_connected.is_err(),
        "a connected client must keep the daemon running"
    );

    drop(client);

    let after_disconnect = tokio::time::timeout(std::time::Duration::from_secs(10), task).await;
    assert!(
        after_disconnect.is_ok(),
        "the daemon must end itself once the last client leaves"
    );
}

/// The class of leak a test run produces: a `cru` command auto-spawns a
/// daemon, the test's `TempDir` goes away with the socket inside it, and the
/// daemon it started is left unreachable. Waiting out the full window would
/// leave it holding memory for half an hour to reach the same answer.
#[tokio::test]
async fn a_daemon_whose_socket_is_gone_exits_at_once() {
    let tmp = TempDir::new().unwrap();
    // The window is longer than this test can run, so only the missing socket
    // can end this daemon — but short enough that the probe derived from it
    // (a quarter of it) fires quickly.
    let (sock_path, server) = bind_idle_server(&tmp, std::time::Duration::from_secs(8)).await;

    let task = tokio::spawn(server.run());
    std::fs::remove_file(&sock_path).unwrap();

    let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), task).await;
    assert!(
        outcome.is_ok(),
        "a daemon no client can find again must not wait out its idle window"
    );
}
