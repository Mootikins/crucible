//! The four app-config RPCs `/api/config` forwards to.
//!
//! Split from `daemon.rs` for the same reason as `daemon_plugins` and
//! `daemon_review`: that file sits against its size gate, and these three
//! belong to one surface.
//!
//! All four go through [`ReconnectingDaemon::call_with_reconnect`], which may
//! replay a call the daemon already ran. That is safe here: three reads, and a
//! save that writes the values it was handed, so a replay lands the same
//! store and the same `settings.json` as the first attempt. (The `review.*`
//! writes next door take `call_once` for exactly the opposite reason.)
//!
//! The method names are literals rather than typed client methods because the
//! CLI reaches `config.effective` the same way — one raw call each, with no
//! request type to keep in step.
//!
//! **Every read redacts credentials here, at the crate boundary, rather than
//! in the route.** The effective config carries `web.api_key` and every
//! `llm.providers.*.api_key`, and an origin row carries the same values under
//! its `value` field; none of them redacts on `Serialize`. A route that
//! forgot the pass would ship them, so the pass sits where the values ENTER
//! the web process and no route can be the one that forgot. The save is the
//! one call that does not redact, because its answer carries no value: `ok`,
//! the refused leaves as `{key, source, file, line}`, and the rejected key
//! names. A pass there could not fail, and a gate that cannot fail is worse
//! than none.
//!
//! **The daemon's own RPC does not redact, deliberately.** Its socket is
//! per-uid and 0700, so a caller already runs as the operator and can read
//! `init.lua` directly — redaction there removes no reach — and the CLI is a
//! real consumer of the resolved values:
//! `crucible-cli/src/factories/embedding.rs` builds an OpenAI embedding
//! client from the `api_key` that `config.effective` returned. The browser is
//! the boundary that crosses to another principal (a LAN client, or any page
//! on loopback, which `bearer_auth` waves through), so the redaction belongs
//! on the crossing, not on the store.

use super::daemon::ReconnectingDaemon;
use crucible_core::config::redact_credentials;

impl ReconnectingDaemon {
    /// The daemon's effective config, plus `config_root`, `boot_hash` and
    /// `kiln_path_is_default`.
    pub async fn config_effective(&self) -> anyhow::Result<serde_json::Value> {
        let mut answer = self
            .call_with_reconnect("config.effective", |daemon| {
                Box::pin(daemon.call("config.effective", serde_json::json!({})))
            })
            .await?;
        redact_credentials(&mut answer);
        Ok(answer)
    }

    /// Every recorded leaf and where it came from:
    /// `{origins: [{key, value, source, file?, line?}]}`.
    pub async fn config_origins(&self) -> anyhow::Result<serde_json::Value> {
        let mut answer = self
            .call_with_reconnect("config.origin", |daemon| {
                Box::pin(daemon.call("config.origin", serde_json::json!({})))
            })
            .await?;
        redact_credentials(&mut answer);
        Ok(answer)
    }

    /// The app config's declared control tree, and the leaves that take no
    /// control: `{options, read_only}`.
    pub async fn config_controls(&self) -> anyhow::Result<serde_json::Value> {
        let mut answer = self
            .call_with_reconnect("config.controls", |daemon| {
                Box::pin(daemon.call("config.controls", serde_json::json!({})))
            })
            .await?;
        redact_credentials(&mut answer);
        Ok(answer)
    }

    /// Save values as the user's durable preference. The answer carries
    /// `refused` — the leaves a higher layer holds, each with the file and
    /// line that holds it — so the caller learns what did not save and why.
    pub async fn config_save(
        &self,
        values: serde_json::Map<String, serde_json::Value>,
    ) -> anyhow::Result<serde_json::Value> {
        self.call_with_reconnect("config.save", move |daemon| {
            let values = values.clone();
            Box::pin(async move {
                daemon
                    .call("config.save", serde_json::json!({ "values": values }))
                    .await
            })
        })
        .await
    }
}
