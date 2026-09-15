//! The idle timer, end to end through a real `Server`.
//!
//! `super::idle` proves the POLICY against a fake clock. These two prove the
//! WIRING: that `run()` actually leaves its accept loop when the window
//! expires, and that a connected client actually holds it open. The window is
//! sub-second here, which the probe floor (`PROBE_MIN`) turns into a one-second
//! check.

use super::*;
use crate::DaemonClient;

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

    tokio::time::pause();
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
    let client = DaemonClient::connect_to(&sock_path).await.unwrap();
    client.ping().await.unwrap();
    tokio::time::pause();

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

    tokio::time::pause();
    let task = tokio::spawn(server.run());
    std::fs::remove_file(&sock_path).unwrap();

    let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), task).await;
    assert!(
        outcome.is_ok(),
        "a daemon no client can find again must not wait out its idle window"
    );
}

/// An agent whose turn parks until it is released.
///
/// It stands for the long autonomous turn the scenario is about: the client
/// has gone, nothing holds a socket, and the daemon is still working.
struct ParkedTurnAgent {
    release: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl crucible_core::turn::Agent for ParkedTurnAgent {
    fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
        crucible_core::turn::AgentCapabilities::default()
    }
    async fn turn<'a>(
        &'a mut self,
        _ctx: crucible_core::turn::TurnContext,
    ) -> Result<
        futures::stream::BoxStream<'a, crucible_core::turn::TurnEvent>,
        crucible_core::turn::AgentError,
    > {
        use crucible_core::turn::{StopReason, TurnEvent};
        let release = self.release.clone();
        let body = async_stream::stream! {
            release.notified().await;
            yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
        };
        Ok(Box::pin(body))
    }
    async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
        Ok(())
    }
    async fn switch_model(&mut self, _: &str) -> Result<(), crucible_core::turn::NotSupported> {
        Err(crucible_core::turn::NotSupported::new("switch_model"))
    }
}

crucible_core::impl_unsupported_session_knobs!(ParkedTurnAgent);

#[async_trait::async_trait]
impl crucible_core::traits::chat::AgentHandle for ParkedTurnAgent {
    async fn send_message_fire_and_forget(
        &mut self,
        _: String,
    ) -> crucible_core::traits::chat::ChatResult<()> {
        Ok(())
    }
    async fn clear_history(&mut self) -> crucible_core::traits::chat::ChatResult<()> {
        Ok(())
    }
    async fn set_mode_str(&mut self, _: &str) -> crucible_core::traits::chat::ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        "ask"
    }
}

fn parked_turn_agent_card() -> crucible_core::session::SessionAgent {
    crucible_core::session::SessionAgent {
        mode: None,
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: crucible_core::config::BackendType::Ollama,
        model: "llama3.2".to_string(),
        system_prompt: "You are helpful.".to_string(),
        max_context_tokens: None,
        endpoint: None,
        env_overrides: std::collections::HashMap::new(),
        mcp_servers: Vec::new(),
        agent_card_name: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
        context_budget: None,
        context_strategy: Default::default(),
        tool_policy: None,
    }
}

/// The defect this exists for: the daemon deliberately survives the TUI that
/// started the turn, so half an hour later the turn is still running and NO
/// client holds a connection. An idle test that counts only connections and
/// background jobs sees an idle daemon and breaks the accept loop mid-turn.
#[tokio::test]
async fn an_in_flight_turn_holds_the_daemon_open_with_no_client() {
    let tmp = TempDir::new().unwrap();
    let (_sock, server) = bind_idle_server(&tmp, std::time::Duration::from_millis(200)).await;

    let session_manager = server.session_manager.clone();
    let agent_manager = server.agent_manager.clone();
    let event_tx = server.rpc_context.event_tx.clone();

    let release = Arc::new(tokio::sync::Notify::new());
    let agent_release = release.clone();
    agent_manager.set_agent_factory_override(Box::new(move |_, _| {
        let release = agent_release.clone();
        Box::pin(async move {
            Ok(Box::new(ParkedTurnAgent { release })
                as Box<
                    dyn crucible_core::traits::chat::AgentHandle + Send + Sync,
                >)
        })
    }));

    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let session = session_manager
        .create_session(
            crucible_core::session::SessionType::Chat,
            Vec::new(),
            Some(workspace),
            None,
        )
        .await
        .unwrap();
    agent_manager
        .configure_agent(&session.id, parked_turn_agent_card())
        .await
        .unwrap();

    // The turn starts and parks. Nothing ever connects to this daemon.
    agent_manager
        .send_message(&session.id, "work".to_string(), &event_tx, false, None)
        .await
        .unwrap();

    tokio::time::pause();
    let mut task = tokio::spawn(server.run());

    // Many probe periods with a turn in flight and zero connections.
    let while_working = tokio::time::timeout(std::time::Duration::from_secs(4), &mut task).await;
    assert!(
        while_working.is_err(),
        "the daemon left its accept loop mid-turn; a running turn must hold it open"
    );

    // And once the turn ends it is idle again, so it must still exit.
    release.notify_waiters();
    let after_the_turn = tokio::time::timeout(std::time::Duration::from_secs(15), task).await;
    assert!(
        after_the_turn.is_ok(),
        "the daemon must end itself once the turn it was holding open has finished"
    );
}
