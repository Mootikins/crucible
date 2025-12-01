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
//! ## Architecture (SOLID Phase 5)
//!
//! - Concrete provider types (OllamaProvider, OpenAIProvider) are PRIVATE
//! - Public API provides factory functions that return trait objects
//! - Configuration types and traits are public
//!
//! ## Modules
//!
//! - [`embeddings`]: Text embedding generation and management
//! - [`reranking`]: Note reranking for improved search relevance
//! - [`text_generation`]: Text generation and completion
//!
//! ## Example
//!
//! ```rust,no_run
//! use crucible_llm::embeddings::{create_provider, EmbeddingConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = EmbeddingConfig::ollama(
//!         Some("https://llama.krohnos.io".to_string()),
//!         Some("nomic-embed-text-v1.5-q8_0".to_string()),
//!     );
//!
//!     // Use factory function - returns trait object
//!     let provider = create_provider(config).await?;
//!     let response = provider.embed("Hello, world!").await?;
//!
//!     println!("Generated embedding with {} dimensions", response.dimensions);
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod agent_runtime;
pub mod chat;
pub mod embeddings;
pub mod reranking;
pub mod text_generation;

// Mock implementations for testing (Phase 5)
#[cfg(any(test, feature = "test-utils"))]
pub mod text_generation_mock;

// Re-export commonly used types at crate root
// SOLID Phase 5: Re-export factory functions and traits, NOT concrete types
pub use embeddings::{
    create_provider,       // Factory function
    CoreProviderAdapter,   // Adapter for core trait
    EmbeddingConfig,       // Configuration (data type)
    EmbeddingError,        // Error type
    EmbeddingProvider,     // Trait (abstraction)
    EmbeddingProviderType, // Enum for provider selection
    // REMOVED: OllamaProvider, OpenAIProvider - use create_provider() instead
    EmbeddingResponse, // Response type (data)
    EmbeddingResult,   // Result type alias
};

pub use reranking::{FastEmbedReranker, RerankResult, Reranker, RerankerModelInfo};

pub use text_generation::{
    create_text_provider,  // Factory function
    from_app_config,       // Create provider from app config
    from_chat_config,      // Create provider from chat config
    ChatCompletionChunk,
    ChatCompletionRequest,
    ChatCompletionResponse,
    ChatMessage,
    CompletionChunk,
    CompletionRequest,
    CompletionResponse,
    OllamaConfig,
    OpenAIConfig,
    TextGenerationProvider, // Trait (abstraction)
    TextProviderConfig,
    TokenUsage,
    // REMOVED: OllamaTextProvider, OpenAITextProvider - use create_text_provider() instead
};

// Re-export agent runtime
pub use agent_runtime::AgentRuntime;

// Re-export chat providers and factory functions
pub use chat::{create_chat_provider, create_from_app_config, OllamaChatProvider, OpenAIChatProvider};

// Re-export mock implementations for testing
#[cfg(any(test, feature = "test-utils"))]
pub use text_generation_mock::MockTextProvider;

// Re-export core enrichment config types for convenience
pub use crucible_core::enrichment::{
    CohereConfig, CustomConfig, EmbeddingProviderConfig as NewEmbeddingProviderConfig,
    EnrichmentConfig, FastEmbedConfig as NewFastEmbedConfig, MockConfig as NewMockConfig,
    OllamaConfig as NewOllamaConfig, OpenAIConfig as NewOpenAIConfig, PipelineConfig,
    VertexAIConfig,
};
