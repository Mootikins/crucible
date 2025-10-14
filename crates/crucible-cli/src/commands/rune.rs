use anyhow::{Context, Result};
use crate::config::CliConfig;
use crucible_mcp::rune_tools::ToolRegistry;
use std::path::PathBuf;
use glob::glob;

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

    // Create Rune context
    let context = rune::Context::with_default_modules()?;
    let context_arc = std::sync::Arc::new(context);
    let tool_dir = script_path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();

    // Create registry and load script
    let mut registry = ToolRegistry::new(tool_dir, context_arc.clone())?;
    let tool_name = registry.load_tool(&script_path)
        .context("Failed to load Rune script")?;

    let tool = registry.get_tool(&tool_name)
        .context("Tool not found after loading")?;

    println!("Tool: {}", tool.metadata().name);
    println!("Description: {}\n", tool.metadata().description);

    // Execute the script
    let result = tool.call(args_obj, &context_arc).await
        .context("Failed to execute script")?;

    println!("Result:");
    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}

pub async fn list_commands(_config: CliConfig) -> Result<()> {
    println!("Available Rune Commands:\n");

    // Search standard locations
    let locations = vec![
        format!("{}/.config/crucible/commands/*.rn", dirs::home_dir().unwrap().display()),
        ".crucible/commands/*.rn".to_string(),
        "crates/crucible-mcp/tools/examples/*.rn".to_string(),
    ];

    let mut found_any = false;
    let context = std::sync::Arc::new(rune::Context::with_default_modules()?);

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

                    // Try to load metadata
                    let tool_dir = script.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
                    if let Ok(mut registry) = ToolRegistry::new(tool_dir, context.clone()) {
                        if let Ok(tool_name) = registry.load_tool(&script) {
                            if let Some(tool) = registry.get_tool(&tool_name) {
                                let meta = tool.metadata();
                                println!("    {}", meta.description);
                            }
                        }
                    }
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
        println!("\nExample scripts are in:");
        println!("  • crates/crucible-mcp/tools/examples/");
    }

    Ok(())
}
