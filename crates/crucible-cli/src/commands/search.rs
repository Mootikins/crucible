use anyhow::Result;
use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::types::SearchResultWithScore;
use crate::config::CliConfig;
use crate::interactive::FuzzyPicker;
use crate::output;

pub async fn execute(
    config: CliConfig,
    query: Option<String>,
    limit: u32,
    format: String,
    show_content: bool,
) -> Result<()> {
    let db = EmbeddingDatabase::new(&config.database_path_str()?).await?;
    
    // Get all files from database
    let files = db.list_files().await?;
    
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
            if let Some(data) = db.get_embedding(&selected.id).await? {
                println!("{}", data.content);
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
    db: &EmbeddingDatabase,
    files: &[String],
    query: &str,
    limit: u32,
) -> Result<Vec<SearchResultWithScore>> {
    let mut picker = FuzzyPicker::new();
    let filtered = picker.filter_items(files, query);
    
    let mut results = Vec::new();
    for (idx, score) in filtered.into_iter().take(limit as usize) {
        if let Some(path) = files.get(idx) {
            let content = if let Some(data) = db.get_embedding(path).await? {
                data.content
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
