use anyhow::Result;
use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::types::SearchResultWithScore;
use crate::config::CliConfig;
use crate::interactive::FuzzyPicker;
use crate::output;

pub async fn execute(
    config: CliConfig,
    query: String,
    search_content: bool,
    search_tags: bool,
    search_paths: bool,
    limit: u32,
) -> Result<()> {
    let db = EmbeddingDatabase::new(&config.database_path_str()?).await?;
    
    let mut all_results = Vec::new();
    let mut picker = FuzzyPicker::new();
    
    // Search file paths
    if search_paths {
        let files = db.list_files().await?;

        // If query is empty, return all files with equal score
        let matches = if query.is_empty() {
            files.iter().enumerate().map(|(idx, _)| (idx, 0)).collect::<Vec<_>>()
        } else {
            picker.filter_items(&files, &query)
        };

        for (idx, score) in matches.into_iter().take(limit as usize) {
            if let Some(path) = files.get(idx) {
                // Get content but NOT the embedding vector
                let content = if let Some(data) = db.get_embedding(path).await? {
                    data.content
                } else {
                    String::new()
                };

                all_results.push(SearchResultWithScore {
                    id: path.clone(),
                    title: path.split('/').next_back().unwrap_or(path).to_string(),
                    content,
                    score: score as f64,
                });
            }
        }
    }
    
    // Search content (simple substring search for now)
    if search_content {
        let files = db.list_files().await?;
        for file in files {
            if let Some(data) = db.get_embedding(&file).await? {
                // If query is empty, include all content
                let should_include = if query.is_empty() {
                    true
                } else {
                    data.content.to_lowercase().contains(&query.to_lowercase())
                };

                if should_include {
                    // Simple scoring based on number of matches
                    let matches = if query.is_empty() {
                        1
                    } else {
                        data.content.to_lowercase().matches(&query.to_lowercase()).count()
                    };
                    all_results.push(SearchResultWithScore {
                        id: file.clone(),
                        title: file.split('/').next_back().unwrap_or(&file).to_string(),
                        content: data.content,
                        score: matches as f64 * 100.0,
                    });
                }
            }
        }
    }
    
    // TODO: Add tag searching
    if search_tags {
        // Tags would be stored in metadata
        // For now, skip this until we have metadata indexing
    }
    
    // Sort by score and limit
    all_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    all_results.truncate(limit as usize);

    if all_results.is_empty() {
        if query.is_empty() {
            println!("No indexed files found in database");
        } else {
            println!("No results found for query: {}", query);
        }
        return Ok(());
    }
    
    // Output results
    let output = output::format_search_results(&all_results, "plain", true, true)?;
    println!("{}", output);
    
    Ok(())
}
