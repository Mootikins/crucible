//! GGUF model loading for embedding models
//!
//! This module handles loading embedding models from GGUF format files.
//! Note: Full GGUF tensor reading requires more complex implementation.
//! For now, this provides model discovery and basic metadata extraction.

use super::error::EmbeddingResult;
use std::path::Path;

/// GGUF model metadata
pub struct GGUFModelInfo {
    /// Vocabulary size
    pub vocab_size: usize,
    /// Embedding dimension
    pub embedding_dim: usize,
    /// Model file path
    pub path: std::path::PathBuf,
}

impl GGUFModelInfo {
    /// Try to extract basic info from a GGUF file
    /// Note: Full parsing requires reading the GGUF format which is complex
    pub fn from_file(path: &Path) -> EmbeddingResult<Self> {
        // For now, we'll use default values and let the actual inference
        // determine the dimensions from the model output
        // Full GGUF parsing would require reading the file format properly
        
        // Try to infer from filename or use defaults
        let filename = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        
        // Try to detect dimensions from filename (e.g., "v1.5" -> 768, "v2" -> 768)
        let embedding_dim = if filename.contains("v1.5") || filename.contains("v1_5") {
            768
        } else if filename.contains("v2") {
            768
        } else {
            768 // Default for nomic-embed-text
        };

        Ok(Self {
            vocab_size: 32000, // Default
            embedding_dim,
            path: path.to_path_buf(),
        })
    }

    /// Get embedding dimension
    pub fn embedding_dim(&self) -> usize {
        self.embedding_dim
    }
}


/// Check if a file is a GGUF file
pub fn is_gguf_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("gguf"))
        .unwrap_or(false)
}

/// Find GGUF embedding models in a directory
pub fn find_gguf_models(base_path: &Path, model_name: &str) -> EmbeddingResult<Vec<std::path::PathBuf>> {
    use walkdir::WalkDir;

    let normalized_model = model_name
        .to_lowercase()
        .replace("nomic-ai/", "")
        .replace("nomic_ai_", "")
        .replace("-", "_");

    let mut models = Vec::new();

    for entry in WalkDir::new(base_path).max_depth(4).into_iter().filter_map(|e| e.ok()) {
        if is_gguf_file(entry.path()) {
            let parent = entry.path().parent().unwrap();
            let parent_name = parent.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_lowercase().replace("-", "_"))
                .unwrap_or_default();
            
            let matches = parent_name.contains(&normalized_model)
                || normalized_model.contains(&parent_name)
                || parent_name.contains("embed")
                || entry.path().to_string_lossy().to_lowercase().contains(&normalized_model);

            if matches {
                models.push(entry.path().to_path_buf());
            }
        }
    }

    Ok(models)
}
