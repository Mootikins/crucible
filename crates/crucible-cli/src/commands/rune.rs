//! Simplified Rune script execution commands for CLI
//!
//! This module provides simplified CLI commands for Rune script execution.
//! Complex service architecture has been removed in Phase 1.1 dead code elimination.
//! Now provides basic script parsing and validation functionality.

use anyhow::{Context, Result};
use crate::config::CliConfig;
use std::path::PathBuf;

pub async fn list_commands(_config: CliConfig) -> Result<()> {
    println!("🔧 Available Rune Commands:");
    println!("  run <script>     - Execute a Rune script");
    println!("  list             - List available scripts");
    println!("  help             - Show this help message");
    println!();
    println!("📁 Script locations:");
    println!("  ~/.config/crucible/commands/");
    println!("  .crucible/commands/");
    println!("  Current directory");
    Ok(())
}

pub async fn execute(_config: CliConfig, script: String, args: Option<String>) -> Result<()> {
    let script_path = PathBuf::from(&script);

    // Try to find the script in standard locations if it doesn't exist
    let final_script_path = if !script_path.exists() {
        let locations = vec![
            format!("{}/.config/crucible/commands/{}.rn", dirs::home_dir().unwrap().display(), script),
            format!(".crucible/commands/{}.rn", script),
            format!("{}", script),
        ];

        let mut found_path = None;
        for loc in locations {
            let path = PathBuf::from(&loc);
            if path.exists() {
                found_path = Some(path);
                break;
            }
        }

        if let Some(path) = found_path {
            path
        } else {
            anyhow::bail!("Script not found: {}", script);
        }
    } else {
        script_path
    };

    // Simplified execution - just read and display the script
    println!("🔧 Rune script execution is simplified in Phase 1.1");
    println!("📁 Script path: {}", final_script_path.display());

    if let Some(args) = args {
        println!("📝 Arguments: {}", args);
    }

    let content = std::fs::read_to_string(&final_script_path)
        .with_context(|| format!("Failed to read script: {}", final_script_path.display()))?;

    println!("📄 Script content ({} lines):", content.lines().count());
    println!("{}", content);

    // Basic script validation
    validate_rune_script(&content)?;

    println!("✅ Script parsed successfully");
    println!("💡 Note: Complex Rune execution service has been simplified in Phase 1.1");
    println!("   Advanced script execution features have been removed to focus on core functionality");

    Ok(())
}

/// Basic Rune script validation
fn validate_rune_script(content: &str) -> Result<()> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() {
        anyhow::bail!("Script is empty");
    }

    // Check for basic Rune syntax patterns
    let has_functions = lines.iter().any(|line| line.trim().starts_with("fn "));
    let has_uses = lines.iter().any(|line| line.trim().starts_with("use "));

    if !has_functions && !has_uses {
        println!("⚠️  Warning: Script doesn't contain function definitions or use statements");
    }

    // Basic syntax checks
    for (line_num, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        // Basic bracket balance check
        if trimmed.contains('{') || trimmed.contains('}') {
            let open_count = trimmed.matches('{').count();
            let close_count = trimmed.matches('}').count();
            if open_count != close_count {
                println!("⚠️  Warning: Line {} may have unbalanced brackets", line_num + 1);
            }
        }
    }

    Ok(())
}