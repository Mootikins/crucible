use anyhow::Result;
use crucible_daemon::LuaPluginHealthRequest;
use serde_json::json;

use super::HealthArgs;
use crate::config::CliConfig;

pub async fn execute(_config: CliConfig, args: HealthArgs) -> Result<()> {
    // Validate path exists
    if !args.path.exists() {
        eprintln!("Error: Plugin path does not exist: {}", args.path.display());
        std::process::exit(2);
    }

    // Connect to daemon
    let client = crate::common::daemon_client().await?;

    // Run health check via daemon RPC
    let response = client
        .lua_plugin_health(LuaPluginHealthRequest {
            plugin_path: args.path.to_string_lossy().to_string(),
        })
        .await?;

    let name = &response.name;
    let healthy = response.healthy;

    if args.json {
        let output = json!({
            "name": name,
            "healthy": healthy,
            "checks": response.checks,
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

        for check in &response.checks {
            let level = check.get("level").and_then(|v| v.as_str()).unwrap_or("?");
            let msg = check.get("msg").and_then(|v| v.as_str()).unwrap_or("");
            let advice: Vec<&str> = check
                .get("advice")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|item| item.as_str()).collect())
                .unwrap_or_default();

            match level {
                "ok" => println!("✅ {}", msg),
                "warn" => {
                    println!("⚠️  {}", msg);
                    for item in &advice {
                        println!("   → {}", item);
                    }
                }
                "error" => {
                    println!("❌ {}", msg);
                    for item in &advice {
                        println!("   → {}", item);
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
