use anyhow::Result;
use crucible_tools::execute_tool;
use serde_json::json;
use crate::config::CliConfig;
use crate::output;

pub async fn execute(config: CliConfig) -> Result<()> {
    // Initialize and load crucible-tools for simplified statistics
    crucible_tools::init();
    if let Err(e) = crucible_tools::load_all_tools().await {
        return Err(anyhow::anyhow!("Failed to load tools: {}", e));
    }

    // Get vault statistics using tools
    let result = execute_tool(
        "get_vault_stats".to_string(),
        json!({}),
        Some("cli_user".to_string()),
        Some("stats_session".to_string()),
    ).await?;

    let mut stats = std::collections::HashMap::new();

    if let Some(data) = result.data {
        if let Some(total_notes) = data.get("total_notes").and_then(|v| v.as_u64()) {
            stats.insert("total_documents".to_string(), total_notes as i64);
        }
        if let Some(total_size) = data.get("total_size_mb").and_then(|v| v.as_f64()) {
            stats.insert("total_size_mb".to_string(), total_size as i64);
        }
        if let Some(folders) = data.get("folders").and_then(|v| v.as_u64()) {
            stats.insert("total_folders".to_string(), folders as i64);
        }
        if let Some(tags) = data.get("tags").and_then(|v| v.as_u64()) {
            stats.insert("total_tags".to_string(), tags as i64);
        }
    } else {
        // Fallback values
        stats.insert("total_documents".to_string(), 0);
        stats.insert("total_size_mb".to_string(), 0);
        stats.insert("total_folders".to_string(), 0);
        stats.insert("total_tags".to_string(), 0);
    }

    // Add additional statistics that weren't provided by the tool
    if !stats.contains_key("total_documents") {
        stats.insert("total_documents".to_string(), 0);
    }
    stats.insert("indexed_files".to_string(), stats.get("total_documents").copied().unwrap_or(0));
    stats.insert("database_size_mb".to_string(), 0); // TODO: Get actual database size

    println!("📊 Vault Statistics\n");
    let output = output::format_stats(&stats);
    println!("{}", output);

    println!("\n📁 Vault: {}", config.vault.path.display());
    println!("💡 Phase 1.1 Simplification: Complex database statistics have been replaced with tool-based statistics.");

    Ok(())
}
