use anyhow::Result;
use crucible_tools::execute_tool;
use serde_json::json;
use crate::config::CliConfig;
use crate::interactive::{FuzzyPicker, SearchResultWithScore};
use crate::output;

pub async fn execute(
    config: CliConfig,
    query: Option<String>,
    limit: u32,
    format: String,
    show_content: bool,
) -> Result<()> {
    // Initialize crucible-tools for simplified search
    crucible_tools::init();

    let results = if let Some(q) = query {
        // Direct search with query using simplified tools
        search_with_tools(&q, limit).await?
    } else {
        // Interactive picker with available files
        let files = get_available_files().await?;

        if files.is_empty() {
            println!("No files available. Run 'crucible index' first.");
            return Ok(());
        }

        let mut picker = FuzzyPicker::new();
        let filtered_indices = picker.filter_items(&files, "");

        let results: Vec<SearchResultWithScore> = filtered_indices
            .into_iter()
            .take(limit as usize)
            .filter_map(|(idx, score)| {
                files.get(idx).map(|path| SearchResultWithScore {
                    id: path.clone(),
                    title: path.split('/').next_back().unwrap_or(path).to_string(),
                    content: String::new(),
                    score: score as f64,
                })
            })
            .collect();

        if let Some(selection) = picker.pick_result(&results)? {
            let selected = &results[selection];
            println!("\nSelected: {}\n", selected.title);

            // Get document content using tools
            if let Ok(content) = get_document_content(&selected.id).await {
                println!("{}", content);
            }
            return Ok(());
        }
        results
    };

    // Output results
    let output = output::format_search_results(&results, &format, false, show_content)?;
    println!("{}", output);

    Ok(())
}

/// Search using simplified crucible-tools
async fn search_with_tools(query: &str, limit: u32) -> Result<Vec<SearchResultWithScore>> {
    let result = execute_tool(
        "search_documents".to_string(),
        json!({
            "query": query,
            "top_k": limit
        }),
        Some("cli_user".to_string()),
        Some("cli_search".to_string()),
    ).await?;

    if let Some(data) = result.data {
        if let Some(results) = data.get("results").and_then(|r| r.as_array()) {
            let search_results: Vec<SearchResultWithScore> = results
                .iter()
                .enumerate()
                .filter_map(|(idx, item)| {
                    if let (Some(file_path), Some(title), Some(score)) = (
                        item.get("file_path").and_then(|p| p.as_str()),
                        item.get("title").and_then(|t| t.as_str()),
                        item.get("score").and_then(|s| s.as_f64())
                    ) {
                        Some(SearchResultWithScore {
                            id: file_path.to_string(),
                            title: title.to_string(),
                            content: item.get("content")
                                .and_then(|c| c.as_str())
                                .unwrap_or("")
                                .to_string(),
                            score,
                        })
                    } else {
                        None
                    }
                })
                .collect();

            return Ok(search_results);
        }
    }

    // Fallback to empty results if tool fails
    Ok(vec![])
}

/// Get available files using simplified tools
async fn get_available_files() -> Result<Vec<String>> {
    let result = execute_tool(
        "search_by_folder".to_string(),
        json!({
            "path": ".",
            "recursive": true
        }),
        Some("cli_user".to_string()),
        Some("cli_search".to_string()),
    ).await?;

    if let Some(data) = result.data {
        if let Some(files) = data.get("files").and_then(|f| f.as_array()) {
            let file_paths: Vec<String> = files
                .iter()
                .filter_map(|item| {
                    item.get("path").and_then(|p| p.as_str()).map(|s| s.to_string())
                })
                .collect();
            return Ok(file_paths);
        }
    }

    // Fallback to empty list
    Ok(vec![])
}

/// Get document content using simplified tools
async fn get_document_content(path: &str) -> Result<String> {
    let result = execute_tool(
        "search_by_content".to_string(),
        json!({
            "query": path,
            "limit": 1
        }),
        Some("cli_user".to_string()),
        Some("cli_search".to_string()),
    ).await?;

    if let Some(data) = result.data {
        if let Some(results) = data.get("results").and_then(|r| r.as_array()) {
            if let Some(first_result) = results.first() {
                if let Some(content) = first_result.get("content").and_then(|c| c.as_str()) {
                    return Ok(content.to_string());
                }
            }
        }
    }

    // Fallback to empty content
    Ok("".to_string())
}
