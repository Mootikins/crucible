//! Skills CLI commands
//!
//! Provides CLI commands for listing, showing, and searching skills.

use anyhow::Result;
use crucible_skills::discovery::{default_discovery_paths, FolderDiscovery};
use std::path::PathBuf;

use crate::cli::SkillsCommands;
use crate::config::CliConfig;

/// Execute skills subcommand
pub async fn execute(config: CliConfig, command: SkillsCommands) -> Result<()> {
    match command {
        SkillsCommands::List { scope } => list(&config, scope).await,
        SkillsCommands::Show { name } => show(&config, name).await,
        SkillsCommands::Search { query, limit } => search(&config, query, limit).await,
    }
}

/// List discovered skills
async fn list(config: &CliConfig, scope_filter: Option<String>) -> Result<()> {
    let paths = default_discovery_paths(Some(&std::env::current_dir()?), Some(&config.kiln_path));

    let discovery = FolderDiscovery::new(paths);
    let skills = discovery.discover()?;

    if skills.is_empty() {
        println!("No skills discovered.");
        println!("\nSkills are searched in:");
        println!("  - ~/.config/crucible/skills/ (personal)");
        println!("  - .claude/skills/, .codex/skills/, etc. (workspace)");
        println!("  - <kiln>/skills/ (kiln-specific)");
        return Ok(());
    }

    println!("Discovered {} skill(s):\n", skills.len());

    for (name, resolved) in &skills {
        let scope = resolved.skill.source.scope.to_string();

        if let Some(ref filter) = scope_filter {
            if &scope != filter {
                continue;
            }
        }

        println!("  {} [{}]", name, scope);
        println!("    {}", resolved.skill.description);
        if !resolved.shadowed.is_empty() {
            println!("    (shadows {} other(s))", resolved.shadowed.len());
        }
        println!();
    }

    Ok(())
}

/// Show skill details
async fn show(config: &CliConfig, name: String) -> Result<()> {
    let paths = default_discovery_paths(Some(&std::env::current_dir()?), Some(&config.kiln_path));

    let discovery = FolderDiscovery::new(paths);
    let skills = discovery.discover()?;

    match skills.get(&name) {
        Some(resolved) => {
            let skill = &resolved.skill;
            println!("Name: {}", skill.name);
            println!("Scope: {}", skill.source.scope);
            println!("Description: {}", skill.description);
            println!("Source: {}", skill.source.path.display());
            if let Some(agent) = &skill.source.agent {
                println!("Agent: {}", agent);
            }
            if let Some(license) = &skill.license {
                println!("License: {}", license);
            }
            println!("\n--- Instructions ---\n");
            println!("{}", skill.body);
        }
        None => {
            println!("Skill not found: {}", name);
            println!("\nAvailable skills:");
            for skill_name in skills.keys() {
                println!("  - {}", skill_name);
            }
        }
    }

    Ok(())
}

/// Search skills (basic text matching)
async fn search(config: &CliConfig, query: String, limit: usize) -> Result<()> {
    println!("Searching for: '{}' (limit: {})", query, limit);

    let paths = default_discovery_paths(Some(&std::env::current_dir()?), Some(&config.kiln_path));

    let discovery = FolderDiscovery::new(paths);
    let skills = discovery.discover()?;

    let query_lower = query.to_lowercase();
    let matches: Vec<_> = skills
        .iter()
        .filter(|(name, resolved)| {
            name.to_lowercase().contains(&query_lower)
                || resolved
                    .skill
                    .description
                    .to_lowercase()
                    .contains(&query_lower)
        })
        .take(limit)
        .collect();

    if matches.is_empty() {
        println!("\nNo skills matched '{}'", query);
    } else {
        println!("\nFound {} matching skill(s):\n", matches.len());
        for (name, resolved) in matches {
            println!("  {} [{}]", name, resolved.skill.source.scope);
            println!("    {}", resolved.skill.description);
            println!();
        }
    }

    Ok(())
}
