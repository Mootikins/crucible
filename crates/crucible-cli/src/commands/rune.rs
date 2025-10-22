use anyhow::{Context, Result};
use crate::config::CliConfig;
use crucible_tools::{RuneService, RuneServiceConfig, ToolMigrationBridge, MigrationConfig};
use crucible_services::traits::tool::ToolService;
use crucible_services::SecurityLevel;
use std::path::PathBuf;
use glob::glob;
use std::sync::Arc;

pub async fn execute(config: CliConfig, script: String, args: Option<String>) -> Result<()> {
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

    // Try to execute using migration bridge first (if enabled)
    if config.migration.enabled {
        match execute_with_migration_bridge(config.clone(), &final_script_path, args).await {
            Ok(result) => {
                println!("✓ Executed using ScriptEngine service");
                return Ok(result);
            }
            Err(e) => {
                eprintln!("Warning: ScriptEngine execution failed: {}", e);
                eprintln!("Falling back to legacy Rune service...");
                // Fall back to legacy execution
            }
        }
    }

    // Fall back to legacy execution
    execute_script_legacy(config, final_script_path, args).await
}

/// Execute script using migration bridge and ScriptEngine service
async fn execute_with_migration_bridge(
    config: CliConfig,
    script_path: &PathBuf,
    args: Option<String>,
) -> Result<()> {
    println!("Executing with ScriptEngine service: {}", script_path.display());

    // Initialize migration bridge
    let rune_config = RuneServiceConfig::default();
    let migration_config = MigrationConfig {
        auto_migrate: true, // Auto-migrate discovered tools
        security_level: parse_security_level_from_config(&config),
        enable_caching: config.migration.enable_caching,
        max_cache_size: config.migration.max_cache_size,
        preserve_tool_ids: config.migration.preserve_tool_ids,
    };

    let migration_bridge = ToolMigrationBridge::new(rune_config, migration_config).await
        .context("Failed to initialize migration bridge")?;

    // Get the tool name from the script filename
    let tool_name = script_path.file_stem()
        .and_then(|s| s.to_str())
        .context("Invalid script filename")?;

    // Parse arguments
    let args_obj: serde_json::Value = if let Some(a) = args {
        serde_json::from_str(&a)?
    } else {
        serde_json::json!({})
    };

    // Execute the tool using the migration bridge
    let execution_result = migration_bridge.execute_migrated_tool(
        tool_name,
        args_obj,
        None, // Use default execution context
    ).await.context("Failed to execute migrated tool")?;

    // Display results
    println!("\nExecution Results:");
    println!("  Tool: {}", execution_result.tool_name);
    println!("  Success: {}", if execution_result.success {
        "✓"
    } else {
        "✗"
    });

    if let Some(result) = execution_result.result {
        println!("  Result: {}", serde_json::to_string_pretty(&result)?);
    }

    if let Some(error) = execution_result.error {
        println!("  Error: {}", error);
    }

    println!("  Execution time: {:?}", execution_result.execution_time);

    // Display metadata if available
    if let Some(metadata) = execution_result.metadata {
        if let Some(stdout) = metadata.get("stdout") {
            if let Some(stdout_str) = stdout.as_str() {
                if !stdout_str.trim().is_empty() {
                    println!("  Output:");
                    for line in stdout_str.lines() {
                        println!("    {}", line);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Parse security level from configuration
fn parse_security_level_from_config(config: &CliConfig) -> SecurityLevel {
    match config.migration.default_security_level.to_lowercase().as_str() {
        "safe" => SecurityLevel::Safe,
        "development" | "dev" => SecurityLevel::Development,
        "production" | "prod" => SecurityLevel::Production,
        _ => SecurityLevel::Safe,
    }
}

/// Execute script using legacy Rune service
async fn execute_script_legacy(config: CliConfig, script_path: PathBuf, args: Option<String>) -> Result<()> {
    println!("Executing: {}", script_path.display());

    // Parse arguments
    let args_obj: serde_json::Value = if let Some(a) = args {
        serde_json::from_str(&a)?
    } else {
        serde_json::json!({})
    };

    // Create Rune service
    let rune_config = RuneServiceConfig::default();
    let rune_service = RuneService::new(rune_config).await?;
    let tool_dir = script_path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();

    // Discover tools from the directory
    rune_service.discover_tools_from_directory(&tool_dir).await
        .context("Failed to discover Rune tools")?;

    // Get the tool name from the script filename
    let tool_name = script_path.file_stem()
        .and_then(|s| s.to_str())
        .context("Invalid script filename")?;

    // Execute the tool using the service
    use crucible_services::traits::tool::ToolExecutionRequest;
    let execution_request = ToolExecutionRequest {
        tool_name: tool_name.to_string(),
        parameters: args_obj,
        context: Default::default(), // TODO: Add proper context
        timeout_ms: Some(30000),
    };

    let result = rune_service.execute_tool(execution_request).await
        .context("Failed to execute script")?;

    println!("Tool: {}", tool_name);
    println!("Description: {}\n", "Rune script execution");
    println!("Result:");
    println!("{}", serde_json::to_string_pretty(&result.result)?);

    Ok(())
}

pub async fn list_commands(_config: CliConfig) -> Result<()> {
    println!("Available Rune Commands:\n");

    // Search standard locations
    let locations = vec![
        format!("{}/.config/crucible/commands/*.rn", dirs::home_dir().unwrap().display()),
        ".crucible/commands/*.rn".to_string(),
        "crates/crucible-rune/examples/*.rn".to_string(),
    ];

    let mut found_any = false;

    for location in locations {
        if let Ok(entries) = glob(&location) {
            let scripts: Vec<PathBuf> = entries.filter_map(Result::ok).collect();

            if !scripts.is_empty() {
                found_any = true;
                let loc_display = location.split('*').next().unwrap();
                println!("From {}:\n", loc_display);

                for script in scripts {
                    let name = script.file_stem().unwrap().to_string_lossy();
                    println!("  • {}", name);
                    // For now, just show the script name
                    // TODO: Load metadata using RuneService when available
                    println!();
                }
            }
        }
    }

    if !found_any {
        println!("No Rune commands found.");
        println!("\nCreate scripts in:");
        println!("  • ~/.config/crucible/commands/");
        println!("  • .crucible/commands/");
        println!("\nExample scripts may be in:");
        println!("  • crates/crucible-rune/examples/");
    }

    Ok(())
}
