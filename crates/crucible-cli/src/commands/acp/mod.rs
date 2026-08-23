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
use std::sync::Arc;

use agent_client_protocol::Stdio;
use anyhow::{Context, Result};
use tracing::info;

use crate::config::CliConfig;
use crate::kiln_attach::CliKilnRegistry;
use crate::kiln_discover::discover_kiln;

pub use agent::CrucibleAcpAgent;

/// Run the ACP agent server until the host closes stdin.
///
/// `kiln_override` comes from `cru acp --kiln <name-or-path>`; otherwise the
/// kiln is taken from config or discovered by walking up from the current
/// directory. This is headless: we never prompt (an editor host has no TTY).
pub async fn execute(
    mut config: CliConfig,
    kiln_override: Option<String>,
    config_path: Option<PathBuf>,
) -> Result<()> {
    resolve_kiln(&mut config, kiln_override, config_path)?;
    info!(kiln = %config.kiln_path.display(), "starting ACP agent (cru acp)");

    // ACP framing is line-delimited JSON on stdio. The SDK transport reads
    // stdin on a blocking thread; the daemon RPC tasks run on the tokio runtime.
    Arc::new(CrucibleAcpAgent::new(config))
        .serve(Stdio::new())
        .await
        .context("ACP stdio connection terminated")
}

/// Decide which kiln this ACP session attaches, and make sure it has a *name*.
///
/// The flag is authoritative when given. It used to fall through to
/// ancestor-walk discovery whenever the named directory had no `.crucible/`,
/// which meant `--kiln ~/notes` could silently attach some entirely different
/// directory the walk happened to find — an explicit flag that gets ignored.
/// A value that cannot be resolved is now an error naming both readings.
///
/// Discovery, the no-flag path, still yields a bare directory, so it goes
/// through the same registration door: sessions address kilns by name, and a
/// discovered directory with no `[kilns]` entry would otherwise produce a
/// session with no kiln at all.
fn resolve_kiln(
    config: &mut CliConfig,
    kiln_override: Option<String>,
    config_path: Option<PathBuf>,
) -> Result<()> {
    let config_path =
        config_path.unwrap_or_else(crucible_core::config::CliAppConfig::default_config_path);

    if let Some(value) = kiln_override {
        let mut registry = CliKilnRegistry::for_cli(config, config_path)?;
        let attached = registry.attach(&value)?;
        if attached.registered {
            info!(kiln = %attached.name, path = %attached.path.display(), "registered a new kiln");
        }
        attached.apply_to(config);
        return Ok(());
    }

    // A configured kiln already has a name by definition — it came out of
    // `[kilns]` — so this branch needs no registration.
    if config.kiln_path.join(".crucible").is_dir() {
        return Ok(());
    }
    if let Some(found) = discover_kiln(None, None) {
        let mut registry = CliKilnRegistry::for_cli(config, config_path)?;
        let attached = registry.attach(&found.path.to_string_lossy())?;
        attached.apply_to(config);
        return Ok(());
    }
    anyhow::bail!(
        "no valid kiln found for `cru acp`; pass --kiln <name|path> or run from inside a kiln \
         (a directory containing .crucible/). Initialize one with `cru init`."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::InitializeRequest;
    use agent_client_protocol::schema::ProtocolVersion;
    use agent_client_protocol::{ByteStreams, Client};
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    // Drives the real ACP framing over an in-process duplex pipe: a host-side
    // client sends `initialize` to our `CrucibleAcpAgent` served through the
    // SDK builder. Initialize needs no daemon.
    #[tokio::test]
    async fn initialize_round_trip_over_stdio_framing() {
        let (agent_end, client_end) = tokio::io::duplex(16 * 1024);
        let (a_read, a_write) = tokio::io::split(agent_end);
        let (c_read, c_write) = tokio::io::split(client_end);

        let agent = Arc::new(CrucibleAcpAgent::new(CliConfig::default()));
        let serving =
            tokio::spawn(agent.serve(ByteStreams::new(a_write.compat_write(), a_read.compat())));

        let resp = Client
            .builder()
            .connect_with(
                ByteStreams::new(c_write.compat_write(), c_read.compat()),
                async |cx| {
                    cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                        .block_task()
                        .await
                },
            )
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

        serving.abort();
    }
}
