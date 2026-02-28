//! Skills CLI commands
//!
//! Provides CLI commands for listing, showing, and searching skills.

use anyhow::Result;

use crate::cli::SkillsCommands;
use crate::common::daemon_client;
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
    let client = daemon_client().await?;
    let response = client
        .skills_list(&config.kiln_path, scope_filter.as_deref())
        .await?;

    let skills = response["skills"].as_array().unwrap_or(&vec![]).to_vec();

    if skills.is_empty() {
        println!("No skills discovered.");
        println!("\nSkills are searched in:");
        println!("  - ~/.config/crucible/skills/ (personal)");
        println!("  - .claude/skills/, .codex/skills/, etc. (workspace)");
        println!("  - <kiln>/skills/ (kiln-specific)");
        return Ok(());
    }

    println!("Discovered {} skill(s):\n", skills.len());

    for skill in skills {
        let name = skill["name"].as_str().unwrap_or("unknown");
        let scope = skill["scope"].as_str().unwrap_or("unknown");
        let description = skill["description"].as_str().unwrap_or("");
        let shadowed_count = skill["shadowed_count"].as_u64().unwrap_or(0);

        println!("  {} [{}]", name, scope);
        println!("    {}", description);
        if shadowed_count > 0 {
            println!("    (shadows {} other(s))", shadowed_count);
        }
        println!();
    }

    Ok(())
}

/// Show skill details
async fn show(config: &CliConfig, name: String) -> Result<()> {
    let client = daemon_client().await?;
    let response = client.skills_get(&name, &config.kiln_path).await?;

    if response.is_null() || response.get("name").is_none() {
        println!("Skill not found: {}", name);
        println!("\nAvailable skills:");
        let list_response = client.skills_list(&config.kiln_path, None).await?;
        if let Some(skills) = list_response["skills"].as_array() {
            for skill in skills {
                if let Some(skill_name) = skill["name"].as_str() {
                    println!("  - {}", skill_name);
                }
            }
        }
        return Ok(());
    }

    println!("Name: {}", response["name"].as_str().unwrap_or("unknown"));
    println!("Scope: {}", response["scope"].as_str().unwrap_or("unknown"));
    println!(
        "Description: {}",
        response["description"].as_str().unwrap_or("")
    );
    println!(
        "Source: {}",
        response["source_path"].as_str().unwrap_or("unknown")
    );
    if let Some(agent) = response["agent"].as_str() {
        println!("Agent: {}", agent);
    }
    if let Some(license) = response["license"].as_str() {
        println!("License: {}", license);
    }
    println!("\n--- Instructions ---\n");
    println!("{}", response["body"].as_str().unwrap_or(""));

    Ok(())
}

/// Search skills (basic text matching)
async fn search(config: &CliConfig, query: String, limit: usize) -> Result<()> {
    println!("Searching for: '{}' (limit: {})", query, limit);

    let client = daemon_client().await?;
    let response = client
        .skills_search(&query, &config.kiln_path, Some(limit))
        .await?;

    let matches = response["skills"].as_array().unwrap_or(&vec![]).to_vec();

    if matches.is_empty() {
        println!("\nNo skills matched '{}'", query);
    } else {
        println!("\nFound {} matching skill(s):\n", matches.len());
        for skill in matches {
            let name = skill["name"].as_str().unwrap_or("unknown");
            let scope = skill["scope"].as_str().unwrap_or("unknown");
            let description = skill["description"].as_str().unwrap_or("");
            println!("  {} [{}]", name, scope);
            println!("    {}", description);
            println!();
        }
    }

    Ok(())
}
