//! Agent card management commands
//!
//! Provides CLI commands for listing, showing, and validating agent cards.

use anyhow::Result;
use crucible_core::agent::{AgentCard, AgentCardLoader, AgentCardRegistry};
use std::path::{Path, PathBuf};

use crate::cli::AgentsCommands;
use crate::config::CliConfig;
use crate::formatting::{OutputFormat, TextFormat};

/// Width of the DESCRIPTION column in the `cru agents list` table.
const DESCRIPTION_MAX_CHARS: usize = 35;

/// Execute agent subcommand
pub async fn execute(config: CliConfig, command: Option<AgentsCommands>) -> Result<()> {
    // When no subcommand is given, default to list
    let cmd = command.unwrap_or(AgentsCommands::List {
        tag: None,
        format: None,
    });

    match cmd {
        AgentsCommands::List { tag, format } => {
            list(&config, tag, OutputFormat::for_stdout(format)).await
        }
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
/// 3. `KILN_DIR/.crucible/agents/` - Kiln config directory
///
/// Must stay in step with `crucible-daemon/src/agent_cards.rs::card_directories`.
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

    // 3. Kiln config: KILN_DIR/.crucible/agents/
    //
    // The kiln's visible `agents/` is deliberately NOT here, matching
    // `crucible-daemon/src/agent_cards.rs`: a card names a model, a system
    // prompt and a tool policy, so a cloned or synced kiln must not be able to
    // introduce one just by containing a directory. A kiln that is a card
    // library adds itself to the path in Lua instead.
    dirs.push(config.kiln_path.join(".crucible").join("agents"));

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
/// An ACP profile as `cru agents list` shows it.
///
/// Flattened out of the daemon's `agents.list_profiles` reply at the edge so
/// the rendering below is not four `["x"].as_str().unwrap_or("")` chains.
struct AcpProfile {
    name: String,
    description: String,
    available: bool,
}

/// The ACP profiles the daemon knows, or an empty list.
///
/// Best-effort on purpose. Cards are read straight off disk, so listing them
/// must not start depending on a daemon: someone running `cru agents list` to
/// find out why their card is not loading should not be told the daemon is
/// down. An unreachable daemon simply means no ACP section.
async fn acp_profiles() -> Vec<AcpProfile> {
    // `connect`, never `connect_or_start`: listing is a read-only inspection,
    // and `connect_or_start` spawns `cru daemon serve` — worse, on a version
    // mismatch it shuts the running daemon down and starts a fresh one. Someone
    // running `cru agents list` to find out why their card is not loading
    // should not have it restart their daemon as a side effect.
    let Ok(client) = crucible_daemon::DaemonClient::connect().await else {
        return Vec::new();
    };
    let Ok(reply) = client.agents_list_profiles().await else {
        return Vec::new();
    };
    reply
        .get("profiles")
        .and_then(|p| p.as_array())
        .map(|entries| {
            entries
                .iter()
                .map(|e| AcpProfile {
                    name: e["name"].as_str().unwrap_or_default().to_string(),
                    description: e["description"].as_str().unwrap_or_default().to_string(),
                    available: e["available"].as_bool().unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// List both things `cru session create` can attach to a session.
///
/// Cards and ACP profiles are different kinds of agent — a card is a persona
/// Crucible runs itself, a profile is an external subprocess — but "what can I
/// talk to?" is one question, and answering half of it was why `--agent` and
/// this command disagreed about what an agent is. Two sections, one command.
async fn list(config: &CliConfig, tag: Option<String>, format: OutputFormat) -> Result<()> {
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

    // A tag filter is a question about cards — profiles have no tags, so
    // showing them all under a filtered heading would misreport them as matches.
    let profiles = match tag {
        Some(_) => Vec::new(),
        None => acp_profiles().await,
    };

    if cards.is_empty() && profiles.is_empty() {
        match &tag {
            Some(t) => println!("No agent cards found with tag '{}'.", t),
            None => println!("No agent cards or ACP profiles found."),
        }
        return Ok(());
    }

    match format {
        OutputFormat::Json => {
            let json = serde_json::json!({
                "cards": cards,
                "acp_profiles": profiles
                    .iter()
                    .map(|p| serde_json::json!({
                        "name": p.name,
                        "description": p.description,
                        "available": p.available,
                    }))
                    .collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        OutputFormat::Table => {
            if !cards.is_empty() {
                let rows: Vec<Vec<String>> = cards
                    .iter()
                    .map(|card| {
                        vec![
                            card.name.clone(),
                            card.version.clone(),
                            card.description.clone(),
                        ]
                    })
                    .collect();
                println!("Agent cards (cru session create --agent <name>)");
                println!(
                    "{}",
                    crate::output::records_table(&["Name", "Version", "Description"], &rows)
                );
            }
            if !profiles.is_empty() {
                let rows: Vec<Vec<String>> = profiles
                    .iter()
                    .map(|p| {
                        vec![
                            p.name.clone(),
                            availability_label(p.available).to_string(),
                            p.description.clone(),
                        ]
                    })
                    .collect();
                if !cards.is_empty() {
                    println!();
                }
                println!("ACP profiles (cru session create --acp <name>)");
                println!(
                    "{}",
                    crate::output::records_table(&["Name", "Installed", "Description"], &rows)
                );
            }
        }
        OutputFormat::Plain => {
            if !cards.is_empty() {
                println!("{:<25} {:<10} DESCRIPTION", "CARD", "VERSION");
                println!("{}", "-".repeat(70));
                for card in &cards {
                    println!(
                        "{:<25} {:<10} {}",
                        card.name,
                        card.version,
                        truncate_description(&card.description)
                    );
                }
            }
            if !profiles.is_empty() {
                if !cards.is_empty() {
                    println!();
                }
                println!("{:<25} {:<10} DESCRIPTION", "ACP PROFILE", "INSTALLED");
                println!("{}", "-".repeat(70));
                for profile in &profiles {
                    println!(
                        "{:<25} {:<10} {}",
                        profile.name,
                        availability_label(profile.available),
                        truncate_description(&profile.description)
                    );
                }
            }
        }
    }

    Ok(())
}

/// Whether an ACP profile's binary was found on PATH.
///
/// Worth a column: a profile is configuration, and the thing it names may
/// simply not be installed — which is otherwise discovered as a spawn failure
/// at the far end of `session create`.
fn availability_label(available: bool) -> &'static str {
    if available {
        "yes"
    } else {
        "no"
    }
}

/// Fit an agent-card description into the `cru agents list` table column.
///
/// Truncates by chars, not bytes: descriptions are hand-authored frontmatter, so
/// an em dash or an accent straddling the cut used to abort the whole command.
fn truncate_description(description: &str) -> std::borrow::Cow<'_, str> {
    crucible_oil::truncate_to_chars(description, DESCRIPTION_MAX_CHARS, true)
}

/// Show details of a specific agent card
async fn show(config: &CliConfig, name: String, format: TextFormat, full: bool) -> Result<()> {
    let registry = load_agent_registry(config);

    let card = match registry.get(&name) {
        Some(c) => c,
        None => {
            anyhow::bail!("Agent card '{}' not found.", name);
        }
    };

    match format {
        TextFormat::Json => {
            let json = serde_json::to_string_pretty(card)?;
            println!("{}", json);
        }
        TextFormat::Text => {
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

        // Find all note files in directory
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if crucible_core::is_note_file(&path) {
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
    use std::fs;
    use tempfile::TempDir;

    /// Cross-platform test path helper
    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("crucible_test_{}", name))
    }

    fn test_config(kiln_path: PathBuf) -> CliConfig {
        CliConfig {
            kiln_path,
            ..Default::default()
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
    fn agents_list_truncates_description_on_a_char_boundary() {
        // Hand-authored frontmatter routinely carries em dashes and accents.
        // The em dash occupies bytes 31..34, so the old `&description[..32]`
        // sliced through the middle of it and panicked. Keep that offset if you
        // reword this fixture — it is what makes the test a regression test.
        let description = "Screens and ranks applications \u{2014} reads r\u{e9}sum\u{e9}s";
        let truncated = truncate_description(description);

        assert!(truncated.starts_with("Screens and ranks"));
        assert!(truncated.ends_with('\u{2026}'));
        assert_eq!(truncated.chars().count(), DESCRIPTION_MAX_CHARS);
        // No partial code point survived the cut.
        assert!(!truncated.contains('\u{FFFD}'));
    }

    #[test]
    fn agents_list_leaves_short_descriptions_alone() {
        let description = "Résumé triage \u{2014} short enough";
        assert_eq!(truncate_description(description), description);
    }

    #[test]
    fn test_collect_agent_directories_includes_defaults() {
        let kiln_path = test_path("test-kiln");
        let config = test_config(kiln_path.clone());
        let dirs = collect_agent_directories(&config);

        // Global default plus the kiln's config dir.
        assert!(dirs.len() >= 2, "{dirs:?}");
        assert!(dirs.contains(&kiln_path.join(".crucible/agents")));
    }

    /// The CLI's list must match the daemon's — the kiln's visible `agents/`
    /// is not a discovery path in either.
    ///
    /// Two implementations of one list is how they drift: this is a second
    /// copy of `crucible-daemon/src/agent_cards.rs::card_directories`, and it
    /// is what `cru agents list` answers from, so a divergence means the CLI
    /// advertises cards the daemon will not resolve.
    #[test]
    fn test_collect_agent_directories_excludes_the_kilns_visible_tree() {
        let kiln_path = test_path("test-kiln");
        let config = test_config(kiln_path.clone());
        let dirs = collect_agent_directories(&config);

        assert!(
            !dirs.contains(&kiln_path.join("agents")),
            "the kiln's visible agents/ must not be searched: {dirs:?}"
        );
        assert!(
            !dirs.contains(&kiln_path.join("Agents")),
            "nor its capitalised form: {dirs:?}"
        );
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
        let mut config = test_config(kiln_path.clone());
        config.agent_directories = vec![PathBuf::from("/custom/agents")];
        let dirs = collect_agent_directories(&config);

        let custom_idx = dirs
            .iter()
            .position(|p| p == &PathBuf::from("/custom/agents"))
            .expect("configured dir present");
        let kiln_idx = dirs
            .iter()
            .position(|p| p == &kiln_path.join(".crucible/agents"))
            .expect("kiln config dir present");

        // Later shadows earlier, so the kiln's own cards win over a
        // globally-configured directory.
        assert!(custom_idx < kiln_idx, "{dirs:?}");
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
        let agents_dir = temp_dir.path().join(".crucible").join("agents");
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

    /// A card in the kiln's visible tree is ignored; the one in `.crucible/`
    /// loads.
    ///
    /// This test used to assert the opposite — that `KILN/agents/` overrode
    /// `KILN/.crucible/agents/`. The visible tree is no longer searched: a
    /// kiln is notes, and a cloned one must not be able to introduce an agent
    /// card just by containing a directory.
    #[test]
    fn test_load_agent_registry_ignores_the_kilns_visible_tree() {
        let temp_dir = TempDir::new().unwrap();

        let hidden_dir = temp_dir.path().join(".crucible").join("agents");
        let visible_dir = temp_dir.path().join("agents");
        fs::create_dir_all(&hidden_dir).unwrap();
        fs::create_dir_all(&visible_dir).unwrap();

        fs::write(
            hidden_dir.join("shared-agent.md"),
            "---\nname: \"Shared Agent\"\nversion: \"1.0.0\"\ndescription: \"Configured version\"\n---\n\nConfigured.\n",
        )
        .unwrap();
        fs::write(
            visible_dir.join("shared-agent.md"),
            "---\nname: \"Shared Agent\"\nversion: \"2.0.0\"\ndescription: \"Ambient version\"\n---\n\nAmbient.\n",
        )
        .unwrap();

        let config = test_config(temp_dir.path().to_path_buf());
        let registry = load_agent_registry(&config);

        assert_eq!(registry.count(), 1);
        let agent = registry.get("Shared Agent").unwrap();
        assert_eq!(
            agent.version, "1.0.0",
            "the .crucible/ card must win; the visible one must not be read at all"
        );
    }
}
