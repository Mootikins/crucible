//! `<data_home>/llm.json` reaches the daemon's provider table.
//!
//! The unit tests in `llm_state.rs` prove the overlay function. This proves the
//! WIRING: that the overlay runs at bind, on the one value both consumers read,
//! and that a provider recorded in the state file is one a client can see over
//! the socket. A correct overlay function that nothing calls looks identical
//! from inside `llm_state.rs`.

mod common;

use common::{InProcessDaemon, InProcessDaemonBuilder};
use crucible_daemon::DaemonClient;
use tempfile::TempDir;

/// Bind over `data_home`, which the caller seeds with an `llm.json` (or
/// leaves untouched) before the daemon reads it at bind — the overlay this
/// suite is proving runs then, not later.
async fn start_server(data_home: &std::path::Path) -> InProcessDaemon {
    InProcessDaemonBuilder::at_data_home(data_home.to_path_buf())
        .start()
        .await
        .expect("start server")
}

/// A provider recorded in `llm.json` is one the daemon serves.
///
/// `providers.list` with `include_models: false` reads the same
/// `AgentManager` provider table every session creation reads, and it does not
/// dial the endpoint — so this asserts the table's CONTENTS without depending
/// on whether an Ollama is running on this box.
#[tokio::test]
async fn a_recorded_provider_reaches_the_daemons_provider_table() {
    let data_home = TempDir::new().expect("data home");

    // Written before the daemon binds, because bind is where the overlay runs.
    let state = crucible_daemon::llm_state::LlmStateStore::new(data_home.path());
    state
        .register_provider(
            "ollama",
            crucible_core::config::BackendType::Ollama,
            "llama3.2",
            true,
        )
        .expect("record the selection");

    let server = start_server(data_home.path()).await;
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("connect");

    let listed = client
        .call(
            "providers.list",
            serde_json::json!({ "include_models": false }),
        )
        .await
        .expect("providers.list answers");

    // Matched on `provider_type` plus `default_model`, not on the display
    // name: the model is the value THIS test wrote, so it cannot be satisfied
    // by a provider the daemon synthesized from the environment. (It does
    // synthesize some — a machine with `GLM_AUTH_TOKEN` set gets a Z.AI entry
    // in this listing, which is why the assertion has to be specific.)
    let providers = listed["providers"].as_array().expect("an array");
    let recorded = providers
        .iter()
        .find(|p| p["provider_type"] == "ollama")
        .unwrap_or_else(|| panic!("no ollama provider in the daemon's table: {listed}"));
    assert_eq!(
        recorded["default_model"], "llama3.2",
        "the model must be the one llm.json recorded: {listed}"
    );

    server.shutdown().await;
}

/// The file the daemon never wrote is not an error.
///
/// A daemon that refused to start because no provider had been chosen would be
/// worse than one with an empty table: a tools-only agent needs no provider.
#[tokio::test]
async fn a_daemon_with_no_recorded_selection_starts_normally() {
    let data_home = TempDir::new().expect("data home");
    let server = start_server(data_home.path()).await;
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("connect");

    let listed = client
        .call(
            "providers.list",
            serde_json::json!({ "include_models": false }),
        )
        .await
        .expect("providers.list answers with an empty table");
    assert!(listed["providers"].is_array());

    assert!(
        !data_home.path().join("llm.json").exists(),
        "reading the state layer must never create the file"
    );

    server.shutdown().await;
}
