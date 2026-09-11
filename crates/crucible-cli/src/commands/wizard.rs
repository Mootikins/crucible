//! First-run setup wizard
//!
//! Detects whether any config exists and, if not, walks the user through
//! choosing an LLM provider, storing an API key, and setting a default kiln
//! path. The result is a minimal `~/.config/crucible/init.lua` (verified by
//! a throwaway evaluation before it is written), the provider selection in
//! `<data_home>/llm.json`, and a `secrets.toml` entry. The wizard writes NO
//! `config.toml`.

use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use crucible_core::config::credentials::SecretsFile;
use crucible_core::config::BackendType;

/// Returns `true` when no config file exists beside `config_path`: no
/// `init.lua` and no `init.luau`. Either name means the user has a config and
/// the wizard stays out of the way.
///
/// `config_path` names the config ROOT, through the file that used to sit in
/// it. A `config.toml` there is deliberately NOT a config: no reader loads it
/// any more, so treating it as one suppressed the wizard for every upgrading
/// user while configuring nothing.
pub fn is_first_run(config_path: &Path) -> bool {
    // Either name counts as "already configured". Looking for `init.lua` alone
    // ran the first-run wizard again for anyone whose config is `init.luau`,
    // and the wizard would then write a second config beside the first.
    let dir = config_path.parent().unwrap_or(std::path::Path::new("."));
    crucible_lua::source_files::init_file(dir)
        .ok()
        .flatten()
        .is_none()
}

/// Interactive first-run wizard. Writes `init.lua`, routes the provider
/// selection to `llm.json`, and stores the API key in `secrets.toml`.
/// Returns `Ok(())` on success or if the user cancels (Ctrl-C) --
/// cancellation is not an error.
pub fn run_setup_wizard(config_path: &Path) -> Result<()> {
    println!();
    println!("  {}", "Welcome to Crucible".bold());
    println!();

    // --- LLM provider ---

    let providers = &["Anthropic", "Ollama", "OpenAI", "OpenRouter"];
    let provider_idx = match dialoguer::Select::new()
        .with_prompt("  LLM Provider")
        .items(providers)
        .default(0)
        .interact_opt()?
    {
        Some(idx) => idx,
        None => {
            println!("  {}", "Setup cancelled.".dimmed());
            return Ok(());
        }
    };

    let provider_id = match provider_idx {
        0 => "anthropic",
        1 => "ollama",
        2 => "openai",
        3 => "openrouter",
        _ => unreachable!(),
    };

    // --- API key (skip for Ollama) ---

    let needs_key = provider_id != "ollama";
    if needs_key {
        let key: String = dialoguer::Password::new()
            .with_prompt(format!("  API key for {}", providers[provider_idx]))
            .allow_empty_password(true)
            .interact()?;

        if key.is_empty() {
            println!(
                "  {}",
                "No key provided -- you can add one later with `cru auth login`.".dimmed()
            );
        } else {
            let mut store = SecretsFile::new();
            store.set(provider_id, &key)?;
            println!(
                "  {} API key stored in {}",
                "✓".green(),
                store.path().display().to_string().dimmed()
            );
        }
    }

    // --- Default kiln path ---

    let kiln_path: String = dialoguer::Input::<String>::new()
        .with_prompt("  Default kiln path")
        .default("~/vault".to_string())
        .interact_text()?;

    // --- Write config ---

    let init_path = apply_wizard_answers(
        config_path,
        &crucible_core::config::crucible_home(),
        provider_id,
        &kiln_path,
    )?;

    println!();
    println!(
        "  {} Config written to {}",
        "✓".green(),
        init_path.display().to_string().dimmed()
    );
    if needs_key {
        println!("  {} API key stored securely", "✓".green());
    }
    println!(
        "  {} Run {} in a project or kiln directory",
        "✓".green(),
        "`cru init`".bold()
    );
    println!();

    Ok(())
}

/// The wizard's non-interactive half: route the provider selection to
/// `<data_home>/llm.json`, generate the `init.lua`, VERIFY it with the same
/// throwaway evaluation the daemon's boot uses, and write it beside
/// `config_path`. Returns the written path. No `config.toml` is written.
pub fn apply_wizard_answers(
    config_path: &Path,
    data_home: &Path,
    provider_id: &str,
    kiln_path: &str,
) -> Result<std::path::PathBuf> {
    // The provider selection is machine state, not user authorship: it goes
    // to llm.json, where the daemon overlays it under the config layer. The
    // Lua config carries what the user actually typed.
    if let Ok(backend) = provider_id.parse::<BackendType>() {
        let model = backend.default_chat_model().unwrap_or("default");
        let llm_state = crucible_daemon::llm_state::LlmStateStore::new(data_home);
        llm_state.register_provider(provider_id, backend, model, true)?;
    }

    let init_lua_source = generate_initial_init_lua(provider_id, kiln_path);

    // Verified BEFORE it is written: a wizard that wrote a config the very
    // next command cannot evaluate would be worse than no wizard.
    crucible_lua::evaluate_config_source(&init_lua_source)?;

    let init_path = config_path
        .parent()
        .map(|dir| dir.join("init.lua"))
        .unwrap_or_else(|| std::path::PathBuf::from("init.lua"));
    if let Some(parent) = init_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&init_path, &init_lua_source)?;
    Ok(init_path)
}

/// Produce a minimal `init.lua` from wizard answers: the values as a
/// literal table through the same emitter `cru config migrate` uses.
///
/// The provider selection is NOT here — it lives in `llm.json`. The chat
/// model rides in the config because the `BackendType` table is the one
/// place a provider's default model lives, and an unknown provider gets a
/// placeholder the user must edit.
pub fn generate_initial_init_lua(provider: &str, default_kiln_path: &str) -> String {
    let model = provider
        .parse::<BackendType>()
        .ok()
        .and_then(|backend| backend.default_chat_model())
        .unwrap_or("default");

    let value = serde_json::json!({
        "default_kiln": "default",
        "kilns": { "default": default_kiln_path },
        "chat": { "model": model },
    });

    format!(
        "-- Crucible configuration (generated by the first-run wizard).\n\
         -- `cru config show` renders the effective result.\n\
         {}",
        crucible_core::config::emit_lua_config(&value)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_first_run_when_no_config_at_all() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        assert!(is_first_run(&config_path));
    }

    /// A leftover `config.toml` is NOT a config: nothing reads it.
    ///
    /// This is the upgrade path. A user of v0.30.0 has that file, and
    /// treating it as "already configured" suppressed the wizard while
    /// configuring nothing — the user then starts on the defaults and meets
    /// "No LLM providers are configured", with the only explanation in a
    /// daemon log a background daemon never shows.
    #[test]
    fn a_leftover_config_toml_does_not_suppress_the_wizard() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, "kiln_path = \"/tmp\"").unwrap();
        assert!(
            is_first_run(&config_path),
            "an unread file must not stand in for a config"
        );
    }

    /// An `init.lua` IS a config: the wizard must not fire over one, or a
    /// Lua-only setup would be walked through setup it already did.
    #[test]
    fn not_first_run_when_init_lua_exists() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(tmp.path().join("init.lua"), "-- config\n").unwrap();
        assert!(!is_first_run(&config_path));
    }

    /// The generated file must survive the same evaluation the daemon's
    /// boot runs — the wizard's own pre-write verification.
    #[test]
    fn generated_init_lua_evaluates_and_carries_the_answers() {
        let lua = generate_initial_init_lua("anthropic", "~/notes");
        let config = crucible_lua::evaluate_config_source(&lua).expect("the file must evaluate");
        assert_eq!(config.default_kiln.as_deref(), Some("default"));
        assert_eq!(
            config.kilns["default"].path(),
            std::path::PathBuf::from("~/notes")
        );
        assert_eq!(
            config.chat.model.as_deref(),
            BackendType::Anthropic.default_chat_model()
        );
    }

    /// The provider is llm.json's business: the Lua carries no [llm] table.
    #[test]
    fn generated_init_lua_names_no_provider() {
        let lua = generate_initial_init_lua("anthropic", "~/notes");
        assert!(
            !lua.contains("llm"),
            "the provider belongs in llm.json: {lua}"
        );
        let config = crucible_lua::evaluate_config_source(&lua).unwrap();
        assert!(config.llm.providers.is_empty());
        assert!(config.llm.default.is_none());
    }

    /// The chat preflight prompt keys off a resolvable kiln; the wizard's
    /// answers must resolve, so the prompt cannot fire right after setup.
    #[test]
    fn the_wizard_config_resolves_a_kiln_so_the_preflight_does_not_prompt() {
        let lua = generate_initial_init_lua("ollama", "~/vault");
        let config = crucible_lua::evaluate_config_source(&lua).unwrap();
        assert_eq!(
            config.resolved_kiln_path(),
            Some(std::path::PathBuf::from("~/vault")),
            "an unresolvable kiln would re-prompt the user the wizard just answered"
        );
    }

    /// The wizard's write path end to end: init.lua lands (and evaluates),
    /// NO config.toml is written, and the provider selection lands in
    /// llm.json as the default.
    #[test]
    fn apply_wizard_answers_writes_lua_and_routes_the_provider_to_llm_json() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_dir = tmp.path().join("config");
        let data_home = tmp.path().join("data");
        let config_path = config_dir.join("config.toml");

        let written =
            apply_wizard_answers(&config_path, &data_home, "anthropic", "~/vault").unwrap();

        assert_eq!(written, config_dir.join("init.lua"));
        assert!(!config_path.exists(), "the wizard writes NO config.toml");
        let lua = std::fs::read_to_string(&written).unwrap();
        crucible_lua::evaluate_config_source(&lua).expect("the boot evaluation accepts it");

        let llm: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(data_home.join("llm.json")).expect("llm.json written"),
        )
        .unwrap();
        assert_eq!(llm["default"], "anthropic");
        assert_eq!(llm["providers"]["anthropic"]["type"], "anthropic");
    }

    #[test]
    fn generated_models_follow_the_backend_table() {
        for (provider, backend) in [
            ("ollama", BackendType::Ollama),
            ("openai", BackendType::OpenAI),
            ("openrouter", BackendType::OpenRouter),
        ] {
            let lua = generate_initial_init_lua(provider, "~/vault");
            let config = crucible_lua::evaluate_config_source(&lua).unwrap();
            assert_eq!(
                config.chat.model.as_deref(),
                backend.default_chat_model(),
                "{provider}"
            );
        }
    }
}
