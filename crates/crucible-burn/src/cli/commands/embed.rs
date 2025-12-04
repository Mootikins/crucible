use anyhow::Result;
use std::fs;
use tracing::{info, warn};

use crate::cli::EmbedCommand;
use crate::config::BurnConfig;
use crate::hardware::HardwareInfo;
use crate::models::{ModelRegistry, ModelType};

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
    _hardware_info: HardwareInfo,
) -> Result<()> {
    info!("Testing embedding with model: {}, backend: {}", model, backend);

    // Initialize model registry and find the model
    let search_paths = vec![config.model_dir.clone()]
        .into_iter()
        .chain(config.model_search_paths.clone())
        .collect();
    let model_registry = ModelRegistry::new(search_paths).await?;
    let _model_info = model_registry.find_model(&model).await?;

    println!("Embedding Test");
    println!("==============");
    println!("Model: {}", model);
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
    _hardware_info: HardwareInfo,
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
    let search_paths = vec![config.model_dir.clone()]
        .into_iter()
        .chain(config.model_search_paths.clone())
        .collect();
    let model_registry = ModelRegistry::new(search_paths).await?;
    let _model_info = model_registry.find_model(&model).await?;

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
    _config: BurnConfig,
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
    let search_paths = vec![_config.model_dir.clone()]
        .into_iter()
        .chain(_config.model_search_paths.clone())
        .collect();
    let _model_registry = ModelRegistry::new(search_paths).await?;
    let _model_info = _model_registry.find_model(&model).await?;

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

    // Load default configuration
    let config = crate::config::BurnConfig::default();

    // Initialize model registry with all search paths
    let search_paths = vec![config.model_dir.clone()]
        .into_iter()
        .chain(config.model_search_paths.clone())
        .collect();
    let model_registry = ModelRegistry::new(search_paths).await?;

    println!("Available Embedding Models");
    println!("==========================");
    println!();

    let embedding_models = model_registry.list_models(Some(ModelType::Embedding));

    if embedding_models.is_empty() {
        println!("No embedding models found in any search path.");
        println!();
        println!("💡 Default search paths:");
        for path in vec![config.model_dir.clone()].into_iter().chain(config.model_search_paths.clone()) {
            println!("   {:?}", path);
        }
        println!();
        println!("💡 Expected structures:");
        println!("   ~/models/embeddings/           (SafeTensors + config)");
        println!("   ├── nomic-embed-text/");
        println!("   │   ├── model.safetensors");
        println!("   │   ├── config.json");
        println!("   │   └── tokenizer.json");
        println!("   ~/models/language/            (GGUF files)");
        println!("   ├── nomic-ai/");
        println!("   │   └── nomic-embed-text-v1.5-GGUF/");
        println!("   │       └── nomic-embed-text-v1.5.Q8_0.gguf");
        println!();
    } else {
        for model in embedding_models {
            println!("   📁 {} ({})", model.name, model.format);
            println!("   📍 Path: {:?}", model.path);
            if let Some(dim) = model.dimensions {
                println!("   📏 Dimensions: {}", dim);
            }
            if let Some(size) = model.file_size_bytes {
                println!("   💾 Size: {} MB", size / (1024 * 1024));
            }
            println!("   🔧 Complete: {}", if model.is_complete() { "✅" } else { "⚠️  Incomplete" });
            println!();
        }
    }

    Ok(())
}