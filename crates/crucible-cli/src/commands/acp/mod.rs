//! `cru acp` — run Crucible as an ACP **agent** over stdio.
//!
//! An ACP host (Zed, JetBrains, Neovim, marimo, or another Crucible instance)
//! spawns `cru acp` and speaks the Agent Client Protocol on stdin/stdout. Each
//! ACP session maps to an ordinary daemon session, so the knowledge graph,
//! Precognition, and session persistence all apply — the agent is exactly the
//! internal Crucible agent, exposed through a different front door.
//!
//! The daemon owns all logic; this module is a thin protocol adapter. See
//! [`agent::CrucibleAcpAgent`] for the translation layer and [`translate`] for
//! the (unit-tested) event/permission mapping.
//!
//! # Manual verification
//!
//! The protocol translation is covered by unit tests and an in-process
//! initialize round-trip (`tests::initialize_round_trip_over_stdio_framing`).
//! A full prompt turn needs a live LLM backend, so it is verified manually
//! rather than in an automated (flaky) test:
//!
//! Raw handshake — pipe a framed `initialize` in and see a valid response:
//! ```text
//! printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}\n' \
//!   | cru acp --kiln ~/my-kiln
//! ```
//!
//! Dogfood (Crucible hosting Crucible) — add to `crucible.toml`:
//! ```toml
//! [acp.agents.crucible]
//! command = "cru"
//! args = ["acp"]
//! ```
//! then `cru chat -a crucible` drives a full round trip through both roles.

mod agent;
mod translate;

use std::path::PathBuf;
use std::rc::Rc;

use agent_client_protocol::AgentSideConnection;
use anyhow::{Context, Result};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::info;

use crate::config::CliConfig;
use crate::kiln_discover::discover_kiln;

pub use agent::CrucibleAcpAgent;

/// Run the ACP agent server until the host closes stdin.
///
/// `kiln_override` comes from `cru acp --kiln <path>`; otherwise the kiln is
/// taken from config or discovered by walking up from the current directory.
/// This is headless: we never prompt (an editor host has no TTY).
pub async fn execute(mut config: CliConfig, kiln_override: Option<PathBuf>) -> Result<()> {
    resolve_kiln(&mut config, kiln_override)?;
    info!(kiln = %config.kiln_path.display(), "starting ACP agent (cru acp)");

    // The ACP `Agent` trait is `?Send`; its serving machinery uses `spawn_local`
    // and must run inside a `LocalSet`. The daemon RPC reader tasks are spawned
    // on the outer multi-thread runtime independently, so this composes cleanly.
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let agent = Rc::new(CrucibleAcpAgent::new(config));

            // ACP framing is line-delimited JSON on stdio. Wrap tokio handles as
            // futures AsyncRead/AsyncWrite for the connection.
            let incoming = tokio::io::stdin().compat();
            let outgoing = tokio::io::stdout().compat_write();

            let (conn, io_task) =
                AgentSideConnection::new(agent.clone(), outgoing, incoming, |fut| {
                    tokio::task::spawn_local(fut);
                });
            agent.set_connection(Rc::new(conn));

            io_task.await.context("ACP stdio connection terminated")
        })
        .await
}

fn resolve_kiln(config: &mut CliConfig, kiln_override: Option<PathBuf>) -> Result<()> {
    if let Some(path) = kiln_override {
        config.kiln_path = path;
    }
    if config.kiln_path.join(".crucible").is_dir() {
        return Ok(());
    }
    if let Some(found) = discover_kiln(None, None) {
        config.kiln_path = found.path;
        return Ok(());
    }
    anyhow::bail!(
        "no valid kiln found for `cru acp`; pass --kiln <path> or run from inside a kiln \
         (a directory containing .crucible/). Initialize one with `cru init`."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::{
        Agent, Client, ClientSideConnection, InitializeRequest, ProtocolVersion,
        RequestPermissionRequest, RequestPermissionResponse, SessionNotification,
    };
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    /// Minimal host-side client: the initialize round-trip never calls back into
    /// the client, so both methods are unreachable in this test.
    struct NoopClient;

    #[async_trait::async_trait(?Send)]
    impl Client for NoopClient {
        async fn request_permission(
            &self,
            _args: RequestPermissionRequest,
        ) -> agent_client_protocol::Result<RequestPermissionResponse> {
            Err(agent_client_protocol::Error::method_not_found())
        }
        async fn session_notification(
            &self,
            _args: SessionNotification,
        ) -> agent_client_protocol::Result<()> {
            Ok(())
        }
    }

    // Drives the real ACP framing over an in-process duplex pipe: a host-side
    // `ClientSideConnection` calls `initialize` on our `CrucibleAcpAgent` served
    // via `AgentSideConnection`. Exercises the LocalSet/spawn_local serving path
    // without needing a daemon (initialize is daemon-independent).
    #[tokio::test]
    async fn initialize_round_trip_over_stdio_framing() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (agent_end, client_end) = tokio::io::duplex(16 * 1024);
                let (a_read, a_write) = tokio::io::split(agent_end);
                let (c_read, c_write) = tokio::io::split(client_end);

                let agent = Rc::new(CrucibleAcpAgent::new(CliConfig::default()));
                let (a_conn, a_io) = AgentSideConnection::new(
                    agent.clone(),
                    a_write.compat_write(),
                    a_read.compat(),
                    |fut| {
                        tokio::task::spawn_local(fut);
                    },
                );
                agent.set_connection(Rc::new(a_conn));
                tokio::task::spawn_local(async move {
                    let _ = a_io.await;
                });

                let (client, c_io) = ClientSideConnection::new(
                    NoopClient,
                    c_write.compat_write(),
                    c_read.compat(),
                    |fut| {
                        tokio::task::spawn_local(fut);
                    },
                );
                tokio::task::spawn_local(async move {
                    let _ = c_io.await;
                });

                let resp = client
                    .initialize(InitializeRequest::new(ProtocolVersion::V1))
                    .await
                    .expect("initialize should succeed");
                assert_eq!(resp.protocol_version, ProtocolVersion::V1);
                assert!(
                    resp.agent_capabilities.load_session,
                    "agent should advertise load_session"
                );
                assert!(
                    resp.agent_capabilities.session_capabilities.close.is_some(),
                    "agent should advertise session/close support"
                );
            })
            .await;
    }
}
