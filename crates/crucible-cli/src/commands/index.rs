use anyhow::Result;
use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::embeddings::create_provider;
use crucible_mcp::types::EmbeddingMetadata;
use crate::config::CliConfig;
use glob::glob;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;

pub async fn execute(
    config: CliConfig,
    path: Option<String>,
    force: bool,
    glob_pattern: String,
) -> Result<()> {
    let vault_path = if let Some(p) = path {
        PathBuf::from(p)
    } else {
        config.vault.path.clone()
    };
    
    println!("Indexing vault: {}", vault_path.display());
    println!("Pattern: {}\n", glob_pattern);
    
    // Find all files matching pattern
    let pattern_str = format!("{}/{}", vault_path.display(), glob_pattern);
    let files: Vec<PathBuf> = glob(&pattern_str)?
        .filter_map(Result::ok)
        .collect();
    
    if files.is_empty() {
        println!("No files found matching pattern");
        return Ok(());
    }
    
    println!("Found {} files\n", files.len());
    
    // Create database and embedding provider
    let db = EmbeddingDatabase::new(&config.database_path_str()?).await?;
    let provider = create_provider(config.to_embedding_config()?).await?;
    
    // Progress bar
    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );
    
    let mut indexed = 0;
    let mut skipped = 0;
    let mut errors = 0;
    
    for file_path in files {
        let file_path_str = file_path.to_string_lossy().to_string();
        pb.set_message(file_path.file_name().unwrap().to_string_lossy().to_string());
        
        // Check if file already exists and skip if not forcing
        if !force && db.file_exists(&file_path_str).await? {
            skipped += 1;
            pb.inc(1);
            continue;
        }
        
        // Read file content
        match std::fs::read_to_string(&file_path) {
            Ok(content) => {
                // Generate embedding
                match provider.embed(&content).await {
                    Ok(response) => {
                        // Store in database
                        let now = chrono::Utc::now();
                        let folder = file_path.parent()
                            .and_then(|p| p.to_str())
                            .unwrap_or("")
                            .to_string();
                        let title = file_path.file_stem()
                            .and_then(|s| s.to_str())
                            .map(|s| s.to_string());

                        let metadata = EmbeddingMetadata {
                            file_path: file_path_str.clone(),
                            title,
                            tags: Vec::new(),
                            folder,
                            properties: std::collections::HashMap::new(),
                            created_at: now,
                            updated_at: now,
                        };
                        
                        if let Err(e) = db.store_embedding(
                            &file_path_str,
                            &content,
                            &response.embedding,
                            &metadata,
                        ).await {
                            eprintln!("Error storing {}: {}", file_path_str, e);
                            errors += 1;
                        } else {
                            indexed += 1;
                        }
                    }
                    Err(e) => {
                        eprintln!("Error embedding {}: {}", file_path_str, e);
                        errors += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading {}: {}", file_path_str, e);
                errors += 1;
            }
        }
        
        pb.inc(1);
    }
    
    pb.finish_with_message("Done!");
    
    println!("\nIndexing complete:");
    println!("  Indexed: {}", indexed);
    println!("  Skipped: {}", skipped);
    println!("  Errors:  {}", errors);
    
    Ok(())
}
