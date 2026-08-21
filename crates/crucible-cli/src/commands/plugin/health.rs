use anyhow::Result;
use crucible_daemon::{LuaPluginHealthRequest, LuaPluginHealthResponse};
use serde_json::json;

use super::HealthArgs;
use crate::config::CliConfig;

/// The verdict line for a health response.
///
/// The daemon answers `healthy: true` for a plugin that has no `health.lua`,
/// because nothing failed. It also sends a `message` that says so, and it sends
/// one on no other path. Printing the verdict without that message claims a
/// check passed that never ran.
fn verdict(response: &LuaPluginHealthResponse) -> &'static str {
    if response.message.is_some() {
        "not checked"
    } else if response.healthy {
        "yes"
    } else {
        "no"
    }
}

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
            "message": response.message,
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

        if let Some(message) = &response.message {
            println!("ℹ️  {}", message);
        }

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
        println!("Healthy: {}", verdict(&response));
    }

    // Exit with appropriate code
    if healthy {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(healthy: bool, message: Option<&str>) -> LuaPluginHealthResponse {
        LuaPluginHealthResponse {
            name: "plugin".to_string(),
            healthy,
            checks: Vec::new(),
            message: message.map(str::to_string),
        }
    }

    #[test]
    fn a_plugin_without_a_health_file_is_reported_as_unchecked() {
        let response = response(true, Some("No health.lua found"));

        assert_eq!(verdict(&response), "not checked");
    }

    #[test]
    fn a_plugin_that_passed_its_checks_is_reported_as_healthy() {
        assert_eq!(verdict(&response(true, None)), "yes");
    }

    #[test]
    fn a_plugin_that_failed_its_checks_is_reported_as_unhealthy() {
        assert_eq!(verdict(&response(false, None)), "no");
    }
}
