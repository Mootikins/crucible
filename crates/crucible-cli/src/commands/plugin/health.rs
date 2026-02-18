use anyhow::Result;
use crucible_lua::LuaExecutor;
use serde_json::json;
use std::path::PathBuf;

use super::HealthArgs;
use crate::config::CliConfig;

pub async fn execute(_config: CliConfig, args: HealthArgs) -> Result<()> {
    // Validate path exists
    if !args.path.exists() {
        eprintln!("Error: Plugin path does not exist: {}", args.path.display());
        std::process::exit(2);
    }

    // Find health.lua in the plugin directory
    let health_path = find_health_lua(&args.path)?;

    // If no health.lua found, exit gracefully
    let health_path = match health_path {
        Some(path) => path,
        None => {
            println!(
                "No health.lua found in {}",
                args.path.display()
            );
            return Ok(());
        }
    };

    // Create Lua runtime
    let executor = LuaExecutor::new()?;
    let lua = executor.lua();

    // Setup test mocks (in case health checks use cru.* APIs)
    lua.load("test_mocks = test_mocks or {}; test_mocks.setup = function() end")
        .exec()?;

    // Load and read health.lua
    let health_lua = std::fs::read_to_string(&health_path)?;

    // Load the health module - it should return {check = function() ... end}
    let health_module: mlua::Table = lua.load(&health_lua).eval()?;

    // Get the check function
    let check_fn: mlua::Function = health_module.get("check")?;

    // Call the check function
    check_fn.call::<()>(())?;

    // Get results from cru.health
    let get_results: mlua::Function = lua
        .load("return cru.health.get_results")
        .eval()?;
    let results: mlua::Table = get_results.call(())?;

    // Extract results
    let name: String = results.get("name")?;
    let healthy: bool = results.get("healthy")?;
    let checks: mlua::Table = results.get("checks")?;

    // If JSON output requested, print JSON and exit
    if args.json {
        let mut checks_vec = Vec::new();
        let checks_len = checks.len()? as usize;
        for i in 1..=checks_len {
            let check: mlua::Table = checks.get(i)?;
            let level: String = check.get("level")?;
            let msg: String = check.get("msg")?;
            let advice: Option<mlua::Table> = check.get("advice").ok();

            let mut check_obj = json!({
                "level": level,
                "msg": msg,
            });

            if let Some(advice_table) = advice {
                let mut advice_vec = Vec::new();
                let advice_len = advice_table.len()? as usize;
                for j in 1..=advice_len {
                    let item: String = advice_table.get(j)?;
                    advice_vec.push(item);
                }
                check_obj["advice"] = json!(advice_vec);
            }

            checks_vec.push(check_obj);
        }

        let output = json!({
            "name": name,
            "healthy": healthy,
            "checks": checks_vec,
        });

        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        // Format and print results
        let plugin_name = args.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        println!("== Health: {} ==", plugin_name);
        println!();

        let checks_len = checks.len()? as usize;
        for i in 1..=checks_len {
            let check: mlua::Table = checks.get(i)?;
            let level: String = check.get("level")?;
            let msg: String = check.get("msg")?;
            let advice: Option<mlua::Table> = check.get("advice").ok();

            match level.as_str() {
                "ok" => println!("✅ {}", msg),
                "warn" => {
                    println!("⚠️  {}", msg);
                    if let Some(advice_table) = advice {
                        let advice_len = advice_table.len()? as usize;
                        for j in 1..=advice_len {
                            let item: String = advice_table.get(j)?;
                            println!("   → {}", item);
                        }
                    }
                }
                "error" => {
                    println!("❌ {}", msg);
                    if let Some(advice_table) = advice {
                        let advice_len = advice_table.len()? as usize;
                        for j in 1..=advice_len {
                            let item: String = advice_table.get(j)?;
                            println!("   → {}", item);
                        }
                    }
                }
                "info" => println!("ℹ️  {}", msg),
                _ => println!("? {}", msg),
            }
        }

        println!();
        println!("Healthy: {}", if healthy { "yes" } else { "no" });
    }

    // Exit with appropriate code
    if healthy {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

/// Find health.lua in the plugin directory
fn find_health_lua(plugin_path: &std::path::Path) -> Result<Option<PathBuf>> {
    // Check if plugin_path itself is health.lua
    if plugin_path.file_name().and_then(|n| n.to_str()) == Some("health.lua") {
        return Ok(Some(plugin_path.to_path_buf()));
    }

    // Check if plugin_path/health.lua exists
    let health_path = plugin_path.join("health.lua");
    if health_path.exists() {
        return Ok(Some(health_path));
    }

    Ok(None)
}
