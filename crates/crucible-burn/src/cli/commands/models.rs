use anyhow::Result;
use tracing::info;

use crate::cli::ModelsCommand;
use crate::config::BurnConfig;
use crate::hardware::HardwareInfo;
use crate::models::{ModelRegistry, ModelType};

pub async fn handle(
    command: ModelsCommand,
    _config: BurnConfig,
    _hardware_info: HardwareInfo,
) -> Result<()> {
    match command {
        ModelsCommand::List { filter, detailed, by_size } => {
            list_models(filter, detailed, by_size).await?;
        }
        ModelsCommand::Search { query, detailed } => {
            search_models(query, detailed).await?;
        }
    }
    Ok(())
}

async fn list_models(filter: String, detailed: bool, by_size: bool) -> Result<()> {
    info!("Auto-discovering models...");

    // Load default configuration
    let config = BurnConfig::default();

    // Initialize model registry with all search paths
    let search_paths = vec![config.model_dir.clone()]
        .into_iter()
        .chain(config.model_search_paths.clone())
        .collect();
    let model_registry = ModelRegistry::new(search_paths).await?;

    println!("Auto-Discovered Models");
    println!("=====================");
    println!();

    let all_models = model_registry.get_all_models();
    let mut models: Vec<_> = all_models.values().collect();

    // Filter by type if specified
    if filter != "all" {
        let model_type = match filter.as_str() {
            "embedding" => Some(ModelType::Embedding),
            "llm" => Some(ModelType::Llm),
            _ => {
                eprintln!("Invalid filter: {}. Use 'embedding', 'llm', or 'all'", filter);
                return Ok(());
            }
        };

        if let Some(filter_type) = model_type {
            models = models.iter()
                .filter(|model| model.model_type == filter_type)
                .copied()
                .collect();
        }
    }

    // Sort by size if requested
    if by_size {
        models.sort_by(|a, b| {
            let size_a = a.file_size_bytes.unwrap_or(0);
            let size_b = b.file_size_bytes.unwrap_or(0);
            size_b.cmp(&size_a)
        });
    } else {
        models.sort_by(|a, b| a.name.cmp(&b.name));
    }

    if models.is_empty() {
        println!("No models found matching filter: {}", filter);
        println!();
        println!("💡 Default search paths:");
        for path in vec![config.model_dir.clone()].into_iter().chain(config.model_search_paths.clone()) {
            println!("   {:?}", path);
        }
        println!();
        return Ok(());
    }

    // Group models by type
    let mut embedding_models = Vec::new();
    let mut llm_models = Vec::new();

    for model in models.iter() {
        if model.model_type == ModelType::Embedding {
            embedding_models.push(model);
        } else {
            llm_models.push(model);
        }
    }

    // Display LLM models
    if !llm_models.is_empty() && (filter == "all" || filter == "llm") {
        println!("🤖 LLM Models ({})", llm_models.len());
        println!("{}", "=".repeat(50));
        for model in llm_models {
            print_model_info(model, detailed);
        }
        println!();
    }

    // Display Embedding models
    if !embedding_models.is_empty() && (filter == "all" || filter == "embedding") {
        println!("🔍 Embedding Models ({})", embedding_models.len());
        println!("{}", "=".repeat(50));
        for model in embedding_models {
            print_model_info(model, detailed);
        }
        println!();
    }

    // Summary
    let total_size_mb: u64 = all_models.iter()
        .map(|(_, m)| m.file_size_bytes.unwrap_or(0) / (1024 * 1024))
        .sum();

    println!("📊 Summary");
    println!("{}", "-".repeat(50));
    println!("Total models: {}", models.len());
    println!("Total size: {} MB", total_size_mb);

    let formats: std::collections::HashSet<_> = models.iter()
        .map(|m| &m.format)
        .collect();
    println!("Formats: {}", formats.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(", "));

    Ok(())
}

async fn search_models(query: String, detailed: bool) -> Result<()> {
    info!("Searching for models with query: {}", query);

    // Load default configuration
    let config = BurnConfig::default();

    // Initialize model registry with all search paths
    let search_paths = vec![config.model_dir.clone()]
        .into_iter()
        .chain(config.model_search_paths.clone())
        .collect();
    let model_registry = ModelRegistry::new(search_paths).await?;

    let all_models = model_registry.get_all_models();
    let query_lower = query.to_lowercase();

    // Search in model names and paths
    let matching_models: Vec<_> = all_models.iter()
        .filter(|(_, model)| {
            model.name.to_lowercase().contains(&query_lower) ||
            model.path.to_string_lossy().to_lowercase().contains(&query_lower) ||
            model.format.to_string().to_lowercase().contains(&query_lower)
        })
        .map(|(_, model)| model)
        .collect();

    println!("Search Results for: '{}'", query);
    println!("========================");
    println!();

    if matching_models.is_empty() {
        println!("No models found matching '{}'", query);
        println!();
        println!("💡 Try searching with:");
        println!("   - Model names: 'qwen', 'mistral', 'llama'");
        println!("   - Formats: 'gguf', 'safetensors', 'mlx'");
        println!("   - Model types: 'embed', 'instruct'");
        println!();
        return Ok(());
    }

    println!("Found {} matching model(s):", matching_models.len());
    println!();

    for model in matching_models {
        print_model_info(model, detailed);
    }

    Ok(())
}

fn print_model_info(model: &crate::models::ModelInfo, detailed: bool) {
    let size_mb = model.file_size_bytes.map(|b| b / (1024 * 1024)).unwrap_or(0);
    let size_display = if size_mb > 1024 {
        format!("{:.1} GB", size_mb as f64 / 1024.0)
    } else {
        format!("{} MB", size_mb)
    };

    let status = if model.is_complete() { "✅" } else { "⚠️" };
    let type_icon = match model.model_type {
        crate::models::ModelType::Embedding => "🔍",
        crate::models::ModelType::Llm => "🤖",
    };

    println!("{} {} {} ({}) - {} {}",
        type_icon,
        status,
        model.name,
        model.format,
        size_display,
        if detailed { "" } else { "" }
    );

    if detailed {
        println!("   📍 Path: {}", model.path.display());
        if let Some(params) = model.parameters {
            println!("   🔢 Parameters: {}B", params);
        }
        if let Some(dimensions) = model.dimensions {
            println!("   📏 Dimensions: {}", dimensions);
        }
        if let Some(modified) = model.last_modified {
            println!("   📅 Modified: {}", modified.format("%Y-%m-%d %H:%M"));
        }
        println!();
    }
}