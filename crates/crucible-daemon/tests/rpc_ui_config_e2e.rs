//! End-to-end: the Lua-defined theme reaches a client over `ui.config`.
//!
//! This is the load-bearing test for the styling transport. Unit tests of the
//! theme parser pass fine while the delivery path is unwired — which is exactly
//! how `crucible.theme.setup()` came to be dead code: it parsed correctly into a
//! process-global that, in split-process mode, nothing on the TUI side ever read.
//!
//! RED-verify this suite by unwiring the `ui.config` handler or the theme->wire
//! conversion, NOT by breaking the parser. A parser-level break would also fail
//! the unit tests, which proves nothing about delivery.

use anyhow::Result;
use crucible_daemon::{DaemonClient, Server};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::task::JoinHandle;

/// In-process test server. Mirrors the pattern in `rpc_config_agent_e2e.rs`:
/// the data root is injected as a *value* via `bind_with_data_home`, never by
/// mutating process env, so parallel runs stay hermetic and a developer's real
/// `~/.crucible` is never read.
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

        // Poll for readiness rather than sleeping a fixed interval. A fixed
        // startup sleep is the standard flake source here: under a loaded box
        // the socket may not be accepting yet when the timer elapses.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if DaemonClient::connect_to(&socket_path).await.is_ok() {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!("daemon did not start accepting connections within 5s");
            }
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

/// The daemon evaluates the theme in Lua; a client must be able to fetch it.
/// Before this transport existed, a client had no way to reach it at all.
#[tokio::test]
async fn ui_config_delivers_the_lua_theme_to_a_client() {
    let server = TestServer::start().await.expect("server starts");
    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("client connects");

    let resp = client
        .call(
            "ui.config",
            json!({ "background": "dark", "color_depth": "truecolor" }),
        )
        .await
        .expect("ui.config is a known method");

    // Versioned from commit one: this payload crosses a process boundary, so a
    // TUI and daemon at different versions will meet in the wild.
    let version = resp
        .get("version")
        .and_then(|v| v.as_u64())
        .expect("payload is versioned");
    assert!(version >= 1, "version starts at 1, got {version}");

    let theme = resp.get("theme").expect("payload carries a theme");

    // The daemon loads `runtime/themes/default.lua` into the Lua config store at
    // boot. Seeing its name here proves we shipped the *Lua-evaluated* theme and
    // not a Rust-side fallback that would look superficially similar.
    assert_eq!(
        theme["name"], "default",
        "expected the Lua-loaded theme, got: {theme:#?}"
    );

    server.shutdown().await;
}

/// Colours cross the wire in the *authoring* representation, unresolved.
///
/// Two reasons this matters, and both are load-bearing:
///   1. oil's `Color` enum (`Rgb(u8,u8,u8)`, `Indexed(u8)`, named variants) is an
///      internal rendering detail. Serializing it would make it a public contract.
///   2. Adaptive pairs must stay pairs. The client resolves them against its own
///      terminal background and colour depth — which is the whole point of the
///      handshake. Shipping a resolved colour would defeat it.
#[tokio::test]
async fn ui_config_ships_colors_unresolved_in_authoring_form() {
    let server = TestServer::start().await.expect("server starts");
    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("client connects");

    let resp = client
        .call(
            "ui.config",
            json!({ "background": "dark", "color_depth": "truecolor" }),
        )
        .await
        .expect("ui.config is a known method");

    let colors = &resp["theme"]["colors"];

    // Named and hex values survive as written in `runtime/themes/default.lua`.
    assert_eq!(
        colors["primary"], "cyan",
        "named colour should cross the wire as its name, got: {colors:#?}"
    );
    assert_eq!(
        colors["background"], "#282c34",
        "hex colour should cross the wire as hex, got: {colors:#?}"
    );

    server.shutdown().await;
}

/// Runtime theme switching: the whole reason the stores became swappable.
/// A bogus name must be refused loudly rather than silently leaving the old
/// theme, or a typo looks like the command did nothing.
#[tokio::test]
async fn ui_set_theme_rejects_an_unknown_name() {
    let server = TestServer::start().await.expect("server starts");
    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("client connects");

    let err = client
        .call("ui.set_theme", json!({ "name": "no-such-theme" }))
        .await
        .expect_err("an unknown theme must be an error");
    assert!(
        format!("{err}").contains("not found"),
        "expected a not-found error, got: {err}"
    );

    server.shutdown().await;
}

/// A theme name is a bare stem by construction, so a path in it is either a
/// mistake or an attempt to read an arbitrary file.
#[tokio::test]
async fn ui_set_theme_refuses_path_traversal() {
    let server = TestServer::start().await.expect("server starts");
    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("client connects");

    for name in ["../../etc/passwd", "sub/theme", "..", ""] {
        let err = client
            .call("ui.set_theme", json!({ "name": name }))
            .await
            .expect_err("traversal must be refused");
        assert!(
            format!("{err}").contains("invalid theme name")
                || format!("{err}").contains("not found"),
            "name {name:?} produced: {err}"
        );
    }

    server.shutdown().await;
}

/// A client that asks for a light background must still receive the same
/// unresolved payload — the daemon does not resolve on the client's behalf.
/// This pins the direction of responsibility so a later "optimisation" that
/// resolves daemon-side fails loudly here.
#[tokio::test]
async fn ui_config_does_not_resolve_adaptive_colors_daemon_side() {
    let server = TestServer::start().await.expect("server starts");
    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("client connects");

    let dark = client
        .call(
            "ui.config",
            json!({ "background": "dark", "color_depth": "truecolor" }),
        )
        .await
        .expect("ui.config is a known method");

    let light = client
        .call(
            "ui.config",
            json!({ "background": "light", "color_depth": "truecolor" }),
        )
        .await
        .expect("ui.config is a known method");

    assert_eq!(
        dark["theme"], light["theme"],
        "the daemon ships one unresolved palette; the client resolves it"
    );

    server.shutdown().await;
}
