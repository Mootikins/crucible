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
//! The [`catalog`](super::catalog) module holds every model, the name each one
//! answers to, and what it costs. This provider decides nothing about models;
//! it reads that table.
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use crucible_daemon::llm::embeddings::{EmbeddingConfig, create_provider};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create FastEmbed provider with default model (BGE-small)
//!     let config = EmbeddingConfig::fastembed(None, None);
//!     let provider = create_provider(config).await?;
//!
//!     // Generate embedding
//!     let response = provider.embed("Hello, world!").await?;
//!     println!("Generated {} dimensional embedding", response.len());
//!
//!     Ok(())
//! }
//! ```

use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::catalog;
use super::error::{EmbeddingError, EmbeddingResult};
use super::provider::ModelInfo;
use crucible_core::enrichment::EmbeddingProvider;

/// Local embedding provider using FastEmbed library
///
/// This provider generates embeddings locally using ONNX-optimized models.
/// It's designed for CPU inference with excellent performance characteristics.
pub struct FastEmbedProvider {
    /// FastEmbed model instance (lazy loaded)
    model: Arc<Mutex<Option<TextEmbedding>>>,
    /// Provider configuration
    config: FastEmbedInitOptions,
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
pub struct FastEmbedInitOptions {
    /// Model to use (enum from fastembed crate)
    pub model: EmbeddingModel,
    /// Cache directory for downloaded models
    pub cache_dir: Option<PathBuf>,
    /// Show download progress
    pub show_download_progress: bool,
    /// Batch size for processing
    pub batch_size: Option<usize>,
}

impl Default for FastEmbedInitOptions {
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
    /// use crucible_daemon::llm::embeddings::{EmbeddingConfig, FastEmbedProvider};
    ///
    /// let config = EmbeddingConfig::fastembed(None, None);
    /// let provider = FastEmbedProvider::new(config)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new(config: super::config::EmbeddingConfig) -> EmbeddingResult<Self> {
        // The catalog owns both answers, so the name a user writes and the
        // metadata the provider reports cannot disagree.
        let model = catalog::parse_model_name(config.model_name())?;
        let model_info = catalog::model_info(&model);

        // Read these off the config rather than hardcoding them. They were
        // `None` and `Some(32)` here, so `[embedding.fastembed] cache_dir` and
        // `batch_size` deserialized, validated and were then thrown away — a
        // user setting a 4 GB model cache onto a large disk still got it in the
        // default location.
        let fastembed_config = FastEmbedInitOptions {
            model,
            cache_dir: config.cache_dir(),
            show_download_progress: true,
            batch_size: Some(config.batch_size()),
        };

        Ok(Self {
            model: Arc::new(Mutex::new(None)),
            config: fastembed_config,
            model_info,
        })
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
                .map_err(|e| {
                    let error_msg = format!("Failed to spawn model loading task: {}", e);
                    #[cfg(target_os = "windows")]
                    {
                        tracing::error!(
                            "{} On Windows, this may indicate a threading or runtime issue.",
                            error_msg
                        );
                    }
                    EmbeddingError::ProviderError {
                        provider: "FastEmbed".to_string(),
                        message: error_msg,
                    }
                })?
                .map_err(|e| {
                    let error_str = e.to_string();
                    let error_msg = format!("Failed to load ONNX model: {}", error_str);

                    #[cfg(target_os = "windows")]
                    {
                        // Add Windows-specific diagnostic information
                        if error_str.contains("DLL") || error_str.contains("dll") {
                            error_msg.push_str(
                                "\n\nWindows DLL Error Detected. Troubleshooting:\n\
                                1. Install Visual C++ Redistributable: https://aka.ms/vs/17/release/vc_redist.x64.exe\n\
                                2. Verify .cargo/config.toml uses dynamic runtime (target-feature=-crt-static)\n\
                                3. Clean and rebuild: cargo clean && cargo build"
                            );
                        } else if error_str.contains("LNK2038") || error_str.contains("RuntimeLibrary") {
                            error_msg.push_str(
                                "\n\nC Runtime Mismatch Detected. Troubleshooting:\n\
                                1. Clean build: cargo clean && cargo build\n\
                                2. Verify .cargo/config.toml exists and uses dynamic runtime\n\
                                3. Check that all dependencies use /MD (dynamic runtime)"
                            );
                        } else {
                            error_msg.push_str(
                                "\n\nWindows-specific troubleshooting:\n\
                                1. Ensure Visual C++ Redistributable is installed\n\
                                2. Check .cargo/config.toml for correct runtime settings\n\
                                3. Try: cargo clean && cargo build"
                            );
                        }
                    }

                    tracing::error!("FastEmbed model loading error: {}", error_msg);
                    EmbeddingError::ProviderError {
                        provider: "FastEmbed".to_string(),
                        message: error_msg,
                    }
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
                .map_err(|e| {
                    let error_msg = format!("Failed to spawn embedding task: {}", e);
                    #[cfg(target_os = "windows")]
                    {
                        tracing::error!(
                            "{} On Windows, this may indicate a threading or runtime issue.",
                            error_msg
                        );
                    }
                    EmbeddingError::ProviderError {
                        provider: "FastEmbed".to_string(),
                        message: error_msg,
                    }
                })?
                .map_err(|e| {
                    let error_str = e.to_string();
                    let error_msg = format!("Failed to generate embeddings: {}", error_str);

                    #[cfg(target_os = "windows")]
                    {
                        if error_str.contains("DLL") || error_str.contains("dll") {
                            error_msg.push_str(
                                "\n\nWindows DLL Error during inference. Check Visual C++ Redistributable installation."
                            );
                        }
                    }

                    EmbeddingError::ProviderError {
                        provider: "FastEmbed".to_string(),
                        message: error_msg,
                    }
                })?;

        Ok(embeddings)
    }
}

#[async_trait]
impl EmbeddingProvider for FastEmbedProvider {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        if text.trim().is_empty() {
            return Err(EmbeddingError::Other("Text cannot be empty".to_string()).into());
        }

        let embeddings = self.embed_internal(vec![text.to_string()]).await?;

        embeddings.into_iter().next().ok_or_else(|| {
            EmbeddingError::ProviderError {
                provider: "FastEmbed".to_string(),
                message: "No embedding returned".to_string(),
            }
            .into()
        })
    }

    async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let owned: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
        self.embed_internal(owned)
            .await
            .map_err(|e| anyhow::anyhow!(e))
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

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        // The whole catalog, not a favourite few. A hand-picked list of seven
        // hid the other thirty-seven models a user may configure.
        Ok(catalog::all()
            .into_iter()
            .map(|entry| entry.canonical_name.to_string())
            .collect())
    }
}

// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Cross-platform test cache path helper
    fn test_cache_path() -> String {
        std::env::temp_dir()
            .join("crucible_test_fastembed_cache")
            .to_string_lossy()
            .into_owned()
    }

    #[tokio::test]
    async fn test_fastembed_provider_creation() {
        let config = super::super::config::EmbeddingConfig::fastembed(None, None);
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
            Some(test_cache_path()),
        );
        let provider = FastEmbedProvider::new(config).unwrap();

        let result = EmbeddingProvider::embed(&provider, "Hello, world!").await;
        if let Err(ref e) = result {
            eprintln!("FastEmbed error: {:?}", e);
        }
        assert!(result.is_ok());

        let embedding = result.unwrap();
        assert_eq!(embedding.len(), 384);

        for &value in &embedding {
            assert!(value.is_finite(), "Embedding values should be finite");
        }
    }

    #[tokio::test]
    async fn test_fastembed_batch_embedding() {
        let config =
            super::super::config::EmbeddingConfig::fastembed(None, Some(test_cache_path()));
        let provider = FastEmbedProvider::new(config).unwrap();

        let texts: Vec<&str> = vec!["First text", "Second text", "Third text"];

        let result = EmbeddingProvider::embed_batch(&provider, &texts).await;
        if let Err(ref e) = result {
            eprintln!("FastEmbed batch error: {:?}", e);
        }
        assert!(result.is_ok());

        let embeddings = result.unwrap();
        assert_eq!(embeddings.len(), 3);

        for embedding in embeddings {
            assert_eq!(embedding.len(), 384);
        }
    }

    #[tokio::test]
    async fn test_fastembed_error_handling() {
        let config =
            super::super::config::EmbeddingConfig::fastembed(None, Some(test_cache_path()));
        let provider = FastEmbedProvider::new(config).unwrap();

        // Test empty text
        let result = EmbeddingProvider::embed(&provider, "").await;
        assert!(result.is_err());

        let result = EmbeddingProvider::embed(&provider, "   ").await;
        assert!(result.is_err());

        let empty: Vec<&str> = vec![];
        let result = EmbeddingProvider::embed_batch(&provider, &empty).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_fastembed_list_models() {
        let config = super::super::config::EmbeddingConfig::fastembed(None, None);
        let provider = FastEmbedProvider::new(config).unwrap();

        let models = provider.list_models().await;
        assert!(models.is_ok());

        let models = models.unwrap();
        assert!(!models.is_empty());

        assert!(models.contains(&"bge-small-en-v1.5".to_string()));
        assert!(models.contains(&"all-MiniLM-L6-v2".to_string()));
        assert!(models.contains(&"nomic-embed-text-v1.5".to_string()));
    }

    /// What the user configured is what the provider is built with.
    ///
    /// `FastEmbedProvider::new` hardcoded `cache_dir: None` and
    /// `batch_size: Some(32)`, so `[embedding.fastembed]` keys deserialized,
    /// validated, and were dropped. A user who pointed the model cache at a
    /// large disk silently kept downloading to the default location.
    #[test]
    fn configured_cache_dir_and_batch_size_reach_the_provider() {
        use crucible_core::config::{EmbeddingProviderConfig, FastEmbedConfig};

        let config = EmbeddingProviderConfig::FastEmbed(FastEmbedConfig {
            model: "bge-small-en-v1.5".to_string(),
            cache_dir: Some("/models/cache".to_string()),
            batch_size: 7,
        });

        let provider = FastEmbedProvider::new(config).expect("a known model builds");
        assert_eq!(
            provider.config.cache_dir.as_deref(),
            Some(std::path::Path::new("/models/cache")),
            "the configured model cache directory was discarded"
        );
        assert_eq!(
            provider.config.batch_size,
            Some(7),
            "the configured batch size was discarded"
        );
    }

    // =========================================================================
}
