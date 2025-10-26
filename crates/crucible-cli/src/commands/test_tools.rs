//! Test commands for verifying tool functionality
//!
//! These commands help test and debug the tool loading system

use crate::config::CliConfig;
use anyhow::Result;

/// Test tool loading and execution
pub async fn execute(_config: CliConfig) -> Result<()> {
    println!("🧪 Testing tool loading and execution...");

    // Initialize crucible-tools
    crucible_tools::init();

    // Test loading all tools
    match crucible_tools::load_all_tools().await {
        Ok(()) => {
            println!("✅ Tools loaded successfully!");

            // List available tools
            let tools = crucible_tools::list_registered_tools().await;
            println!("📋 Available tools ({}):", tools.len());

            for tool in &tools {
                println!("  - {}", tool);
            }

            // Test a simple tool
            if tools.contains(&"system_info".to_string()) {
                println!("\n🔧 Testing system_info tool...");
                match crucible_tools::execute_tool(
                    "system_info".to_string(),
                    serde_json::json!({}),
                    Some("test_user".to_string()),
                    Some("test_session".to_string()),
                )
                .await
                {
                    Ok(result) => {
                        if result.success {
                            println!("✅ system_info tool executed successfully");
                            if let Some(data) = result.data {
                                println!(
                                    "📊 System info: {}",
                                    serde_json::to_string_pretty(&data)?
                                );
                            }
                        } else {
                            println!("❌ system_info tool failed: {:?}", result.error);
                        }
                    }
                    Err(e) => {
                        println!("❌ Error executing system_info tool: {}", e);
                    }
                }
            }

            // Test get_environment tool
            if tools.contains(&"get_environment".to_string()) {
                println!("\n🌍 Testing get_environment tool...");
                match crucible_tools::execute_tool(
                    "get_environment".to_string(),
                    serde_json::json!({}),
                    Some("test_user".to_string()),
                    Some("test_session".to_string()),
                )
                .await
                {
                    Ok(result) => {
                        if result.success {
                            println!("✅ get_environment tool executed successfully");
                            if let Some(data) = result.data {
                                println!(
                                    "📊 Environment info: {}",
                                    serde_json::to_string_pretty(&data)?
                                );
                            }
                        } else {
                            println!("❌ get_environment tool failed: {:?}", result.error);
                        }
                    }
                    Err(e) => {
                        println!("❌ Error executing get_environment tool: {}", e);
                    }
                }
            }
        }
        Err(e) => {
            println!("❌ Failed to load tools: {}", e);
            return Err(anyhow::anyhow!("Tool loading failed: {}", e));
        }
    }

    Ok(())
}
