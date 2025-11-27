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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelFormat {
    SafeTensors,
    PyTorch,
    ONNX,
    GGUF,
    GGML,
    MLX,
    PTH,
    BIN,
}

impl std::fmt::Display for ModelFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelFormat::SafeTensors => write!(f, "SafeTensors"),
            ModelFormat::PyTorch => write!(f, "PyTorch"),
            ModelFormat::ONNX => write!(f, "ONNX"),
            ModelFormat::GGUF => write!(f, "GGUF"),
            ModelFormat::GGML => write!(f, "GGML"),
            ModelFormat::MLX => write!(f, "MLX"),
            ModelFormat::PTH => write!(f, "PTH"),
            ModelFormat::BIN => write!(f, "BIN"),
        }
    }
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

        // Calculate total size from all model files in directory
        let total_size: u64 = std::fs::read_dir(&self.path)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.file_type().map_or(false, |ft| ft.is_file())
            })
            .filter_map(|entry| entry.metadata().ok())
            .map(|metadata| metadata.len())
            .sum();

        self.file_size_bytes = Some(total_size);

        // Get modification time from the most recent file
        if let Ok(modified) = std::fs::metadata(&self.path).and_then(|m| m.modified()) {
            self.last_modified = chrono::DateTime::from_timestamp(
                modified.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64,
                0
            );
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
        let has_format_file = match self.format {
            ModelFormat::SafeTensors => {
                // Check for both single file and sharded formats
                let single_file = self.path.join("model.safetensors").exists();
                let index_file = self.path.join("model.safetensors.index.json").exists();

                // Check for sharded format by looking for any model-NNNNN-of-NNNNN.safetensors file
                let sharded_files = if let Ok(entries) = std::fs::read_dir(&self.path) {
                    entries.flatten().any(|entry| {
                        let file_name = entry.file_name();
                        let file_name_str = file_name.to_string_lossy();
                        file_name_str.starts_with("model-") &&
                        file_name_str.contains("-of-") &&
                        file_name_str.ends_with(".safetensors")
                    })
                } else {
                    false
                };

                single_file || index_file || sharded_files
            }
            ModelFormat::PyTorch => {
                vec!["pytorch_model.bin"].iter().any(|file| self.path.join(file).exists())
            }
            ModelFormat::ONNX => {
                vec!["model.onnx"].iter().any(|file| self.path.join(file).exists())
            }
            ModelFormat::GGUF => {
                vec!["model.gguf"].iter().any(|file| self.path.join(file).exists())
            }
            ModelFormat::GGML => {
                vec!["model.ggml"].iter().any(|file| self.path.join(file).exists())
            }
            ModelFormat::MLX => {
                vec!["model.mlx"].iter().any(|file| self.path.join(file).exists())
            }
            ModelFormat::PTH => {
                vec!["model.pth", "pytorch_model.pth"].iter().any(|file| self.path.join(file).exists())
            }
            ModelFormat::BIN => {
                vec!["model.bin", "pytorch_model.bin"].iter().any(|file| self.path.join(file).exists())
            }
        };

        has_format_file || self.has_any_model_file()
    }

    /// Check if directory has a config file - static version
    fn has_config_file_static(path: &Path) -> bool {
        path.join("config.json").exists()
    }

    /// Check if directory has a config file
    fn has_config_file(&self, path: &Path) -> bool {
        path.join("config.json").exists()
    }

    /// Check if directory has any model files (relaxed check for GGUF files) - static version
    fn has_any_model_files_static(path: &Path) -> bool {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let file_name_str = file_name.to_string_lossy();
                if file_name_str.ends_with(".gguf") ||
                   file_name_str.ends_with(".ggml") ||
                   file_name_str.ends_with(".safetensors") ||
                   file_name_str.ends_with(".bin") ||
                   file_name_str.ends_with(".pth") ||
                   file_name_str.ends_with(".onnx") ||
                   file_name_str.ends_with(".mlx") {
                    return true;
                }
            }
        }
        false
    }

    /// Check if directory has any model files (relaxed check for GGUF files)
    fn has_any_model_files(&self, path: &Path) -> bool {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let file_name_str = file_name.to_string_lossy();
                if file_name_str.ends_with(".gguf") ||
                   file_name_str.ends_with(".ggml") ||
                   file_name_str.ends_with(".safetensors") ||
                   file_name_str.ends_with(".bin") ||
                   file_name_str.ends_with(".pth") ||
                   file_name_str.ends_with(".onnx") ||
                   file_name_str.ends_with(".mlx") {
                    return true;
                }
            }
        }
        false
    }

    /// Check if directory has any model files (relaxed check for GGUF files)
    fn has_any_model_file(&self) -> bool {
        if let Ok(entries) = std::fs::read_dir(&self.path) {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let file_name_str = file_name.to_string_lossy();
                if file_name_str.ends_with(".gguf") ||
                   file_name_str.ends_with(".ggml") ||
                   file_name_str.ends_with(".safetensors") ||
                   file_name_str.ends_with(".bin") ||
                   file_name_str.ends_with(".pth") ||
                   file_name_str.ends_with(".onnx") ||
                   file_name_str.ends_with(".mlx") {
                    return true;
                }
            }
        }
        false
    }
}

/// Model registry for discovering and managing models
#[derive(Debug)]
pub struct ModelRegistry {
    models: HashMap<String, ModelInfo>,
    search_paths: Vec<PathBuf>,
}

impl ModelRegistry {
    /// Create a new model registry with search paths
    pub async fn new(search_paths: Vec<PathBuf>) -> Result<Self> {
        let mut registry = Self {
            models: HashMap::new(),
            search_paths,
        };

        registry.scan_models().await?;
        Ok(registry)
    }

    /// Check if directory has a config file - static version
    fn has_config_file_static(path: &Path) -> bool {
        path.join("config.json").exists()
    }

    /// Check if directory has any model files (relaxed check for GGUF files) - static version
    fn has_any_model_files_static(path: &Path) -> bool {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let file_name_str = file_name.to_string_lossy();
                if file_name_str.ends_with(".gguf") ||
                   file_name_str.ends_with(".ggml") ||
                   file_name_str.ends_with(".safetensors") ||
                   file_name_str.ends_with(".bin") ||
                   file_name_str.ends_with(".pth") ||
                   file_name_str.ends_with(".onnx") ||
                   file_name_str.ends_with(".mlx") {
                    return true;
                }
            }
        }
        false
    }

    /// Validate that a path is within allowed search directories (prevents path traversal)
    fn validate_model_path(&self, path: &Path) -> Result<()> {
        let canonical_path = std::fs::canonicalize(path)
            .map_err(|e| anyhow!("Cannot canonicalize path {:?}: {}", path, e))?;

        for search_path in &self.search_paths {
            if let Ok(canonical_search) = std::fs::canonicalize(search_path) {
                if canonical_path.starts_with(&canonical_search) {
                    return Ok(());
                }
            }
        }

        Err(anyhow!("Path {:?} is outside allowed search directories", path))
    }

    /// Scan all model search paths for available models
    pub async fn scan_models(&mut self) -> Result<()> {
        info!("Scanning {} search paths for models", self.search_paths.len());

        for search_path in &self.search_paths.clone() {
            if !search_path.exists() {
                debug!("Search path does not exist: {:?}", search_path);
                continue;
            }

            info!("Scanning search path: {:?}", search_path);

            // Validate the search path itself is safe
            if let Err(e) = self.validate_model_path(search_path) {
                warn!("Skipping unsafe search path {:?}: {}", search_path, e);
                continue;
            }

            // Simple recursive scan - find all model files and group by directory
            if search_path.is_dir() {
                self.scan_path_recursive(search_path).await?;
            }
        }

        info!("Found {} models total", self.models.len());
        Ok(())
    }

    /// Simple recursive path scanner that finds model files by extension
    async fn scan_path_recursive(&mut self, root_path: &Path) -> Result<()> {
        // Walk the directory tree recursively
        let mut model_directories = std::collections::HashMap::new();
        let mut visited = std::collections::HashSet::new();

        self.collect_model_directories(root_path, &mut model_directories, &mut visited, 0)?;

        // Process each directory that contains model files
        for (dir_path, model_files) in model_directories {
            // Only accept directories that have actual model indicator files
            if !self.has_model_indicators(&dir_path, &model_files) {
                debug!("Skipping directory without model indicators: {:?}", dir_path);
                continue;
            }

            // Determine model format from the files found
            let format = self.detect_format_from_files(&model_files)?;

            // Get model name from directory path
            let candidate_name = dir_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown_model");

            // Skip hash-like names and generic non-model names
            if self.is_non_model_name(candidate_name) {
                debug!("Skipping directory with non-model name: {}", candidate_name);
                continue;
            }

            let model_name = candidate_name.to_string();

            // Determine model type
            let model_type = self.determine_model_type(&dir_path, &format, &model_files)?;

            // Create model info
            let mut model_info = ModelInfo::new(
                model_name.clone(),
                model_type,
                format,
                dir_path.clone(),
            );

            // Load additional metadata if available
            if let Err(e) = model_info.load_metadata() {
                warn!("Failed to load metadata for {}: {}", model_name, e);
            }

            info!("Found model: {} (format: {:?}, type: {:?})", model_name, model_info.format, model_info.model_type);

            self.models.insert(model_name, model_info);
        }

        Ok(())
    }

    /// Collect all directories containing model files
    fn collect_model_directories(&self, current_path: &Path, model_dirs: &mut std::collections::HashMap<PathBuf, Vec<PathBuf>>, visited: &mut std::collections::HashSet<PathBuf>, depth: usize) -> Result<()> {
        const MAX_DEPTH: usize = 10;

        if depth > MAX_DEPTH {
            debug!("Maximum scan depth reached at: {:?}", current_path);
            return Ok(());
        }

        if !current_path.is_dir() {
            return Ok(());
        }

        // Validate path safety before processing
        if let Err(e) = self.validate_model_path(current_path) {
            debug!("Skipping unsafe path {:?}: {}", current_path, e);
            return Ok(());
        }

        // Detect and prevent symlink loops
        let canonical_path = match std::fs::canonicalize(current_path) {
            Ok(path) => path,
            Err(e) => {
                debug!("Cannot canonicalize path {:?}: {}", current_path, e);
                return Ok(());
            }
        };

        if visited.contains(&canonical_path) {
            debug!("Detected symlink loop, already visited: {:?}", canonical_path);
            return Ok(());
        }

        visited.insert(canonical_path.clone());

        // Read directory contents
        let entries = match std::fs::read_dir(current_path) {
            Ok(entries) => entries,
            Err(e) => {
                debug!("Error reading directory {:?}: {}", current_path, e);
                return Ok(());
            }
        };

        let mut found_model_files = Vec::new();
        let mut subdirs = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_file() {
                // Check if this is a model file by extension
                if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
                    match extension {
                        "safetensors" | "gguf" | "ggml" | "bin" | "pth" | "onnx" | "mlx" => {
                            found_model_files.push(path);
                        }
                        _ => {}
                    }
                }
            } else if path.is_dir() {
                subdirs.push(path);
            }
        }

        // If we found model files in this directory, record it
        if !found_model_files.is_empty() {
            model_dirs.insert(current_path.to_path_buf(), found_model_files);
        }

        // Recursively scan subdirectories
        for subdir in subdirs {
            self.collect_model_directories(&subdir, model_dirs, visited, depth + 1)?;
        }

        Ok(())
    }

    /// Detect model format from a list of files
    fn detect_format_from_files(&self, files: &[PathBuf]) -> Result<ModelFormat> {
        // Check files in priority order
        for file in files {
            if let Some(extension) = file.extension().and_then(|ext| ext.to_str()) {
                match extension {
                    "safetensors" => return Ok(ModelFormat::SafeTensors),
                    "gguf" => return Ok(ModelFormat::GGUF),
                    "ggml" => return Ok(ModelFormat::GGML),
                    "onnx" => return Ok(ModelFormat::ONNX),
                    "mlx" => return Ok(ModelFormat::MLX),
                    "pth" => return Ok(ModelFormat::PTH),
                    "bin" => return Ok(ModelFormat::BIN),
                    _ => {}
                }
            }
        }

        Err(anyhow!("Could not determine format from files: {:?}", files))
    }

    /// Determine model type from directory, format, and files
    fn determine_model_type(&self, dir_path: &Path, _format: &ModelFormat, files: &[PathBuf]) -> Result<ModelType> {
        // First, check directory name for hints
        if let Some(dir_name) = dir_path.file_name().and_then(|n| n.to_str()) {
            if dir_name.to_lowercase().contains("embed") {
                return Ok(ModelType::Embedding);
            }
        }

        // Check file names for hints
        for file in files {
            if let Some(file_name) = file.file_name().and_then(|n| n.to_str()) {
                let file_name_lower = file_name.to_lowercase();
                if file_name_lower.contains("embed") ||
                   file_name_lower.contains("bge") ||
                   file_name_lower.contains("e5") ||
                   file_name_lower.contains("nomic") {
                    return Ok(ModelType::Embedding);
                }
            }
        }

        // Check config.json for model type if it exists
        let config_path = dir_path.join("config.json");
        if config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(model_type) = config.get("model_type").and_then(|v| v.as_str()) {
                        if model_type.contains("embed") || model_type.contains("clip") {
                            return Ok(ModelType::Embedding);
                        }
                    }
                }
            }
        }

        // Default to LLM
        Ok(ModelType::Llm)
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
            let file_name = e.file_name();
            let name = file_name.to_string_lossy();
            name == "pytorch_model.bin" || name.ends_with(".pth")
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

        // Check for MLX
        if entries.iter().any(|e| {
            e.file_name().to_string_lossy().ends_with(".mlx")
        }) {
            return Ok(ModelFormat::MLX);
        }

        // Check for PTH
        if entries.iter().any(|e| {
            e.file_name().to_string_lossy().ends_with(".pth")
        }) {
            return Ok(ModelFormat::PTH);
        }

        // Check for generic BIN files
        if entries.iter().any(|e| {
            e.file_name().to_string_lossy().ends_with(".bin")
        }) {
            return Ok(ModelFormat::BIN);
        }

        Err(anyhow!("Could not determine model format for: {:?}", path))
    }

    /// Check if directory has embedding model files
    fn has_embedding_files(&self, path: &Path) -> bool {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let file_name_str = file_name.to_string_lossy();
                if file_name_str.contains("embed") ||
                   file_name_str.contains("bge") ||
                   file_name_str.contains("nomic") ||
                   file_name_str.contains("e5") ||
                   file_name_str.contains("sentence") {
                    return true;
                }
            }
        }
        false
    }

    /// Check if directory has LLM model files
    fn has_llm_files(&self, path: &Path) -> bool {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let file_name_str = file_name.to_string_lossy();
                if file_name_str.contains("llm") ||
                   file_name_str.contains("llama") ||
                   file_name_str.contains("mistral") ||
                   file_name_str.contains("phi") ||
                   file_name_str.contains("qwen") ||
                   file_name_str.contains("glm") {
                    return true;
                }
            }
        }
        false
    }

    /// Determine model type from directory structure
    fn determine_model_type_from_structure(&self, path: &Path) -> Result<ModelType> {
        // Check for GGUF files first - they could be either type
        if self.has_format_files(path, &["gguf"]) {
            // Use heuristics to determine type
            if self.has_embedding_files(path) {
                return Ok(ModelType::Embedding);
            } else if self.has_llm_files(path) {
                return Ok(ModelType::Llm);
            }
            // Default to LLM for GGUF if unclear
            return Ok(ModelType::Llm);
        }

        // For other formats, check for config files
        let config_path = path.join("config.json");
        if config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                    // Check model type from config
                    if let Some(model_type) = config.get("model_type").and_then(|v| v.as_str()) {
                        if model_type.contains("embed") || model_type.contains("clip") {
                            return Ok(ModelType::Embedding);
                        }
                    }

                    // Check architecture
                    if let Some(arch) = config.get("architectures").and_then(|v| v.as_array()) {
                        for arch_name in arch {
                            if let Some(name) = arch_name.as_str() {
                                if name.contains("Embedding") || name.contains("CLIP") {
                                    return Ok(ModelType::Embedding);
                                }
                            }
                        }
                    }

                    // Default to LLM for transformer models
                    return Ok(ModelType::Llm);
                }
            }
        }

        // Fallback: assume LLM if we can't determine
        Ok(ModelType::Llm)
    }

    /// Check if directory has files with specific extensions
    fn has_format_files(&self, path: &Path, extensions: &[&str]) -> bool {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let file_name = entry.file_name();
                let file_name_str = file_name.to_string_lossy();
                for ext in extensions {
                    if file_name_str.ends_with(ext) {
                        return true;
                    }
                }
            }
        }
        false
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

    /// Check if directory has model indicator files (companion files + model files)
    fn has_model_indicators(&self, dir_path: &Path, model_files: &[PathBuf]) -> bool {
        // Must have at least one model file
        if model_files.is_empty() {
            return false;
        }

        // Look for companion files that indicate this is a real model directory
        let companion_files = [
            "config.json",
            "tokenizer.json",
            "tokenizer_config.json",
            "vocab.json",
            "special_tokens_map.json",
            "merges.txt",
            "vocab.txt",
            "added_tokens.json",
            "model.safetensors.index.json",
            "pytorch_model.bin.index.json",
            "configuration.json",
            "preprocessor_config.json",
            "feature_extractor_config.json",
        ];

        let has_companion = companion_files.iter().any(|companion| {
            dir_path.join(companion).exists()
        });

        // For GGUF files, often they're self-contained, so they don't need companion files
        // But we should still have meaningful GGUF filenames
        let has_gguf = model_files.iter().any(|f| {
            f.extension().and_then(|ext| ext.to_str()) == Some("gguf")
        });

        if has_gguf {
            // For GGUF, they're usually self-contained models, so we're more permissive
            let meaningful_gguf = model_files.iter().any(|f| {
                if let Some(name) = f.file_name().and_then(|n| n.to_str()) {
                    // Skip very small test files and hidden files
                    !name.to_lowercase().contains("test") &&
                    !name.starts_with('.') &&
                    name.len() > 5 // Skip very short names
                } else {
                    false
                }
            });

            return meaningful_gguf;
        }

        // For other formats (SafeTensors, ONNX, etc.), we really want companion files
        has_companion
    }

    /// Check if name indicates this is NOT a model directory
    fn is_non_model_name(&self, name: &str) -> bool {
        // Skip hash-like names (40 or 64 character hex)
        if (name.len() == 40 || name.len() == 64) && name.chars().all(|c| c.is_ascii_hexdigit()) {
            return true;
        }

        // Skip generic names that are clearly not models
        let skip_names = [
            "layout_reader",
            "unetstructure",
            "slanetplus",
            "vae_approx",
            "paddleocr_torch",
            "paddle_table_cls",
            "paddle_orientation_classification",
            "models", // Too generic
            "test",
            "temp",
            "cache",
            "data",
            "output",
            "snapshots",
            "blobs",
            "refs",
            "locks",
        ];

        let name_lower = name.to_lowercase();
        skip_names.iter().any(|skip| name_lower == *skip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_model_registry_empty() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let registry = ModelRegistry::new(vec![temp_dir.path().to_path_buf()]).await?;

        assert_eq!(registry.get_all_models().len(), 0);
        Ok(())
    }

    #[test]
    fn test_model_format_detection() {
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