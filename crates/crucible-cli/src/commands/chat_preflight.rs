//! CLI-side preflight narrowed to kiln validation.
//!
//! All other setup (provider detection, workspace/kiln indexing, plugin
//! discovery, MCP config read, context-length fetch) now runs inside the
//! daemon's `session.create` setup task and arrives at the CLI as session
//! events. This module keeps only what is genuinely CLI-local: prompting
//! the user interactively for a kiln path on first run — something a
//! headless daemon cannot do.

use anyhow::Result;
use colored::Colorize;
use std::io::IsTerminal;
use tracing::{info, warn};

use crate::config::CliConfig;
use crate::kiln_discover::{discover_kiln, DiscoverySource};
use crate::provider_detect::detect_providers;

/// The zero-provider remedies, shared by chat preflight, the TUI fallback
/// warning, and `cru doctor`, so they cannot drift apart. Doctor prints this
/// form; everything else appends the pointer to doctor itself.
pub fn no_providers_remedies() -> &'static str {
    "No LLM providers are configured. \
     Try: `cru auth login` (cloud API key) or `ollama serve` (local models)."
}

/// [`no_providers_remedies`] plus the diagnostics pointer — the message for
/// every surface except `cru doctor` (which would be telling the user to run
/// the command they are already in).
pub fn no_providers_message() -> String {
    format!(
        "{} Run `cru doctor` to verify your setup.",
        no_providers_remedies()
    )
}

/// Block chat startup when the daemon can resolve zero providers.
///
/// Without this the user gets a normal prompt, types a message, and receives
/// a raw transport error mid-conversation — the worst possible first-run
/// failure. Only meaningful for the internal agent; external (`-a`) agents
/// bring their own provider.
pub async fn ensure_providers_available(
    client: &crucible_daemon::DaemonClient,
    kiln: &std::path::Path,
) -> Result<()> {
    // Best-effort: a listing hiccup must not block a chat that might work —
    // if something is genuinely wrong, the turn surfaces it with the
    // (now much better) daemon-side error.
    let providers = match client.list_providers_summary(Some(kiln)).await {
        Ok(providers) => providers,
        Err(e) => {
            warn!("providers.list failed during preflight; continuing: {e}");
            return Ok(());
        }
    };
    if providers.is_empty() {
        anyhow::bail!("{}", no_providers_message());
    }
    Ok(())
}

/// Ensure the CLI has a valid kiln to hand to the daemon.
///
/// Filesystem check: `<kiln>/.crucible/` must be a directory. If missing,
/// try auto-discovery (git root ascent). If that fails AND stdin is a TTY,
/// prompt the user. If stdin is not a TTY, bail with a clear error — we
/// cannot prompt in a headless context.
pub async fn ensure_valid_kiln(config: &mut CliConfig) -> Result<()> {
    let config_kiln_valid = config.kiln_path.join(".crucible").is_dir();
    if config_kiln_valid {
        info!("Using kiln from config: {}", config.kiln_path.display());
        return Ok(());
    }

    // Hand discovery the configured kiln, or the `[kilns]` entry the setup
    // wizard writes is never consulted: passing None here skipped the
    // global-config branch entirely, so the wizard's answer was invisible and
    // the user got prompted a second time with a different default.
    let configured = config.resolved_kiln_path();
    if let Some(found) = discover_kiln(None, configured.as_deref()) {
        info!(
            "Discovered kiln at {} (via {:?})",
            found.path.display(),
            found.source
        );
        if found.source != DiscoverySource::CliFlag {
            config.kiln_path = found.path;
        }
        return Ok(());
    }

    // Before prompting, ask the daemon. It holds `<data_home>/kilns.json`,
    // which this process's config does not see — so a kiln the user named at
    // this very prompt on a previous run lives somewhere `resolved_kiln_path`
    // cannot reach. Skipping this is how "No kiln found" greets a user forever
    // no matter how many times they answer it.
    if let Some(path) = registered_default_kiln().await {
        info!(
            "Using the kiln registered with the daemon: {}",
            path.display()
        );
        config.kiln_path = path;
        return Ok(());
    }

    if !std::io::stdin().is_terminal() {
        anyhow::bail!("no valid kiln configured; run `cru init` first");
    }

    info!("No kiln found, prompting for path");
    println!(
        "{} No kiln found. A kiln is a folder where Crucible stores your notes and sessions.",
        "Setup:".cyan().bold()
    );
    println!(
        "  {} A kiln is like a vault — it holds all your markdown notes, embeddings, and chat history.",
        "What is a kiln?".dimmed()
    );
    println!(
        "  {} A good default is a folder in your home directory or Documents (e.g., ~/crucible).",
        "Tip:".dimmed()
    );

    let path_input: String = dialoguer::Input::new()
        .with_prompt("Kiln path")
        .default("~/crucible".to_string())
        .interact_text()?;

    let expanded = crate::kiln_validate::expand_tilde_home(path_input.trim());

    if !expanded.exists() {
        std::fs::create_dir_all(&expanded)?;
    }

    if ensure_kiln_scaffold(&expanded)? {
        println!("{} Kiln initialized at {}", "✓".green(), expanded.display());
    }

    // Persist, or this prompt fires again on every run from outside a kiln.
    // Through the daemon, which owns the registry: the CLI used to edit the
    // user's config file here, which put storage in a layer that must not have
    // any and buried a machine-written entry in a hand-edited file.
    match crate::common::daemon_client().await {
        Ok(client) => {
            match client
                .kiln_register(
                    "default", &expanded, /* auto */ true, /* make_default */ true,
                )
                .await
            {
                Ok(reply) => println!(
                    "{} Saved kiln path to {}",
                    "✓".green(),
                    reply["state_file"].as_str().unwrap_or_default()
                ),
                // Non-fatal: the session can still proceed with the in-memory
                // value. Say so rather than failing the chat the user asked for.
                Err(e) => {
                    warn!("could not register the kiln: {e}");
                    println!(
                        "{} Could not save the kiln path — you may be asked again next time.",
                        "Note:".yellow()
                    );
                }
            }
        }
        Err(e) => {
            warn!("could not reach the daemon to register the kiln: {e}");
            println!(
                "{} Could not save the kiln path — you may be asked again next time.",
                "Note:".yellow()
            );
        }
    }

    config.kiln_path = expanded;
    Ok(())
}

/// The path of the kiln the daemon would pick by default, if it knows one.
///
/// Asks over RPC rather than reading a file, because the answer spans two
/// layers the daemon merges and this process sees only one of them. A daemon
/// that cannot be reached is not an error here — it means the same as "no kiln
/// registered", and the caller's next step is the prompt either way.
///
/// A `discovered` entry is skipped: that is a directory something opened by
/// path, not a kiln any session can name.
async fn registered_default_kiln() -> Option<std::path::PathBuf> {
    let client = crate::common::daemon_client().await.ok()?;
    let reply = client.kiln_registry_list().await.ok()?;
    let rows = reply["kilns"].as_array()?;

    let usable = |row: &&serde_json::Value| {
        row["origin"] != "discovered" && row["missing"] != true && row["path"].is_string()
    };
    let chosen = rows
        .iter()
        .find(|row| row["default"] == true && usable(row))
        .or_else(|| rows.iter().find(usable))?;
    Some(std::path::PathBuf::from(chosen["path"].as_str()?))
}

/// Backfill `config.chat.model` from the detected Ollama provider's default
/// model when the config has none set. First-run Ollama users would otherwise
/// land on `DEFAULT_CHAT_MODEL`, which may not match what they actually have
/// installed locally.
///
/// This mirrors a side-effect that `run_preflight_checks` did before setup
/// moved daemon-side. Detection is purely local (env + config + credentials),
/// no HTTP probing.
pub fn fill_default_model_if_missing(config: &mut CliConfig) {
    if config.chat.model.is_some() {
        return;
    }

    let providers = detect_providers(&config.chat);
    if let Some(ollama) = providers.iter().find(|p| p.provider_type == "ollama") {
        info!("Auto-detected Ollama: {}", ollama.reason());
        if let Some(ref model) = ollama.default_model {
            config.chat.model = Some(model.clone());
            info!("Set default model to {}", model);
        }
    }
}

/// Generate the kiln-local `.crucible/init.lua` scaffold, once.
///
/// Returns whether it generated. Nothing is written when the kiln already
/// has an `init.lua`, or a pre-Lua `config.toml` — the not-yet-migrated
/// form; generating a second config file beside it would leave the kiln
/// with two, and the templates carry no provider detection at this point
/// (that is daemon-side, after session.create).
pub(crate) fn ensure_kiln_scaffold(kiln_root: &std::path::Path) -> Result<bool> {
    let crucible_dir = kiln_root.join(".crucible");
    if crucible_dir.join("init.lua").exists() || crucible_dir.join("config.toml").exists() {
        return Ok(false);
    }
    let init_lua = crate::commands::init::generate_kiln_init_lua("ollama", "llama3.2");
    crate::commands::init::create_kiln_with_init_lua(&crucible_dir, &init_lua, false)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The message is the product's single highest-value error string; each
    /// remedy is load-bearing (§1.2 of the launch plan).
    #[test]
    fn the_zero_provider_message_names_every_remedy() {
        let msg = no_providers_message();
        assert!(msg.contains("cru auth login"), "must offer the cloud path");
        assert!(msg.contains("ollama serve"), "must offer the local path");
        assert!(msg.contains("cru doctor"), "must point at diagnostics");
    }

    #[test]
    fn the_scaffold_generates_init_lua_once() {
        let tmp = tempfile::TempDir::new().unwrap();

        assert!(ensure_kiln_scaffold(tmp.path()).unwrap());
        let init_lua = tmp.path().join(".crucible/init.lua");
        assert!(init_lua.exists());
        assert!(!tmp.path().join(".crucible/config.toml").exists());

        // Second run: the file is there, nothing regenerates.
        let before = std::fs::read_to_string(&init_lua).unwrap();
        std::fs::write(&init_lua, format!("{before}-- user edit\n")).unwrap();
        assert!(!ensure_kiln_scaffold(tmp.path()).unwrap());
        assert!(
            std::fs::read_to_string(&init_lua).unwrap().contains("user edit"),
            "an existing init.lua must not be overwritten"
        );
    }

    /// A kiln that still has the pre-Lua `config.toml` is configured, not
    /// fresh: generating an `init.lua` beside it would leave two config
    /// files, one of them unexplained.
    #[test]
    fn the_scaffold_leaves_an_unmigrated_kiln_alone() {
        let tmp = tempfile::TempDir::new().unwrap();
        let crucible_dir = tmp.path().join(".crucible");
        std::fs::create_dir_all(&crucible_dir).unwrap();
        std::fs::write(crucible_dir.join("config.toml"), "[chat]\n").unwrap();

        assert!(!ensure_kiln_scaffold(tmp.path()).unwrap());
        assert!(!crucible_dir.join("init.lua").exists());
    }
}
