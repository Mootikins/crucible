//! FastEmbed provider for local text embedding generation
//!
//! This module provides a local embedding provider using the FastEmbed library,
//! which uses ONNX Runtime for efficient CPU-based inference. FastEmbed supports
//! multiple sentence transformer models and provides fast, reliable embeddings
//! without requiring external API calls.
//!
//! ## Features
//!
//! - **Local Inference**: No API keys, works offline
//! - **Fast Performance**: 5k-14k sentences/sec on CPU with ONNX optimization
//! - **18+ Models**: Pre-configured models including BGE, Nomic, MiniLM, E5
//! - **Auto-downloading**: Models download automatically from HuggingFace Hub
//! - **Caching**: Downloaded models are cached locally
//! - **Production Ready**: Stable v5.2.0, battle-tested in Qdrant ecosystem
//!
//! ## Threading Model
//!
//! FastEmbed uses ONNX Runtime which handles parallelism internally:
//! - **Intra-op parallelism**: ONNX parallelizes operations within each inference call
//! - **Thread control**: Set `ORT_NUM_THREADS` environment variable to control thread count
//! - **Default behavior**: Uses all available CPU cores
//!
//! Note: This provider uses a mutex for thread-safety, so concurrent `embed_batch()`
//! calls are serialized. For maximum throughput, batch your texts before calling
//! `embed_batch()` rather than making many concurrent calls.
//!
//! ## Supported Models
//!
//! ### Priority Models (Recommended)
//! - `BGESmallENV15` (384 dims) - Default, fast, high quality
//! - `AllMiniLML6V2` (384 dims) - Very fast, lightweight
//! - `NomicEmbedTextV15` (768 dims) - High quality, larger
//! - `MxbaiEmbedLargeV1` (1024 dims) - Best quality, slower
//! - `MultilingualE5Large` (384 dims) - Multilingual support
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use crucible_llm::embeddings::{EmbeddingConfig, create_provider};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create FastEmbed provider with default model (BGE-small)
//!     let config = EmbeddingConfig::fastembed(None, None, None);
//!     let provider = create_provider(config).await?;
//!
//!     // Generate embedding
//!     let response = provider.embed("Hello, world!").await?;
//!     println!("Generated {} dimensional embedding", response.dimensions);
//!
//!     Ok(())
//! }
//! ```

use async_trait::async_trait;
use crucible_config::BackendType;
use crucible_core::traits::llm::LlmResult;
use crucible_core::traits::provider::{
    CanEmbed, EmbeddingResponse as UnifiedEmbeddingResponse, ExtendedCapabilities,
    Provider as UnifiedProvider,
};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::error::{EmbeddingError, EmbeddingResult};
use super::provider::{
    EmbeddingProvider, EmbeddingResponse, ModelFamily, ModelInfo, ParameterSize,
};

/// Local embedding provider using FastEmbed library
///
/// This provider generates embeddings locally using ONNX-optimized models.
/// It's designed for CPU inference with excellent performance characteristics.
pub struct FastEmbedProvider {
    /// FastEmbed model instance (lazy loaded)
    model: Arc<Mutex<Option<TextEmbedding>>>,
    /// Provider configuration
    config: FastEmbedConfig,
    /// Model metadata
    model_info: ModelInfo,
}

impl std::fmt::Debug for FastEmbedProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FastEmbedProvider")
            .field("config", &self.config)
            .field("model_info", &self.model_info)
            .field(
                "model_loaded",
                &self
                    .model
                    .try_lock()
                    .ok()
                    .and_then(|g| g.as_ref().map(|_| true)),
            )
            .finish()
    }
}

/// Configuration for FastEmbed provider
#[derive(Debug, Clone)]
pub struct FastEmbedConfig {
    /// Model to use (enum from fastembed crate)
    pub model: EmbeddingModel,
    /// Cache directory for downloaded models
    pub cache_dir: Option<PathBuf>,
    /// Show download progress
    pub show_download_progress: bool,
    /// Batch size for processing
    pub batch_size: Option<usize>,
}

impl Default for FastEmbedConfig {
    fn default() -> Self {
        Self {
            model: EmbeddingModel::BGESmallENV15,
            cache_dir: None,
            show_download_progress: true,
            batch_size: Some(32),
        }
    }
}

impl FastEmbedProvider {
    /// Create a new FastEmbed provider with the given configuration
    ///
    /// The model is lazy-loaded on first use to avoid blocking during provider creation.
    ///
    /// # Arguments
    ///
    /// * `config` - Embedding configuration from crucible-config
    ///
    /// # Returns
    ///
    /// A configured FastEmbedProvider ready for embedding generation
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_llm::embeddings::{EmbeddingConfig, FastEmbedProvider};
    ///
    /// let config = EmbeddingConfig::fastembed(None, None, None);
    /// let provider = FastEmbedProvider::new(config)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new(config: super::config::EmbeddingConfig) -> EmbeddingResult<Self> {
        // Parse model name to fastembed's EmbeddingModel enum
        let model = Self::parse_model_name(config.model_name())?;

        // Get model metadata
        let model_info = Self::get_model_info(&model);

        let fastembed_config = FastEmbedConfig {
            model,
            cache_dir: None, // Use fastembed's default cache
            show_download_progress: true,
            batch_size: Some(32),
        };

        Ok(Self {
            model: Arc::new(Mutex::new(None)),
            config: fastembed_config,
            model_info,
        })
    }

    /// Parse model name string to FastEmbed's EmbeddingModel enum
    fn parse_model_name(name: &str) -> EmbeddingResult<EmbeddingModel> {
        // Support both HuggingFace names and simple names
        let model = match name.to_lowercase().as_str() {
            // BGE models
            "bge-small-en-v1.5" | "baai/bge-small-en-v1.5" => EmbeddingModel::BGESmallENV15,
            "bge-base-en-v1.5" | "baai/bge-base-en-v1.5" => EmbeddingModel::BGEBaseENV15,
            "bge-large-en-v1.5" | "baai/bge-large-en-v1.5" => EmbeddingModel::BGELargeENV15,

            // MiniLM models
            "all-minilm-l6-v2" | "sentence-transformers/all-minilm-l6-v2" => {
                EmbeddingModel::AllMiniLML6V2
            }
            "all-minilm-l12-v2" | "sentence-transformers/all-minilm-l12-v2" => {
                EmbeddingModel::AllMiniLML12V2
            }

            // Nomic models
            "nomic-embed-text-v1" | "nomic-ai/nomic-embed-text-v1" => {
                EmbeddingModel::NomicEmbedTextV1
            }
            "nomic-embed-text-v1.5" | "nomic-ai/nomic-embed-text-v1.5" => {
                EmbeddingModel::NomicEmbedTextV15
            }

            // E5 models
            "multilingual-e5-large" | "intfloat/multilingual-e5-large" => {
                EmbeddingModel::MultilingualE5Large
            }
            "multilingual-e5-base" | "intfloat/multilingual-e5-base" => {
                EmbeddingModel::MultilingualE5Base
            }
            "multilingual-e5-small" | "intfloat/multilingual-e5-small" => {
                EmbeddingModel::MultilingualE5Small
            }

            // Other models
            "mxbai-embed-large-v1" | "mixedbread-ai/mxbai-embed-large-v1" => {
                EmbeddingModel::MxbaiEmbedLargeV1
            }
            "paraphrase-minilm-l12-v2" | "sentence-transformers/paraphrase-minilm-l12-v2" => {
                EmbeddingModel::ParaphraseMLMiniLML12V2
            }

            _ => {
                return Err(EmbeddingError::ConfigError(format!(
                    "Unsupported FastEmbed model: {}. Supported models: bge-small-en-v1.5, \
                    all-minilm-l6-v2, nomic-embed-text-v1.5, mxbai-embed-large-v1, etc.",
                    name
                )))
            }
        };

        Ok(model)
    }

    /// Get model metadata for a given EmbeddingModel
    fn get_model_info(model: &EmbeddingModel) -> ModelInfo {
        match model {
            EmbeddingModel::BGESmallENV15 => ModelInfo::builder()
                .name("BAAI/bge-small-en-v1.5")
                .display_name("BGE Small EN v1.5")
                .family(ModelFamily::Bert)
                .dimensions(384)
                .parameter_size(ParameterSize::new(33, true))
                .format("onnx")
                .recommended(true)
                .build(),

            EmbeddingModel::AllMiniLML6V2 => ModelInfo::builder()
                .name("all-MiniLM-L6-v2")
                .display_name("all-MiniLM-L6-v2")
                .family(ModelFamily::Bert)
                .dimensions(384)
                .parameter_size(ParameterSize::new(22, true))
                .format("onnx")
                .recommended(true)
                .build(),

            EmbeddingModel::NomicEmbedTextV15 => ModelInfo::builder()
                .name("nomic-ai/nomic-embed-text-v1.5")
                .display_name("Nomic Embed Text v1.5")
                .family(ModelFamily::Bert)
                .dimensions(768)
                .parameter_size(ParameterSize::new(137, true))
                .format("onnx")
                .recommended(true)
                .build(),

            EmbeddingModel::MxbaiEmbedLargeV1 => ModelInfo::builder()
                .name("mixedbread-ai/mxbai-embed-large-v1")
                .display_name("Mixedbread Embed Large v1")
                .family(ModelFamily::Bert)
                .dimensions(1024)
                .parameter_size(ParameterSize::new(335, true))
                .format("onnx")
                .build(),

            EmbeddingModel::MultilingualE5Large => ModelInfo::builder()
                .name("intfloat/multilingual-e5-large")
                .display_name("Multilingual E5 Large")
                .family(ModelFamily::Bert)
                .dimensions(1024)
                .parameter_size(ParameterSize::new(560, true))
                .format("onnx")
                .build(),

            _ => ModelInfo::builder()
                .name(format!("{:?}", model))
                .display_name(format!("{:?}", model))
                .family(ModelFamily::Bert)
                .dimensions(768)
                .format("onnx")
                .build(),
        }
    }

    /// Ensure the model is loaded, loading it if necessary
    async fn ensure_model_loaded(&self) -> EmbeddingResult<()> {
        let mut model_guard = self.model.lock().await;

        if model_guard.is_none() {
            tracing::info!("Loading FastEmbed model: {:?}", self.config.model);

            // Create init options
            let mut init_options = InitOptions::new(self.config.model.clone())
                .with_show_download_progress(self.config.show_download_progress);

            if let Some(cache_dir) = &self.config.cache_dir {
                init_options = init_options.with_cache_dir(cache_dir.clone());
            }

            // Load model (this runs in blocking thread pool via tokio::task::spawn_blocking)
            let model = tokio::task::spawn_blocking(move || TextEmbedding::try_new(init_options))
                .await
                .map_err(|e| EmbeddingError::ProviderError {
                    provider: "FastEmbed".to_string(),
                    message: format!("Failed to spawn model loading task: {}", e),
                })?
                .map_err(|e| EmbeddingError::ProviderError {
                    provider: "FastEmbed".to_string(),
                    message: format!("Failed to load model: {}", e),
                })?;

            *model_guard = Some(model);
            tracing::info!("FastEmbed model loaded successfully");
        }

        Ok(())
    }

    /// Generate embeddings for texts (internal method)
    async fn embed_internal(&self, texts: Vec<String>) -> EmbeddingResult<Vec<Vec<f32>>> {
        self.ensure_model_loaded().await?;

        // Clone the Arc to share with the blocking task
        let model_arc = Arc::clone(&self.model);
        let batch_size = self.config.batch_size;

        // Run embedding in blocking thread pool
        let embeddings =
            tokio::task::spawn_blocking(move || -> Result<Vec<Vec<f32>>, fastembed::Error> {
                // Get lock inside the blocking task
                let mut model_guard = model_arc.blocking_lock();
                let model = model_guard
                    .as_mut()
                    .ok_or_else(|| fastembed::Error::msg("Model not loaded"))?;

                // Convert to references for fastembed API
                let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

                // Generate embeddings
                model.embed(text_refs, batch_size)
            })
            .await
            .map_err(|e| EmbeddingError::ProviderError {
                provider: "FastEmbed".to_string(),
                message: format!("Failed to spawn embedding task: {}", e),
            })?
            .map_err(|e| EmbeddingError::ProviderError {
                provider: "FastEmbed".to_string(),
                message: format!("Failed to generate embeddings: {}", e),
            })?;

        Ok(embeddings)
    }
}

#[async_trait]
impl EmbeddingProvider for FastEmbedProvider {
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        if text.trim().is_empty() {
            return Err(EmbeddingError::Other("Text cannot be empty".to_string()));
        }

        let embeddings = self.embed_internal(vec![text.to_string()]).await?;

        let embedding =
            embeddings
                .into_iter()
                .next()
                .ok_or_else(|| EmbeddingError::ProviderError {
                    provider: "FastEmbed".to_string(),
                    message: "No embedding returned".to_string(),
                })?;

        Ok(EmbeddingResponse::new(
            embedding,
            self.model_info.name.clone(),
        ))
    }

    async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let embeddings = self.embed_internal(texts).await?;

        let responses = embeddings
            .into_iter()
            .map(|embedding| EmbeddingResponse::new(embedding, self.model_info.name.clone()))
            .collect();

        Ok(responses)
    }

    fn model_name(&self) -> &str {
        &self.model_info.name
    }

    fn dimensions(&self) -> usize {
        self.model_info.dimensions.unwrap_or(768)
    }

    fn provider_name(&self) -> &str {
        "FastEmbed"
    }

    async fn health_check(&self) -> EmbeddingResult<bool> {
        match EmbeddingProvider::embed(self, "health check").await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn list_models(&self) -> EmbeddingResult<Vec<ModelInfo>> {
        // Return supported models
        Ok(vec![
            Self::get_model_info(&EmbeddingModel::BGESmallENV15),
            Self::get_model_info(&EmbeddingModel::AllMiniLML6V2),
            Self::get_model_info(&EmbeddingModel::NomicEmbedTextV15),
            Self::get_model_info(&EmbeddingModel::MxbaiEmbedLargeV1),
            Self::get_model_info(&EmbeddingModel::MultilingualE5Large),
            Self::get_model_info(&EmbeddingModel::BGEBaseENV15),
            Self::get_model_info(&EmbeddingModel::BGELargeENV15),
        ])
    }
}

// =============================================================================
// Unified Provider Trait Implementations
// =============================================================================

#[async_trait]
impl UnifiedProvider for FastEmbedProvider {
    fn name(&self) -> &str {
        "fastembed"
    }

    fn backend_type(&self) -> BackendType {
        BackendType::FastEmbed
    }

    fn endpoint(&self) -> Option<&str> {
        None // Local provider, no endpoint
    }

    fn capabilities(&self) -> ExtendedCapabilities {
        ExtendedCapabilities::embedding_only(self.model_info.dimensions.unwrap_or(768))
    }

    async fn health_check(&self) -> LlmResult<bool> {
        // Reuse the legacy health check logic
        match EmbeddingProvider::health_check(self).await {
            Ok(healthy) => Ok(healthy),
            Err(e) => Err(crucible_core::traits::llm::LlmError::ProviderError {
                provider: "FastEmbed".to_string(),
                message: e.to_string(),
            }),
        }
    }
}

#[async_trait]
impl CanEmbed for FastEmbedProvider {
    async fn embed(&self, text: &str) -> LlmResult<UnifiedEmbeddingResponse> {
        // Delegate to legacy impl and convert response type
        let response = EmbeddingProvider::embed(self, text)
            .await
            .map_err(|e| crucible_core::traits::llm::LlmError::ProviderError {
                provider: "FastEmbed".to_string(),
                message: e.to_string(),
            })?;

        Ok(UnifiedEmbeddingResponse {
            embedding: response.embedding,
            token_count: None, // FastEmbed doesn't provide token count
            model: response.model,
        })
    }

    async fn embed_batch(&self, texts: Vec<String>) -> LlmResult<Vec<UnifiedEmbeddingResponse>> {
        // Delegate to legacy impl and convert response type
        let responses = EmbeddingProvider::embed_batch(self, texts)
            .await
            .map_err(|e| crucible_core::traits::llm::LlmError::ProviderError {
                provider: "FastEmbed".to_string(),
                message: e.to_string(),
            })?;

        Ok(responses
            .into_iter()
            .map(|r| UnifiedEmbeddingResponse {
                embedding: r.embedding,
                token_count: None,
                model: r.model,
            })
            .collect())
    }

    fn embedding_dimensions(&self) -> usize {
        self.model_info.dimensions.unwrap_or(768)
    }

    fn embedding_model(&self) -> &str {
        &self.model_info.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fastembed_provider_creation() {
        let config = super::super::config::EmbeddingConfig::fastembed(None, None, None);
        let provider = FastEmbedProvider::new(config);
        assert!(provider.is_ok());

        let provider = provider.unwrap();
        assert_eq!(provider.provider_name(), "FastEmbed");
        assert_eq!(provider.dimensions(), 384); // BGE-small default
    }

    #[tokio::test]
    async fn test_fastembed_single_embedding() {
        let config = super::super::config::EmbeddingConfig::fastembed(
            Some("all-MiniLM-L6-v2".to_string()),
            Some("/tmp/fastembed_cache".to_string()),
            None,
        );
        let provider = FastEmbedProvider::new(config).unwrap();

        let response = EmbeddingProvider::embed(&provider, "Hello, world!").await;
        if let Err(ref e) = response {
            eprintln!("FastEmbed error: {:?}", e);
        }
        assert!(response.is_ok());

        let response = response.unwrap();
        assert_eq!(response.dimensions, 384);
        assert_eq!(response.embedding.len(), 384);

        // Verify embedding values are reasonable
        for &value in &response.embedding {
            assert!(value.is_finite(), "Embedding values should be finite");
        }
    }

    #[tokio::test]
    async fn test_fastembed_batch_embedding() {
        let config = super::super::config::EmbeddingConfig::fastembed(
            None,
            Some("/tmp/fastembed_cache".to_string()),
            None,
        );
        let provider = FastEmbedProvider::new(config).unwrap();

        let texts = vec![
            "First text".to_string(),
            "Second text".to_string(),
            "Third text".to_string(),
        ];

        let responses = EmbeddingProvider::embed_batch(&provider, texts).await;
        if let Err(ref e) = responses {
            eprintln!("FastEmbed batch error: {:?}", e);
        }
        assert!(responses.is_ok());

        let responses = responses.unwrap();
        assert_eq!(responses.len(), 3);

        for response in responses {
            assert_eq!(response.dimensions, 384);
            assert_eq!(response.embedding.len(), 384);
        }
    }

    #[tokio::test]
    async fn test_fastembed_error_handling() {
        let config = super::super::config::EmbeddingConfig::fastembed(
            None,
            Some("/tmp/fastembed_cache".to_string()),
            None,
        );
        let provider = FastEmbedProvider::new(config).unwrap();

        // Test empty text
        let result = EmbeddingProvider::embed(&provider, "").await;
        assert!(result.is_err());

        let result = EmbeddingProvider::embed(&provider, "   ").await;
        assert!(result.is_err());

        // Test empty batch
        let result = EmbeddingProvider::embed_batch(&provider, vec![]).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_fastembed_list_models() {
        let config = super::super::config::EmbeddingConfig::fastembed(None, None, None);
        let provider = FastEmbedProvider::new(config).unwrap();

        let models = provider.list_models().await;
        assert!(models.is_ok());

        let models = models.unwrap();
        assert!(!models.is_empty());

        // Check for specific models
        let model_names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
        assert!(model_names.contains(&"BAAI/bge-small-en-v1.5"));
        assert!(model_names.contains(&"all-MiniLM-L6-v2"));
        assert!(model_names.contains(&"nomic-ai/nomic-embed-text-v1.5"));
    }

    #[test]
    fn test_model_name_parsing() {
        // Test various model name formats
        assert!(FastEmbedProvider::parse_model_name("bge-small-en-v1.5").is_ok());
        assert!(FastEmbedProvider::parse_model_name("BAAI/bge-small-en-v1.5").is_ok());
        assert!(FastEmbedProvider::parse_model_name("all-MiniLM-L6-v2").is_ok());
        assert!(FastEmbedProvider::parse_model_name("nomic-embed-text-v1.5").is_ok());

        // Test invalid model
        assert!(FastEmbedProvider::parse_model_name("invalid-model").is_err());
    }

    // =========================================================================
    // TDD Tests for Unified Provider Traits (Provider + CanEmbed)
    // =========================================================================

    mod unified_traits {
        use super::*;
        use crucible_config::BackendType;
        use crucible_core::traits::provider::{CanEmbed, Provider};

        #[test]
        fn test_fastembed_implements_provider_trait() {
            let config = super::super::super::config::EmbeddingConfig::fastembed(None, None, None);
            let provider = FastEmbedProvider::new(config).unwrap();

            // Test Provider trait methods
            assert_eq!(provider.name(), "fastembed");
            assert_eq!(provider.backend_type(), BackendType::FastEmbed);
            assert!(provider.endpoint().is_none()); // Local provider, no endpoint
        }

        #[test]
        fn test_fastembed_provider_capabilities() {
            let config = super::super::super::config::EmbeddingConfig::fastembed(None, None, None);
            let provider = FastEmbedProvider::new(config).unwrap();

            let caps = provider.capabilities();

            // FastEmbed is embedding-only
            assert!(caps.embeddings);
            assert!(caps.embeddings_batch);
            assert_eq!(caps.embedding_dimensions, Some(384)); // BGE-small default
            assert!(!caps.llm.chat_completion); // No chat support
        }

        #[tokio::test]
        async fn test_fastembed_can_embed_trait() {
            let config = super::super::super::config::EmbeddingConfig::fastembed(
                Some("all-MiniLM-L6-v2".to_string()),
                Some("/tmp/fastembed_cache".to_string()),
                None,
            );
            let provider = FastEmbedProvider::new(config).unwrap();

            // Test CanEmbed trait methods
            assert_eq!(provider.embedding_dimensions(), 384);
            assert!(provider.embedding_model().contains("MiniLM"));

            // Test embed via CanEmbed trait
            let response = CanEmbed::embed(&provider, "Hello world").await;
            assert!(response.is_ok());

            let response = response.unwrap();
            assert_eq!(response.embedding.len(), 384);
            assert_eq!(response.model, provider.embedding_model());
        }

        #[tokio::test]
        async fn test_fastembed_can_embed_batch() {
            let config = super::super::super::config::EmbeddingConfig::fastembed(
                None,
                Some("/tmp/fastembed_cache".to_string()),
                None,
            );
            let provider = FastEmbedProvider::new(config).unwrap();

            let texts = vec!["First".to_string(), "Second".to_string()];
            let responses = CanEmbed::embed_batch(&provider, texts).await;
            assert!(responses.is_ok());

            let responses = responses.unwrap();
            assert_eq!(responses.len(), 2);
        }

        #[tokio::test]
        async fn test_fastembed_unified_health_check() {
            let config = super::super::super::config::EmbeddingConfig::fastembed(
                None,
                Some("/tmp/fastembed_cache".to_string()),
                None,
            );
            let provider = FastEmbedProvider::new(config).unwrap();

            // Test health_check via Provider trait (returns LlmResult<bool>)
            let health = Provider::health_check(&provider).await;
            assert!(health.is_ok());
            assert!(health.unwrap());
        }

        #[test]
        fn test_fastembed_can_be_used_as_dyn_provider() {
            let config = super::super::super::config::EmbeddingConfig::fastembed(None, None, None);
            let provider = FastEmbedProvider::new(config).unwrap();

            // Should be usable as Box<dyn Provider>
            let _boxed: Box<dyn Provider> = Box::new(provider);
        }

        #[test]
        fn test_fastembed_can_be_used_as_dyn_can_embed() {
            let config = super::super::super::config::EmbeddingConfig::fastembed(None, None, None);
            let provider = FastEmbedProvider::new(config).unwrap();

            // Should be usable as Box<dyn CanEmbed>
            let _boxed: Box<dyn CanEmbed> = Box::new(provider);
        }
    }
}
