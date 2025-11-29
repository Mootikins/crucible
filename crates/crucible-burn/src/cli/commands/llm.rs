use anyhow::Result;

use crate::cli::LlmCommand;
use crate::config::BurnConfig;
use crate::hardware::HardwareInfo;
use crate::models::{ModelRegistry, ModelType};

pub async fn handle(
    command: LlmCommand,
    config: BurnConfig,
    hardware_info: HardwareInfo,
) -> Result<()> {
    match command {
        LlmCommand::Infer { model, prompt, max_tokens, backend } => {
            llm_inference(model, prompt, max_tokens, backend, config, hardware_info).await?;
        }
        LlmCommand::Stream { model, prompt, max_tokens, backend } => {
            llm_stream(model, prompt, max_tokens, backend, config, hardware_info).await?;
        }
        LlmCommand::List => {
            list_llm_models().await?;
        }
    }
    Ok(())
}

async fn llm_inference(
    model: String,
    prompt: String,
    max_tokens: usize,
    backend: String,
    _config: BurnConfig,
    _hardware_info: HardwareInfo,
) -> Result<()> {
    println!("LLM Inference");
    println!("===============");
    println!("Model: {}", model);
    println!("Backend: {}", backend);
    println!("Max tokens: {}", max_tokens);
    println!("Prompt: \"{}\"", prompt);
    println!();

    println!("🔄 LLM inference will be implemented with Burn framework");
    println!("   (This is a placeholder implementation)");

    Ok(())
}

async fn llm_stream(
    model: String,
    prompt: String,
    max_tokens: usize,
    backend: String,
    _config: BurnConfig,
    _hardware_info: HardwareInfo,
) -> Result<()> {
    println!("LLM Streaming");
    println!("=============");
    println!("Model: {}", model);
    println!("Backend: {}", backend);
    println!("Max tokens: {}", max_tokens);
    println!("Prompt: \"{}\"", prompt);
    println!();

    println!("🔄 LLM streaming will be implemented with Burn framework");
    println!("   (This is a placeholder implementation)");

    Ok(())
}

async fn list_llm_models() -> Result<()> {
    // Load default configuration
    let config = crate::config::BurnConfig::default();

    // Initialize model registry with all search paths
    let search_paths = vec![config.model_dir.clone()]
        .into_iter()
        .chain(config.model_search_paths.clone())
        .collect();
    let model_registry = ModelRegistry::new(search_paths).await?;

    println!("Available LLM Models");
    println!("====================");
    println!();

    let llm_models = model_registry.list_models(Some(ModelType::Llm));

    if llm_models.is_empty() {
        println!("No LLM models found in any search path.");
        println!();
        println!("💡 Default search paths:");
        for path in vec![config.model_dir.clone()].into_iter().chain(config.model_search_paths.clone()) {
            println!("   {:?}", path);
        }
        println!();
    } else {
        for model in llm_models {
            println!("   📁 {} ({})", model.name, model.format);
            println!("   📍 Path: {:?}", model.path);
            if let Some(params) = model.parameters {
                println!("   🔢 Parameters: {}B", params);
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