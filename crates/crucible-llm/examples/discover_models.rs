//! Example: Discover local GGUF models
//!
//! This example demonstrates how to use the ModelDiscovery system to find and catalog
//! local GGUF models. It scans configured directories and displays information about
//! discovered models.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example discover_models
//! ```

use crucible_llm::model_discovery::{DiscoveryConfig, ModelDiscovery};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for debug output
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🔍 GGUF Model Discovery Example\n");

    // Configure discovery settings
    let config = DiscoveryConfig {
        // Custom paths to search (in addition to common locations)
        custom_paths: vec![
            PathBuf::from("~/models"),
            // Add more paths as needed
        ],
        // Also search common locations like ~/.ollama/models, ~/.cache/huggingface/hub
        search_common_locations: true,
        // Maximum directory depth to traverse
        max_depth: 5,
        // Cache results for 5 minutes
        cache_ttl_seconds: 300,
    };

    // Create discovery instance
    let discovery = ModelDiscovery::new(config);

    // Discover all models
    println!("Scanning for GGUF models...\n");
    let models = discovery.discover_models().await?;

    if models.is_empty() {
        println!("❌ No GGUF models found.");
        println!("   Make sure you have GGUF models in one of these locations:");
        println!("   - ~/models/");
        println!("   - ~/.ollama/models/");
        println!("   - ~/.cache/huggingface/hub/");
        return Ok(());
    }

    println!("✅ Found {} models:\n", models.len());

    // Display all discovered models
    for (idx, model) in models.iter().enumerate() {
        println!("{}. {}", idx + 1, model.name);
        println!("   Path: {}", model.path.display());
        println!("   Model type: {:?}", model.model_type);

        if let Some(arch) = &model.architecture {
            println!("   Architecture: {}", arch);
        }

        if let Some(dims) = model.dimensions {
            println!("   Dimensions: {}", dims);
        }

        if let Some(params) = model.parameter_count {
            println!("   Parameters: {}", format_param_count(params));
        }

        if let Some(quant) = &model.quantization {
            println!("   Quantization: {}", quant);
        }

        println!();
    }

    // Filter by capability
    println!("\n📊 Models by Capability:\n");

    let embedding_models = discovery.get_embedding_models().await?;
    println!("Embedding Models: {}", embedding_models.len());
    for model in &embedding_models {
        println!("  - {} ({})", model.name, model.path.display());
    }

    let text_gen_models = discovery.get_text_generation_models().await?;
    println!("\nText Generation Models: {}", text_gen_models.len());
    for model in &text_gen_models {
        println!("  - {} ({})", model.name, model.path.display());
    }

    // Demonstrate cache usage
    println!("\n🔄 Testing cache (should be instant)...");
    let start = std::time::Instant::now();
    let cached_models = discovery.discover_models().await?;
    let elapsed = start.elapsed();
    println!(
        "Found {} models in {:.2}ms (from cache)",
        cached_models.len(),
        elapsed.as_secs_f64() * 1000.0
    );

    // Demonstrate cache invalidation
    println!("\n♻️  Invalidating cache...");
    discovery.invalidate_cache().await;
    println!("Cache cleared. Next discovery will rescan filesystem.");

    Ok(())
}

/// Format parameter count in a human-readable way
fn format_param_count(count: u64) -> String {
    const BILLION: u64 = 1_000_000_000;
    const MILLION: u64 = 1_000_000;

    if count >= BILLION {
        format!("{:.1}B", count as f64 / BILLION as f64)
    } else if count >= MILLION {
        format!("{:.1}M", count as f64 / MILLION as f64)
    } else {
        count.to_string()
    }
}
