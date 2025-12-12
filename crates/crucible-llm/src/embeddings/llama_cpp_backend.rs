//! llama.cpp backend for GGUF model inference
//!
//! This module implements both `InferenceBackend` and `EmbeddingProvider` traits using
//! llama.cpp via the `llama-cpp-2` crate. It supports GPU acceleration through Vulkan,
//! CUDA, Metal, or ROCm/HIP depending on compile-time features.
//!
//! The backend supports background model loading - construction returns immediately
//! while the model loads in a background thread, allowing other work to proceed.
//!
//! ## Device Detection
//!
//! The backend uses runtime detection via `list_available_devices()` to find GPUs.
//! When `DeviceType::Auto` is specified, it picks the best available accelerator.

use async_trait::async_trait;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;

use llama_cpp_2::context::params::{LlamaContextParams, LlamaPoolingType};
use llama_cpp_2::list_llama_ggml_backend_devices;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};

use super::error::{EmbeddingError, EmbeddingResult};
use super::inference::{BackendConfig, DeviceType, InferenceBackend, LoadedModelInfo, ModelFormat};
use super::provider::{
    EmbeddingProvider, EmbeddingResponse, ModelFamily, ModelInfo, ParameterSize,
};

/// Default context size for embeddings
const DEFAULT_CONTEXT_SIZE: u32 = 512;

/// Information about an available compute device
#[derive(Debug, Clone)]
pub struct AvailableDevice {
    /// Device name (e.g., "Radeon 8060S Graphics")
    pub name: String,
    /// Device description
    pub description: String,
    /// Backend name (e.g., "Vulkan", "CUDA", "CPU")
    pub backend: String,
    /// Total memory in bytes
    pub memory_total: u64,
    /// Free memory in bytes
    pub memory_free: u64,
    /// Mapped device type
    pub device_type: DeviceType,
}

/// State of model loading
enum LoadState {
    /// Model is currently loading in background thread
    Loading(JoinHandle<Result<LoadedState, String>>),
    /// Model is loaded and ready
    Ready(LoadedState),
    /// Model loading failed
    Failed(String),
    /// No model configured (uninitialized)
    Empty,
}

/// State when model is loaded
struct LoadedState {
    backend: LlamaBackend,
    model: Arc<LlamaModel>,
    model_info: LoadedModelInfo,
    model_name: String,
}

/// llama.cpp-based inference backend for GGUF models
///
/// This backend implements both `InferenceBackend` (low-level, synchronous) and
/// `EmbeddingProvider` (high-level, async) traits, allowing it to be used directly
/// as an embedding provider via `Arc<LlamaCppBackend>`.
///
/// Model loading happens in a background thread - construction returns immediately
/// and the first embedding request will wait for loading to complete.
pub struct LlamaCppBackend {
    /// Model loading state (protected by RwLock for background loading)
    state: RwLock<LoadState>,
    /// Preferred device type
    preferred_device: DeviceType,
    /// Number of threads for inference
    n_threads: i32,
    /// Backend configuration
    config: BackendConfig,
    /// Model path (for display/debugging)
    model_path: PathBuf,
}

impl std::fmt::Debug for LlamaCppBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlamaCppBackend")
            .field("model_path", &self.model_path)
            .field("preferred_device", &self.preferred_device)
            .field("n_threads", &self.n_threads)
            .finish()
    }
}

impl LlamaCppBackend {
    /// Create a new llama.cpp backend with the specified device preference
    ///
    /// This creates an uninitialized backend. Call `load_model` or use
    /// `new_with_model` to load a model.
    pub fn new(preferred_device: DeviceType) -> EmbeddingResult<Self> {
        // Validate that the requested device is supported by this build
        let supported = Self::get_supported_devices();
        if !matches!(preferred_device, DeviceType::Auto | DeviceType::Cpu)
            && !supported.contains(&preferred_device)
        {
            tracing::warn!(
                "Requested device {:?} not supported by this build. Available: {:?}",
                preferred_device,
                supported
            );
        }

        // Default to number of physical cores - 1
        let n_threads = (num_cpus::get_physical().saturating_sub(1)).max(1) as i32;

        Ok(Self {
            state: RwLock::new(LoadState::Empty),
            preferred_device,
            n_threads,
            config: BackendConfig::default(),
            model_path: PathBuf::new(),
        })
    }

    /// Create a new backend and start loading a model in the background
    ///
    /// Returns immediately - the model loads in a background thread.
    /// First embedding request will wait for loading to complete.
    pub fn new_with_model(model_path: PathBuf, device: DeviceType) -> EmbeddingResult<Self> {
        Self::new_with_model_and_config(model_path, device, BackendConfig::default())
    }

    /// Create a new backend with custom config and start loading in background
    pub fn new_with_model_and_config(
        model_path: PathBuf,
        device: DeviceType,
        config: BackendConfig,
    ) -> EmbeddingResult<Self> {
        let n_threads = (num_cpus::get_physical().saturating_sub(1)).max(1) as i32;

        // Validate path before spawning thread
        if !model_path.exists() {
            return Err(EmbeddingError::ConfigError(format!(
                "Model file not found: {}",
                model_path.display()
            )));
        }

        let format = ModelFormat::from_path(&model_path);
        if format != ModelFormat::Gguf {
            return Err(EmbeddingError::ConfigError(format!(
                "LlamaCppBackend only supports GGUF files, got: {}",
                model_path.display()
            )));
        }

        // Clone for background thread
        let path_clone = model_path.clone();
        let config_clone = config.clone();
        let device_clone = device.clone();

        // Start background loading
        let handle = std::thread::spawn(move || {
            Self::load_model_sync(&path_clone, &config_clone, &device_clone, n_threads)
        });

        Ok(Self {
            state: RwLock::new(LoadState::Loading(handle)),
            preferred_device: device,
            n_threads,
            config,
            model_path,
        })
    }

    /// Synchronous model loading (runs in background thread)
    fn load_model_sync(
        model_path: &Path,
        config: &BackendConfig,
        preferred_device: &DeviceType,
        _n_threads: i32, // Reserved for future use; thread count applied during inference
    ) -> Result<LoadedState, String> {
        let device = Self::detect_best_device_static(preferred_device);

        tracing::info!(
            "Loading GGUF model from {} with device {:?}",
            model_path.display(),
            device
        );

        // Initialize llama.cpp backend
        let backend =
            LlamaBackend::init().map_err(|e| format!("Failed to initialize llama.cpp: {}", e))?;

        // Build model params
        let gpu_layers = if config.gpu_layers < 0 {
            999
        } else {
            config.gpu_layers as u32
        };
        let model_params = LlamaModelParams::default().with_n_gpu_layers(gpu_layers);

        // Load the model
        let model = LlamaModel::load_from_file(&backend, model_path, &model_params)
            .map_err(|e| format!("Failed to load model: {}", e))?;

        // Extract model info
        let dimensions = model.n_embd() as usize;
        let vocab_size = model.n_vocab() as usize;
        let context_length = model.n_ctx_train() as usize;

        // Get model name from path
        let model_name = model_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("llama-cpp-model")
            .to_string();

        // Try to get quantization info from filename
        let quantization = model_path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|name| {
                let patterns = [
                    "Q2_K", "Q3_K_S", "Q3_K_M", "Q3_K_L", "Q4_0", "Q4_K_S", "Q4_K_M", "Q5_0",
                    "Q5_K_S", "Q5_K_M", "Q6_K", "Q8_0", "F16", "F32",
                ];
                for p in patterns {
                    if name.contains(p) {
                        return Some(p.to_string());
                    }
                }
                None
            });

        let info = LoadedModelInfo {
            path: model_path.to_path_buf(),
            format: ModelFormat::Gguf,
            dimensions,
            vocab_size,
            context_length,
            quantization,
            device: device.clone(),
            gpu_layers: config.gpu_layers,
        };

        tracing::info!(
            "Model loaded: {} dimensions, {} vocab, {} context, {:?} device",
            dimensions,
            vocab_size,
            context_length,
            device
        );

        Ok(LoadedState {
            backend,
            model: Arc::new(model),
            model_info: info,
            model_name,
        })
    }

    /// Wait for model to be ready, returning a reference to loaded state
    fn ensure_loaded(&self) -> EmbeddingResult<()> {
        // First check if already ready (fast path)
        {
            let state = self
                .state
                .read()
                .map_err(|e| EmbeddingError::InferenceFailed(format!("Lock poisoned: {}", e)))?;

            match &*state {
                LoadState::Ready(_) => return Ok(()),
                LoadState::Failed(e) => {
                    return Err(EmbeddingError::InferenceFailed(e.clone()));
                }
                LoadState::Empty => {
                    return Err(EmbeddingError::InferenceFailed(
                        "No model loaded".to_string(),
                    ));
                }
                LoadState::Loading(_) => {
                    // Need to wait - drop read lock and acquire write lock
                }
            }
        }

        // Acquire write lock to wait for loading
        let mut state = self
            .state
            .write()
            .map_err(|e| EmbeddingError::InferenceFailed(format!("Lock poisoned: {}", e)))?;

        // Check again after acquiring write lock (another thread may have completed)
        match &*state {
            LoadState::Ready(_) => return Ok(()),
            LoadState::Failed(e) => {
                return Err(EmbeddingError::InferenceFailed(e.clone()));
            }
            LoadState::Empty => {
                return Err(EmbeddingError::InferenceFailed(
                    "No model loaded".to_string(),
                ));
            }
            LoadState::Loading(_) => {
                // We need to take ownership of the handle to join it
            }
        }

        // Take the loading handle
        let old_state = std::mem::replace(&mut *state, LoadState::Empty);
        if let LoadState::Loading(handle) = old_state {
            match handle.join() {
                Ok(Ok(loaded)) => {
                    *state = LoadState::Ready(loaded);
                    Ok(())
                }
                Ok(Err(e)) => {
                    *state = LoadState::Failed(e.clone());
                    Err(EmbeddingError::InferenceFailed(e))
                }
                Err(_) => {
                    let msg = "Background loading thread panicked".to_string();
                    *state = LoadState::Failed(msg.clone());
                    Err(EmbeddingError::InferenceFailed(msg))
                }
            }
        } else {
            // Shouldn't happen, but handle gracefully
            *state = old_state;
            Err(EmbeddingError::InferenceFailed(
                "Unexpected state".to_string(),
            ))
        }
    }

    /// Execute a function with the loaded state
    fn with_loaded<F, T>(&self, f: F) -> EmbeddingResult<T>
    where
        F: FnOnce(&LoadedState) -> EmbeddingResult<T>,
    {
        self.ensure_loaded()?;

        let state = self
            .state
            .read()
            .map_err(|e| EmbeddingError::InferenceFailed(format!("Lock poisoned: {}", e)))?;

        match &*state {
            LoadState::Ready(loaded) => f(loaded),
            _ => Err(EmbeddingError::InferenceFailed(
                "Model not ready".to_string(),
            )),
        }
    }

    /// List available backend devices at runtime
    ///
    /// This queries llama.cpp for actually available devices (GPUs, CPUs) rather
    /// than just compile-time feature flags. Useful for diagnostics and auto-detection.
    pub fn list_available_devices() -> Vec<AvailableDevice> {
        list_llama_ggml_backend_devices()
            .into_iter()
            .map(|dev| AvailableDevice {
                name: dev.name,
                description: dev.description,
                backend: dev.backend,
                memory_total: dev.memory_total as u64,
                memory_free: dev.memory_free as u64,
                device_type: match dev.device_type {
                    llama_cpp_2::LlamaBackendDeviceType::Gpu => DeviceType::Vulkan, // Most common
                    llama_cpp_2::LlamaBackendDeviceType::IntegratedGpu => DeviceType::Vulkan,
                    llama_cpp_2::LlamaBackendDeviceType::Accelerator => DeviceType::Cuda,
                    _ => DeviceType::Cpu,
                },
            })
            .collect()
    }

    /// Get the list of devices supported by this build (compile-time check)
    fn get_supported_devices() -> Vec<DeviceType> {
        let mut devices = vec![DeviceType::Cpu];

        #[cfg(feature = "llama-cpp-vulkan")]
        devices.push(DeviceType::Vulkan);

        #[cfg(feature = "llama-cpp-cuda")]
        devices.push(DeviceType::Cuda);

        #[cfg(feature = "llama-cpp-metal")]
        devices.push(DeviceType::Metal);

        devices
    }

    /// Detect the best available device using runtime detection
    ///
    /// Prefers GPU backends in order: Vulkan > CUDA > Metal > CPU
    fn detect_best_device_static(preferred: &DeviceType) -> DeviceType {
        if *preferred != DeviceType::Auto {
            return preferred.clone();
        }

        // Use runtime detection for actual available devices
        let runtime_devices = Self::list_available_devices();

        // Check for GPU backends by name
        for dev in &runtime_devices {
            let backend_lower = dev.backend.to_lowercase();
            if backend_lower.contains("vulkan") {
                tracing::info!(
                    "Auto-detected Vulkan GPU: {} ({})",
                    dev.name,
                    dev.description
                );
                return DeviceType::Vulkan;
            }
        }

        for dev in &runtime_devices {
            let backend_lower = dev.backend.to_lowercase();
            if backend_lower.contains("cuda") {
                tracing::info!("Auto-detected CUDA GPU: {} ({})", dev.name, dev.description);
                return DeviceType::Cuda;
            }
        }

        for dev in &runtime_devices {
            let backend_lower = dev.backend.to_lowercase();
            if backend_lower.contains("metal") {
                tracing::info!(
                    "Auto-detected Metal GPU: {} ({})",
                    dev.name,
                    dev.description
                );
                return DeviceType::Metal;
            }
        }

        // Fall back to compile-time check
        let supported = Self::get_supported_devices();
        for device in [DeviceType::Vulkan, DeviceType::Cuda, DeviceType::Metal] {
            if supported.contains(&device) {
                return device;
            }
        }

        DeviceType::Cpu
    }

    /// Internal batch embedding for a chunk of texts using an existing context
    ///
    /// The context is cleared and reused between chunks to avoid expensive
    /// context creation/destruction on GPU.
    fn embed_chunk_with_context(
        ctx: &mut llama_cpp_2::context::LlamaContext<'_>,
        model: &LlamaModel,
        texts: &[&str],
    ) -> EmbeddingResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Tokenize all texts first
        let mut token_batches: Vec<Vec<llama_cpp_2::token::LlamaToken>> =
            Vec::with_capacity(texts.len());
        let mut total_tokens = 0;

        for text in texts.iter() {
            let tokens = model.str_to_token(text, AddBos::Always).map_err(|e| {
                EmbeddingError::InferenceFailed(format!("Tokenization failed: {}", e))
            })?;
            total_tokens += tokens.len();
            token_batches.push(tokens);
        }

        let n_seqs = texts.len();

        // Create a batch for all sequences
        let mut batch = LlamaBatch::new(total_tokens, n_seqs as i32);

        // Add all tokens to batch, each text gets its own sequence ID
        for (seq_id, tokens) in token_batches.iter().enumerate() {
            for (pos, token) in tokens.iter().enumerate() {
                let is_last = pos == tokens.len() - 1;
                batch
                    .add(*token, pos as i32, &[seq_id as i32], is_last)
                    .map_err(|e| {
                        EmbeddingError::InferenceFailed(format!(
                            "Failed to add token to batch: {}",
                            e
                        ))
                    })?;
            }
        }

        // Process all sequences in one decode call
        ctx.decode(&mut batch)
            .map_err(|e| EmbeddingError::InferenceFailed(format!("Decode failed: {}", e)))?;

        // Extract embeddings for each sequence
        let mut embeddings = Vec::with_capacity(texts.len());

        for seq_id in 0..texts.len() {
            let embedding = ctx.embeddings_seq_ith(seq_id as i32).map_err(|e| {
                EmbeddingError::InferenceFailed(format!(
                    "Failed to get embeddings for sequence {}: {}",
                    seq_id, e
                ))
            })?;

            // Normalize the embedding (L2 normalization)
            let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
            let normalized: Vec<f32> = if norm > 0.0 {
                embedding.iter().map(|x| x / norm).collect()
            } else {
                embedding.to_vec()
            };

            embeddings.push(normalized);
        }

        // Clear KV cache for next batch (embedding models don't use it, but be safe)
        ctx.clear_kv_cache();

        Ok(embeddings)
    }

    /// Internal batch embedding - creates ONE context and reuses it for all chunks
    fn embed_batch_internal(
        loaded: &LoadedState,
        texts: &[&str],
        n_threads: i32,
    ) -> EmbeddingResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let model = &loaded.model;
        let backend = &loaded.backend;

        // Calculate max context size needed for any chunk
        // Use a generous size that can handle most text batches
        let ctx_size = DEFAULT_CONTEXT_SIZE;
        const MAX_BATCH_SIZE: usize = 8;

        // Create ONE context with enough capacity, reuse for all chunks
        let ctx_params = LlamaContextParams::default()
            .with_embeddings(true)
            .with_n_threads(n_threads)
            .with_n_threads_batch(n_threads)
            .with_pooling_type(LlamaPoolingType::Mean)
            .with_n_ctx(NonZeroU32::new(ctx_size))
            .with_n_batch(ctx_size)
            .with_n_ubatch(ctx_size)
            .with_n_seq_max(MAX_BATCH_SIZE as u32);

        let mut ctx = model.new_context(backend, ctx_params).map_err(|e| {
            EmbeddingError::InferenceFailed(format!("Failed to create context: {}", e))
        })?;

        // Process all texts in chunks, reusing the same context
        let mut all_embeddings = Vec::with_capacity(texts.len());

        for chunk in texts.chunks(MAX_BATCH_SIZE) {
            let chunk_embeddings = Self::embed_chunk_with_context(&mut ctx, model, chunk)?;
            all_embeddings.extend(chunk_embeddings);
        }

        Ok(all_embeddings)
    }
}

// ============================================================================
// InferenceBackend implementation (low-level synchronous API)
// ============================================================================

impl InferenceBackend for LlamaCppBackend {
    fn load_model(
        &mut self,
        model_path: &Path,
        config: &BackendConfig,
    ) -> EmbeddingResult<LoadedModelInfo> {
        // Verify it's a GGUF file
        let format = ModelFormat::from_path(model_path);
        if format != ModelFormat::Gguf {
            return Err(EmbeddingError::ConfigError(format!(
                "LlamaCppBackend only supports GGUF files, got: {}",
                model_path.display()
            )));
        }

        // Verify file exists
        if !model_path.exists() {
            return Err(EmbeddingError::ConfigError(format!(
                "Model file not found: {}",
                model_path.display()
            )));
        }

        // Load synchronously (this is the legacy API)
        let loaded =
            Self::load_model_sync(model_path, config, &self.preferred_device, self.n_threads)
                .map_err(EmbeddingError::InferenceFailed)?;

        let info = loaded.model_info.clone();

        // Update state
        let mut state = self
            .state
            .write()
            .map_err(|e| EmbeddingError::InferenceFailed(format!("Lock poisoned: {}", e)))?;
        *state = LoadState::Ready(loaded);
        self.model_path = model_path.to_path_buf();
        self.config = config.clone();

        Ok(info)
    }

    fn embed_tokens(&self, _token_batches: &[Vec<u32>]) -> EmbeddingResult<Vec<Vec<f32>>> {
        Err(EmbeddingError::InferenceFailed(
            "embed_tokens not directly supported by llama.cpp backend. Use embed_texts instead."
                .to_string(),
        ))
    }

    fn embed_texts(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let n_threads = self.n_threads;

        // embed_batch_internal creates ONE context and reuses it for all chunks
        self.with_loaded(|loaded| Self::embed_batch_internal(loaded, texts, n_threads))
    }

    fn dimensions(&self) -> usize {
        self.with_loaded(|loaded| Ok(loaded.model_info.dimensions))
            .unwrap_or(0)
    }

    fn model_info(&self) -> Option<&LoadedModelInfo> {
        // Can't return reference through RwLock, so this always returns None
        // Use the EmbeddingProvider API instead
        None
    }

    fn is_loaded(&self) -> bool {
        self.state
            .read()
            .map(|s| matches!(&*s, LoadState::Ready(_)))
            .unwrap_or(false)
    }

    fn unload(&mut self) {
        if let Ok(mut state) = self.state.write() {
            *state = LoadState::Empty;
        }
        tracing::info!("Model unloaded");
    }

    fn backend_name(&self) -> &'static str {
        "llama.cpp"
    }

    fn supported_devices(&self) -> Vec<DeviceType> {
        Self::get_supported_devices()
    }
}

// ============================================================================
// EmbeddingProvider implementation (high-level async API)
// ============================================================================

#[async_trait]
impl EmbeddingProvider for LlamaCppBackend {
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        if text.trim().is_empty() {
            return Err(EmbeddingError::Other("Text cannot be empty".to_string()));
        }

        let embeddings = self.embed_texts(&[text])?;

        let embedding = embeddings
            .into_iter()
            .next()
            .ok_or_else(|| EmbeddingError::InferenceFailed("No embedding returned".to_string()))?;

        let model_name = self.with_loaded(|loaded| Ok(loaded.model_name.clone()))?;

        Ok(EmbeddingResponse::new(embedding, model_name))
    }

    async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let embeddings = self.embed_texts(&text_refs)?;

        let model_name = self.with_loaded(|loaded| Ok(loaded.model_name.clone()))?;

        Ok(embeddings
            .into_iter()
            .map(|emb| EmbeddingResponse::new(emb, model_name.clone()))
            .collect())
    }

    fn model_name(&self) -> &str {
        // Return from model_path since we can't hold a reference through the lock
        self.model_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("llama-cpp")
    }

    fn dimensions(&self) -> usize {
        InferenceBackend::dimensions(self)
    }

    fn provider_name(&self) -> &str {
        "LlamaCpp"
    }

    async fn health_check(&self) -> EmbeddingResult<bool> {
        match self.embed("health check").await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn list_models(&self) -> EmbeddingResult<Vec<ModelInfo>> {
        self.with_loaded(|loaded| {
            let info = ModelInfo::builder()
                .name(&loaded.model_name)
                .display_name(&loaded.model_name)
                .family(ModelFamily::Bert) // nomic-embed is BERT-based
                .dimensions(loaded.model_info.dimensions)
                .parameter_size(ParameterSize::new(137, true))
                .format("gguf")
                .build();

            Ok(vec![info])
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_creation() {
        let backend = LlamaCppBackend::new(DeviceType::Auto).unwrap();
        assert!(!backend.is_loaded());
        assert_eq!(backend.backend_name(), "llama.cpp");
    }

    #[test]
    fn test_supported_devices() {
        let backend = LlamaCppBackend::new(DeviceType::Cpu).unwrap();
        let devices = backend.supported_devices();
        assert!(devices.contains(&DeviceType::Cpu));
    }

    #[test]
    fn test_model_format_validation() {
        let mut backend = LlamaCppBackend::new(DeviceType::Cpu).unwrap();
        let config = BackendConfig::default();

        // Should reject non-GGUF files
        let result = backend.load_model(Path::new("model.safetensors"), &config);
        assert!(result.is_err());
    }
}
