use anyhow::Result;
use crucible_lua::LuaExecutor;
use serde_json::json;
use std::path::PathBuf;

use super::HealthArgs;
use crate::config::CliConfig;

struct CheckResult {
    level: String,
    msg: String,
    advice: Vec<String>,
}

fn extract_checks(checks: &mlua::Table) -> Result<Vec<CheckResult>> {
    let len = checks.len()? as usize;
    let mut out = Vec::with_capacity(len);
    for i in 1..=len {
        let check: mlua::Table = checks.get(i)?;
        let level: String = check.get("level")?;
        let msg: String = check.get("msg")?;
        let advice_table: Option<mlua::Table> = check.get("advice").ok();
        let advice = match advice_table {
            Some(tbl) => {
                let alen = tbl.len()? as usize;
                let mut items = Vec::with_capacity(alen);
                for j in 1..=alen {
                    items.push(tbl.get::<String>(j)?);
                }
                items
            }
            None => Vec::new(),
        };
        out.push(CheckResult { level, msg, advice });
    }
    Ok(out)
}

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
            println!("No health.lua found in {}", args.path.display());
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
    let get_results: mlua::Function = lua.load("return cru.health.get_results").eval()?;
    let results: mlua::Table = get_results.call(())?;

    // Extract results
    let name: String = results.get("name")?;
    let healthy: bool = results.get("healthy")?;
    let checks_table: mlua::Table = results.get("checks")?;
    let checks = extract_checks(&checks_table)?;

    if args.json {
        let checks_vec: Vec<_> = checks
            .iter()
            .map(|c| {
                let mut obj = json!({ "level": c.level, "msg": c.msg });
                if !c.advice.is_empty() {
                    obj["advice"] = json!(c.advice);
                }
                obj
            })
            .collect();

        let output = json!({
            "name": name,
            "healthy": healthy,
            "checks": checks_vec,
        });

        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        let plugin_name = args
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        println!("== Health: {} ==", plugin_name);
        println!();

        for c in &checks {
            match c.level.as_str() {
                "ok" => println!("✅ {}", c.msg),
                "warn" => {
                    println!("⚠️  {}", c.msg);
                    for item in &c.advice {
                        println!("   → {}", item);
                    }
                }
                "error" => {
                    println!("❌ {}", c.msg);
                    for item in &c.advice {
                        println!("   → {}", item);
                    }
                }
                "info" => println!("ℹ️  {}", c.msg),
                _ => println!("? {}", c.msg),
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
