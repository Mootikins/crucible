use anyhow::Result;

use crate::cli::LlmCommand;
use crate::config::BurnConfig;
use crate::hardware::HardwareInfo;

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
    config: BurnConfig,
    hardware_info: HardwareInfo,
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
    config: BurnConfig,
    hardware_info: HardwareInfo,
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
    println!("Available LLM Models");
    println!("====================");
    println!();

    let models_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?
        .join("models")
        .join("llm");

    if !models_dir.exists() {
        println!("No LLM models directory found at: {:?}", models_dir);
        println!("💡 Create the directory and add models in subdirectories");
        return Ok(());
    }

    let mut found_models = false;

    for entry in std::fs::read_dir(&models_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let model_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            println!("📁 {}", model_name);
            found_models = true;
        }
    }

    if !found_models {
        println!("No LLM models found in: {:?}", models_dir);
    }

    Ok(())
}