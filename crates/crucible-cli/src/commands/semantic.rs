use anyhow::Result;
use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::embeddings::create_provider;
use crate::config::CliConfig;
use crate::output;
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
    
    pb.set_message("Generating query embedding...");
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    // Create embedding provider
    let provider = create_provider(config.to_embedding_config()?).await?;
    
    // Generate embedding for query
    let response = provider.embed(&query).await?;
    pb.set_message(format!("Searching {} documents...", "vault"));
    
    // Search database
    let db = EmbeddingDatabase::new(&config.database_path_str()?).await?;
    let results = db.search_similar(&response.embedding, top_k).await?;
    
    pb.finish_and_clear();
    
    if results.is_empty() {
        println!("No results found. Make sure your vault is indexed with embeddings.");
        println!("Run: crucible index");
        return Ok(());
    }
    
    // Output results
    let output = output::format_search_results(&results, &format, show_scores, true)?;
    println!("{}", output);
    
    Ok(())
}
