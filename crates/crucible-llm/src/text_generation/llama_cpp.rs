//! LlamaCpp text generation provider with grammar-constrained generation
//!
//! This module provides text generation using llama.cpp via the `llama-cpp-2` crate,
//! with support for GBNF grammar constraints for structured output.
//!
//! ## Grammar Integration
//!
//! The provider integrates with crucible-core's Grammar type for structured generation:
//!
//! ```rust,no_run
//! use crucible_core::types::grammar::presets;
//! use crucible_llm::text_generation::LlamaCppTextProvider;
//!
//! // Create provider with a loaded model
//! let provider = LlamaCppTextProvider::new_with_model("model.gguf".into())?;
//!
//! // Generate with grammar constraint
//! let grammar = presets::l0_l1_tools();
//! let (response, tokens) = provider.generate_text(
//!     "Call a tool to read a file",
//!     Some(grammar.as_str()),
//!     256,
//!     0.7,
//!     None
//! )?;
//! ```

use async_trait::async_trait;
use chrono::Utc;
use crucible_config::BackendType;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;
use uuid::Uuid;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;

use super::types::{
    ChatCompletionChoice, ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse,
    CompletionChunk, CompletionRequest, CompletionResponse, LlmError, LlmMessage, LlmResult,
    MessageRole, ModelCapability, ModelStatus, ProviderCapabilities, TextGenerationProvider,
    TextModelInfo, TokenUsage,
};

// Provider trait imports
use crucible_core::traits::provider::{
    CanConstrainGeneration, ConstrainedRequest, ConstrainedResponse, ExtendedCapabilities,
    Provider, SchemaFormat,
};

/// Default context size for text generation
const DEFAULT_CONTEXT_SIZE: u32 = 4096;

/// Default maximum tokens to generate
const DEFAULT_MAX_TOKENS: u32 = 256;

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
    #[allow(dead_code)]
    backend: LlamaBackend,
    model: Arc<LlamaModel>,
    model_name: String,
    context_length: usize,
}

/// Configuration for LlamaCpp text generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlamaCppTextConfig {
    /// Path to GGUF model file
    pub model_path: PathBuf,
    /// Device type for inference (Auto, Vulkan, Cuda, etc.)
    pub device: Option<String>,
    /// Number of GPU layers to offload (-1 for all)
    pub gpu_layers: Option<i32>,
    /// Context size (default: 4096)
    pub context_size: Option<u32>,
    /// Default temperature
    pub temperature: Option<f32>,
    /// Top-p sampling parameter
    pub top_p: Option<f32>,
    /// Top-k sampling parameter
    pub top_k: Option<i32>,
}

impl Default for LlamaCppTextConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::new(),
            device: None,
            gpu_layers: Some(-1), // All layers to GPU by default
            context_size: Some(DEFAULT_CONTEXT_SIZE),
            temperature: Some(0.7),
            top_p: Some(0.9),
            top_k: Some(40),
        }
    }
}

/// LlamaCpp text generation provider
///
/// Provides text generation using local GGUF models with optional grammar constraints.
/// Uses llama-cpp-2 for inference with GPU acceleration support.
pub struct LlamaCppTextProvider {
    /// Model loading state
    state: RwLock<LoadState>,
    /// Configuration
    config: LlamaCppTextConfig,
    /// Number of threads for inference
    n_threads: i32,
}

impl std::fmt::Debug for LlamaCppTextProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlamaCppTextProvider")
            .field("model_path", &self.config.model_path)
            .field("context_size", &self.config.context_size)
            .finish()
    }
}

impl LlamaCppTextProvider {
    /// Create a new uninitialized provider
    pub fn new() -> Self {
        let n_threads = (num_cpus::get_physical().saturating_sub(1)).max(1) as i32;

        Self {
            state: RwLock::new(LoadState::Empty),
            config: LlamaCppTextConfig::default(),
            n_threads,
        }
    }

    /// Create a new provider and load model in background
    pub fn new_with_model(model_path: PathBuf) -> LlmResult<Self> {
        Self::new_with_config(LlamaCppTextConfig {
            model_path,
            ..Default::default()
        })
    }

    /// Create a new provider with custom configuration
    pub fn new_with_config(config: LlamaCppTextConfig) -> LlmResult<Self> {
        let n_threads = (num_cpus::get_physical().saturating_sub(1)).max(1) as i32;

        // Validate path
        if !config.model_path.exists() {
            return Err(LlmError::ConfigError(format!(
                "Model file not found: {}",
                config.model_path.display()
            )));
        }

        // Clone for background thread
        let path_clone = config.model_path.clone();
        let gpu_layers = config.gpu_layers.unwrap_or(-1);

        // Start background loading
        let handle = std::thread::spawn(move || Self::load_model_sync(&path_clone, gpu_layers));

        Ok(Self {
            state: RwLock::new(LoadState::Loading(handle)),
            config,
            n_threads,
        })
    }

    /// Create a provider from a discovered model
    ///
    /// This allows integration with the unified model discovery system.
    pub fn from_discovered_model(
        model: &crate::model_discovery::DiscoveredModel,
    ) -> LlmResult<Self> {
        Self::new_with_model(model.path.clone())
    }

    /// Discover local GGUF models using the model discovery system
    ///
    /// Returns a list of discovered models that can be used with `from_discovered_model`.
    pub async fn discover_local_models(
        config: &crate::model_discovery::DiscoveryConfig,
    ) -> LlmResult<Vec<crate::model_discovery::DiscoveredModel>> {
        let discovery = crate::model_discovery::ModelDiscovery::new(config.clone());
        discovery
            .discover_models()
            .await
            .map_err(|e| LlmError::ConfigError(format!("Failed to discover models: {}", e)))
    }

    /// Synchronous model loading (runs in background thread)
    fn load_model_sync(
        model_path: &std::path::Path,
        gpu_layers: i32,
    ) -> Result<LoadedState, String> {
        tracing::info!("Loading GGUF model from {}", model_path.display());

        // Initialize llama.cpp backend
        let backend =
            LlamaBackend::init().map_err(|e| format!("Failed to initialize llama.cpp: {}", e))?;

        // Build model params
        let n_gpu_layers = if gpu_layers < 0 {
            999
        } else {
            gpu_layers as u32
        };
        let model_params = LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers);

        // Load the model
        let model = LlamaModel::load_from_file(&backend, model_path, &model_params)
            .map_err(|e| format!("Failed to load model: {}", e))?;

        let context_length = model.n_ctx_train() as usize;

        // Get model name from path
        let model_name = model_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("llama-cpp-model")
            .to_string();

        tracing::info!(
            "Model loaded: {} (context: {} tokens)",
            model_name,
            context_length
        );

        Ok(LoadedState {
            backend,
            model: Arc::new(model),
            model_name,
            context_length,
        })
    }

    /// Wait for model to be ready
    fn ensure_loaded(&self) -> LlmResult<()> {
        // First check if already ready (fast path)
        {
            let state = self.state.read().map_err(|e| LlmError::ProviderError {
                provider: "LlamaCpp".to_string(),
                message: format!("Lock poisoned: {}", e),
            })?;

            match &*state {
                LoadState::Ready(_) => return Ok(()),
                LoadState::Failed(e) => {
                    return Err(LlmError::ProviderError {
                        provider: "LlamaCpp".to_string(),
                        message: e.clone(),
                    });
                }
                LoadState::Empty => {
                    return Err(LlmError::ProviderError {
                        provider: "LlamaCpp".to_string(),
                        message: "No model loaded".to_string(),
                    });
                }
                LoadState::Loading(_) => {}
            }
        }

        // Acquire write lock to wait for loading
        let mut state = self.state.write().map_err(|e| LlmError::ProviderError {
            provider: "LlamaCpp".to_string(),
            message: format!("Lock poisoned: {}", e),
        })?;

        // Check again after acquiring write lock
        match &*state {
            LoadState::Ready(_) => return Ok(()),
            LoadState::Failed(e) => {
                return Err(LlmError::ProviderError {
                    provider: "LlamaCpp".to_string(),
                    message: e.clone(),
                });
            }
            LoadState::Empty => {
                return Err(LlmError::ProviderError {
                    provider: "LlamaCpp".to_string(),
                    message: "No model loaded".to_string(),
                });
            }
            LoadState::Loading(_) => {}
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
                    Err(LlmError::ProviderError {
                        provider: "LlamaCpp".to_string(),
                        message: e,
                    })
                }
                Err(_) => {
                    let msg = "Background loading thread panicked".to_string();
                    *state = LoadState::Failed(msg.clone());
                    Err(LlmError::ProviderError {
                        provider: "LlamaCpp".to_string(),
                        message: msg,
                    })
                }
            }
        } else {
            *state = old_state;
            Err(LlmError::ProviderError {
                provider: "LlamaCpp".to_string(),
                message: "Unexpected state".to_string(),
            })
        }
    }

    /// Execute a function with the loaded state
    fn with_loaded<F, T>(&self, f: F) -> LlmResult<T>
    where
        F: FnOnce(&LoadedState) -> LlmResult<T>,
    {
        self.ensure_loaded()?;

        let state = self.state.read().map_err(|e| LlmError::ProviderError {
            provider: "LlamaCpp".to_string(),
            message: format!("Lock poisoned: {}", e),
        })?;

        match &*state {
            LoadState::Ready(loaded) => f(loaded),
            _ => Err(LlmError::ProviderError {
                provider: "LlamaCpp".to_string(),
                message: "Model not ready".to_string(),
            }),
        }
    }

    /// Generate text with optional grammar constraint
    ///
    /// This is the core generation method that supports GBNF grammar constraints.
    ///
    /// # Arguments
    /// * `prompt` - The input prompt (already formatted with chat template if needed)
    /// * `grammar` - Optional GBNF grammar string (e.g., from crucible_grammar::presets)
    /// * `max_tokens` - Maximum tokens to generate
    /// * `temperature` - Sampling temperature (0.0 for greedy)
    /// * `stop_tokens` - Optional stop sequences
    pub fn generate_text(
        &self,
        prompt: &str,
        grammar: Option<&str>,
        max_tokens: u32,
        temperature: f32,
        stop_tokens: Option<&[String]>,
    ) -> LlmResult<(String, u32)> {
        self.with_loaded(|loaded| {
            let model = &loaded.model;
            let backend = &loaded.backend;

            // Create context
            let ctx_size = self.config.context_size.unwrap_or(DEFAULT_CONTEXT_SIZE);
            let ctx_params = LlamaContextParams::default()
                .with_n_ctx(NonZeroU32::new(ctx_size))
                .with_n_threads(self.n_threads)
                .with_n_threads_batch(self.n_threads);

            let mut ctx =
                model
                    .new_context(backend, ctx_params)
                    .map_err(|e| LlmError::ProviderError {
                        provider: "LlamaCpp".to_string(),
                        message: format!("Failed to create context: {}", e),
                    })?;

            // Tokenize prompt
            let tokens = model.str_to_token(prompt, AddBos::Always).map_err(|e| {
                LlmError::ProviderError {
                    provider: "LlamaCpp".to_string(),
                    message: format!("Tokenization failed: {}", e),
                }
            })?;

            let prompt_tokens = tokens.len() as u32;

            // Create batch for prompt
            let mut batch = LlamaBatch::new(tokens.len(), 1);
            for (pos, token) in tokens.iter().enumerate() {
                let is_last = pos == tokens.len() - 1;
                batch.add(*token, pos as i32, &[0], is_last).map_err(|e| {
                    LlmError::ProviderError {
                        provider: "LlamaCpp".to_string(),
                        message: format!("Failed to add token to batch: {}", e),
                    }
                })?;
            }

            // Process prompt
            ctx.decode(&mut batch)
                .map_err(|e| LlmError::ProviderError {
                    provider: "LlamaCpp".to_string(),
                    message: format!("Decode failed: {}", e),
                })?;

            // Build sampler chain
            let mut samplers: Vec<LlamaSampler> = Vec::new();

            // Add grammar constraint if provided
            if let Some(grammar_str) = grammar {
                let grammar_sampler =
                    LlamaSampler::grammar(model, grammar_str, "root").map_err(|e| {
                        LlmError::ProviderError {
                            provider: "LlamaCpp".to_string(),
                            message: format!("Failed to create grammar sampler: {:?}", e),
                        }
                    })?;
                samplers.push(grammar_sampler);
            }

            // Add temperature
            if temperature > 0.0 {
                samplers.push(LlamaSampler::temp(temperature));
                samplers.push(LlamaSampler::top_p(self.config.top_p.unwrap_or(0.9), 1));
                samplers.push(LlamaSampler::top_k(self.config.top_k.unwrap_or(40)));
                samplers.push(LlamaSampler::dist(42)); // Random sampling
            } else {
                samplers.push(LlamaSampler::greedy());
            }

            let mut sampler = LlamaSampler::chain_simple(samplers);

            // Generate tokens
            let mut output_tokens: Vec<LlamaToken> = Vec::new();
            let mut current_pos = tokens.len() as i32;
            let eos_token = model.token_eos();

            // Convert stop tokens to token sequences for detection
            let stop_sequences: Vec<Vec<LlamaToken>> = stop_tokens
                .map(|stops| {
                    stops
                        .iter()
                        .filter_map(|s| model.str_to_token(s, AddBos::Never).ok())
                        .collect()
                })
                .unwrap_or_default();

            for _ in 0..max_tokens {
                // Sample next token
                let token = sampler.sample(&ctx, -1);

                // Check for EOS
                if token == eos_token {
                    break;
                }

                // Accept token (updates grammar state)
                sampler.accept(token);
                output_tokens.push(token);

                // Check stop sequences
                let should_stop = stop_sequences.iter().any(|seq| {
                    if output_tokens.len() >= seq.len() {
                        let start = output_tokens.len() - seq.len();
                        &output_tokens[start..] == seq.as_slice()
                    } else {
                        false
                    }
                });
                if should_stop {
                    break;
                }

                // Prepare next decode
                batch.clear();
                batch
                    .add(token, current_pos, &[0], true)
                    .map_err(|e| LlmError::ProviderError {
                        provider: "LlamaCpp".to_string(),
                        message: format!("Failed to add token: {}", e),
                    })?;

                ctx.decode(&mut batch)
                    .map_err(|e| LlmError::ProviderError {
                        provider: "LlamaCpp".to_string(),
                        message: format!("Decode failed: {}", e),
                    })?;

                current_pos += 1;
            }

            // Decode output tokens to string
            let output = model
                .tokens_to_str(&output_tokens, Special::Plaintext)
                .map_err(|e| LlmError::ProviderError {
                    provider: "LlamaCpp".to_string(),
                    message: format!("Failed to decode tokens: {}", e),
                })?;

            let completion_tokens = output_tokens.len() as u32;

            Ok((output, prompt_tokens + completion_tokens))
        })
    }
}

impl Default for LlamaCppTextProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TextGenerationProvider for LlamaCppTextProvider {
    async fn generate_completion(
        &self,
        request: CompletionRequest,
    ) -> LlmResult<CompletionResponse> {
        let max_tokens = request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
        let temperature = request.temperature.unwrap_or(0.7);

        let (output, total_tokens) = self.generate_text(
            &request.prompt,
            None, // No grammar for basic completion
            max_tokens,
            temperature,
            request.stop.as_ref().map(|v| v.as_slice()),
        )?;

        Ok(CompletionResponse {
            id: format!("cmpl-{}", Uuid::new_v4()),
            object: "text_completion".to_string(),
            created: Utc::now(),
            model: request.model,
            choices: vec![super::types::CompletionChoice {
                index: 0,
                text: output,
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            }],
            usage: TokenUsage {
                prompt_tokens: 0, // We don't track this separately for now
                completion_tokens: total_tokens,
                total_tokens,
            },
            system_fingerprint: None,
        })
    }

    fn generate_completion_stream<'a>(
        &'a self,
        _request: CompletionRequest,
    ) -> BoxStream<'a, LlmResult<CompletionChunk>> {
        // Streaming not yet implemented for direct llama.cpp
        Box::pin(futures::stream::empty())
    }

    async fn generate_chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> LlmResult<ChatCompletionResponse> {
        // Format messages into a prompt (simple format for now)
        let prompt = format_chat_prompt(&request.messages);

        let max_tokens = request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
        let temperature = request.temperature.unwrap_or(0.7);

        let (output, total_tokens) = self.generate_text(
            &prompt,
            None, // TODO: Extract grammar from request if provided
            max_tokens,
            temperature,
            request.stop.as_ref().map(|v| v.as_slice()),
        )?;

        let message = LlmMessage {
            role: MessageRole::Assistant,
            content: output,
            function_call: None,
            tool_calls: None,
            name: None,
            tool_call_id: None,
        };

        Ok(ChatCompletionResponse {
            id: format!("chatcmpl-{}", Uuid::new_v4()),
            object: "chat.completion".to_string(),
            created: Utc::now(),
            model: request.model,
            choices: vec![ChatCompletionChoice {
                index: 0,
                message,
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            }],
            usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: total_tokens,
                total_tokens,
            },
            system_fingerprint: None,
        })
    }

    fn generate_chat_completion_stream<'a>(
        &'a self,
        _request: ChatCompletionRequest,
    ) -> BoxStream<'a, LlmResult<ChatCompletionChunk>> {
        // Streaming not yet implemented for direct llama.cpp
        Box::pin(futures::stream::empty())
    }

    fn provider_name(&self) -> &str {
        "LlamaCpp"
    }

    fn default_model(&self) -> &str {
        self.config
            .model_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("llama-cpp")
    }

    async fn list_models(&self) -> LlmResult<Vec<TextModelInfo>> {
        self.with_loaded(|loaded| {
            Ok(vec![TextModelInfo {
                id: loaded.model_name.clone(),
                name: loaded.model_name.clone(),
                owner: Some("local".to_string()),
                capabilities: vec![
                    ModelCapability::TextCompletion,
                    ModelCapability::ChatCompletion,
                ],
                max_context_length: Some(loaded.context_length as u32),
                max_output_tokens: Some(DEFAULT_MAX_TOKENS),
                input_price: None,
                output_price: None,
                created: None,
                status: ModelStatus::Available,
            }])
        })
    }

    async fn health_check(&self) -> LlmResult<bool> {
        match self.ensure_loaded() {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            text_completion: true,
            chat_completion: true,
            streaming: false, // Not yet implemented
            function_calling: false,
            tool_use: false,
            vision: false,
            audio: false,
            max_batch_size: Some(1),
            input_formats: vec!["text".to_string()],
            output_formats: vec!["text".to_string()],
        }
    }
}

// ============================================================================
// Provider trait implementation (unified provider abstraction)
// ============================================================================

#[async_trait]
impl Provider for LlamaCppTextProvider {
    fn name(&self) -> &str {
        "llama-cpp"
    }

    fn backend_type(&self) -> BackendType {
        BackendType::LlamaCpp
    }

    fn endpoint(&self) -> Option<&str> {
        // Local model - no endpoint
        None
    }

    fn capabilities(&self) -> ExtendedCapabilities {
        ExtendedCapabilities {
            llm: ProviderCapabilities {
                text_completion: true,
                chat_completion: true,
                streaming: false,
                function_calling: false,
                tool_use: true, // Via grammar-constrained generation
                vision: false,
                audio: false,
                max_batch_size: Some(1),
                input_formats: vec!["text".to_string()],
                output_formats: vec!["text".to_string()],
            },
            embeddings: false,
            embeddings_batch: false,
            embedding_dimensions: None,
            max_batch_size: None,
        }
    }

    async fn health_check(&self) -> LlmResult<bool> {
        // Delegate to existing implementation
        TextGenerationProvider::health_check(self).await
    }
}

// ============================================================================
// CanConstrainGeneration implementation (GBNF grammar support)
// ============================================================================

#[async_trait]
impl CanConstrainGeneration for LlamaCppTextProvider {
    fn supported_formats(&self) -> Vec<SchemaFormat> {
        vec![SchemaFormat::Gbnf]
    }

    async fn generate_constrained(
        &self,
        request: ConstrainedRequest,
    ) -> LlmResult<ConstrainedResponse> {
        // Only GBNF is supported
        if request.format != SchemaFormat::Gbnf {
            return Err(LlmError::ConfigError(format!(
                "LlamaCpp only supports GBNF grammar format, got {:?}",
                request.format
            )));
        }

        let max_tokens = request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
        let temperature = request.temperature.unwrap_or(0.7);
        let stop_tokens = request.stop.as_ref().map(|v| v.as_slice());

        let (text, tokens) = self.generate_text(
            &request.prompt,
            Some(&request.schema),
            max_tokens,
            temperature,
            stop_tokens,
        )?;

        let truncated = tokens >= max_tokens;

        Ok(ConstrainedResponse {
            text,
            tokens,
            truncated,
        })
    }
}

/// Format chat messages into a simple prompt
fn format_chat_prompt(messages: &[LlmMessage]) -> String {
    let mut prompt = String::new();

    for msg in messages {
        let role = match msg.role {
            MessageRole::System => "System",
            MessageRole::User => "User",
            MessageRole::Assistant => "Assistant",
            MessageRole::Function => "Function",
            MessageRole::Tool => "Tool",
        };

        prompt.push_str(&format!("{}: {}\n", role, msg.content));
    }

    prompt.push_str("Assistant: ");
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_chat_prompt() {
        let messages = vec![
            LlmMessage {
                role: MessageRole::System,
                content: "You are a helpful assistant.".to_string(),
                function_call: None,
                tool_calls: None,
                name: None,
                tool_call_id: None,
            },
            LlmMessage {
                role: MessageRole::User,
                content: "Hello!".to_string(),
                function_call: None,
                tool_calls: None,
                name: None,
                tool_call_id: None,
            },
        ];

        let prompt = format_chat_prompt(&messages);
        assert!(prompt.contains("System: You are a helpful assistant."));
        assert!(prompt.contains("User: Hello!"));
        assert!(prompt.ends_with("Assistant: "));
    }

    #[test]
    fn test_default_config() {
        let config = LlamaCppTextConfig::default();
        assert_eq!(config.gpu_layers, Some(-1));
        assert_eq!(config.context_size, Some(4096));
        assert!(config.temperature.is_some());
    }
}
