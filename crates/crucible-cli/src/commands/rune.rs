use anyhow::{Context, Result};
use crate::config::CliConfig;
use crucible_rune::{RuneService, RuneServiceConfig};
use crucible_services::traits::tool::ToolService;
use std::path::PathBuf;
use glob::glob;
use std::sync::Arc;

pub async fn execute(config: CliConfig, script: String, args: Option<String>) -> Result<()> {
    let script_path = PathBuf::from(&script);
    
    if !script_path.exists() {
        // Try to find it in standard locations
        let locations = vec![
            format!("{}/.config/crucible/commands/{}.rn", dirs::home_dir().unwrap().display(), script),
            format!(".crucible/commands/{}.rn", script),
            format!("{}", script),
        ];
        
        for loc in locations {
            let path = PathBuf::from(&loc);
            if path.exists() {
                return execute_script(config, path, args).await;
            }
        }
        
        anyhow::bail!("Script not found: {}", script);
    }
    
    execute_script(config, script_path, args).await
}

async fn execute_script(_config: CliConfig, script_path: PathBuf, args: Option<String>) -> Result<()> {
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
