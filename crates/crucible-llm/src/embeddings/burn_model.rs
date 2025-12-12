//! Burn model implementation for embedding generation
//!
//! This module implements the actual model loading and inference using Burn framework.
//! It handles loading SafeTensors models and running forward passes.

use super::error::EmbeddingResult;
use safetensors::SafeTensors;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Model configuration loaded from config.json
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModelConfig {
    #[serde(rename = "hidden_size")]
    pub hidden_size: Option<usize>,
    #[serde(rename = "vocab_size")]
    pub vocab_size: Option<usize>,
    #[serde(rename = "num_hidden_layers")]
    pub num_hidden_layers: Option<usize>,
    #[serde(rename = "num_attention_heads")]
    pub num_attention_heads: Option<usize>,
    #[serde(rename = "intermediate_size")]
    pub intermediate_size: Option<usize>,
    #[serde(rename = "max_position_embeddings")]
    pub max_position_embeddings: Option<usize>,
    #[serde(rename = "model_type")]
    pub model_type: Option<String>,
}

impl ModelConfig {
    /// Load model config from a JSON file
    pub fn from_file(path: &Path) -> EmbeddingResult<Self> {
        let content = fs::read_to_string(path).map_err(|e| {
            super::error::EmbeddingError::InferenceFailed(format!(
                "Failed to read config.json: {}",
                e
            ))
        })?;

        serde_json::from_str(&content).map_err(|e| {
            super::error::EmbeddingError::InferenceFailed(format!(
                "Failed to parse config.json: {}",
                e
            ))
        })
    }

    /// Get the embedding dimension (hidden_size)
    pub fn embedding_dim(&self) -> usize {
        self.hidden_size.unwrap_or(768) // Default for BERT-base
    }
}

/// Loaded model weights from SafeTensors
///
/// We store the raw bytes and deserialize on demand to avoid lifetime issues.
pub struct ModelWeights {
    /// Raw model file data (owned)
    data: Vec<u8>,
    /// Model configuration
    pub config: ModelConfig,
}

impl ModelWeights {
    /// Load model weights from a SafeTensors file
    pub fn from_file(model_path: &Path, config_path: Option<&Path>) -> EmbeddingResult<Self> {
        // Load config
        let config = if let Some(config_path) = config_path {
            ModelConfig::from_file(config_path)?
        } else {
            // Try to find config.json in the same directory
            let default_config = model_path.parent().and_then(|p| {
                let config_path = p.join("config.json");
                if config_path.exists() {
                    Some(config_path)
                } else {
                    None
                }
            });

            if let Some(config_path) = default_config {
                ModelConfig::from_file(&config_path)?
            } else {
                // Use default config
                ModelConfig {
                    hidden_size: Some(768),
                    vocab_size: Some(30522),
                    num_hidden_layers: Some(12),
                    num_attention_heads: Some(12),
                    intermediate_size: Some(3072),
                    max_position_embeddings: Some(512),
                    model_type: Some("bert".to_string()),
                }
            }
        };

        // Load SafeTensors file data
        let data = fs::read(model_path).map_err(|e| {
            super::error::EmbeddingError::InferenceFailed(format!(
                "Failed to read model file {}: {}",
                model_path.display(),
                e
            ))
        })?;

        // Validate that it's a valid SafeTensors file
        SafeTensors::deserialize(&data).map_err(|e| {
            super::error::EmbeddingError::InferenceFailed(format!(
                "Failed to parse SafeTensors file: {}",
                e
            ))
        })?;

        Ok(Self { data, config })
    }

    /// Get a tensor by name (returns raw bytes)
    pub fn get_tensor(&self, name: &str) -> Option<Vec<u8>> {
        let tensors = SafeTensors::deserialize(&self.data).ok()?;
        tensors.tensor(name).ok().map(|t| t.data().to_vec())
    }

    /// List all tensor names
    pub fn tensor_names(&self) -> Vec<String> {
        if let Ok(tensors) = SafeTensors::deserialize(&self.data) {
            tensors.names().into_iter().map(|s| s.to_string()).collect()
        } else {
            Vec::new()
        }
    }
}

/// Find model files (config.json and model.safetensors) in a directory
pub fn find_model_files(
    base_path: &Path,
    model_name: &str,
) -> EmbeddingResult<Option<(PathBuf, Option<PathBuf>)>> {
    // Normalize model name for matching (handle variations)
    let normalized_model = model_name
        .to_lowercase()
        .replace("nomic-ai/", "")
        .replace("nomic_ai_", "")
        .replace('-', "_");

    let mut candidates: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();

    for entry in WalkDir::new(base_path)
        .max_depth(4)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let file_name = entry.file_name().to_string_lossy().to_lowercase();

        if file_name.ends_with(".safetensors") {
            let parent = entry.path().parent().unwrap();
            let parent_name = parent
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_lowercase().replace('-', "_"))
                .unwrap_or_default();

            // Check if parent directory name matches model name
            let matches = parent_name.contains(&normalized_model)
                || normalized_model.contains(&parent_name)
                || parent_name.contains("embed")
                || entry
                    .path()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains(&normalized_model);

            if matches {
                // Look for config.json in the same directory
                let config_path = parent.join("config.json");
                let config = if config_path.exists() {
                    Some(config_path)
                } else {
                    None
                };

                candidates.push((entry.path().to_path_buf(), config));
            }
        }
    }

    // Prefer model.safetensors over other .safetensors files
    if let Some(candidate) = candidates
        .iter()
        .find(|(p, _)| p.file_name().and_then(|n| n.to_str()) == Some("model.safetensors"))
    {
        return Ok(Some(candidate.clone()));
    }

    // Otherwise return the first match
    Ok(candidates.into_iter().next())
}
