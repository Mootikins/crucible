//! Inference backend abstraction for embedding generation
//!
//! This module provides a pluggable backend system for running embedding model inference.
//! Different backends can be used depending on platform capabilities:
//! - `LlamaCppBackend`: Uses llama.cpp for GGUF models (Vulkan, CUDA, Metal, CPU)
//! - `BurnBackend`: Uses Burn framework for SafeTensors models (wgpu/Vulkan)
//! - `MockBackend`: For testing without actual model loading

use super::error::{EmbeddingError, EmbeddingResult};
use std::path::Path;

/// Device type for inference acceleration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceType {
    /// Automatic detection (prefer GPU, fallback to CPU)
    Auto,
    /// CPU only
    Cpu,
    /// Vulkan GPU (AMD, Intel, NVIDIA on Linux/Windows)
    Vulkan,
    /// CUDA GPU (NVIDIA)
    Cuda,
    /// Metal GPU (Apple Silicon)
    Metal,
    /// ROCm/HIP (AMD with ROCm)
    Rocm,
    /// OpenCL (cross-platform GPU)
    OpenCL,
}

impl Default for DeviceType {
    fn default() -> Self {
        Self::Auto
    }
}

impl DeviceType {
    /// Parse from string (case-insensitive)
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "auto" => Self::Auto,
            "cpu" => Self::Cpu,
            "vulkan" => Self::Vulkan,
            "cuda" => Self::Cuda,
            "metal" => Self::Metal,
            "rocm" | "hip" | "hipblas" => Self::Rocm,
            "opencl" => Self::OpenCL,
            _ => Self::Auto,
        }
    }

    /// Check if this is a GPU device type
    pub fn is_gpu(&self) -> bool {
        !matches!(self, Self::Cpu)
    }
}

/// Configuration for inference backend initialization
#[derive(Debug, Clone)]
pub struct BackendConfig {
    /// Device type to use
    pub device: DeviceType,
    /// Number of GPU layers to offload (0 = CPU only, -1 = all)
    pub gpu_layers: i32,
    /// Number of threads for CPU operations
    pub threads: Option<usize>,
    /// Context size (max tokens)
    pub context_size: usize,
    /// Batch size for inference
    pub batch_size: usize,
    /// Whether to use memory mapping for model loading
    pub use_mmap: bool,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            device: DeviceType::Auto,
            gpu_layers: -1, // Offload all layers to GPU by default
            threads: None,  // Auto-detect
            context_size: 512,
            batch_size: 512,
            use_mmap: true,
        }
    }
}

/// Model format detected from file
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelFormat {
    /// GGUF quantized format (llama.cpp compatible)
    Gguf,
    /// SafeTensors format (HuggingFace)
    SafeTensors,
    /// PyTorch checkpoint
    PyTorch,
    /// Unknown format
    Unknown,
}

impl ModelFormat {
    /// Detect format from file path
    pub fn from_path(path: &Path) -> Self {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());

        match ext.as_deref() {
            Some("gguf") => Self::Gguf,
            Some("safetensors") => Self::SafeTensors,
            Some("pt") | Some("pth") | Some("bin") => Self::PyTorch,
            _ => Self::Unknown,
        }
    }
}

/// Information about a loaded model
#[derive(Debug, Clone)]
pub struct LoadedModelInfo {
    /// Model file path
    pub path: std::path::PathBuf,
    /// Model format
    pub format: ModelFormat,
    /// Embedding dimensions
    pub dimensions: usize,
    /// Vocabulary size
    pub vocab_size: usize,
    /// Context length
    pub context_length: usize,
    /// Quantization type (e.g., "Q8_0", "F16")
    pub quantization: Option<String>,
    /// Device being used
    pub device: DeviceType,
    /// Number of layers offloaded to GPU
    pub gpu_layers: i32,
}

/// Trait for inference backends that can generate embeddings
///
/// This trait abstracts over different inference engines (llama.cpp, Burn, etc.)
/// to provide a unified interface for embedding generation.
///
/// # Implementors
///
/// - `LlamaCppBackend`: GGUF models via llama.cpp (Vulkan, CUDA, Metal, CPU)
/// - `BurnBackend`: SafeTensors models via Burn framework
/// - `MockBackend`: Testing without real models
pub trait InferenceBackend: Send + Sync {
    /// Load a model from file
    ///
    /// # Arguments
    /// * `model_path` - Path to the model file (.gguf, .safetensors, etc.)
    /// * `config` - Backend configuration
    ///
    /// # Returns
    /// Information about the loaded model
    fn load_model(&mut self, model_path: &Path, config: &BackendConfig) -> EmbeddingResult<LoadedModelInfo>;

    /// Generate embeddings for a batch of token sequences
    ///
    /// # Arguments
    /// * `token_batches` - Vector of token ID sequences
    ///
    /// # Returns
    /// Vector of embedding vectors, one per input sequence
    fn embed_tokens(&self, token_batches: &[Vec<u32>]) -> EmbeddingResult<Vec<Vec<f32>>>;

    /// Generate embeddings for a batch of texts (tokenizes internally)
    ///
    /// # Arguments
    /// * `texts` - Vector of text strings to embed
    ///
    /// # Returns
    /// Vector of embedding vectors, one per input text
    fn embed_texts(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>>;

    /// Get the embedding dimensions for this model
    fn dimensions(&self) -> usize;

    /// Get information about the loaded model
    fn model_info(&self) -> Option<&LoadedModelInfo>;

    /// Check if a model is currently loaded
    fn is_loaded(&self) -> bool;

    /// Unload the current model and free resources
    fn unload(&mut self);

    /// Get the backend name (for logging/debugging)
    fn backend_name(&self) -> &'static str;

    /// Check what device types this backend supports
    fn supported_devices(&self) -> Vec<DeviceType>;
}

/// Factory for creating inference backends based on model format and available hardware
pub struct BackendFactory;

impl BackendFactory {
    /// Detect the best backend for a given model file
    pub fn detect_backend(model_path: &Path, preferred_device: DeviceType) -> EmbeddingResult<Box<dyn InferenceBackend>> {
        let format = ModelFormat::from_path(model_path);

        match format {
            ModelFormat::Gguf => {
                #[cfg(feature = "llama-cpp")]
                {
                    return Ok(Box::new(super::llama_cpp_backend::LlamaCppBackend::new(preferred_device)?));
                }

                #[cfg(not(feature = "llama-cpp"))]
                {
                    Err(EmbeddingError::ConfigError(
                        "GGUF models require the 'llama-cpp' feature. Enable it with: --features llama-cpp".to_string()
                    ))
                }
            }
            ModelFormat::SafeTensors => {
                #[cfg(feature = "burn")]
                {
                    return Ok(Box::new(super::burn_backend::BurnBackend::new(preferred_device)?));
                }

                #[cfg(not(feature = "burn"))]
                {
                    Err(EmbeddingError::ConfigError(
                        "SafeTensors models require the 'burn' feature. Enable it with: --features burn-vulkan".to_string()
                    ))
                }
            }
            _ => Err(EmbeddingError::ConfigError(format!(
                "Unsupported model format for: {}",
                model_path.display()
            ))),
        }
    }

    /// Create a mock backend for testing
    #[cfg(any(test, feature = "test-utils"))]
    pub fn mock(dimensions: usize) -> Box<dyn InferenceBackend> {
        Box::new(MockBackend::new(dimensions))
    }
}

/// Mock backend for testing without actual model loading
#[cfg(any(test, feature = "test-utils"))]
pub struct MockBackend {
    dimensions: usize,
    loaded: bool,
}

#[cfg(any(test, feature = "test-utils"))]
impl MockBackend {
    pub fn new(dimensions: usize) -> Self {
        Self {
            dimensions,
            loaded: false,
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl InferenceBackend for MockBackend {
    fn load_model(&mut self, _model_path: &Path, _config: &BackendConfig) -> EmbeddingResult<LoadedModelInfo> {
        self.loaded = true;
        Ok(LoadedModelInfo {
            path: std::path::PathBuf::from("mock_model.gguf"),
            format: ModelFormat::Gguf,
            dimensions: self.dimensions,
            vocab_size: 32000,
            context_length: 512,
            quantization: Some("Q8_0".to_string()),
            device: DeviceType::Cpu,
            gpu_layers: 0,
        })
    }

    fn embed_tokens(&self, token_batches: &[Vec<u32>]) -> EmbeddingResult<Vec<Vec<f32>>> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        Ok(token_batches
            .iter()
            .map(|tokens| {
                let mut hasher = DefaultHasher::new();
                tokens.hash(&mut hasher);
                let hash = hasher.finish();

                let mut embedding = vec![0.0f32; self.dimensions];
                for (i, val) in embedding.iter_mut().enumerate() {
                    let v = hash.wrapping_mul(31).wrapping_add(i as u64);
                    *val = ((v % 10000) as f32 / 5000.0 - 1.0) * 0.1;
                }

                // Normalize
                let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for val in &mut embedding {
                        *val /= norm;
                    }
                }

                embedding
            })
            .collect())
    }

    fn embed_texts(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
        // Simple mock tokenization: just use character codes
        let token_batches: Vec<Vec<u32>> = texts
            .iter()
            .map(|text| text.chars().map(|c| c as u32).collect())
            .collect();

        self.embed_tokens(&token_batches)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_info(&self) -> Option<&LoadedModelInfo> {
        None
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    fn unload(&mut self) {
        self.loaded = false;
    }

    fn backend_name(&self) -> &'static str {
        "mock"
    }

    fn supported_devices(&self) -> Vec<DeviceType> {
        vec![DeviceType::Cpu]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_type_from_str() {
        assert_eq!(DeviceType::from_str("vulkan"), DeviceType::Vulkan);
        assert_eq!(DeviceType::from_str("CUDA"), DeviceType::Cuda);
        assert_eq!(DeviceType::from_str("rocm"), DeviceType::Rocm);
        assert_eq!(DeviceType::from_str("hip"), DeviceType::Rocm);
        assert_eq!(DeviceType::from_str("unknown"), DeviceType::Auto);
    }

    #[test]
    fn test_model_format_detection() {
        assert_eq!(
            ModelFormat::from_path(Path::new("model.gguf")),
            ModelFormat::Gguf
        );
        assert_eq!(
            ModelFormat::from_path(Path::new("model.safetensors")),
            ModelFormat::SafeTensors
        );
        assert_eq!(
            ModelFormat::from_path(Path::new("model.pt")),
            ModelFormat::PyTorch
        );
    }

    #[test]
    fn test_mock_backend() {
        let mut backend = MockBackend::new(384);
        assert!(!backend.is_loaded());

        let info = backend
            .load_model(Path::new("test.gguf"), &BackendConfig::default())
            .unwrap();
        assert!(backend.is_loaded());
        assert_eq!(info.dimensions, 384);

        let embeddings = backend.embed_texts(&["hello", "world"]).unwrap();
        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), 384);

        backend.unload();
        assert!(!backend.is_loaded());
    }
}
