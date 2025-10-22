//! # Crucible LLM
//!
//! LLM and AI integration library for Crucible knowledge management system.
//!
//! ## Features
//!
//! - **Embeddings**: Text embeddings for semantic search
//! - **Multi-provider**: Support for Ollama, OpenAI, and more
//! - **Type-safe**: Compile-time safety with trait-based design
//! - **Async**: Built on tokio for high performance
//!
//! ## Modules
//!
//! - [`embeddings`]: Text embedding generation and management
//!
//! ## Example
//!
//! ```rust,no_run
//! use crucible_llm::embeddings::{EmbeddingConfig, EmbeddingProvider, OllamaProvider};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = EmbeddingConfig::ollama(
//!         Some("https://llama.krohnos.io".to_string()),
//!         Some("nomic-embed-text-v1.5-q8_0".to_string()),
//!     );
//!
//!     let provider = OllamaProvider::new(config)?;
//!     let response = provider.embed("Hello, world!").await?;
//!
//!     println!("Generated embedding with {} dimensions", response.dimensions);
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod embeddings;
pub mod text_generation;

// Re-export commonly used types at crate root
pub use embeddings::{
    EmbeddingConfig, EmbeddingError, EmbeddingProvider, EmbeddingResponse, EmbeddingResult,
    OllamaProvider, OpenAIProvider,
};

pub use text_generation::{
    TextGenerationProvider, create_text_provider, TextProviderConfig,
    CompletionRequest, CompletionResponse, ChatCompletionRequest, ChatCompletionResponse,
    ChatMessage, CompletionChunk, ChatCompletionChunk, TokenUsage,
    OpenAITextProvider, OllamaTextProvider, OpenAIConfig, OllamaConfig,
};
