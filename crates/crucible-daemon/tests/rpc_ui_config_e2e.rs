//! End-to-end: the Lua-defined theme reaches a client over `ui.config`.
//!
//! This is the load-bearing test for the styling transport. Unit tests of the
//! theme parser pass fine while the delivery path is unwired — which is exactly
//! how `cru.colorscheme.setup()` came to be dead code: it parsed correctly into a
//! process-global that, in split-process mode, nothing on the TUI side ever read.
//!
//! RED-verify this suite by unwiring the `ui.config` handler or the theme->wire
//! conversion, NOT by breaking the parser. A parser-level break would also fail
//! the unit tests, which proves nothing about delivery.

mod common;

use crucible_core::protocol::RpcMethod;
use crucible_daemon::DaemonClient;
use serde_json::json;

/// In-process test server. Mirrors the pattern in `rpc_config_agent_e2e.rs`:
/// the data root is injected as a *value* via `bind_with_data_home`, never by
/// mutating process env, so parallel runs stay hermetic and a developer's real
/// `~/.crucible` is never read.
async fn start_server() -> common::InProcessDaemon {
    common::InProcessDaemonBuilder::new()
        .expect("a test daemon builder")
        .with_kiln("kiln")
        .start()
        .await
        .expect("Failed to start server")
}

/// The daemon evaluates the theme in Lua; a client must be able to fetch it.
/// Before this transport existed, a client had no way to reach it at all.
#[tokio::test]
async fn ui_config_delivers_the_lua_theme_to_a_client() {
    let server = start_server().await;
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("client connects");

    let resp = client
        .call::<_, serde_json::Value>(
            RpcMethod::UiConfig,
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
    let server = start_server().await;
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("client connects");

    let resp = client
        .call::<_, serde_json::Value>(
            RpcMethod::UiConfig,
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
    let server = start_server().await;
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("client connects");

    let err = client
        .call::<_, serde_json::Value>(RpcMethod::UiSetTheme, json!({ "name": "no-such-theme" }))
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
    let server = start_server().await;
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("client connects");

    for name in ["../../etc/passwd", "sub/theme", "..", ""] {
        let err = client
            .call::<_, serde_json::Value>(RpcMethod::UiSetTheme, json!({ "name": name }))
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
    let server = start_server().await;
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("client connects");

    let dark = client
        .call::<_, serde_json::Value>(
            RpcMethod::UiConfig,
            json!({ "background": "dark", "color_depth": "truecolor" }),
        )
        .await
        .expect("ui.config is a known method");

    let light = client
        .call::<_, serde_json::Value>(
            RpcMethod::UiConfig,
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
