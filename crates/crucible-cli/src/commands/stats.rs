use anyhow::Result;
use crucible_mcp::database::EmbeddingDatabase;
use crate::config::CliConfig;
use crate::output;

pub async fn execute(config: CliConfig) -> Result<()> {
    let db = EmbeddingDatabase::new(&config.database_path_str()?).await?;
    let stats = db.get_stats().await?;
    
    println!("Vault Statistics\n");
    let output = output::format_stats(&stats);
    println!("{}", output);

    println!("\nDatabase: {}", config.database_path().display());
    println!("Vault: {}", config.vault.path.display());
    
    Ok(())
}
