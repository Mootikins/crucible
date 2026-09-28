//! Agent card management commands
//!
//! `cru agents` lists the agent cards and ACP profiles the daemon resolves;
//! `cru agents validate` checks the card files on disk.

use anyhow::{Context, Result};
use crucible_core::agent::{AgentCard, AgentCardLoader};
use crucible_daemon::agent_cards::card_directories;
use crucible_daemon::runtime_path::SourceRoots;
use crucible_daemon::DaemonClient;
use std::path::{Path, PathBuf};

use crate::cli::AgentsCommands;
use crate::common::daemon_client;
use crate::config::CliAppConfig;
use crate::formatting::OutputFormat;

/// Width of the DESCRIPTION column in the `cru agents` table.
const DESCRIPTION_MAX_CHARS: usize = 35;

/// Execute `cru agents`: the list, or the `validate` subcommand.
pub async fn execute(
    config: CliAppConfig,
    tag: Option<String>,
    format: Option<OutputFormat>,
    command: Option<AgentsCommands>,
) -> Result<()> {
    match command {
        None => list(&config, tag, OutputFormat::for_stdout(format)).await,
        Some(AgentsCommands::Validate { verbose }) => validate(&config, verbose).await,
    }
}

/// The agent card directories a session started from `workspace` would
/// search, in precedence order (highest first, first match wins).
///
/// This is the daemon's own list, [`crucible_daemon::agent_cards::card_directories`],
/// built from the same config: the workspace's `.crucible/agents/`, then the
/// kiln's, then `agent_directories`, then global cards. A second
/// implementation is how `cru agents` came to advertise cards the daemon
/// would not resolve. The kiln's visible `agents/` is deliberately not in it;
/// see the daemon function for why.
///
/// `list` asks the daemon (`agents.list_cards`); `validate` reads disk,
/// because it reports per-file errors the daemon does not expose.
pub fn collect_agent_directories(config: &CliAppConfig, workspace: &Path) -> Vec<PathBuf> {
    let roots = source_roots(config, dirs::config_dir(), dirs::home_dir().as_deref());
    card_directories(&roots, workspace, std::slice::from_ref(&config.kiln_path))
}

/// The roots behind [`collect_agent_directories`],
/// read the same way the daemon reads them:
/// [`SourceRoots::from_app_config`] over the config serialized to JSON. A
/// second, struct-field reader here is how the CLI came to build its own
/// `agent_directories` list instead of the daemon's.
///
/// `config_home` and `home` are injected values, never read from `dirs`
/// inside this function; the callers above read them once, and a test passes
/// its own, so it does not see the developer's real
/// `~/.config/crucible/agents`.
fn source_roots(
    config: &CliAppConfig,
    config_home: Option<PathBuf>,
    home: Option<&Path>,
) -> SourceRoots {
    let app_config = serde_json::to_value(config).ok();
    // The daemon names each kiln source by its registered name. The same
    // registry builder, over the same config, gives the CLI the same names.
    SourceRoots::from_app_config_with_registry(
        config_home,
        app_config.as_ref(),
        home,
        crucible_core::config::crucible_home(),
    )
}

/// The workspace `cru agents` answers for: the current directory, which is
/// what `cru chat` started here attaches as the session workspace.
fn current_workspace() -> PathBuf {
    std::env::current_dir().unwrap_or_default()
}

/// An ACP profile as `cru agents` shows it.
///
/// Flattened out of the daemon's `agents.list_profiles` reply at the edge so
/// the rendering below is not four `["x"].as_str().unwrap_or("")` chains.
struct AcpProfile {
    name: String,
    description: String,
    available: bool,
}

/// The `cards` array of an `agents.list_cards` reply, as the shared type.
fn parse_cards_reply(reply: serde_json::Value) -> Option<Vec<AgentCard>> {
    let cards = reply.get("cards")?.clone();
    serde_json::from_value(cards).ok()
}

/// The ACP profiles the daemon knows, or an empty list.
///
/// Best-effort on purpose: a failed profile query means no ACP section.
async fn acp_profiles(client: &DaemonClient) -> Vec<AcpProfile> {
    let Ok(reply) = client.agents_list_profiles().await else {
        return Vec::new();
    };
    reply
        .profiles
        .into_iter()
        .map(|profile| AcpProfile {
            name: profile.name,
            description: profile.description,
            available: profile.available,
        })
        .collect()
}

/// List both things `cru session create` can attach to a session.
///
/// Cards and ACP profiles are different kinds of agent — a card is a persona
/// Crucible runs itself, a profile is an external subprocess — but "what can I
/// talk to?" is one question, and answering half of it was why `--agent` and
/// this command disagreed about what an agent is. Two sections, one command.
async fn list(config: &CliAppConfig, tag: Option<String>, format: OutputFormat) -> Result<()> {
    let client = daemon_client().await?;
    let reply = client
        .agents_list_cards(&current_workspace(), Some(&config.kiln_path))
        .await?;
    let all_cards = parse_cards_reply(reply).context("The daemon reply has no agent cards")?;

    // Get cards, optionally filtered by tag
    let cards: Vec<&AgentCard> = match &tag {
        Some(tag_filter) => all_cards
            .iter()
            .filter(|card| card.tags.iter().any(|t| t == tag_filter))
            .collect(),
        None => all_cards.iter().collect(),
    };

    // A tag filter is a question about cards — profiles have no tags, so
    // showing them all under a filtered heading would misreport them as matches.
    let profiles = match tag {
        Some(_) => Vec::new(),
        None => acp_profiles(&client).await,
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

/// Fit an agent-card description into the `cru agents` table column.
///
/// Truncates by chars, not bytes: descriptions are hand-authored frontmatter, so
/// an em dash or an accent straddling the cut used to abort the whole command.
fn truncate_description(description: &str) -> std::borrow::Cow<'_, str> {
    crucible_oil::truncate_to_chars(description, DESCRIPTION_MAX_CHARS, true)
}

/// Validation result for an agent card file
struct ValidationResult {
    path: PathBuf,
    success: bool,
    error: Option<String>,
    warnings: Vec<String>,
}

/// Validate all agent cards
async fn validate(config: &CliAppConfig, verbose: bool) -> Result<()> {
    let dirs = collect_agent_directories(config, &current_workspace());
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

    /// Cross-platform test path helper
    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("crucible_test_{}", name))
    }

    fn test_config(kiln_path: PathBuf) -> CliAppConfig {
        CliAppConfig {
            kiln_path,
            ..Default::default()
        }
    }

    /// The CLI reads the daemon's `agents.list_cards` reply through the
    /// shared `AgentCard` type, so the wire shape pinned in
    /// `rpc::dispatch::tests::dispatch_agents_list_cards_pins_the_card_json`
    /// is the shape parsed here.
    #[test]
    fn parse_cards_reply_reads_the_daemon_wire_shape() {
        let reply = serde_json::json!({
            "cards": [{
                "id": "9d7a4a3e-3c2f-4a4a-9e2b-0d0f3f5d8b11",
                "name": "alpha",
                "version": "0.1.0",
                "description": "First by name",
                "tags": ["review"],
                "system_prompt": "Help.",
                "mcp_servers": [],
                "config": {},
                "loaded_at": "2026-08-23T00:00:00Z",
            }]
        });
        let cards = parse_cards_reply(reply).expect("cards parse");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].name, "alpha");
        assert_eq!(cards[0].tags, vec!["review".to_string()]);
    }

    /// A reply without `cards` is not a list.
    #[test]
    fn parse_cards_reply_without_cards_is_none() {
        assert!(parse_cards_reply(serde_json::json!({})).is_none());
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

    /// The directories for `config` with a fixed config home and home, so
    /// the test never reads the developer's own.
    fn hermetic_dirs(config: &CliAppConfig) -> Vec<PathBuf> {
        let roots = source_roots(
            config,
            Some(PathBuf::from("/cfg")),
            Some(Path::new("/home/test")),
        );
        card_directories(
            &roots,
            Path::new("/ws"),
            std::slice::from_ref(&config.kiln_path),
        )
    }

    #[test]
    fn test_collect_agent_directories_includes_defaults() {
        let kiln_path = test_path("test-kiln");
        let config = test_config(kiln_path.clone());
        let dirs = hermetic_dirs(&config);

        // Highest-priority first: the personal directory is FIRST, and the
        // kiln's own directory is present below it.
        assert_eq!(
            dirs.first(),
            Some(&PathBuf::from("/cfg/crucible/agents")),
            "{dirs:?}"
        );
        assert!(dirs.contains(&kiln_path.join(".crucible/agents")));
    }

    #[test]
    fn source_roots_expands_a_tilde_against_the_injected_home() {
        let mut config = test_config(test_path("test-kiln"));
        config.agent_directories = vec![PathBuf::from("~/cards")];
        let roots = source_roots(&config, None, Some(Path::new("/home/test")));
        assert_eq!(
            roots.agent_directories,
            vec![PathBuf::from("/home/test/cards")]
        );
        assert_eq!(roots.config_home, None);
    }

    /// The CLI's list is the daemon's — the kiln's visible `agents/` is not
    /// a discovery path, so `cru agents` never advertises a card the
    /// daemon will not resolve.
    #[test]
    fn test_collect_agent_directories_excludes_the_kilns_visible_tree() {
        let kiln_path = test_path("test-kiln");
        let config = test_config(kiln_path.clone());
        let dirs = hermetic_dirs(&config);

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

        let dirs = hermetic_dirs(&config);

        // Should include custom directories
        assert!(dirs.contains(&PathBuf::from("/custom/agents")));
        assert!(dirs.contains(&PathBuf::from("./local-agents")));
    }

    #[test]
    fn test_collect_agent_directories_order() {
        let kiln_path = test_path("test-kiln");
        let mut config = test_config(kiln_path.clone());
        config.agent_directories = vec![PathBuf::from("/custom/agents")];
        let dirs = hermetic_dirs(&config);

        let custom_idx = dirs
            .iter()
            .position(|p| p == &PathBuf::from("/custom/agents"))
            .expect("configured dir present");
        let kiln_idx = dirs
            .iter()
            .position(|p| p == &kiln_path.join(".crucible/agents"))
            .expect("kiln config dir present");

        // Highest priority first. A configured directory is personal, and
        // the personal layer is above the kiln.
        assert!(custom_idx < kiln_idx, "{dirs:?}");
    }

    /// The personal directory is searched first, then the workspace's own
    /// `.crucible/agents/`, as in the daemon.
    #[test]
    fn test_collect_agent_directories_starts_with_the_personal_layer() {
        let kiln_path = test_path("test-kiln");
        let config = test_config(kiln_path);
        let dirs = hermetic_dirs(&config);
        assert_eq!(dirs.first(), Some(&PathBuf::from("/cfg/crucible/agents")));
        assert_eq!(dirs.get(1), Some(&PathBuf::from("/ws/.crucible/agents")));
    }
}
