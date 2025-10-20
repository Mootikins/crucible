use anyhow::Result;
use crucible_core::database::{Database, SearchOptions};
use crate::config::CliConfig;
use crate::output;
use crate::interactive::SearchResultWithScore;
use indicatif::{ProgressBar, ProgressStyle};

pub async fn execute(
    config: CliConfig,
    query: String,
    top_k: u32,
    format: String,
    show_scores: bool,
) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );

    pb.set_message("Searching vault...");
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    // Search database using core Database interface
    let db = Database::new(&config.database_path_str()?).await?;

    let search_options = SearchOptions {
        limit: Some(top_k),
        offset: Some(0),
        filters: None,
    };

    let search_results = db.search(&query, search_options).await?;
    pb.finish_and_clear();

    if search_results.is_empty() {
        println!("No results found. Make sure your vault is indexed.");
        println!("Run: crucible index");
        return Ok(());
    }

    // Convert to compatibility format
    let results: Vec<SearchResultWithScore> = search_results
        .into_iter()
        .map(|result| SearchResultWithScore {
            id: result.document_id.0,
            title: result.document_id.0.split('/').next_back()
                .unwrap_or(&result.document_id.0)
                .to_string(),
            content: result.snippet.unwrap_or_default(),
            score: result.score,
        })
        .collect();

    // Output results
    let output = output::format_search_results(&results, &format, show_scores, true)?;
    println!("{}", output);

    Ok(())
}
