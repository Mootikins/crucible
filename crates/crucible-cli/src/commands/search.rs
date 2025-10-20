use anyhow::Result;
use crucible_core::database::{Database, SearchResult, DocumentId, SearchOptions};
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
    let db = Database::new(&config.database_path_str()?).await?;

    // Get all files from database
    let files = list_all_files(&db).await?;

    if files.is_empty() {
        println!("No files indexed. Run 'crucible index' first.");
        return Ok(());
    }

    let results = if let Some(q) = query {
        // Direct search with query
        search_files(&db, &files, &q, limit).await?
    } else {
        // Interactive picker
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
            if let Some(doc) = db.get_document(&DocumentId(selected.id.clone())).await? {
                println!("{}", doc.content);
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

async fn search_files(
    db: &Database,
    files: &[String],
    query: &str,
    limit: u32,
) -> Result<Vec<SearchResultWithScore>> {
    let mut picker = FuzzyPicker::new();
    let filtered = picker.filter_items(files, query);

    let mut results = Vec::new();
    for (idx, score) in filtered.into_iter().take(limit as usize) {
        if let Some(path) = files.get(idx) {
            let content = if let Some(doc) = db.get_document(&DocumentId(path.clone())).await? {
                doc.content
            } else {
                String::new()
            };

            results.push(SearchResultWithScore {
                id: path.clone(),
                title: path.split('/').next_back().unwrap_or(path).to_string(),
                content,
                score: score as f64,
            });
        }
    }

    Ok(results)
}

/// Helper function to list all files from the database
async fn list_all_files(db: &Database) -> Result<Vec<String>> {
    // For now, use a simple approach. In the future, this could use a proper
    // service layer method to list all documents
    let search_options = SearchOptions {
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
