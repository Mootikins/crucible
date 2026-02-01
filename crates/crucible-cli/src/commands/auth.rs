//! Credential management commands
//!
//! Provides `cru auth login/logout/list` for managing LLM provider API keys.
//! Credentials are stored in `~/.config/crucible/secrets.toml` (0o600 permissions).

use anyhow::Result;
use colored::Colorize;
use crucible_config::credentials::{
    env_var_for_provider, CredentialSource, CredentialStore, SecretsFile,
};

use crate::cli::AuthCommands;

/// Execute auth subcommand
pub async fn execute(command: Option<AuthCommands>) -> Result<()> {
    let cmd = command.unwrap_or(AuthCommands::List);

    match cmd {
        AuthCommands::Login { provider, key } => login(provider, key).await,
        AuthCommands::Logout { provider } => logout(provider).await,
        AuthCommands::List => list().await,
    }
}

/// Store a credential for a provider
async fn login(provider: Option<String>, key: Option<String>) -> Result<()> {
    let provider = match provider {
        Some(p) => p,
        None => {
            let items = &["openai", "anthropic", "ollama (no key needed)", "other"];
            let selection = dialoguer::Select::new()
                .with_prompt("Select provider")
                .items(items)
                .default(0)
                .interact()?;

            match selection {
                0 => "openai".to_string(),
                1 => "anthropic".to_string(),
                2 => {
                    println!(
                        "{}",
                        "Ollama doesn't require an API key. No credential stored.".yellow()
                    );
                    return Ok(());
                }
                _ => dialoguer::Input::<String>::new()
                    .with_prompt("Provider ID")
                    .interact_text()?,
            }
        }
    };

    let key = match key {
        Some(k) => k,
        None => dialoguer::Password::new()
            .with_prompt(format!("API key for {}", provider))
            .interact()?,
    };

    if key.is_empty() {
        anyhow::bail!("API key cannot be empty");
    }

    let mut store = SecretsFile::new();
    store.set(&provider, &key)?;

    println!("{} Credential stored for {}", "✓".green(), provider.bold());
    println!(
        "  {}",
        format!("Stored in {}", store.path().display()).dimmed()
    );

    Ok(())
}

/// Remove a credential for a provider
async fn logout(provider: Option<String>) -> Result<()> {
    let mut store = SecretsFile::new();

    let provider = match provider {
        Some(p) => p,
        None => {
            let stored = store.list()?;
            if stored.is_empty() {
                println!("{}", "No stored credentials to remove.".yellow());
                return Ok(());
            }

            let items: Vec<&String> = stored.keys().collect();
            let selection = dialoguer::Select::new()
                .with_prompt("Select provider to remove")
                .items(&items)
                .interact()?;

            items[selection].clone()
        }
    };

    let removed = store.remove(&provider)?;
    if removed {
        println!("{} Credential removed for {}", "✓".green(), provider.bold());
    } else {
        println!(
            "{} No credential found for {}",
            "!".yellow(),
            provider.bold()
        );
    }

    Ok(())
}

/// List all configured credentials and their sources
async fn list() -> Result<()> {
    let store = SecretsFile::new();
    let stored = store.list()?;

    // Known providers to check
    let known_providers = ["openai", "anthropic"];

    let mut found_any = false;

    // Check stored credentials
    for (provider, key) in &stored {
        found_any = true;
        let masked = mask_key(key);
        println!(
            "  {} {} ({})",
            provider.bold(),
            masked.dimmed(),
            CredentialSource::Store.to_string().cyan()
        );
    }

    // Check environment variables (for providers not already shown from store)
    for provider in &known_providers {
        if stored.contains_key(*provider) {
            // Already shown from store, but check if env var also exists
            if let Some(env_var) = env_var_for_provider(provider) {
                if std::env::var(env_var).is_ok() {
                    println!(
                        "  {} {} ({}{})",
                        provider.bold(),
                        format!("${}", env_var).dimmed(),
                        CredentialSource::EnvVar.to_string().cyan(),
                        ", overrides file".dimmed()
                    );
                }
            }
            continue;
        }

        if let Some(env_var) = env_var_for_provider(provider) {
            if let Ok(value) = std::env::var(env_var) {
                if !value.is_empty() {
                    found_any = true;
                    let masked = mask_key(&value);
                    println!(
                        "  {} {} ({})",
                        provider.bold(),
                        masked.dimmed(),
                        CredentialSource::EnvVar.to_string().cyan()
                    );
                }
            }
        }
    }

    if !found_any {
        println!("{}", "No credentials configured.".dimmed());
        println!();
        println!("Add credentials with:");
        println!(
            "  {} --provider openai --key sk-...",
            "cru auth login".bold()
        );
        println!("  {} {}", "or set".dimmed(), "OPENAI_API_KEY".bold());
    }

    Ok(())
}

/// Mask an API key for display (show first 5 chars + ****)
fn mask_key(key: &str) -> String {
    if key.len() <= 5 {
        "****".to_string()
    } else {
        format!("{}****", &key[..5])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_key_short() {
        assert_eq!(mask_key("abc"), "****");
        assert_eq!(mask_key(""), "****");
    }

    #[test]
    fn mask_key_normal() {
        assert_eq!(mask_key("sk-test-key-12345"), "sk-te****");
        assert_eq!(mask_key("sk-ant-api3"), "sk-an****");
    }
}
