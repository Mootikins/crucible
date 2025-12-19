//! Local GGUF model discovery system
//!
//! This module provides functionality to discover, catalog, and cache information about
//! local GGUF models. It scans configured directories, extracts metadata from GGUF files,
//! and categorizes models by their capabilities.
//!
//! # Features
//!
//! - Automatic discovery of GGUF files in user-configured directories
//! - Metadata extraction from GGUF file headers
//! - Model capability classification (embedding vs text generation)
//! - TTL-based caching to minimize filesystem scans
//! - Support for common model directory structures
//!
//! # Example
//!
//! ```rust,no_run
//! use crucible_llm::model_discovery::{ModelDiscovery, DiscoveryConfig};
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = DiscoveryConfig {
//!         custom_paths: vec![PathBuf::from("~/models")],
//!         search_common_locations: true,
//!         max_depth: 5,
//!         cache_ttl_seconds: 300,
//!     };
//!
//!     let discovery = ModelDiscovery::new(config);
//!     let models = discovery.discover_models().await?;
//!
//!     for model in models {
//!         println!("Found: {} ({:?})", model.name, model.capability);
//!     }
//!
//!     Ok(())
//! }
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use walkdir::WalkDir;

/// Model capability types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(Default)]
pub enum ModelCapability {
    /// Embedding model (produces vector representations)
    Embedding,
    /// Text generation model (produces text completions)
    TextGeneration,
    /// Multi-modal or unknown capability
    #[default]
    Unknown,
}

/// Information about a discovered GGUF model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredModel {
    /// Model name (derived from path or metadata)
    pub name: String,
    /// Full path to the GGUF file
    pub path: PathBuf,
    /// Model capability
    pub capability: ModelCapability,
    /// Embedding dimensions (if applicable)
    pub dimensions: Option<usize>,
    /// Model architecture (e.g., "llama", "bert", "nomic-bert")
    pub architecture: Option<String>,
    /// Parameter count (if available in metadata)
    pub parameter_count: Option<u64>,
    /// Quantization type (e.g., "Q8_0", "F16")
    pub quantization: Option<String>,
    /// When this model was discovered
    pub discovered_at: SystemTime,
}

/// Configuration for model discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Custom paths to search for models
    pub custom_paths: Vec<PathBuf>,
    /// Whether to search common locations (~/models, ~/.ollama/models, etc.)
    pub search_common_locations: bool,
    /// Maximum directory depth to traverse
    pub max_depth: usize,
    /// Cache time-to-live in seconds
    pub cache_ttl_seconds: u64,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            custom_paths: Vec::new(),
            search_common_locations: true,
            max_depth: 5,
            cache_ttl_seconds: 300, // 5 minutes
        }
    }
}

/// Cached discovery results
#[derive(Debug, Clone)]
struct CachedDiscovery {
    models: Vec<DiscoveredModel>,
    cached_at: SystemTime,
    ttl: Duration,
}

impl CachedDiscovery {
    fn is_valid(&self) -> bool {
        if let Ok(elapsed) = self.cached_at.elapsed() {
            elapsed < self.ttl
        } else {
            false
        }
    }
}

/// Model discovery system for local GGUF files
pub struct ModelDiscovery {
    config: DiscoveryConfig,
    cache: Arc<RwLock<Option<CachedDiscovery>>>,
}

impl ModelDiscovery {
    /// Create a new model discovery instance with the given configuration
    pub fn new(config: DiscoveryConfig) -> Self {
        Self {
            config,
            cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Get all search paths based on configuration
    fn get_search_paths(&self) -> Vec<PathBuf> {
        let mut paths = self.config.custom_paths.clone();

        if self.config.search_common_locations {
            // Add common model locations
            if let Some(home) = dirs::home_dir() {
                paths.push(home.join("models"));
                paths.push(home.join(".ollama").join("models"));
                paths.push(home.join(".cache").join("huggingface").join("hub"));
            }
        }

        // Expand tilde and resolve paths
        paths
            .into_iter()
            .filter_map(|p| {
                let path_str = p.to_string_lossy();
                shellexpand::tilde(&path_str)
                    .parse::<PathBuf>()
                    .ok()
                    .filter(|expanded| expanded.exists())
            })
            .collect()
    }

    /// Discover all GGUF models in configured directories
    pub async fn discover_models(&self) -> Result<Vec<DiscoveredModel>> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.as_ref() {
                if cached.is_valid() {
                    tracing::debug!("Using cached model discovery results");
                    return Ok(cached.models.clone());
                }
            }
        }

        // Perform discovery
        let models = self.discover_models_uncached().await?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            *cache = Some(CachedDiscovery {
                models: models.clone(),
                cached_at: SystemTime::now(),
                ttl: Duration::from_secs(self.config.cache_ttl_seconds),
            });
        }

        Ok(models)
    }

    /// Discover models without using cache
    async fn discover_models_uncached(&self) -> Result<Vec<DiscoveredModel>> {
        let search_paths = self.get_search_paths();
        let mut models = Vec::new();

        tracing::info!("Scanning {} paths for GGUF models", search_paths.len());

        for base_path in search_paths {
            tracing::debug!("Scanning directory: {}", base_path.display());

            let discovered = self.scan_directory(&base_path).await?;
            models.extend(discovered);
        }

        tracing::info!("Discovered {} GGUF models", models.len());

        Ok(models)
    }

    /// Scan a single directory for GGUF files
    async fn scan_directory(&self, path: &Path) -> Result<Vec<DiscoveredModel>> {
        let mut models = Vec::new();

        // Use walkdir for traversal
        for entry in WalkDir::new(path)
            .max_depth(self.config.max_depth)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let entry_path = entry.path();

            // Check if it's a GGUF file
            if !self.is_gguf_file(entry_path) {
                continue;
            }

            // Try to extract model information
            match self.extract_model_info(entry_path).await {
                Ok(model) => {
                    tracing::debug!(
                        "Discovered model: {} at {}",
                        model.name,
                        model.path.display()
                    );
                    models.push(model);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to extract info from {}: {}",
                        entry_path.display(),
                        e
                    );
                }
            }
        }

        Ok(models)
    }

    /// Check if a file is a GGUF file
    fn is_gguf_file(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("gguf"))
            .unwrap_or(false)
    }

    /// Extract model information from a GGUF file
    async fn extract_model_info(&self, path: &Path) -> Result<DiscoveredModel> {
        // Read GGUF file and extract metadata
        // The gguf crate requires a byte slice, not a file handle
        let file_bytes = std::fs::read(path)
            .with_context(|| format!("Failed to read GGUF file: {}", path.display()))?;

        let gguf_file = gguf::GGUFFile::read(&file_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to parse GGUF file: {}", e))?
            .ok_or_else(|| anyhow::anyhow!("Incomplete GGUF file"))?;

        // Extract metadata
        let metadata = self.parse_metadata(&gguf_file)?;

        // Derive model name from path
        let name = self.derive_model_name(path, &metadata);

        Ok(DiscoveredModel {
            name,
            path: path.to_path_buf(),
            capability: metadata.capability,
            dimensions: metadata.dimensions,
            architecture: metadata.architecture,
            parameter_count: metadata.parameter_count,
            quantization: metadata.quantization,
            discovered_at: SystemTime::now(),
        })
    }

    /// Parse GGUF metadata to extract model information
    fn parse_metadata(&self, gguf_file: &gguf::GGUFFile) -> Result<ModelMetadata> {
        let mut metadata = ModelMetadata::default();

        // Build metadata map for easier access
        // GGUFMetadata is a struct with key and value fields
        let metadata_map: HashMap<String, &gguf::GGUFMetadataValue> = gguf_file
            .header
            .metadata
            .iter()
            .map(|m| (m.key.clone(), &m.value))
            .collect();

        // Extract architecture
        if let Some(arch) = metadata_map.get("general.architecture") {
            if let gguf::GGUFMetadataValue::String(s) = arch {
                metadata.architecture = Some(s.clone());
            }
        }

        // Determine capability based on architecture
        metadata.capability = self.classify_capability(&metadata.architecture);

        // Extract embedding dimensions
        if let Some(dim) = metadata_map
            .get("bert.embedding_length")
            .or_else(|| metadata_map.get("llama.embedding_length"))
            .or_else(|| metadata_map.get("nomic-bert.embedding_length"))
        {
            if let gguf::GGUFMetadataValue::Uint32(d) = dim {
                metadata.dimensions = Some(*d as usize);
            }
        }

        // Extract parameter count
        if let Some(params) = metadata_map.get("general.parameter_count") {
            if let gguf::GGUFMetadataValue::Uint64(p) = params {
                metadata.parameter_count = Some(*p);
            }
        }

        // Extract quantization from file type or infer from filename
        if let Some(file_type) = metadata_map.get("general.file_type") {
            if let gguf::GGUFMetadataValue::Uint32(ft) = file_type {
                metadata.quantization = Some(self.file_type_to_quantization(*ft));
            }
        }

        Ok(metadata)
    }

    /// Classify model capability based on architecture
    fn classify_capability(&self, architecture: &Option<String>) -> ModelCapability {
        match architecture.as_deref() {
            Some(arch) => {
                let arch_lower = arch.to_lowercase();
                if arch_lower.contains("bert")
                    || arch_lower.contains("embed")
                    || arch_lower.contains("nomic")
                {
                    ModelCapability::Embedding
                } else if arch_lower.contains("llama")
                    || arch_lower.contains("gpt")
                    || arch_lower.contains("mistral")
                {
                    ModelCapability::TextGeneration
                } else {
                    ModelCapability::Unknown
                }
            }
            None => ModelCapability::Unknown,
        }
    }

    /// Convert GGUF file type to quantization string
    fn file_type_to_quantization(&self, file_type: u32) -> String {
        match file_type {
            0 => "F32".to_string(),
            1 => "F16".to_string(),
            2 => "Q4_0".to_string(),
            3 => "Q4_1".to_string(),
            6 => "Q5_0".to_string(),
            7 => "Q5_1".to_string(),
            8 => "Q8_0".to_string(),
            9 => "Q8_1".to_string(),
            10 => "Q2_K".to_string(),
            11 => "Q3_K_S".to_string(),
            12 => "Q3_K_M".to_string(),
            13 => "Q3_K_L".to_string(),
            14 => "Q4_K_S".to_string(),
            15 => "Q4_K_M".to_string(),
            16 => "Q5_K_S".to_string(),
            17 => "Q5_K_M".to_string(),
            18 => "Q6_K".to_string(),
            _ => format!("UNKNOWN_{}", file_type),
        }
    }

    /// Derive a user-friendly model name from the file path
    fn derive_model_name(&self, path: &Path, metadata: &ModelMetadata) -> String {
        // Try to use metadata name if available
        if let Some(arch) = &metadata.architecture {
            // Get the filename without extension
            if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                return format!("{}/{}", arch, file_stem);
            }
        }

        // Fallback to deriving from path structure
        // Expected structure: ~/models/{type}/{publisher}/{model-slug}/*.gguf
        let components: Vec<&str> = path
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect();

        if components.len() >= 3 {
            // Try to extract publisher/model-slug
            let len = components.len();
            if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                return format!(
                    "{}/{}/{}",
                    components[len - 3],
                    components[len - 2],
                    file_stem
                );
            }
        }

        // Ultimate fallback: just the filename
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// Invalidate the cache, forcing a fresh discovery on next call
    pub async fn invalidate_cache(&self) {
        let mut cache = self.cache.write().await;
        *cache = None;
    }

    /// Get models filtered by capability
    pub async fn get_models_by_capability(
        &self,
        capability: ModelCapability,
    ) -> Result<Vec<DiscoveredModel>> {
        let all_models = self.discover_models().await?;
        Ok(all_models
            .into_iter()
            .filter(|m| m.capability == capability)
            .collect())
    }

    /// Get embedding models only
    pub async fn get_embedding_models(&self) -> Result<Vec<DiscoveredModel>> {
        self.get_models_by_capability(ModelCapability::Embedding)
            .await
    }

    /// Get text generation models only
    pub async fn get_text_generation_models(&self) -> Result<Vec<DiscoveredModel>> {
        self.get_models_by_capability(ModelCapability::TextGeneration)
            .await
    }
}

/// Internal struct for collecting metadata during parsing
#[derive(Debug, Default)]
struct ModelMetadata {
    capability: ModelCapability,
    dimensions: Option<usize>,
    architecture: Option<String>,
    parameter_count: Option<u64>,
    quantization: Option<String>,
}


#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_discovery_config_defaults() {
        let config = DiscoveryConfig::default();
        assert!(config.search_common_locations);
        assert_eq!(config.max_depth, 5);
        assert_eq!(config.cache_ttl_seconds, 300);
    }

    #[tokio::test]
    async fn test_model_capability_classification() {
        let discovery = ModelDiscovery::new(DiscoveryConfig::default());

        assert_eq!(
            discovery.classify_capability(&Some("bert".to_string())),
            ModelCapability::Embedding
        );

        assert_eq!(
            discovery.classify_capability(&Some("llama".to_string())),
            ModelCapability::TextGeneration
        );

        assert_eq!(
            discovery.classify_capability(&Some("nomic-bert".to_string())),
            ModelCapability::Embedding
        );

        assert_eq!(
            discovery.classify_capability(&None),
            ModelCapability::Unknown
        );
    }

    #[tokio::test]
    async fn test_file_type_to_quantization() {
        let discovery = ModelDiscovery::new(DiscoveryConfig::default());

        assert_eq!(discovery.file_type_to_quantization(0), "F32");
        assert_eq!(discovery.file_type_to_quantization(1), "F16");
        assert_eq!(discovery.file_type_to_quantization(8), "Q8_0");
        assert_eq!(discovery.file_type_to_quantization(18), "Q6_K");
    }

    #[tokio::test]
    async fn test_is_gguf_file() {
        let discovery = ModelDiscovery::new(DiscoveryConfig::default());

        assert!(discovery.is_gguf_file(Path::new("model.gguf")));
        assert!(discovery.is_gguf_file(Path::new("model.GGUF")));
        assert!(!discovery.is_gguf_file(Path::new("model.bin")));
        assert!(!discovery.is_gguf_file(Path::new("model")));
    }

    #[tokio::test]
    async fn test_cache_invalidation() {
        let config = DiscoveryConfig {
            custom_paths: vec![],
            search_common_locations: false,
            max_depth: 1,
            cache_ttl_seconds: 60,
        };

        let discovery = ModelDiscovery::new(config);

        // Initially cache should be empty
        {
            let cache = discovery.cache.read().await;
            assert!(cache.is_none());
        }

        // Perform discovery (will cache results)
        let _ = discovery.discover_models().await;

        // Cache should now be populated
        {
            let cache = discovery.cache.read().await;
            assert!(cache.is_some());
        }

        // Invalidate cache
        discovery.invalidate_cache().await;

        // Cache should be empty again
        {
            let cache = discovery.cache.read().await;
            assert!(cache.is_none());
        }
    }

    #[tokio::test]
    async fn test_empty_directory_scan() {
        let temp_dir = TempDir::new().unwrap();

        let config = DiscoveryConfig {
            custom_paths: vec![temp_dir.path().to_path_buf()],
            search_common_locations: false,
            max_depth: 3,
            cache_ttl_seconds: 60,
        };

        let discovery = ModelDiscovery::new(config);
        let models = discovery.discover_models().await.unwrap();

        assert_eq!(models.len(), 0);
    }

    #[test]
    fn test_model_capability_enum() {
        assert_eq!(ModelCapability::default(), ModelCapability::Unknown);

        // Test serialization roundtrip
        let cap = ModelCapability::Embedding;
        let json = serde_json::to_string(&cap).unwrap();
        let deserialized: ModelCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(cap, deserialized);
    }

    #[test]
    fn test_discovered_model_serialization() {
        let model = DiscoveredModel {
            name: "test-model".to_string(),
            path: PathBuf::from("/path/to/model.gguf"),
            capability: ModelCapability::Embedding,
            dimensions: Some(768),
            architecture: Some("bert".to_string()),
            parameter_count: Some(110_000_000),
            quantization: Some("Q8_0".to_string()),
            discovered_at: SystemTime::now(),
        };

        let json = serde_json::to_string(&model).unwrap();
        let deserialized: DiscoveredModel = serde_json::from_str(&json).unwrap();

        assert_eq!(model.name, deserialized.name);
        assert_eq!(model.capability, deserialized.capability);
        assert_eq!(model.dimensions, deserialized.dimensions);
    }
}
