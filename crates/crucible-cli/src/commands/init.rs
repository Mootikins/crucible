use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::task;
use tracing::info;

use crate::kiln_validate::{expand_tilde, validate_kiln_path, ValidationSeverity};
use crate::provider_detect::{detect_providers, DetectedProvider};
use crucible_config::components::DataClassification;
use crucible_config::{
    read_kiln_config, read_project_config, write_kiln_config, write_project_config, CliAppConfig,
    KilnAttachment, KilnConfig, KilnMeta, ProjectConfig, SecurityConfig,
};
pub async fn execute(
    path: Option<PathBuf>,
    force: bool,
    interactive: bool,
    personal: bool,
) -> Result<()> {
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

    let providers = detect_providers(&crucible_config::ChatConfig::default());

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

    let classification = if interactive {
        prompt_classification_selection()?
    } else {
        DataClassification::Public
    };
    let config_content = generate_config_with_provider(&provider, &model);
    let target_for_display = target_path.clone();
    let classification_for_write = classification;
    task::spawn_blocking(move || {
        create_kiln_with_config(&crucible_dir, &config_content, force)?;
        write_kiln_and_project_config(&crucible_dir, classification_for_write)?;
        Ok::<(), anyhow::Error>(())
    })
    .await??;

    println!(
        "{} Kiln initialized at: {}",
        "Success:".green().bold(),
        target_for_display.display()
    );
    println!("  Provider: {}", provider.cyan());
    println!("  Model: {}", model.cyan());
    println!("  Classification: {}", classification.as_str().cyan());

    if validation.markdown_file_count > 0 {
        println!(
            "  Found {} markdown file(s) — Crucible will index these.",
            validation.markdown_file_count
        );
    }

    // If --personal flag, update global config to set session_kiln
    if personal {
        let absolute_path = if target_for_display.is_absolute() {
            target_for_display.clone()
        } else {
            std::env::current_dir()?.join(&target_for_display)
        };
        update_global_config_session_kiln(&absolute_path)?;
        println!(
            "  {} session_kiln set to {} in config",
            "✓".green(),
            absolute_path.display()
        );
    }

    Ok(())
}

/// Upsert a key=value line in config contents.
///
/// Handles three cases:
/// 1. Key exists (commented or not) - replace the line
/// 2. Key doesn't exist, but preferred_anchor exists - insert after anchor
/// 3. Neither exists - prepend to file
fn upsert_kv_line(contents: &str, key: &str, value: &str, preferred_anchor: Option<&str>) -> String {
    let new_line = format!("{}= \"{}\"", key, value);

    // Case 1: Key already exists (commented or not)
    if let Some(idx) = contents.find(key) {
        let line_start = contents[..idx].rfind('\n').map_or(0, |p| p + 1);
        let line_end = contents[idx..]
            .find('\n')
            .map_or(contents.len(), |p| idx + p);
        let mut new_contents = String::with_capacity(contents.len());
        new_contents.push_str(&contents[..line_start]);
        new_contents.push_str(&new_line);
        new_contents.push_str(&contents[line_end..]);
        return new_contents;
    }

    // Case 2: Key doesn't exist, but preferred_anchor does
    if let Some(anchor) = preferred_anchor {
        if let Some(idx) = contents.find(anchor) {
            let line_end = contents[idx..]
                .find('\n')
                .map_or(contents.len(), |p| idx + p);
            let mut new_contents = String::with_capacity(contents.len() + new_line.len() + 1);
            new_contents.push_str(&contents[..line_end]);
            new_contents.push('\n');
            new_contents.push_str(&new_line);
            new_contents.push_str(&contents[line_end..]);
            return new_contents;
        }
    }

    // Case 3: Neither key nor anchor exists - prepend
    let mut new_contents = String::with_capacity(contents.len() + new_line.len() + 2);
    new_contents.push_str(&new_line);
    new_contents.push('\n');
    new_contents.push_str(contents);
    new_contents
}

/// Update `~/.config/crucible/config.toml` to set `session_kiln`.
///
/// If the config file exists, inserts or replaces the `session_kiln` line.
/// If not, creates a minimal config with just `session_kiln`.
fn update_global_config_session_kiln(kiln_path: &Path) -> Result<()> {
    let config_path = CliAppConfig::default_config_path();
    let path_str = kiln_path.to_string_lossy();

    if config_path.exists() {
        let contents = fs::read_to_string(&config_path)?;
        let new_contents = upsert_kv_line(&contents, "session_kiln", &path_str, Some("kiln_path"));
        fs::write(&config_path, new_contents)?;
    } else {
        // Create config file with session_kiln
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let new_line = format!("session_kiln = \"{}\"", path_str);
        fs::write(&config_path, format!("{}\n", new_line))?;
    }

    info!("Updated {} with session_kiln", config_path.display());
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

fn prompt_classification_selection() -> Result<DataClassification> {
    use dialoguer::{theme::ColorfulTheme, Select};

    let theme = ColorfulTheme::default();
    let levels = DataClassification::all();
    let items: Vec<&str> = levels.iter().map(|c| c.as_str()).collect();

    let selection = Select::with_theme(&theme)
        .with_prompt("Data classification for this kiln")
        .items(&items)
        .default(0)
        .interact()?;

    Ok(levels[selection])
}

fn write_kiln_and_project_config(
    crucible_dir: &Path,
    classification: DataClassification,
) -> Result<()> {
    // Get the root directory (parent of .crucible/)
    let root_dir = crucible_dir.parent().unwrap_or(crucible_dir);

    // Read or create kiln.toml
    let kiln_config = if let Some(config) = read_kiln_config(root_dir) {
        config
    } else {
        let dir_name = root_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Crucible Kiln")
            .to_string();
        KilnConfig {
            kiln: KilnMeta { name: dir_name },
        }
    };

    // Write kiln.toml
    write_kiln_config(root_dir, &kiln_config)?;

    // Read or create project.toml
    let mut project_config = if let Some(config) = read_project_config(root_dir) {
        config
    } else {
        ProjectConfig {
            project: None,
            kilns: vec![],
            security: SecurityConfig::default(),
        }
    };

    // Ensure there's a kiln entry for "." with the classification
    if let Some(kiln) = project_config
        .kilns
        .iter_mut()
        .find(|k| k.path == Path::new("."))
    {
        kiln.data_classification = Some(classification);
    } else {
        project_config.kilns.push(KilnAttachment {
            path: PathBuf::from("."),
            name: None,
            data_classification: Some(classification),
        });
    }

    // Write project.toml
    write_project_config(root_dir, &project_config)?;

    Ok(())
}

pub fn generate_config_with_provider(provider: &str, model: &str) -> String {
    let endpoint = match provider {
        "ollama" => "http://localhost:11434",
        "openai" => "https://api.openai.com/v1",
        "anthropic" => "https://api.anthropic.com/v1",
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
        assert!(config.contains("endpoint = \"https://api.anthropic.com/v1\""));
    }

    #[test]
    fn test_generate_config_endpoint_mapping() {
        let ollama_config = generate_config_with_provider("ollama", "test");
        assert!(ollama_config.contains("endpoint = \"http://localhost:11434\""));

        let openai_config = generate_config_with_provider("openai", "test");
        assert!(openai_config.contains("endpoint = \"https://api.openai.com/v1\""));

        let anthropic_config = generate_config_with_provider("anthropic", "test");
        assert!(anthropic_config.contains("endpoint = \"https://api.anthropic.com/v1\""));

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
