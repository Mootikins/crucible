use crucible_mcp::EmbeddingConfig;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = EmbeddingConfig::from_env()?;

    println!("Provider: {:?}", config.provider);
    println!("Model: {}", config.model);
    println!("Endpoint: {}", config.endpoint);

    Ok(())
}