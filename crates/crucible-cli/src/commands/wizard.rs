//! First-run setup wizard
//!
//! Detects whether a config file exists and, if not, walks the user through
//! choosing an LLM provider, storing an API key, selecting an embedding
//! backend, and setting a default kiln path. The result is a minimal
//! `~/.config/crucible/config.toml` plus a `secrets.toml` entry.

use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use crucible_config::credentials::{CredentialStore, SecretsFile};

/// Returns `true` when the global config file does not yet exist.
pub fn is_first_run(config_path: &Path) -> bool {
    !config_path.exists()
}

/// Interactive first-run wizard. Writes `config.toml` and stores the API key
/// in `secrets.toml`. Returns `Ok(())` on success or if the user cancels
/// (Ctrl-C) -- cancellation is not an error.
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

    // --- Embedding backend ---

    let embedding_items = &[
        "FastEmbed (local, no key needed)",
        "Ollama",
        "OpenAI",
    ];
    let embedding_idx = match dialoguer::Select::new()
        .with_prompt("  Embeddings")
        .items(embedding_items)
        .default(0)
        .interact_opt()?
    {
        Some(idx) => idx,
        None => {
            println!("  {}", "Setup cancelled.".dimmed());
            return Ok(());
        }
    };

    let embedding_provider = match embedding_idx {
        0 => "fastembed",
        1 => "ollama",
        2 => "openai",
        _ => unreachable!(),
    };

    // --- Default kiln path ---

    let kiln_path: String = dialoguer::Input::<String>::new()
        .with_prompt("  Default kiln path")
        .default("~/vault".to_string())
        .interact_text()?;

    // --- Write config ---

    let config_toml = generate_initial_config(provider_id, embedding_provider, &kiln_path);

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(config_path, &config_toml)?;

    println!();
    println!(
        "  {} Config written to {}",
        "✓".green(),
        config_path.display().to_string().dimmed()
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

/// Produce a minimal but valid `config.toml` from wizard answers.
pub fn generate_initial_config(
    provider: &str,
    embedding_provider: &str,
    default_kiln_path: &str,
) -> String {
    let model = match provider {
        "anthropic" => "claude-sonnet-4-20250514",
        "openai" => "gpt-4o",
        "openrouter" => "anthropic/claude-sonnet-4-20250514",
        "ollama" => "llama3.2",
        _ => "default",
    };

    let mut out = String::new();

    out.push_str(&format!(
        "[kilns]\ndefault = \"{}\"\n\n",
        default_kiln_path
    ));
    out.push_str(&format!("[chat]\nmodel = \"{}\"\n\n", model));
    out.push_str(&format!("[llm]\ndefault = \"{}\"\n\n", provider));
    out.push_str(&format!(
        "[enrichment.provider]\ntype = \"{}\"\n",
        embedding_provider
    ));

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_first_run_when_no_config() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        assert!(is_first_run(&config_path));
    }

    #[test]
    fn not_first_run_when_config_exists() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, "kiln_path = \"/tmp\"").unwrap();
        assert!(!is_first_run(&config_path));
    }

    #[test]
    fn generate_initial_config_produces_valid_toml() {
        let config = generate_initial_config("anthropic", "fastembed", "~/vault");
        let parsed: toml::Value = toml::from_str(&config).unwrap();
        assert!(parsed.get("kilns").is_some());
        assert!(parsed.get("chat").is_some());
        assert!(parsed.get("llm").is_some());
        assert!(parsed.get("enrichment").is_some());
    }

    #[test]
    fn generate_config_anthropic_defaults() {
        let config = generate_initial_config("anthropic", "fastembed", "~/notes");
        let parsed: toml::Value = toml::from_str(&config).unwrap();
        assert_eq!(parsed["kilns"]["default"].as_str().unwrap(), "~/notes");
        assert_eq!(
            parsed["chat"]["model"].as_str().unwrap(),
            "claude-sonnet-4-20250514"
        );
        assert_eq!(parsed["llm"]["default"].as_str().unwrap(), "anthropic");
        assert_eq!(
            parsed["enrichment"]["provider"]["type"].as_str().unwrap(),
            "fastembed"
        );
    }

    #[test]
    fn generate_config_ollama_defaults() {
        let config = generate_initial_config("ollama", "ollama", "~/vault");
        let parsed: toml::Value = toml::from_str(&config).unwrap();
        assert_eq!(parsed["chat"]["model"].as_str().unwrap(), "llama3.2");
        assert_eq!(parsed["llm"]["default"].as_str().unwrap(), "ollama");
        assert_eq!(
            parsed["enrichment"]["provider"]["type"].as_str().unwrap(),
            "ollama"
        );
    }

    #[test]
    fn generate_config_openai_model() {
        let config = generate_initial_config("openai", "openai", "~/vault");
        let parsed: toml::Value = toml::from_str(&config).unwrap();
        assert_eq!(parsed["chat"]["model"].as_str().unwrap(), "gpt-4o");
    }

    #[test]
    fn generate_config_openrouter_model() {
        let config = generate_initial_config("openrouter", "fastembed", "~/vault");
        let parsed: toml::Value = toml::from_str(&config).unwrap();
        assert_eq!(
            parsed["chat"]["model"].as_str().unwrap(),
            "anthropic/claude-sonnet-4-20250514"
        );
        assert_eq!(parsed["llm"]["default"].as_str().unwrap(), "openrouter");
    }
}
