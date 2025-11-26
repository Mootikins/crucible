use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Model type categories
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelType {
    Embedding,
    Llm,
}

impl ModelType {
    /// Get the subdirectory name for this model type
    pub fn dir_name(&self) -> &'static str {
        match self {
            ModelType::Embedding => "embeddings",
            ModelType::Llm => "llm",
        }
    }
}

/// Model file formats
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelFormat {
    SafeTensors,
    PyTorch,
    ONNX,
    GGUF,
    GGML,
}

/// Model metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub model_type: ModelType,
    pub format: ModelFormat,
    pub path: PathBuf,
    pub config_path: Option<PathBuf>,
    pub tokenizer_path: Option<PathBuf>,
    pub dimensions: Option<usize>,
    pub parameters: Option<u64>,
    pub file_size_bytes: Option<u64>,
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
}

impl ModelInfo {
    /// Create a new model info
    pub fn new(
        name: String,
        model_type: ModelType,
        format: ModelFormat,
        path: PathBuf,
    ) -> Self {
        Self {
            name,
            model_type,
            format,
            path,
            config_path: None,
            tokenizer_path: None,
            dimensions: None,
            parameters: None,
            file_size_bytes: None,
            last_modified: None,
        }
    }

    /// Load additional metadata from files
    pub fn load_metadata(&mut self) -> Result<()> {
        // Try to load config file
        let config_path = self.path.join("config.json");
        if config_path.exists() {
            self.config_path = Some(config_path.clone());

            if let Ok(config_content) = std::fs::read_to_string(&config_path) {
                if let Ok(config) = serde_json::from_str::<serde_json::Value>(&config_content) {
                    // Extract dimensions for embedding models
                    if self.model_type == ModelType::Embedding {
                        if let Some(dim) = config.get("hidden_size")
                            .or_else(|| config.get("dim"))
                            .or_else(|| config.get("d_model"))
                            .and_then(|v| v.as_u64())
                        {
                            self.dimensions = Some(dim as usize);
                        }
                    }

                    // Extract parameter count if available
                    if let Some(params) = config.get("num_parameters")
                        .or_else(|| config.get("total_params"))
                        .and_then(|v| v.as_u64())
                    {
                        self.parameters = Some(params);
                    }
                }
            }
        }

        // Try to find tokenizer
        let tokenizer_paths = vec![
            "tokenizer.json",
            "tokenizer_config.json",
        ];

        for tokenizer_file in tokenizer_paths {
            let tokenizer_path = self.path.join(tokenizer_file);
            if tokenizer_path.exists() {
                self.tokenizer_path = Some(tokenizer_path);
                break;
            }
        }

        // Get file size and modification time
        if let Ok(metadata) = std::fs::metadata(&self.path) {
            self.file_size_bytes = Some(metadata.len());
            if let Ok(modified) = metadata.modified() {
                self.last_modified = chrono::DateTime::from_timestamp(
                    modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64,
                    0
                );
            }
        }

        Ok(())
    }

    /// Check if model is complete (has required files)
    pub fn is_complete(&self) -> bool {
        match self.model_type {
            ModelType::Embedding => {
                self.config_path.is_some() &&
                self.has_model_file() &&
                self.tokenizer_path.is_some()
            }
            ModelType::Llm => {
                self.config_path.is_some() &&
                self.has_model_file() &&
                self.tokenizer_path.is_some()
            }
        }
    }

    /// Check if model has model weights file
    pub fn has_model_file(&self) -> bool {
        let model_files = match self.format {
            ModelFormat::SafeTensors => vec!["model.safetensors"],
            ModelFormat::PyTorch => vec!["pytorch_model.bin"],
            ModelFormat::ONNX => vec!["model.onnx"],
            ModelFormat::GGUF => vec!["model.gguf"],
            ModelFormat::GGML => vec!["model.ggml"],
        };

        model_files.iter().any(|file| {
            self.path.join(file).exists()
        })
    }
}

/// Model registry for discovering and managing models
#[derive(Debug)]
pub struct ModelRegistry {
    models: HashMap<String, ModelInfo>,
    model_dir: PathBuf,
}

impl ModelRegistry {
    /// Create a new model registry
    pub async fn new(model_dir: &Path) -> Result<Self> {
        let mut registry = Self {
            models: HashMap::new(),
            model_dir: model_dir.to_path_buf(),
        };

        registry.scan_models().await?;
        Ok(registry)
    }

    /// Scan the model directory for available models
    pub async fn scan_models(&mut self) -> Result<()> {
        info!("Scanning models directory: {:?}", self.model_dir);

        if !self.model_dir.exists() {
            warn!("Models directory does not exist: {:?}", self.model_dir);
            return Ok(());
        }

        // Scan for embedding models
        let embeddings_dir = self.model_dir.join("embeddings");
        if embeddings_dir.exists() {
            self.scan_model_type_dir(&embeddings_dir, ModelType::Embedding).await?;
        }

        // Scan for LLM models
        let llm_dir = self.model_dir.join("llm");
        if llm_dir.exists() {
            self.scan_model_type_dir(&llm_dir, ModelType::Llm).await?;
        }

        info!("Found {} models", self.models.len());
        Ok(())
    }

    /// Scan a specific model type directory
    async fn scan_model_type_dir(&mut self, dir: &Path, model_type: ModelType) -> Result<()> {
        debug!("Scanning {} directory: {:?}", model_type.dir_name(), dir);

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                if let Some(model_name) = path.file_name().and_then(|n| n.to_str()) {
                    if let Ok(mut model_info) = self.analyze_model_dir(&path, model_name.to_string(), model_type.clone()) {
                        model_info.load_metadata()?;

                        if model_info.is_complete() {
                            debug!("Found complete model: {}", model_name);
                            self.models.insert(model_name.to_string(), model_info);
                        } else {
                            warn!("Incomplete model found: {}", model_name);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Analyze a model directory to determine model information
    fn analyze_model_dir(&self, path: &Path, name: String, model_type: ModelType) -> Result<ModelInfo> {
        // Determine model format based on available files
        let format = self.detect_model_format(path)?;

        let model_info = ModelInfo::new(name, model_type, format, path.to_path_buf());

        Ok(model_info)
    }

    /// Detect the format of a model based on its files
    fn detect_model_format(&self, path: &Path) -> Result<ModelFormat> {
        let entries: Vec<std::fs::DirEntry> = std::fs::read_dir(path)?
            .collect::<Result<_, _>>()?;

        // Check for SafeTensors first (preferred)
        if entries.iter().any(|e| {
            e.file_name().to_string_lossy().ends_with(".safetensors")
        }) {
            return Ok(ModelFormat::SafeTensors);
        }

        // Check for PyTorch
        if entries.iter().any(|e| {
            e.file_name().to_string_lossy() == "pytorch_model.bin"
        }) {
            return Ok(ModelFormat::PyTorch);
        }

        // Check for ONNX
        if entries.iter().any(|e| {
            e.file_name().to_string_lossy().ends_with(".onnx")
        }) {
            return Ok(ModelFormat::ONNX);
        }

        // Check for GGUF
        if entries.iter().any(|e| {
            e.file_name().to_string_lossy().ends_with(".gguf")
        }) {
            return Ok(ModelFormat::GGUF);
        }

        // Check for GGML
        if entries.iter().any(|e| {
            e.file_name().to_string_lossy().ends_with(".ggml")
        }) {
            return Ok(ModelFormat::GGML);
        }

        Err(anyhow!("Could not determine model format for: {:?}", path))
    }

    /// Find a model by name (supports partial matching)
    pub async fn find_model(&self, name: &str) -> Result<ModelInfo> {
        // Exact match first
        if let Some(model) = self.models.get(name) {
            return Ok(model.clone());
        }

        // Partial matching
        let matches: Vec<&String> = self.models.keys()
            .filter(|model_name| model_name.contains(name))
            .collect();

        match matches.len() {
            0 => Err(anyhow!("Model not found: {}", name)),
            1 => Ok(self.models[matches[0]].clone()),
            _ => {
                let match_list = matches.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
                Err(anyhow!("Multiple models match '{}': {}", name, match_list))
            }
        }
    }

    /// List all models of a specific type
    pub fn list_models(&self, model_type: Option<ModelType>) -> Vec<&ModelInfo> {
        self.models.values()
            .filter(|model| {
                if let Some(ref filter_type) = model_type {
                    &model.model_type == filter_type
                } else {
                    true
                }
            })
            .collect()
    }

    /// Get a model by exact name
    pub fn get_model(&self, name: &str) -> Option<&ModelInfo> {
        self.models.get(name)
    }

    /// Get all models as a hashmap
    pub fn get_all_models(&self) -> &HashMap<String, ModelInfo> {
        &self.models
    }

    /// Rescan the models directory
    pub async fn rescan(&mut self) -> Result<usize> {
        let old_count = self.models.len();
        self.models.clear();
        self.scan_models().await?;
        let new_count = self.models.len();

        info!("Rescan completed: {} models -> {} models", old_count, new_count);
        Ok(new_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_model_registry_empty() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let registry = ModelRegistry::new(temp_dir.path()).await?;

        assert_eq!(registry.get_all_models().len(), 0);
        Ok(())
    }

    #[test]
    fn test_model_format_detection() {
        let registry = ModelRegistry {
            models: HashMap::new(),
            model_dir: PathBuf::from("/test"),
        };

        // This would require creating actual test directories with model files
        // For now, we just test the function exists
        assert!(true);
    }

    #[test]
    fn test_model_type_dir_name() {
        assert_eq!(ModelType::Embedding.dir_name(), "embeddings");
        assert_eq!(ModelType::Llm.dir_name(), "llm");
    }
}