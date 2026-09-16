//! Skills CLI commands
//!
//! Provides CLI commands for listing, showing, and searching skills.

use anyhow::Result;
use serde::Serialize;

use crate::cli::SkillsCommands;
use crate::common::daemon_client;
use crate::config::CliConfig;
use crate::formatting::OutputFormat;

#[derive(Debug, Serialize)]
pub struct SkillOutput {
    pub name: String,
    pub scope: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadowed_count: Option<u64>,
}

/// Execute skills subcommand
pub async fn execute(config: CliConfig, command: SkillsCommands) -> Result<()> {
    match command {
        SkillsCommands::List { scope, format } => {
            list(&config, scope, OutputFormat::for_stdout(format)).await
        }
        SkillsCommands::Show { name } => show(&config, name).await,
        SkillsCommands::Search { query, limit } => search(&config, query, limit).await,
    }
}

/// List discovered skills
async fn list(
    config: &CliConfig,
    scope_filter: Option<String>,
    format: OutputFormat,
) -> Result<()> {
    let client = daemon_client().await?;
    let skills = client
        .skills_list(&config.kiln_path, scope_filter.as_deref())
        .await?
        .skills;

    if skills.is_empty() {
        println!("No skills discovered.");
        println!("\nSkills are searched in:");
        println!("  - ~/.config/crucible/skills/ (personal)");
        println!("  - .claude/skills/, .codex/skills/, etc. (workspace)");
        println!("  - <kiln>/skills/ (kiln-specific)");
        return Ok(());
    }

    match format {
        OutputFormat::Json => {
            let output: Vec<SkillOutput> = skills
                .iter()
                .map(|skill| SkillOutput {
                    name: skill.name.clone(),
                    scope: skill.scope.clone(),
                    description: skill.description.clone(),
                    shadowed_count: Some(skill.shadowed_count as u64),
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        OutputFormat::Table => {
            let rows: Vec<Vec<String>> = skills
                .iter()
                .map(|skill| {
                    vec![
                        skill.name.clone(),
                        skill.scope.clone(),
                        skill.description.clone(),
                        if skill.shadowed_count > 0 {
                            skill.shadowed_count.to_string()
                        } else {
                            String::new()
                        },
                    ]
                })
                .collect();
            println!(
                "{}",
                crate::output::records_table(&["Skill", "Scope", "Description", "Shadows"], &rows)
            );
        }
        OutputFormat::Plain => {
            println!("Discovered {} skill(s):\n", skills.len());

            for skill in skills {
                println!("  {} [{}]", skill.name, skill.scope);
                println!("    {}", skill.description);
                if skill.shadowed_count > 0 {
                    println!("    (shadows {} other(s))", skill.shadowed_count);
                }
                println!();
            }
        }
    }

    Ok(())
}

/// Show skill details
async fn show(config: &CliConfig, name: String) -> Result<()> {
    let client = daemon_client().await?;
    // A missing skill surfaces as an RPC error from the daemon, so `?` above
    // is the not-found path — no fallback listing here.
    let skill = client.skills_get(&name, &config.kiln_path).await?;

    println!("Name: {}", skill.name);
    println!("Scope: {}", skill.scope);
    println!("Description: {}", skill.description);
    println!("Source: {}", skill.source_path);
    if let Some(agent) = &skill.agent {
        println!("Agent: {}", agent);
    }
    if let Some(license) = &skill.license {
        println!("License: {}", license);
    }
    println!("\n--- Instructions ---\n");
    println!("{}", skill.body);

    Ok(())
}

/// Search skills (basic text matching)
async fn search(config: &CliConfig, query: String, limit: usize) -> Result<()> {
    println!("Searching for: '{}' (limit: {})", query, limit);

    let client = daemon_client().await?;
    let matches = client
        .skills_search(&query, &config.kiln_path, Some(limit))
        .await?
        .skills;

    if matches.is_empty() {
        println!("\nNo skills matched '{}'", query);
    } else {
        println!("\nFound {} matching skill(s):\n", matches.len());
        for skill in matches {
            println!("  {} [{}]", skill.name, skill.scope);
            println!("    {}", skill.description);
            println!();
        }
    }

    Ok(())
}
