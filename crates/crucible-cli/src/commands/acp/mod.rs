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
//! Dogfood (Crucible hosting Crucible) — add to `~/.config/crucible/init.lua`:
//! ```lua
//! cru.config.set({
//!     acp = {
//!         agents = {
//!             crucible = { command = "cru", args = { "acp" } },
//!         },
//!     },
//! })
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
use crate::kiln_attach::{AttachedKiln, CliKilnRegistry, KilnTarget};
use crate::kiln_discover::discover_kiln;

pub use agent::CrucibleAcpAgent;

/// Run the ACP agent server until the host closes stdin.
///
/// `kiln_override` comes from `cru acp --kiln <name-or-path>`; otherwise the
/// kiln is taken from config or discovered by walking up from the current
/// directory. This is headless: we never prompt (an editor host has no TTY).
pub async fn execute(mut config: CliConfig, kiln_override: Option<String>) -> Result<()> {
    resolve_kiln(&mut config, kiln_override).await?;
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
async fn resolve_kiln(config: &mut CliConfig, kiln_override: Option<String>) -> Result<()> {
    if let Some(value) = kiln_override {
        let attached = attach_kiln(config, &value).await?;
        attached.apply_to(config);
        return Ok(());
    }

    // A configured kiln already has a name by definition — it came out of the
    // registry — so this branch needs no registration.
    if config.kiln_path.join(".crucible").is_dir() {
        return Ok(());
    }
    if let Some(found) = discover_kiln(None, None) {
        let attached = attach_kiln(config, &found.path.to_string_lossy()).await?;
        attached.apply_to(config);
        return Ok(());
    }
    anyhow::bail!(
        "no valid kiln found for `cru acp`; pass --kiln <name|path> or run from inside a kiln \
         (a directory containing .crucible/). Initialize one with `cru init`."
    )
}

/// Resolve a `--kiln` value, registering the directory it names if it needs one.
///
/// The resolver decides; this does the registering, because a registration is
/// the daemon's to make and this is the layer holding the connection. The NAME
/// comes back from the daemon rather than being derived here: it depends on
/// what is already registered — `notes`, then `notes-2` — so a caller deriving
/// its own would derive against a different set.
async fn attach_kiln(config: &CliConfig, value: &str) -> Result<AttachedKiln> {
    let registry = CliKilnRegistry::for_cli(config)?;
    let directory = match registry.resolve(value)? {
        KilnTarget::Registered(attached) => return Ok(attached),
        KilnTarget::Directory(path) => path,
    };

    let client = crate::common::daemon_client().await?;
    let reply = client
        .kiln_register_derived(
            &directory, /* auto */ true, /* make_default */ false,
        )
        .await
        .with_context(|| format!("registering kiln at {}", directory.display()))?;

    let name = reply["name"]
        .as_str()
        .and_then(|n| crucible_core::config::KilnName::parse(n).ok())
        .ok_or_else(|| anyhow::anyhow!("the daemon returned no usable kiln name: {reply}"))?;
    let path = reply["path"]
        .as_str()
        .map(PathBuf::from)
        .unwrap_or(directory);
    let registered = reply["outcome"].as_str() == Some("added");
    if registered {
        info!(kiln = %name, path = %path.display(), "registered a new kiln");
    }
    Ok(AttachedKiln {
        name,
        path,
        registered,
    })
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
