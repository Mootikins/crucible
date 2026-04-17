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
use tracing::info;

use crate::config::CliConfig;
use crate::kiln_discover::{discover_kiln, DiscoverySource};
use crate::provider_detect::detect_providers;

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

    if let Some(found) = discover_kiln(None, None) {
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

    let expanded = crate::kiln_validate::expand_tilde(path_input.trim());

    if !expanded.exists() {
        std::fs::create_dir_all(&expanded)?;
    }

    let crucible_dir = expanded.join(".crucible");
    if !crucible_dir.join("config.toml").exists() {
        // We don't have provider detection at this point (that's now daemon-side
        // and fires after session.create). Use conservative defaults for the
        // generated config; the user can edit it later via `cru init` or
        // `cru config`.
        let config_content =
            crate::commands::init::generate_config_with_provider("ollama", "llama3.2");
        crate::commands::init::create_kiln_with_config(&crucible_dir, &config_content, false)?;
        println!("{} Kiln initialized at {}", "✓".green(), expanded.display());
    }

    config.kiln_path = expanded;
    Ok(())
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
        info!("Auto-detected Ollama: {}", ollama.reason.as_str());
        if let Some(ref model) = ollama.default_model {
            config.chat.model = Some(model.clone());
            info!("Set default model to {}", model);
        }
    }
}
