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
    read_kiln_config, read_project_config, register_project_in_config, write_kiln_config,
    write_project_config, CliAppConfig, KilnAttachment, KilnConfig, KilnMeta, ProjectConfig,
    SecurityConfig,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitType {
    Kiln,
    Project,
    Unknown,
}

/// Detect whether a directory is a kiln, project, or uninitialized.
pub fn detect_init_type(path: &Path) -> InitType {
    let crucible_dir = path.join(".crucible");
    if crucible_dir.join("kiln.toml").exists() {
        InitType::Kiln
    } else if crucible_dir.join("project.toml").exists() {
        InitType::Project
    } else {
        InitType::Unknown
    }
}

pub async fn execute(path: Option<PathBuf>, force: bool, yes: bool) -> Result<()> {
    let target_path = match path {
        Some(p) => expand_tilde(&p.to_string_lossy()),
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
        anyhow::bail!("Cannot initialize at {}", target_path.display());
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

    // Detect what this directory already is
    let init_type = detect_init_type(&target_path);

    match init_type {
        InitType::Kiln if !force => {
            println!(
                "{} Kiln already exists at {}. No changes made.",
                "Info:".cyan().bold(),
                target_path.display()
            );
            return Ok(());
        }
        InitType::Project if !force => {
            println!(
                "{} Project already exists at {}. No changes made.",
                "Info:".cyan().bold(),
                target_path.display()
            );
            return Ok(());
        }
        _ => {}
    }

    // Determine what to initialize
    let resolved_type = match init_type {
        InitType::Kiln => InitType::Kiln,
        InitType::Project => InitType::Project,
        InitType::Unknown if yes => InitType::Kiln,
        InitType::Unknown => prompt_init_type()?,
    };

    match resolved_type {
        InitType::Kiln => run_kiln_init(&target_path, force, yes, &validation).await,
        InitType::Project => run_project_init(&target_path, force, yes).await,
        InitType::Unknown => unreachable!(),
    }
}

async fn run_kiln_init(
    target_path: &Path,
    force: bool,
    yes: bool,
    validation: &crate::kiln_validate::ValidationResult,
) -> Result<()> {
    let crucible_dir = target_path.join(".crucible");

    let providers = detect_providers(&crucible_config::ChatConfig::default());

    let (provider, model) = if !yes && !providers.is_empty() {
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

    let (name, classification) = if yes {
        let dir_name = dir_name_or_default(target_path);
        (dir_name, DataClassification::Public)
    } else {
        prompt_kiln_init(target_path)?
    };

    let config_content = generate_config_with_provider(&provider, &model);
    let target_for_display = target_path.to_path_buf();
    let markdown_count = validation.markdown_file_count;

    let name_clone = name.clone();
    let classification_copy = classification;
    task::spawn_blocking(move || {
        create_kiln_with_config(&crucible_dir, &config_content, force)?;
        write_kiln_and_project_config(&crucible_dir, &name_clone, classification_copy)?;
        Ok::<(), anyhow::Error>(())
    })
    .await??;

    println!(
        "{} Kiln initialized at: {}",
        "Success:".green().bold(),
        target_for_display.display()
    );
    println!("  Name: {}", name.cyan());
    println!("  Provider: {}", provider.cyan());
    println!("  Model: {}", model.cyan());
    println!("  Classification: {}", classification.as_str().cyan());

    if markdown_count > 0 {
        println!(
            "  Found {} markdown file(s) — Crucible will index these.",
            markdown_count
        );
    }

    Ok(())
}

async fn run_project_init(target_path: &Path, force: bool, yes: bool) -> Result<()> {
    let crucible_dir = target_path.join(".crucible");

    let (name, kilns, default_kiln) = if yes {
        let dir_name = dir_name_or_default(target_path);
        (dir_name, vec![], None)
    } else {
        prompt_project_init(target_path)?
    };

    let target_for_display = target_path.to_path_buf();
    let name_clone = name.clone();
    let kilns_clone = kilns.clone();
    task::spawn_blocking(move || {
        if force && crucible_dir.exists() {
            fs::remove_dir_all(&crucible_dir)?;
        }
        fs::create_dir_all(&crucible_dir)?;

        let project_config = ProjectConfig {
            project: Some(crucible_config::ProjectMeta {
                name: Some(name_clone),
            }),
            kilns: kilns_clone
                .iter()
                .map(|k| KilnAttachment {
                    path: PathBuf::from(k),
                    name: Some(k.clone()),
                    data_classification: None,
                })
                .collect(),
            security: SecurityConfig::default(),
        };

        let root_dir = crucible_dir.parent().unwrap_or(&crucible_dir);
        write_project_config(root_dir, &project_config)?;

        Ok::<(), anyhow::Error>(())
    })
    .await??;

    // Register the project in global config
    let absolute_path = std::fs::canonicalize(&target_for_display).unwrap_or(target_for_display.clone());
    let config_path = CliAppConfig::default_config_path();
    let kiln_refs: Vec<&str> = kilns.iter().map(|s| s.as_str()).collect();
    match register_project_in_config(
        &config_path,
        &name,
        &absolute_path,
        &kiln_refs,
        default_kiln.as_deref(),
    ) {
        Ok(()) => {
            println!("  {} Registered in global config", "\u{2713}".green());
        }
        Err(e) => {
            eprintln!(
                "{} Could not register project in global config: {}",
                "Warning:".yellow().bold(),
                e
            );
        }
    }

    println!(
        "{} Project initialized at: {}",
        "Success:".green().bold(),
        target_for_display.display()
    );
    println!("  Name: {}", name.cyan());
    if !kilns.is_empty() {
        println!("  Kilns: {}", kilns.join(", ").cyan());
    }
    if let Some(dk) = &default_kiln {
        println!("  Default kiln: {}", dk.cyan());
    }

    Ok(())
}

fn dir_name_or_default(path: &Path) -> String {
    path.canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .or_else(|| {
            path.file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "crucible".to_string())
}

// --- Interactive prompts ---

fn prompt_init_type() -> Result<InitType> {
    use dialoguer::{theme::ColorfulTheme, Select};

    let theme = ColorfulTheme::default();
    let items = ["Kiln (knowledge store for notes and sessions)", "Project (code repository with kiln bindings)"];

    let selection = Select::with_theme(&theme)
        .with_prompt("What is this directory?")
        .items(&items)
        .default(0)
        .interact()?;

    Ok(match selection {
        0 => InitType::Kiln,
        1 => InitType::Project,
        _ => unreachable!(),
    })
}

fn prompt_kiln_init(path: &Path) -> Result<(String, DataClassification)> {
    use dialoguer::{theme::ColorfulTheme, Input};

    let theme = ColorfulTheme::default();
    let default_name = dir_name_or_default(path);

    let name: String = Input::with_theme(&theme)
        .with_prompt("Kiln name")
        .default(default_name)
        .interact_text()?;

    let classification = prompt_classification_selection()?;

    Ok((name, classification))
}

fn prompt_project_init(path: &Path) -> Result<(String, Vec<String>, Option<String>)> {
    use dialoguer::{theme::ColorfulTheme, Input, MultiSelect, Select};

    let theme = ColorfulTheme::default();
    let default_name = dir_name_or_default(path);

    let name: String = Input::with_theme(&theme)
        .with_prompt("Project name")
        .default(default_name)
        .interact_text()?;

    // Load global config to discover registered kilns
    let config = CliAppConfig::load(None, None, None).ok();
    let resolved = config
        .as_ref()
        .map(|c| c.resolved_kilns())
        .unwrap_or_default();
    let kiln_names: Vec<String> = resolved.keys().cloned().collect();

    let selected_kilns = if kiln_names.is_empty() {
        println!(
            "{} No kilns registered in global config. You can attach kilns later.",
            "Note:".blue().bold()
        );
        vec![]
    } else {
        let selections = MultiSelect::with_theme(&theme)
            .with_prompt("Select kilns to attach (space to toggle, enter to confirm)")
            .items(&kiln_names)
            .interact()?;

        selections.iter().map(|&i| kiln_names[i].clone()).collect()
    };

    let default_kiln = if selected_kilns.len() > 1 {
        let idx = Select::with_theme(&theme)
            .with_prompt("Default kiln")
            .items(&selected_kilns)
            .default(0)
            .interact()?;
        Some(selected_kilns[idx].clone())
    } else if selected_kilns.len() == 1 {
        Some(selected_kilns[0].clone())
    } else {
        None
    };

    Ok((name, selected_kilns, default_kiln))
}

// --- Existing helpers (kept) ---

// Kept for Task 6 (global config registration).
#[allow(dead_code)]
/// Upsert a key=value line in config contents.
///
/// Handles three cases:
/// 1. Key exists (commented or not) - replace the line
/// 2. Key doesn't exist, but preferred_anchor exists - insert after anchor
/// 3. Neither exists - prepend to file
fn upsert_kv_line(
    contents: &str,
    key: &str,
    value: &str,
    preferred_anchor: Option<&str>,
) -> String {
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

#[allow(dead_code)]
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
    name: &str,
    classification: DataClassification,
) -> Result<()> {
    let root_dir = crucible_dir.parent().unwrap_or(crucible_dir);

    // Read or create kiln.toml
    let kiln_config = if let Some(config) = read_kiln_config(root_dir) {
        config
    } else {
        KilnConfig {
            kiln: KilnMeta {
                name: name.to_string(),
            },
        }
    };

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
    use tempfile::TempDir;

    #[test]
    fn detect_kiln_from_kiln_toml() {
        let tmp = TempDir::new().unwrap();
        let crucible_dir = tmp.path().join(".crucible");
        fs::create_dir_all(&crucible_dir).unwrap();
        fs::write(crucible_dir.join("kiln.toml"), "[kiln]\nname = \"test\"").unwrap();
        assert_eq!(detect_init_type(tmp.path()), InitType::Kiln);
    }

    #[test]
    fn detect_project_from_project_toml() {
        let tmp = TempDir::new().unwrap();
        let crucible_dir = tmp.path().join(".crucible");
        fs::create_dir_all(&crucible_dir).unwrap();
        fs::write(crucible_dir.join("project.toml"), "[project]\nname = \"test\"").unwrap();
        assert_eq!(detect_init_type(tmp.path()), InitType::Project);
    }

    #[test]
    fn detect_unknown_when_no_crucible_dir() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(detect_init_type(tmp.path()), InitType::Unknown);
    }

    #[test]
    fn detect_kiln_when_both_exist() {
        let tmp = TempDir::new().unwrap();
        let crucible_dir = tmp.path().join(".crucible");
        fs::create_dir_all(&crucible_dir).unwrap();
        fs::write(crucible_dir.join("kiln.toml"), "[kiln]\nname = \"test\"").unwrap();
        fs::write(crucible_dir.join("project.toml"), "").unwrap();
        // Kiln takes precedence (it's the primary identity)
        assert_eq!(detect_init_type(tmp.path()), InitType::Kiln);
    }

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

    #[test]
    fn dir_name_or_default_returns_dir_name() {
        let tmp = TempDir::new().unwrap();
        let name = dir_name_or_default(tmp.path());
        // tempdir names are random but non-empty
        assert!(!name.is_empty());
    }

    #[test]
    fn dir_name_or_default_falls_back() {
        // A path like "." should resolve to something meaningful
        let name = dir_name_or_default(Path::new("."));
        assert!(!name.is_empty());
    }

    #[tokio::test]
    async fn execute_yes_creates_kiln_by_default() {
        let tmp = TempDir::new().unwrap();
        execute(Some(tmp.path().to_path_buf()), false, true)
            .await
            .unwrap();

        // Should have created kiln.toml (kiln is the default with --yes)
        assert!(tmp.path().join(".crucible/kiln.toml").exists());
        assert!(tmp.path().join(".crucible/config.toml").exists());
    }

    #[tokio::test]
    async fn execute_skips_existing_kiln_without_force() {
        let tmp = TempDir::new().unwrap();
        let crucible_dir = tmp.path().join(".crucible");
        fs::create_dir_all(&crucible_dir).unwrap();
        fs::write(crucible_dir.join("kiln.toml"), "[kiln]\nname = \"test\"").unwrap();

        // Should succeed without error (early return)
        execute(Some(tmp.path().to_path_buf()), false, true)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn execute_skips_existing_project_without_force() {
        let tmp = TempDir::new().unwrap();
        let crucible_dir = tmp.path().join(".crucible");
        fs::create_dir_all(&crucible_dir).unwrap();
        fs::write(
            crucible_dir.join("project.toml"),
            "[project]\nname = \"test\"",
        )
        .unwrap();

        // Should succeed without error (early return)
        execute(Some(tmp.path().to_path_buf()), false, true)
            .await
            .unwrap();
    }
}
