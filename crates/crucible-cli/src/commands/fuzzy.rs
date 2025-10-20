use anyhow::Result;
use crucible_core::database::{Database, DocumentId};
use crate::config::CliConfig;
use crate::interactive::{FuzzyPicker, SearchResultWithScore};
use crate::output;

pub async fn execute(
    config: CliConfig,
    query: String,
    search_content: bool,
    search_tags: bool,
    search_paths: bool,
    limit: u32,
) -> Result<()> {
    let db = Database::new(&config.database_path_str()?).await?;

    let mut all_results = Vec::new();
    let mut picker = FuzzyPicker::new();

    // Get all files first
    let files = list_all_files(&db).await?;

    // Search file paths
    if search_paths {
        // If query is empty, return all files with equal score
        let matches = if query.is_empty() {
            files.iter().enumerate().map(|(idx, _)| (idx, 0)).collect::<Vec<_>>()
        } else {
            picker.filter_items(&files, &query)
        };

        for (idx, score) in matches.into_iter().take(limit as usize) {
            if let Some(path) = files.get(idx) {
                // Get content
                let content = if let Some(doc) = db.get_document(&DocumentId(path.clone())).await? {
                    doc.content
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
        for file in &files {
            if let Some(doc) = db.get_document(&DocumentId(file.clone())).await? {
                // If query is empty, include all content
                let should_include = if query.is_empty() {
                    true
                } else {
                    doc.content.to_lowercase().contains(&query.to_lowercase())
                };

                if should_include {
                    // Simple scoring based on number of matches
                    let matches = if query.is_empty() {
                        1
                    } else {
                        doc.content.to_lowercase().matches(&query.to_lowercase()).count()
                    };
                    all_results.push(SearchResultWithScore {
                        id: file.clone(),
                        title: file.split('/').next_back().unwrap_or(file).to_string(),
                        content: doc.content,
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

/// Helper function to list all files from the database
async fn list_all_files(db: &Database) -> Result<Vec<String>> {
    // For now, use a simple approach. In the future, this could use a proper
    // service layer method to list all documents
    let search_options = crucible_core::database::SearchOptions {
        limit: Some(10000), // Large limit to get all files
        offset: Some(0),
        filters: None,
    };

    // Search with empty query to get all documents
    let search_results = db.search("", search_options).await?;
    let files = search_results
        .into_iter()
        .map(|result| result.document_id.0)
        .collect();

    Ok(files)
}
