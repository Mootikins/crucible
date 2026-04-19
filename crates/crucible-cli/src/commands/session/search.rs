use super::helpers::truncate;
use super::io::{list_session_dirs, sessions_dir};
use crate::common::daemon_client;
use crate::config::CliConfig;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use tokio::fs;

pub(super) async fn search(
    config: CliConfig,
    query: String,
    limit: u32,
    format: String,
) -> Result<()> {
    if let Ok(client) = daemon_client().await {
        if let Ok(result) = client
            .session_search(&query, Some(&config.kiln_path), Some(limit as usize))
            .await
        {
            let matches = result
                .get("matches")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if matches.is_empty() {
                if format == "json" {
                    println!("{}", serde_json::json!({"matches": []}));
                } else {
                    println!("No sessions matching '{}' found.", query);
                }
            } else if format == "json" {
                println!("{}", serde_json::json!({"matches": matches}));
            } else {
                println!("Sessions matching '{}':\n", query);
                for m in &matches {
                    let session_id = m["session_id"].as_str().unwrap_or("");
                    let line = m["line"].as_u64().unwrap_or(0);
                    let context = m["context"].as_str().unwrap_or("");
                    println!("  {} (line {})", session_id, line);
                    println!("    {}\n", context);
                }
            }
            return Ok(());
        }
    }

    let sessions_path = sessions_dir(&config);
    if !sessions_path.exists() {
        if format == "json" {
            println!("{}", serde_json::json!({"matches": []}));
        } else {
            println!("No sessions found.");
        }
        return Ok(());
    }
    let matches = match search_with_ripgrep(&sessions_path, &query, limit).await {
        Ok(results) => results,
        Err(e) => {
            tracing::debug!(
                "Ripgrep search failed ({}), falling back to in-memory scan",
                e
            );
            search_in_memory(&sessions_path, &query, limit).await?
        }
    };
    if matches.is_empty() {
        if format == "json" {
            println!("{}", serde_json::json!({"matches": []}));
        } else {
            println!("No sessions matching '{}' found.", query);
        }
        return Ok(());
    }
    if format == "json" {
        let json_matches: Vec<serde_json::Value> = matches
            .iter()
            .map(|(session_id, line_num, context)| {
                serde_json::json!({
                    "session_id": session_id,
                    "line": line_num,
                    "context": context
                })
            })
            .collect();
        println!("{}", serde_json::json!({"matches": json_matches}));
    } else {
        println!("Sessions matching '{}':\n", query);
        for (session_id, line_num, context) in matches {
            println!("  {} (line {})", session_id, line_num);
            println!("    {}\n", context);
        }
    }
    Ok(())
}

pub(super) async fn search_with_ripgrep(
    sessions_path: &PathBuf,
    query: &str,
    limit: u32,
) -> Result<Vec<(String, usize, String)>> {
    use std::process::Command;

    let rg_check = Command::new("rg").arg("--version").output();
    if rg_check.is_err() {
        return Err(anyhow!("ripgrep not found"));
    }

    let output = Command::new("rg")
        .arg("--json")
        .arg("--max-count")
        .arg(limit.to_string())
        .arg("--context")
        .arg("2")
        .arg("--glob")
        .arg("*.jsonl")
        .arg(query)
        .arg(sessions_path)
        .output()
        .map_err(|e| anyhow!("Failed to run ripgrep: {}", e))?;

    if !output.status.success() {
        // Exit code 1 means no matches, not a failure
        if output.status.code() == Some(1) {
            return Ok(vec![]);
        }
        return Err(anyhow!("ripgrep failed with status: {}", output.status));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();

    for line in stdout.lines() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            if json["type"] == "match" {
                if let Some(data) = json["data"].as_object() {
                    let path = data["path"]["text"].as_str().unwrap_or("");
                    let session_id = extract_session_id_from_path(path);
                    let line_num = data["line_number"].as_u64().unwrap_or(0) as usize;
                    let content = data["lines"]["text"]
                        .as_str()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let content = truncate(&content, 100);

                    results.push((session_id, line_num, content));

                    if results.len() >= limit as usize {
                        break;
                    }
                }
            }
        }
    }

    Ok(results)
}

pub(super) async fn search_in_memory(
    sessions_path: &Path,
    query: &str,
    limit: u32,
) -> Result<Vec<(String, usize, String)>> {
    let ids = list_session_dirs(sessions_path).await?;
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    for id in ids {
        let jsonl_path = sessions_path.join(id.as_str()).join("session.jsonl");
        if !jsonl_path.exists() {
            continue;
        }

        let content = match fs::read_to_string(&jsonl_path).await {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (line_num, line) in content.lines().enumerate() {
            if line.to_lowercase().contains(&query_lower) {
                results.push((id.to_string(), line_num + 1, truncate(line, 100)));

                if results.len() >= limit as usize {
                    return Ok(results);
                }
            }
        }
    }

    Ok(results)
}

pub(super) fn extract_session_id_from_path(path: &str) -> String {
    // Path format: .../sessions/{session_id}/session.jsonl
    std::path::Path::new(path)
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}
