//! Unified model discovery across all sources
//!
//! This module aggregates model discovery from multiple sources:
//! - Local GGUF/ONNX/SafeTensors files (via ModelDiscovery)
//! - Ollama API (/api/tags)
//! - OpenAI API (/v1/models)
//!
//! Results are cached with TTL to minimize API calls.

use std::time::Duration;

use crucible_config::BackendType;
use crucible_core::traits::provider::{ModelCapability, UnifiedModelInfo};

use super::model_cache::ModelCache;
use crate::model_discovery::{DiscoveredModel, DiscoveryConfig, ModelDiscovery};

/// Convert local ModelCapability to core ModelCapability
fn convert_capability(cap: crate::model_discovery::ModelCapability) -> ModelCapability {
    match cap {
        crate::model_discovery::ModelCapability::Embedding => ModelCapability::Embedding,
        crate::model_discovery::ModelCapability::TextGeneration => ModelCapability::TextGeneration,
        crate::model_discovery::ModelCapability::Unknown => ModelCapability::Chat, // Default to chat for unknown
    }
}

/// Convert DiscoveredModel to UnifiedModelInfo
impl From<&DiscoveredModel> for UnifiedModelInfo {
    fn from(model: &DiscoveredModel) -> Self {
        let mut info = UnifiedModelInfo::new(&model.name, BackendType::LlamaCpp);

        // Set capability
        info.capabilities = vec![convert_capability(model.capability)];

        // Set dimensions if available
        if let Some(dims) = model.dimensions {
            info.dimensions = Some(dims);
        }

        // Set size if we can get it from path
        if let Ok(metadata) = std::fs::metadata(&model.path) {
            info.size_bytes = Some(metadata.len());
        }

        // Store path in metadata
        info.metadata.insert(
            "path".to_string(),
            serde_json::json!(model.path.to_string_lossy()),
        );

        if let Some(arch) = &model.architecture {
            info.metadata
                .insert("architecture".to_string(), serde_json::json!(arch));
        }

        if let Some(quant) = &model.quantization {
            info.metadata
                .insert("quantization".to_string(), serde_json::json!(quant));
        }

        info
    }
}

/// Unified model discovery across all sources
pub struct UnifiedModelDiscovery {
    /// Local file discovery (GGUF, ONNX, SafeTensors)
    local: ModelDiscovery,
    /// Model cache for API-based providers
    cache: ModelCache,
    /// Discovery configuration
    config: DiscoveryConfig,
}

impl UnifiedModelDiscovery {
    /// Create a new unified discovery instance
    pub fn new(config: DiscoveryConfig) -> Self {
        Self {
            local: ModelDiscovery::new(config.clone()),
            cache: ModelCache::new(Duration::from_secs(config.cache_ttl_seconds)),
            config,
        }
    }

    /// Create with custom cache TTL
    pub fn with_cache_ttl(config: DiscoveryConfig, ttl: Duration) -> Self {
        Self {
            local: ModelDiscovery::new(config.clone()),
            cache: ModelCache::new(ttl),
            config,
        }
    }

    /// Discover all available models from all sources
    pub async fn discover_all(
        &self,
    ) -> Result<Vec<UnifiedModelInfo>, Box<dyn std::error::Error + Send + Sync>> {
        let mut all_models = Vec::new();

        // Discover local models
        let local_models = self.discover_local().await?;
        all_models.extend(local_models);

        Ok(all_models)
    }

    /// Discover local models (GGUF, ONNX, etc.)
    pub async fn discover_local(
        &self,
    ) -> Result<Vec<UnifiedModelInfo>, Box<dyn std::error::Error + Send + Sync>> {
        let discovered = self.local.discover_models().await.map_err(|e| {
            let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(std::io::Error::other(
                e.to_string(),
            ));
            boxed
        })?;

        Ok(discovered.iter().map(UnifiedModelInfo::from).collect())
    }

    /// Discover models for a specific backend
    pub async fn discover_by_backend(
        &self,
        backend: BackendType,
    ) -> Result<Vec<UnifiedModelInfo>, Box<dyn std::error::Error + Send + Sync>> {
        let all = self.discover_all().await?;
        Ok(all.into_iter().filter(|m| m.backend == backend).collect())
    }

    /// Discover embedding-capable models
    pub async fn discover_embedding_models(
        &self,
    ) -> Result<Vec<UnifiedModelInfo>, Box<dyn std::error::Error + Send + Sync>> {
        let all = self.discover_all().await?;
        Ok(all
            .into_iter()
            .filter(|m| m.capabilities.contains(&ModelCapability::Embedding))
            .collect())
    }

    /// Discover chat-capable models
    pub async fn discover_chat_models(
        &self,
    ) -> Result<Vec<UnifiedModelInfo>, Box<dyn std::error::Error + Send + Sync>> {
        let all = self.discover_all().await?;
        Ok(all
            .into_iter()
            .filter(|m| m.capabilities.contains(&ModelCapability::Chat))
            .collect())
    }

    /// Invalidate all cached discovery results
    pub async fn invalidate_cache(&self) {
        self.cache.invalidate_all().await;
    }

    /// Get the discovery configuration
    pub fn config(&self) -> &DiscoveryConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Create a mock GGUF file for testing
    fn create_mock_gguf(path: &std::path::Path) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;
        // GGUF magic number: GGUF in ASCII
        file.write_all(&[0x47, 0x47, 0x55, 0x46])?;
        // Version: 3
        file.write_all(&[0x03, 0x00, 0x00, 0x00])?;
        // Tensor count: 0
        file.write_all(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])?;
        // Metadata count: 0
        file.write_all(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])?;
        Ok(())
    }

    // RED: Discovery aggregates from multiple sources
    #[tokio::test]
    async fn test_discover_all_aggregates_sources() {
        let temp = TempDir::new().unwrap();
        create_mock_gguf(&temp.path().join("model.gguf")).unwrap();

        let config = DiscoveryConfig {
            custom_paths: vec![temp.path().to_path_buf()],
            search_common_locations: false,
            max_depth: 3,
            cache_ttl_seconds: 300,
        };

        let discovery = UnifiedModelDiscovery::new(config);
        let models = discovery.discover_all().await.unwrap();

        // Should find our local model
        assert!(!models.is_empty(), "Should discover at least one model");

        // All models should have valid backend types
        for model in &models {
            assert!(
                matches!(
                    model.backend,
                    BackendType::LlamaCpp | BackendType::Ollama | BackendType::OpenAI
                ),
                "Model should have valid backend"
            );
        }
    }

    // RED: Embedding filter works
    #[tokio::test]
    async fn test_discover_embedding_models_filters() {
        let temp = TempDir::new().unwrap();
        // Create an embedding model (name contains 'embed')
        create_mock_gguf(&temp.path().join("nomic-embed-text.gguf")).unwrap();

        let config = DiscoveryConfig {
            custom_paths: vec![temp.path().to_path_buf()],
            search_common_locations: false,
            max_depth: 3,
            cache_ttl_seconds: 300,
        };

        let discovery = UnifiedModelDiscovery::new(config);
        let models = discovery.discover_embedding_models().await.unwrap();

        // All returned models should have embedding capability
        for model in &models {
            assert!(
                model.capabilities.contains(&ModelCapability::Embedding),
                "Model {} should have embedding capability",
                model.id
            );
        }
    }

    // RED: Chat filter works
    #[tokio::test]
    async fn test_discover_chat_models_filters() {
        let temp = TempDir::new().unwrap();
        // Create a chat model
        create_mock_gguf(&temp.path().join("llama-3.gguf")).unwrap();

        let config = DiscoveryConfig {
            custom_paths: vec![temp.path().to_path_buf()],
            search_common_locations: false,
            max_depth: 3,
            cache_ttl_seconds: 300,
        };

        let discovery = UnifiedModelDiscovery::new(config);
        let models = discovery.discover_chat_models().await.unwrap();

        // All returned models should have chat capability
        for model in &models {
            assert!(
                model.capabilities.contains(&ModelCapability::Chat),
                "Model {} should have chat capability",
                model.id
            );
        }
    }

    // RED: Backend filter works
    #[tokio::test]
    async fn test_discover_by_backend_filters() {
        let temp = TempDir::new().unwrap();
        create_mock_gguf(&temp.path().join("model.gguf")).unwrap();

        let config = DiscoveryConfig {
            custom_paths: vec![temp.path().to_path_buf()],
            search_common_locations: false,
            max_depth: 3,
            cache_ttl_seconds: 300,
        };

        let discovery = UnifiedModelDiscovery::new(config);
        let models = discovery
            .discover_by_backend(BackendType::LlamaCpp)
            .await
            .unwrap();

        // All returned models should be LlamaCpp
        for model in &models {
            assert_eq!(
                model.backend,
                BackendType::LlamaCpp,
                "Model should be LlamaCpp backend"
            );
        }
    }

    // RED: Conversion from DiscoveredModel preserves data
    #[test]
    fn test_discovered_model_conversion() {
        let discovered = DiscoveredModel {
            name: "test-model".to_string(),
            path: PathBuf::from("/tmp/test-model.gguf"),
            capability: crate::model_discovery::ModelCapability::Embedding,
            dimensions: Some(768),
            architecture: Some("bert".to_string()),
            parameter_count: Some(137_000_000),
            quantization: Some("Q8_0".to_string()),
            discovered_at: std::time::SystemTime::now(),
        };

        let unified = UnifiedModelInfo::from(&discovered);

        assert_eq!(unified.id, "test-model");
        assert_eq!(unified.name, "test-model");
        assert_eq!(unified.backend, BackendType::LlamaCpp);
        assert!(unified.capabilities.contains(&ModelCapability::Embedding));
        assert_eq!(unified.dimensions, Some(768));
        assert!(unified.metadata.contains_key("architecture"));
        assert!(unified.metadata.contains_key("quantization"));
    }

    // RED: Empty config returns empty results
    #[tokio::test]
    async fn test_discover_empty_config() {
        let config = DiscoveryConfig {
            custom_paths: vec![],
            search_common_locations: false,
            max_depth: 0,
            cache_ttl_seconds: 300,
        };

        let discovery = UnifiedModelDiscovery::new(config);
        let models = discovery.discover_all().await.unwrap();

        // Should return empty, not error
        assert!(models.is_empty());
    }

    // RED: LlamaCpp uses local discovery
    #[cfg(feature = "llama-cpp")]
    #[tokio::test]
    async fn test_llamacpp_discovers_local_gguf() {
        use crate::text_generation::LlamaCppTextProvider;

        let temp = TempDir::new().unwrap();
        create_mock_gguf(&temp.path().join("model.gguf")).unwrap();

        let config = DiscoveryConfig {
            custom_paths: vec![temp.path().to_path_buf()],
            search_common_locations: false,
            max_depth: 3,
            cache_ttl_seconds: 300,
        };

        let models = LlamaCppTextProvider::discover_local_models(&config)
            .await
            .unwrap();
        assert!(!models.is_empty(), "Should discover at least one model");
    }

    // RED: Discovered model can create provider (path validation test)
    #[cfg(feature = "llama-cpp")]
    #[test]
    fn test_discovered_model_creates_provider_validates_path() {
        use crate::model_discovery::DiscoveredModel;
        use crate::text_generation::LlamaCppTextProvider;

        // Create with non-existent path - should fail
        let discovered = DiscoveredModel {
            name: "test-model".to_string(),
            path: PathBuf::from("/nonexistent/path/model.gguf"),
            capability: crate::model_discovery::ModelCapability::TextGeneration,
            dimensions: None,
            architecture: Some("llama".to_string()),
            parameter_count: Some(7_000_000_000),
            quantization: Some("Q8_0".to_string()),
            discovered_at: std::time::SystemTime::now(),
        };

        // Should fail because path doesn't exist
        let result = LlamaCppTextProvider::from_discovered_model(&discovered);
        assert!(result.is_err(), "Should fail for non-existent model path");
    }
}
