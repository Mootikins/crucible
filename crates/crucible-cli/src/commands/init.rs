use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::task;

use crate::kiln_validate::{expand_tilde, validate_kiln_path, ValidationSeverity};
use crate::provider_detect::{detect_providers_available, DetectedProvider};

pub async fn execute(path: Option<PathBuf>, force: bool, interactive: bool) -> Result<()> {
    let target_path = match path {
        Some(p) => {
            let expanded = expand_tilde(&p.to_string_lossy());
            expanded
        }
        None => PathBuf::from("."),
    };

    let validation = validate_kiln_path(&target_path);

    if validation.is_blocked() && !force {
        for finding in validation.findings_by_severity(ValidationSeverity::HardBlock) {
            eprintln!("{} {}", "Error:".red().bold(), finding.message);
            if let Some(ref suggestion) = finding.suggestion {
                eprintln!("  {}", suggestion);
            }
        }
        anyhow::bail!("Cannot initialize kiln at {}", target_path.display());
    }

    if validation.is_existing_kiln && !force {
        println!(
            "{} Kiln already exists at {}. No changes made.",
            "Info:".cyan().bold(),
            target_path.display()
        );
        return Ok(());
    }

    for finding in validation.findings_by_severity(ValidationSeverity::StrongWarning) {
        eprintln!("{} {}", "Warning:".yellow().bold(), finding.message);
        if let Some(ref suggestion) = finding.suggestion {
            eprintln!("  {}", suggestion);
        }
    }

    for finding in validation.findings_by_severity(ValidationSeverity::MildWarning) {
        eprintln!("{} {}", "Note:".blue().bold(), finding.message);
    }

    let crucible_dir = target_path.join(".crucible");

    let providers = detect_providers_available().await;

    let (provider, model) = if interactive && !providers.is_empty() {
        prompt_provider_selection(&providers)?
    } else if !providers.is_empty() {
        let p = providers[0].provider_type.clone();
        let m = providers[0]
            .default_model
            .clone()
            .unwrap_or_else(|| default_model_for(&p).to_string());
        (p, m)
    } else {
        ("ollama".to_string(), "llama3.2".to_string())
    };

    let config_content = generate_config_with_provider(&provider, &model);
    let target_for_display = target_path.clone();
    task::spawn_blocking(move || create_kiln_with_config(&crucible_dir, &config_content, force))
        .await??;

    println!(
        "{} Kiln initialized at: {}",
        "Success:".green().bold(),
        target_for_display.display()
    );
    println!("  Provider: {}", provider.cyan());
    println!("  Model: {}", model.cyan());

    if validation.markdown_file_count > 0 {
        println!(
            "  Found {} markdown file(s) — Crucible will index these.",
            validation.markdown_file_count
        );
    }

    Ok(())
}

pub fn create_kiln_with_config(
    crucible_dir: &Path,
    config_content: &str,
    force: bool,
) -> Result<()> {
    if force && crucible_dir.exists() {
        fs::remove_dir_all(crucible_dir)?;
    }

    if let Some(parent) = crucible_dir.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    fs::create_dir_all(crucible_dir)?;
    fs::create_dir_all(crucible_dir.join("sessions"))?;
    fs::create_dir_all(crucible_dir.join("plugins"))?;
    fs::write(crucible_dir.join("config.toml"), config_content)?;

    Ok(())
}

fn prompt_provider_selection(providers: &[DetectedProvider]) -> Result<(String, String)> {
    use dialoguer::{theme::ColorfulTheme, Input, Select};

    let theme = ColorfulTheme::default();

    println!("{}", "Detected providers:".green().bold());
    for (i, p) in providers.iter().enumerate() {
        println!("  {}. {} - {}", i + 1, p.name, p.reason);
    }

    let items: Vec<&str> = providers.iter().map(|p| p.name.as_str()).collect();
    let selection = Select::with_theme(&theme)
        .with_prompt("Select LLM provider")
        .items(&items)
        .default(0)
        .interact()?;

    let selected = &providers[selection];
    let default_model = selected
        .default_model
        .clone()
        .unwrap_or_else(|| default_model_for(&selected.provider_type).to_string());

    let model: String = Input::with_theme(&theme)
        .with_prompt("Model")
        .default(default_model)
        .interact_text()?;

    Ok((selected.provider_type.clone(), model))
}

pub fn generate_config_with_provider(provider: &str, model: &str) -> String {
    let endpoint = match provider {
        "ollama" => "http://localhost:11434",
        "openai" => "https://api.openai.com/v1",
        "anthropic" => "https://api.anthropic.com",
        _ => "http://localhost:11434",
    };

    format!(
        r#"# Crucible kiln configuration
# See https://github.com/mootless/crucible for options

[storage]
backend = "sqlite"

[chat]
provider = "{provider}"
model = "{model}"
endpoint = "{endpoint}"

[llm]
default = "chat"

[llm.providers.chat]
type = "{provider}"
endpoint = "{endpoint}"
default_model = "{model}"
"#
    )
}

fn default_model_for(provider: &str) -> &'static str {
    match provider {
        "ollama" => "llama3.2",
        "openai" => "gpt-4o-mini",
        "anthropic" => "claude-3-5-sonnet-latest",
        _ => "llama3.2",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_config_with_provider() {
        let config = generate_config_with_provider("ollama", "llama3.2");
        assert!(config.contains("[chat]"));
        assert!(config.contains("provider = \"ollama\""));
        assert!(config.contains("model = \"llama3.2\""));
    }

    #[test]
    fn test_generate_config_openai() {
        let config = generate_config_with_provider("openai", "gpt-4o");
        assert!(config.contains("provider = \"openai\""));
        assert!(config.contains("model = \"gpt-4o\""));
    }

    #[test]
    fn test_generate_config_anthropic() {
        let config = generate_config_with_provider("anthropic", "claude-3-5-sonnet-latest");
        assert!(config.contains("provider = \"anthropic\""));
        assert!(config.contains("model = \"claude-3-5-sonnet-latest\""));
        assert!(config.contains("endpoint = \"https://api.anthropic.com\""));
    }

    #[test]
    fn test_generate_config_endpoint_mapping() {
        let ollama_config = generate_config_with_provider("ollama", "test");
        assert!(ollama_config.contains("endpoint = \"http://localhost:11434\""));

        let openai_config = generate_config_with_provider("openai", "test");
        assert!(openai_config.contains("endpoint = \"https://api.openai.com/v1\""));

        let anthropic_config = generate_config_with_provider("anthropic", "test");
        assert!(anthropic_config.contains("endpoint = \"https://api.anthropic.com\""));

        let unknown_config = generate_config_with_provider("unknown", "test");
        assert!(unknown_config.contains("endpoint = \"http://localhost:11434\""));
    }

    #[test]
    fn test_default_model_for_providers() {
        assert_eq!(default_model_for("ollama"), "llama3.2");
        assert_eq!(default_model_for("openai"), "gpt-4o-mini");
        assert_eq!(default_model_for("anthropic"), "claude-3-5-sonnet-latest");
        assert_eq!(default_model_for("unknown"), "llama3.2");
    }
}
