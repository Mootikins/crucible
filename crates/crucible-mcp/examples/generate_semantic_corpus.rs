// crates/crucible-mcp/examples/generate_semantic_corpus.rs

//! Corpus generation script for semantic test fixtures
//!
//! This example generates real embeddings from the Ollama endpoint
//! and saves them to a JSON file for use in crucible-daemon tests.
//!
//! Usage:
//!   cargo run --example generate_semantic_corpus -- [output_path]
//!
//! Environment variables:
//!   EMBEDDING_MODEL - Model to use (default: nomic-embed-text-v1.5-q8_0)
//!   EMBEDDING_ENDPOINT - Endpoint URL (default: https://llama.krohnos.io)

use anyhow::{Context, Result};
use crucible_mcp::embeddings::{EmbeddingConfig, EmbeddingProvider};
use crucible_mcp::embeddings::ollama::OllamaProvider;
use std::path::PathBuf;
use std::time::Duration;

// Include test fixtures from daemon crate
#[path = "../../crucible-daemon/tests/fixtures/semantic_corpus.rs"]
mod semantic_corpus;

#[path = "../../crucible-daemon/tests/fixtures/corpus_builder.rs"]
mod corpus_builder;

use corpus_builder::build_sample_corpus;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let output_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from("crates/crucible-daemon/tests/fixtures/corpus_v1.json")
    };

    // Get configuration from environment or use defaults
    let model = std::env::var("EMBEDDING_MODEL")
        .unwrap_or_else(|_| "nomic-embed-text-v1.5-q8_0".to_string());
    let endpoint =
        std::env::var("EMBEDDING_ENDPOINT").unwrap_or_else(|_| "https://llama.krohnos.io".to_string());

    println!("🚀 Semantic Corpus Generator");
    println!("   Model: {}", model);
    println!("   Endpoint: {}", endpoint);
    println!("   Output: {}", output_path.display());
    println!();

    // Build corpus structure
    println!("📦 Building corpus structure...");
    let mut corpus = build_sample_corpus();
    println!("   {} documents", corpus.documents.len());
    println!("   {} expectations", corpus.expectations.len());
    println!();

    // Create embedding provider
    println!("🔌 Connecting to embedding provider...");
    let config = EmbeddingConfig::ollama(Some(endpoint.clone()), Some(model.clone()));
    let provider = OllamaProvider::new(config)
        .context("Failed to create embedding provider")?;

    // Test connection
    println!("🏥 Testing connection...");
    provider
        .health_check()
        .await
        .context("Health check failed")?;
    println!("   ✓ Connection successful");
    println!();

    // Generate embeddings for each document
    println!("🧮 Generating embeddings...");
    let total = corpus.documents.len();
    for (i, doc) in corpus.documents.iter_mut().enumerate() {
        let progress = i + 1;
        print!("   [{}/{}] Embedding '{}' ... ", progress, total, doc.id);
        std::io::Write::flush(&mut std::io::stdout())?;

        match provider.embed(&doc.content).await {
            Ok(response) => {
                doc.embedding = Some(response.embedding);
                println!("✓ ({} dims)", response.dimensions);
            }
            Err(e) => {
                println!("✗ FAILED: {}", e);
                anyhow::bail!("Failed to embed document '{}': {}", doc.id, e);
            }
        }

        // Rate limiting - be nice to the server
        if progress < total {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    println!();

    // Update metadata
    corpus.metadata.model = model;
    corpus.metadata.endpoint = endpoint;
    corpus.metadata.generated_at = chrono::Utc::now().to_rfc3339();

    // Validate that all documents have embeddings
    let missing: Vec<String> = corpus
        .documents
        .iter()
        .filter(|d| d.embedding.is_none())
        .map(|d| d.id.clone())
        .collect();

    if !missing.is_empty() {
        anyhow::bail!(
            "Some documents are missing embeddings: {}",
            missing.join(", ")
        );
    }

    // Save to JSON
    println!("💾 Saving corpus to {}...", output_path.display());
    let json = serde_json::to_string_pretty(&corpus).context("Failed to serialize corpus")?;

    // Ensure parent directory exists
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create output directory")?;
    }

    std::fs::write(&output_path, json).context("Failed to write corpus file")?;

    let file_size = std::fs::metadata(&output_path)?.len();
    println!("   ✓ Saved ({} bytes)", file_size);
    println!();

    // Print summary
    println!("✨ Corpus generation complete!");
    println!("   {} documents with embeddings", corpus.documents.len());
    println!("   {} similarity expectations", corpus.expectations.len());
    println!("   {} dimensions per embedding", corpus.metadata.dimensions);
    println!();
    println!("Next steps:");
    println!("   1. Run tests to validate corpus:");
    println!("      cargo test --package crucible-daemon semantic");
    println!("   2. Review the generated file:");
    println!("      cat {}", output_path.display());

    Ok(())
}
