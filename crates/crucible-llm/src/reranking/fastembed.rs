//! FastEmbed-based reranking implementation.
//!
//! Provides local reranking using ONNX models via the FastEmbed library.
//! Supports multiple reranker models with different speed/quality trade-offs.

use super::{RerankResult, Reranker, RerankerModelInfo};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

// Re-export fastembed types for convenience
pub use fastembed::{RerankInitOptions, RerankerModel, TextRerank};

/// FastEmbed reranker implementation using local ONNX models
pub struct FastEmbedReranker {
    model: Arc<Mutex<Option<TextRerank>>>,
    config: FastEmbedRerankerConfig,
    model_info: RerankerModelInfo,
}

/// Configuration for FastEmbed reranker
#[derive(Debug, Clone)]
pub struct FastEmbedRerankerConfig {
    /// Reranker model to use
    pub model: RerankerModel,
    /// Cache directory for model files
    pub cache_dir: Option<PathBuf>,
    /// Show download progress when fetching models
    pub show_download: bool,
    /// Batch size for processing multiple documents
    pub batch_size: Option<usize>,
}

impl Default for FastEmbedRerankerConfig {
    fn default() -> Self {
        Self {
            model: RerankerModel::BGERerankerBase,
            cache_dir: None,
            show_download: true,
            batch_size: Some(32),
        }
    }
}

impl FastEmbedRerankerConfig {
    /// Create config with BGE reranker base model (default, fast)
    pub fn bge_base() -> Self {
        Self {
            model: RerankerModel::BGERerankerBase,
            ..Default::default()
        }
    }

    /// Create config with BGE reranker v2-m3 (multilingual, 111 languages)
    pub fn bge_v2_m3() -> Self {
        Self {
            model: RerankerModel::BGERerankerV2M3,
            ..Default::default()
        }
    }

    /// Create config with Jina reranker v1-turbo (fastest)
    pub fn jina_v1_turbo() -> Self {
        Self {
            model: RerankerModel::JINARerankerV1TurboEn,
            ..Default::default()
        }
    }

    /// Create config with Jina reranker v2-base (best quality, multilingual)
    pub fn jina_v2_multilingual() -> Self {
        Self {
            model: RerankerModel::JINARerankerV2BaseMultiligual,
            ..Default::default()
        }
    }

    /// Set cache directory
    pub fn with_cache_dir(mut self, cache_dir: PathBuf) -> Self {
        self.cache_dir = Some(cache_dir);
        self
    }

    /// Set batch size
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = Some(batch_size);
        self
    }

    /// Set whether to show download progress
    pub fn with_show_download(mut self, show_download: bool) -> Self {
        self.show_download = show_download;
        self
    }
}

impl FastEmbedReranker {
    /// Create a new FastEmbed reranker with the given configuration
    pub fn new(config: FastEmbedRerankerConfig) -> Result<Self> {
        let model_info = RerankerModelInfo {
            name: format!("{:?}", config.model),
            provider: "FastEmbed".to_string(),
            max_input_length: 512, // Standard max length for most reranker models
        };

        Ok(Self {
            model: Arc::new(Mutex::new(None)),
            config,
            model_info,
        })
    }

    /// Create a reranker with default configuration (BGE base model)
    pub fn default() -> Result<Self> {
        Self::new(FastEmbedRerankerConfig::default())
    }

    /// Ensure the reranker model is loaded, loading it if necessary
    async fn ensure_model_loaded(&self) -> Result<()> {
        let mut model_guard = self.model.lock().await;
        if model_guard.is_none() {
            let cache_dir = self.config.cache_dir.clone();
            let show_download = self.config.show_download;
            let model = self.config.model.clone();

            let loaded_model = tokio::task::spawn_blocking(move || -> Result<TextRerank> {
                let mut init_options = RerankInitOptions::default();
                init_options.model_name = model;
                init_options.cache_dir =
                    cache_dir.unwrap_or_else(|| PathBuf::from("./.fastembed_cache"));
                init_options.show_download_progress = show_download;
                init_options.max_length = 512;

                TextRerank::try_new(init_options)
                    .map_err(|e| anyhow!("Failed to initialize FastEmbed reranker: {}", e))
            })
            .await
            .map_err(|e| anyhow!("Failed to spawn reranker loading task: {}", e))??;

            *model_guard = Some(loaded_model);
        }
        Ok(())
    }

    /// List available reranker models
    pub fn list_supported_models() -> Vec<RerankerModel> {
        vec![
            RerankerModel::BGERerankerBase,
            RerankerModel::BGERerankerV2M3,
            RerankerModel::JINARerankerV1TurboEn,
            RerankerModel::JINARerankerV2BaseMultiligual,
        ]
    }
}

#[async_trait]
impl Reranker for FastEmbedReranker {
    async fn rerank(
        &self,
        query: &str,
        documents: Vec<(String, String, f64)>,
        top_n: Option<usize>,
    ) -> Result<Vec<RerankResult>> {
        // Ensure model is loaded
        self.ensure_model_loaded().await?;

        if documents.is_empty() {
            return Ok(Vec::new());
        }

        // Prepare data for blocking task
        let model_arc = Arc::clone(&self.model);
        let query_owned = query.to_string();
        let doc_texts: Vec<String> = documents.iter().map(|(_, text, _)| text.clone()).collect();
        let batch_size = self.config.batch_size;

        // Run reranking in blocking thread pool
        let reranked =
            tokio::task::spawn_blocking(move || -> Result<Vec<fastembed::RerankResult>> {
                let mut model_guard = model_arc.blocking_lock();
                let model = model_guard
                    .as_mut()
                    .ok_or_else(|| anyhow!("Reranker model not loaded"))?;

                model
                    .rerank(query_owned, doc_texts, true, batch_size)
                    .map_err(|e| anyhow!("Reranking failed: {}", e))
            })
            .await
            .map_err(|e| anyhow!("Failed to spawn reranking task: {}", e))??;

        // Combine rerank scores with original note data
        let mut results: Vec<RerankResult> = reranked
            .into_iter()
            .enumerate()
            .map(|(idx, rerank_result)| {
                let (doc_id, text, _original_score) = &documents[idx];
                RerankResult {
                    document_id: doc_id.clone(),
                    text: text.clone(),
                    score: rerank_result.score as f64,
                    original_index: idx,
                }
            })
            .collect();

        // Sort by rerank score (descending - highest relevance first)
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Apply top_n limit if specified
        if let Some(n) = top_n {
            results.truncate(n);
        }

        Ok(results)
    }

    fn model_info(&self) -> RerankerModelInfo {
        self.model_info.clone()
    }

    async fn health_check(&self) -> Result<bool> {
        // Try to ensure model is loaded as a health check
        match self.ensure_model_loaded().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

impl std::fmt::Debug for FastEmbedReranker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FastEmbedReranker")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Cross-platform test cache path helper
    fn test_cache_path() -> PathBuf {
        std::env::temp_dir().join("crucible_test_fastembed_cache")
    }

    #[test]
    fn test_config_defaults() {
        let config = FastEmbedRerankerConfig::default();
        assert!(matches!(config.model, RerankerModel::BGERerankerBase));
        assert!(config.show_download);
        assert_eq!(config.batch_size, Some(32));
    }

    #[test]
    fn test_config_builders() {
        let bge_base = FastEmbedRerankerConfig::bge_base();
        assert!(matches!(bge_base.model, RerankerModel::BGERerankerBase));

        let bge_v2 = FastEmbedRerankerConfig::bge_v2_m3();
        assert!(matches!(bge_v2.model, RerankerModel::BGERerankerV2M3));

        let jina_v1 = FastEmbedRerankerConfig::jina_v1_turbo();
        assert!(matches!(
            jina_v1.model,
            RerankerModel::JINARerankerV1TurboEn
        ));

        let jina_v2 = FastEmbedRerankerConfig::jina_v2_multilingual();
        assert!(matches!(
            jina_v2.model,
            RerankerModel::JINARerankerV2BaseMultiligual
        ));
    }

    #[test]
    fn test_config_with_methods() {
        let cache_dir = test_cache_path();
        let config = FastEmbedRerankerConfig::default()
            .with_cache_dir(cache_dir.clone())
            .with_batch_size(64)
            .with_show_download(false);

        assert_eq!(config.cache_dir, Some(cache_dir));
        assert_eq!(config.batch_size, Some(64));
        assert!(!config.show_download);
    }

    #[test]
    fn test_reranker_creation() {
        let config = FastEmbedRerankerConfig::default();
        let reranker = FastEmbedReranker::new(config);
        assert!(reranker.is_ok());

        let reranker = reranker.unwrap();
        let info = reranker.model_info();
        assert_eq!(info.provider, "FastEmbed");
        assert_eq!(info.max_input_length, 512);
    }

    #[test]
    fn test_list_supported_models() {
        let models = FastEmbedReranker::list_supported_models();
        assert_eq!(models.len(), 4);
        assert!(models.contains(&RerankerModel::BGERerankerBase));
        assert!(models.contains(&RerankerModel::BGERerankerV2M3));
        assert!(models.contains(&RerankerModel::JINARerankerV1TurboEn));
        assert!(models.contains(&RerankerModel::JINARerankerV2BaseMultiligual));
    }

    #[tokio::test]
    async fn test_rerank_empty_documents() {
        let mut config = FastEmbedRerankerConfig::default();
        config.cache_dir = Some(test_cache_path());
        let reranker = FastEmbedReranker::new(config).unwrap();

        let results = reranker.rerank("test query", vec![], None).await.unwrap();

        assert!(results.is_empty());
    }
}
