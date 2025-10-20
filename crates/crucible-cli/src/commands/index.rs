use anyhow::Result;
use crucible_core::database::{Database, DocumentId, Document};
use crucible_services::LLMService;
use crate::config::CliConfig;
use glob::glob;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::sync::Arc;
use chrono;

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

    // Create database
    let db = Database::new(&config.database_path_str()?).await?;

    // Create a simple embedding service for now
    // TODO: Replace with proper LLMService integration
    let embedding_service = create_simple_embedding_service(&config).await?;

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
        if !force && document_exists(&db, &file_path_str).await? {
            skipped += 1;
            pb.inc(1);
            continue;
        }

        // Read file content
        match std::fs::read_to_string(&file_path) {
            Ok(content) => {
                // Generate embedding
                match embedding_service.embed(&content).await {
                    Ok(embedding) => {
                        // Store in database
                        let now = chrono::Utc::now();
                        let folder = file_path.parent()
                            .and_then(|p| p.to_str())
                            .unwrap_or("")
                            .to_string();
                        let title = file_path.file_stem()
                            .and_then(|s| s.to_str())
                            .map(|s| s.to_string());

                        let document = Document {
                            id: DocumentId(file_path_str.clone()),
                            content,
                            title,
                            folder,
                            tags: Vec::new(),
                            properties: std::collections::HashMap::new(),
                            created_at: now,
                            updated_at: now,
                            embedding: Some(embedding),
                        };

                        if let Err(e) = db.store_document(document).await {
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

/// Simple embedding service for compatibility
/// TODO: Replace with proper LLMService integration
async fn create_simple_embedding_service(_config: &CliConfig) -> Result<Arc<dyn EmbeddingService>> {
    Ok(Arc::new(MockEmbeddingService))
}

/// Check if document exists
async fn document_exists(db: &Database, file_path: &str) -> Result<bool> {
    Ok(db.get_document(&DocumentId(file_path.to_string())).await?.is_some())
}

/// Trait for embedding services
#[async_trait::async_trait]
trait EmbeddingService: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

/// Mock embedding service for testing
struct MockEmbeddingService;

#[async_trait::async_trait]
impl EmbeddingService for MockEmbeddingService {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Create a simple mock embedding based on text hash
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();

        // Create a simple 384-dimensional embedding (common size)
        let mut embedding = Vec::with_capacity(384);
        for i in 0..384 {
            embedding.push(((hash >> (i % 64)) % 1000) as f32 / 1000.0);
        }

        Ok(embedding)
    }
}
