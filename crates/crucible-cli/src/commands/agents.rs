//! Agent card management commands
//!
//! Provides CLI commands for listing, showing, and validating agent cards.

use anyhow::Result;
use crucible_core::agent::{AgentCard, AgentCardLoader, AgentCardRegistry};
use std::path::{Path, PathBuf};

use crate::cli::AgentsCommands;
use crate::config::CliConfig;

/// Execute agent subcommand
pub async fn execute(config: CliConfig, command: Option<AgentsCommands>) -> Result<()> {
    // When no subcommand is given, default to list
    let cmd = command.unwrap_or(AgentsCommands::List {
        tag: None,
        format: "table".to_string(),
    });

    match cmd {
        AgentsCommands::List { tag, format } => list(&config, tag, format).await,
        AgentsCommands::Show { name, format, full } => show(&config, name, format, full).await,
        AgentsCommands::Validate { verbose } => validate(&config, verbose).await,
    }
}

/// Load all agent cards from configured directories
fn load_agent_registry(config: &CliConfig) -> AgentCardRegistry {
    let mut registry = AgentCardRegistry::default();
    let dirs = collect_agent_directories(config);

    for dir in dirs {
        if dir.exists() && dir.is_dir() {
            if let Ok(count) = registry.load_from_directory(dir.to_string_lossy().as_ref()) {
                if count > 0 {
                    tracing::debug!("Loaded {} agent cards from {:?}", count, dir);
                }
            }
        }
    }

    registry
}

/// Collect all agent card directories in load order per spec.
///
/// Load order (later sources override earlier by agent name):
/// 1. `~/.config/crucible/agents/` - Global default directory
/// 2. Paths from global config `agent_directories`
/// 3. `KILN_DIR/.crucible/agents/` - Kiln hidden config directory
/// 4. `KILN_DIR/agents/` - Kiln visible content directory
/// 5. Paths from kiln config `agent_directories` (future)
pub fn collect_agent_directories(config: &CliConfig) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // 1. Global default: ~/.config/crucible/agents/ (or %APPDATA%\crucible\agents\ on Windows)
    if let Some(config_dir) = dirs::config_dir() {
        let global_agents = config_dir.join("crucible").join("agents");
        dirs.push(global_agents);
    }

    // 2. Global config agent_directories
    for dir in &config.agent_directories {
        let resolved = resolve_path(dir, None);
        dirs.push(resolved);
    }

    // 3. Kiln hidden: KILN_DIR/.crucible/agents/
    let kiln_hidden = config.kiln_path.join(".crucible").join("agents");
    dirs.push(kiln_hidden);

    // 4. Kiln visible: KILN_DIR/agents/
    let kiln_visible = config.kiln_path.join("agents");
    dirs.push(kiln_visible);

    // 5. Kiln config agent_directories - TODO: Load kiln-level config

    dirs
}

/// Resolve a path, handling home directory expansion.
///
/// - Absolute paths are used as-is
/// - Paths starting with ~ are expanded to home directory
/// - Relative paths are returned as-is (caller should resolve relative to config file)
fn resolve_path(path: &Path, _config_dir: Option<&PathBuf>) -> PathBuf {
    let path_str = path.to_string_lossy();

    if let Some(rest) = path_str.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }

    path.to_path_buf()
}

/// List all registered agent cards
async fn list(config: &CliConfig, tag: Option<String>, format: String) -> Result<()> {
    let registry = load_agent_registry(config);

    // Get cards, optionally filtered by tag
    let cards: Vec<&AgentCard> = if let Some(ref tag_filter) = tag {
        registry.get_by_tag(tag_filter)
    } else {
        registry
            .list()
            .iter()
            .filter_map(|name| registry.get(name))
            .collect()
    };

    if cards.is_empty() {
        if let Some(t) = &tag {
            println!("No agent cards found with tag '{}'.", t);
        } else {
            println!("No agent cards found.");
        }
        return Ok(());
    }

    match format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&cards)?;
            println!("{}", json);
        }
        _ => {
            // Table format
            println!("{:<25} {:<10} DESCRIPTION", "NAME", "VERSION");
            println!("{}", "-".repeat(70));
            for card in cards {
                let desc = if card.description.len() > 35 {
                    format!("{}...", &card.description[..32])
                } else {
                    card.description.clone()
                };
                println!("{:<25} {:<10} {}", card.name, card.version, desc);
            }
        }
    }

    Ok(())
}

/// Show details of a specific agent card
async fn show(config: &CliConfig, name: String, format: String, full: bool) -> Result<()> {
    let registry = load_agent_registry(config);

    let card = match registry.get(&name) {
        Some(c) => c,
        None => {
            anyhow::bail!("Agent card '{}' not found.", name);
        }
    };

    match format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(card)?;
            println!("{}", json);
        }
        _ => {
            // Table/human-readable format
            println!("Name:        {}", card.name);
            println!("Version:     {}", card.version);
            println!("Description: {}", card.description);

            if !card.tags.is_empty() {
                println!("Tags:        {}", card.tags.join(", "));
            }

            if !card.mcp_servers.is_empty() {
                println!("MCP Servers: {}", card.mcp_servers.join(", "));
            }

            if !card.config.is_empty() {
                println!("Config:      {} entries", card.config.len());
            }

            println!("\nSystem Prompt:");
            println!("{}", "-".repeat(50));

            if full || card.system_prompt.lines().count() <= 10 {
                println!("{}", card.system_prompt);
            } else {
                // Truncate to first 10 lines
                let truncated: String = card
                    .system_prompt
                    .lines()
                    .take(10)
                    .collect::<Vec<_>>()
                    .join("\n");
                println!("{}", truncated);
                println!("...");
                println!("\n(Use --full to see complete system prompt)");
            }
        }
    }

    Ok(())
}

/// Validation result for an agent card file
struct ValidationResult {
    path: PathBuf,
    success: bool,
    error: Option<String>,
    warnings: Vec<String>,
}

/// Validate all agent cards
async fn validate(config: &CliConfig, verbose: bool) -> Result<()> {
    let dirs = collect_agent_directories(config);
    let mut loader = AgentCardLoader::new();
    let mut results: Vec<ValidationResult> = Vec::new();
    let mut total_files = 0;
    let mut valid_count = 0;
    let mut warning_count = 0;
    let mut error_count = 0;

    for dir in dirs {
        if !dir.exists() || !dir.is_dir() {
            continue;
        }

        // Find all .md files in directory
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                total_files += 1;
                let mut warnings = Vec::new();

                // Try to load the agent card
                match loader.load_from_file(path.to_string_lossy().as_ref()) {
                    Ok(card) => {
                        // Check for warnings (recommended fields)
                        // Check if type: agent is present (we need to read raw frontmatter)
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if !content.contains("type: agent")
                                && !content.contains("type: \"agent\"")
                            {
                                warnings.push(
                                    "Missing recommended 'type: agent' frontmatter field"
                                        .to_string(),
                                );
                                warning_count += 1;
                            }
                        }

                        // Check for empty tags
                        if card.tags.is_empty() {
                            warnings
                                .push("No tags defined (recommended for discovery)".to_string());
                            warning_count += 1;
                        }

                        results.push(ValidationResult {
                            path: path.clone(),
                            success: true,
                            error: None,
                            warnings: warnings.clone(),
                        });

                        if warnings.is_empty() {
                            valid_count += 1;
                        } else {
                            valid_count += 1; // Still valid, just has warnings
                        }
                    }
                    Err(e) => {
                        error_count += 1;
                        results.push(ValidationResult {
                            path: path.clone(),
                            success: false,
                            error: Some(e.to_string()),
                            warnings: vec![],
                        });
                    }
                }
            }
        }
    }

    // Output results
    if total_files == 0 {
        println!("No agent card files found in configured directories.");
        return Ok(());
    }

    if verbose {
        for result in &results {
            if result.success {
                if result.warnings.is_empty() {
                    println!("✓ {:?}", result.path);
                } else {
                    println!("✓ {:?} (with warnings)", result.path);
                    for warning in &result.warnings {
                        println!("  ⚠ {}", warning);
                    }
                }
            } else {
                println!("✗ {:?}", result.path);
                if let Some(ref err) = result.error {
                    println!("  Error: {}", err);
                }
            }
        }
        println!();
    }

    // Summary
    println!("Validation Summary:");
    println!("  Total files:  {}", total_files);
    println!("  Valid:        {}", valid_count);
    println!("  Errors:       {}", error_count);
    println!("  Warnings:     {}", warning_count);

    if error_count > 0 {
        anyhow::bail!("{} agent card(s) failed validation", error_count);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_config::CliConfig as CliAppConfig;
    use crucible_config::{
        AcpConfig, ChatConfig, EmbeddingConfig, LlmConfig, ProcessingConfig, ProvidersConfig,
    };
    use std::fs;
    use tempfile::TempDir;

    /// Cross-platform test path helper
    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("crucible_test_{}", name))
    }

    fn test_config(kiln_path: PathBuf) -> CliConfig {
        CliConfig {
            kiln_path,
            agent_directories: Vec::new(),
            embedding: EmbeddingConfig::default(),
            acp: AcpConfig::default(),
            chat: ChatConfig::default(),
            llm: LlmConfig::default(),
            cli: CliAppConfig::default(),
            logging: None,
            processing: ProcessingConfig::default(),
            providers: ProvidersConfig::default(),
            context: None,
            storage: None,
            mcp: None,
            plugins: std::collections::HashMap::new(),
            source_map: None,
        }
    }

    fn create_test_agent_card(dir: &std::path::Path, name: &str) -> std::io::Result<()> {
        let content = format!(
            r#"---
type: agent
name: "{name}"
version: "1.0.0"
description: "Test agent for unit testing"
tags:
  - "test"
  - "documentation"
---

# System Prompt

You are a test agent.
"#,
            name = name
        );
        fs::write(
            dir.join(format!("{}.md", name.to_lowercase().replace(" ", "-"))),
            content,
        )
    }

    #[test]
    fn test_collect_agent_directories_includes_defaults() {
        let kiln_path = test_path("test-kiln");
        let config = test_config(kiln_path.clone());
        let dirs = collect_agent_directories(&config);

        // Should include at least:
        // 1. Global default
        // 2. Kiln hidden
        // 3. Kiln visible
        assert!(dirs.len() >= 3);

        // Check kiln directories are present
        assert!(dirs.contains(&kiln_path.join(".crucible/agents")));
        assert!(dirs.contains(&kiln_path.join("agents")));
    }

    #[test]
    fn test_collect_agent_directories_includes_config() {
        let kiln_path = test_path("test-kiln");
        let mut config = test_config(kiln_path);
        config.agent_directories = vec![
            PathBuf::from("/custom/agents"),
            PathBuf::from("./local-agents"),
        ];

        let dirs = collect_agent_directories(&config);

        // Should include custom directories
        assert!(dirs.contains(&PathBuf::from("/custom/agents")));
        assert!(dirs.contains(&PathBuf::from("./local-agents")));
    }

    #[test]
    fn test_collect_agent_directories_order() {
        let kiln_path = test_path("test-kiln");
        let config = test_config(kiln_path.clone());
        let dirs = collect_agent_directories(&config);

        // Find indices
        let kiln_hidden_idx = dirs
            .iter()
            .position(|p| p == &kiln_path.join(".crucible/agents"));
        let kiln_visible_idx = dirs.iter().position(|p| p == &kiln_path.join("agents"));

        // Kiln hidden should come before kiln visible
        assert!(kiln_hidden_idx.is_some());
        assert!(kiln_visible_idx.is_some());
        assert!(kiln_hidden_idx.unwrap() < kiln_visible_idx.unwrap());
    }

    #[test]
    fn test_resolve_path_absolute() {
        let path = PathBuf::from("/absolute/path");
        let resolved = resolve_path(&path, None);
        assert_eq!(resolved, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_resolve_path_home_expansion() {
        let path = PathBuf::from("~/some/path");
        let resolved = resolve_path(&path, None);

        // Should have expanded ~ to home dir
        if let Some(home) = dirs::home_dir() {
            assert_eq!(resolved, home.join("some/path"));
        }
    }

    #[test]
    fn test_resolve_path_relative() {
        let path = PathBuf::from("./relative/path");
        let resolved = resolve_path(&path, None);
        // Relative paths are returned as-is for now
        assert_eq!(resolved, PathBuf::from("./relative/path"));
    }

    #[test]
    fn test_load_agent_registry_from_kiln() {
        // Create temp dir structure with agents
        let temp_dir = TempDir::new().unwrap();
        let agents_dir = temp_dir.path().join("agents");
        fs::create_dir_all(&agents_dir).unwrap();

        // Create a test agent card
        create_test_agent_card(&agents_dir, "Test Agent").unwrap();

        // Create config pointing to temp dir as kiln
        let config = test_config(temp_dir.path().to_path_buf());
        let registry = load_agent_registry(&config);

        // Should have loaded the agent
        assert_eq!(registry.count(), 1);
        assert!(registry.has("Test Agent"));
    }

    #[test]
    fn test_load_agent_registry_empty_when_no_dirs() {
        // Create temp dir without agents directory
        let temp_dir = TempDir::new().unwrap();
        let config = test_config(temp_dir.path().to_path_buf());
        let registry = load_agent_registry(&config);

        // Should be empty (no dirs exist)
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_load_agent_registry_later_overrides_earlier() {
        // Test that kiln/agents overrides .crucible/agents
        let temp_dir = TempDir::new().unwrap();

        // Create both agent directories
        let hidden_dir = temp_dir.path().join(".crucible").join("agents");
        let visible_dir = temp_dir.path().join("agents");
        fs::create_dir_all(&hidden_dir).unwrap();
        fs::create_dir_all(&visible_dir).unwrap();

        // Create agent with same name in hidden dir
        let hidden_content = r#"---
name: "Shared Agent"
version: "1.0.0"
description: "Hidden version"
---

You are the hidden version.
"#;
        fs::write(hidden_dir.join("shared-agent.md"), hidden_content).unwrap();

        // Create agent with same name in visible dir (should override)
        let visible_content = r#"---
name: "Shared Agent"
version: "2.0.0"
description: "Visible version (should win)"
---

You are the visible version.
"#;
        fs::write(visible_dir.join("shared-agent.md"), visible_content).unwrap();

        let config = test_config(temp_dir.path().to_path_buf());
        let registry = load_agent_registry(&config);

        // Should have only one agent (later overrides)
        assert_eq!(registry.count(), 1);

        // The visible version should have won
        let agent = registry.get("Shared Agent").unwrap();
        assert_eq!(agent.version, "2.0.0");
        assert!(agent.description.contains("Visible version"));
    }
}
