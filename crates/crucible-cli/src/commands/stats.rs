use anyhow::Result;
use crucible_core::database::{Database, SearchOptions};
use crate::config::CliConfig;
use crate::output;

pub async fn execute(config: CliConfig) -> Result<()> {
    let db = Database::new(&config.database_path_str()?).await?;

    // Get basic statistics using search
    let search_options = SearchOptions {
        limit: Some(1), // Just need to know if there are any documents
        offset: Some(0),
        filters: None,
    };

    let total_docs = match db.search("", search_options).await {
        Ok(_) => {
            // For now, we'll do a more comprehensive search to count documents
            let count_options = SearchOptions {
                limit: Some(10000), // Large limit to count all documents
                offset: Some(0),
                filters: None,
            };
            db.search("", count_options).await?.len() as i64
        }
        Err(_) => 0,
    };

    let mut stats = std::collections::HashMap::new();
    stats.insert("total_documents".to_string(), total_docs);
    stats.insert("indexed_files".to_string(), total_docs);
    stats.insert("database_size_mb".to_string(), 0); // TODO: Get actual database size

    println!("Vault Statistics\n");
    let output = output::format_stats(&stats);
    println!("{}", output);

    println!("\nDatabase: {}", config.database_path().display());
    println!("Vault: {}", config.vault.path.display());

    Ok(())
}
