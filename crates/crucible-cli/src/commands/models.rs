use crate::config::CliConfig;
use crate::provider_detect::fetch_provider_models;
use anyhow::Result;

pub async fn execute(config: CliConfig) -> Result<()> {
    let effective = config.effective_llm_provider().ok();
    let provider = effective
        .as_ref()
        .map(|p| p.provider_type)
        .unwrap_or(crucible_config::BackendType::Ollama);
    let endpoint = effective
        .map(|p| p.endpoint)
        .unwrap_or_else(|| crucible_config::DEFAULT_OLLAMA_ENDPOINT.to_string());

    eprintln!("Fetching models from {:?} at {}...", provider, endpoint);

    let models = fetch_provider_models(&provider, &endpoint).await;

    if models.is_empty() {
        eprintln!("No models available.");
        eprintln!("\nTroubleshooting:");
        eprintln!("  - Check if the provider is running/accessible");
        eprintln!("  - Verify endpoint in config: cru config show");
        return Ok(());
    }

    println!("\nAvailable models ({}):\n", models.len());
    for model in &models {
        println!("  {}", model);
    }

    println!("\nSwitch model in chat with: :model <name>");
    println!("Or start chat with: cru chat --model <name>");

    Ok(())
}
