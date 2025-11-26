use anyhow::Result;
use std::fs;
use std::io::BufRead;
use tracing::{info, debug, warn, error};

use crate::cli::EmbedCommand;
use crate::config::BurnConfig;
use crate::hardware::HardwareInfo;
use crate::models::ModelRegistry;

pub async fn handle(
    command: EmbedCommand,
    config: BurnConfig,
    hardware_info: HardwareInfo,
) -> Result<()> {
    match command {
        EmbedCommand::Test { model, text, backend } => {
            test_embedding(model, text, backend, config, hardware_info).await?;
        }
        EmbedCommand::Batch { model, file, backend } => {
            batch_embedding(model, file, backend, config, hardware_info).await?;
        }
        EmbedCommand::Compare { model, text, iterations } => {
            compare_backends(model, text, iterations, config, hardware_info).await?;
        }
        EmbedCommand::List => {
            list_models().await?;
        }
    }
    Ok(())
}

async fn test_embedding(
    model: String,
    text: String,
    backend: String,
    config: BurnConfig,
    hardware_info: HardwareInfo,
) -> Result<()> {
    info!("Testing embedding with model: {}, backend: {}", model, backend);

    // Initialize model registry and find the model
    let model_registry = ModelRegistry::new(&config.model_dir).await?;
    let model_info = model_registry.find_model(&model).await?;

    println!("Embedding Test");
    println!("==============");
    println!("Model: {}", model_info.name);
    println!("Backend: {}", backend);
    println!("Text: \"{}\"", text);
    println!();

    // TODO: Implement actual embedding inference with Burn
    // For now, just show what would happen
    println!("🔄 Loading model...");
    println!("🔄 Initializing {} backend...", backend);
    println!("🔄 Processing text...");
    println!();

    println!("✅ Embedding test completed successfully");
    println!("   (Actual embedding generation will be implemented with Burn framework)");

    Ok(())
}

async fn batch_embedding(
    model: String,
    file: std::path::PathBuf,
    backend: String,
    config: BurnConfig,
    hardware_info: HardwareInfo,
) -> Result<()> {
    info!("Batch embedding with model: {}, file: {:?}", model, file);

    // Read input file
    let file_content = fs::read_to_string(&file)?;
    let texts: Vec<String> = file_content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .collect();

    if texts.is_empty() {
        warn!("No texts found in input file: {:?}", file);
        return Ok(());
    }

    println!("Batch Embedding");
    println!("================");
    println!("Model: {}", model);
    println!("Backend: {}", backend);
    println!("Input file: {:?}", file);
    println!("Texts to process: {}", texts.len());
    println!();

    // Initialize model registry
    let model_registry = ModelRegistry::new(&config.model_dir).await?;
    let model_info = model_registry.find_model(&model).await?;

    println!("🔄 Loading model...");
    println!("🔄 Processing {} texts...", texts.len());

    // TODO: Implement actual batch embedding with Burn
    for (i, text) in texts.iter().enumerate() {
        println!("  [{}] Processing: \"{}\"...", i + 1, &text[..text.len().min(50).min(40)]);
        // TODO: Actual embedding generation here
    }

    println!();
    println!("✅ Batch embedding completed successfully");
    println!("   (Actual embedding generation will be implemented with Burn framework)");

    Ok(())
}

async fn compare_backends(
    model: String,
    text: String,
    iterations: usize,
    config: BurnConfig,
    hardware_info: HardwareInfo,
) -> Result<()> {
    info!("Comparing backends for model: {}, iterations: {}", model, iterations);

    let backends = if hardware_info.gpus.is_empty() {
        vec!["cpu"]
    } else {
        vec!["vulkan", "rocm", "cpu"]
    };

    println!("Backend Comparison");
    println!("==================");
    println!("Model: {}", model);
    println!("Text: \"{}\"", text);
    println!("Iterations: {}", iterations);
    println!("Backends to test: {}", backends.join(", "));
    println!();

    // Initialize model registry
    let model_registry = ModelRegistry::new(&config.model_dir).await?;
    let model_info = model_registry.find_model(&model).await?;

    for backend in backends {
        println!("Testing {} backend...", backend);

        // TODO: Implement actual backend testing with Burn
        for i in 0..iterations {
            if i % 10 == 0 {
                println!("  Iteration {}/{}", i + 1, iterations);
            }
            // TODO: Actual embedding generation and timing
        }

        println!("  ✅ {} backend test completed", backend);
        println!("     (Actual timing will be implemented with Burn framework)");
        println!();
    }

    println!("✅ Backend comparison completed");

    Ok(())
}

async fn list_models() -> Result<()> {
    info!("Listing available embedding models");

    let models_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
        .join("models")
        .join("embeddings");

    if !models_dir.exists() {
        println!("No embedding models directory found at: {:?}", models_dir);
        println!("💡 Create the directory and add models in subdirectories (e.g., ~/models/embeddings/nomic-embed-text/)");
        return Ok(());
    }

    println!("Available Embedding Models");
    println!("==========================");
    println!();

    let mut found_models = false;

    for entry in fs::read_dir(&models_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let model_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            println!("📁 {}", model_name);

            // Look for model files
            let mut has_safetensors = false;
            let mut has_config = false;
            let mut has_tokenizer = false;

            for file_entry in fs::read_dir(&path)? {
                let file_entry = file_entry?;
                let file_name = file_entry.file_name();
                let file_name_str = file_name.to_string_lossy();

                if file_name_str.ends_with(".safetensors") {
                    has_safetensors = true;
                } else if file_name_str == "config.json" {
                    has_config = true;
                } else if file_name_str == "tokenizer.json" {
                    has_tokenizer = true;
                }
            }

            println!("   📄 Model files: {}{}{}",
                if has_safetensors { "✓ safetensors " } else { "✗ safetensors " },
                if has_config { "✓ config " } else { "✗ config " },
                if has_tokenizer { "✓ tokenizer" } else { "✗ tokenizer" }
            );

            found_models = true;
            println!();
        }
    }

    if !found_models {
        println!("No models found in: {:?}", models_dir);
        println!();
        println!("💡 Expected structure:");
        println!("   ~/models/embeddings/");
        println!("   ├── nomic-embed-text/");
        println!("   │   ├── model.safetensors");
        println!("   │   ├── config.json");
        println!("   │   └── tokenizer.json");
        println!("   └── bge-small-en-v1.5/");
        println!("       ├── model.safetensors");
        println!("       ├── config.json");
        println!("       └── tokenizer.json");
    }

    Ok(())
}