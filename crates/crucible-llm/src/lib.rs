//! # Crucible LLM
//!
//! LLM and AI integration library for Crucible knowledge management system.
//!
//! ## Features
//!
//! - **Embeddings**: Text embeddings for semantic search
//! - **Multi-provider**: Support for Ollama, OpenAI, FastEmbed, and more
//! - **Type-safe**: Compile-time safety with trait-based design
//! - **Async**: Built on tokio for high performance
//!
//! ## Architecture
//!
//! - Concrete provider types (OllamaProvider, OpenAIProvider) are PRIVATE
//! - Public API provides factory functions that return trait objects
//! - Configuration types and traits are public
//!
//! ## Modules
//!
//! - [`embeddings`]: Text embedding generation and management
//! - [`model_discovery`]: Local GGUF model discovery and cataloging
//! - [`reranking`]: Note reranking for improved search relevance
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

pub mod embeddings;
pub mod model_discovery;
pub mod reranking;

// Re-export commonly used types at crate root
pub use embeddings::{
    create_provider,       // Factory function
    CoreProviderAdapter,   // Adapter for core trait
    EmbeddingConfig,       // Configuration (data type)
    EmbeddingError,        // Error type
    EmbeddingProvider,     // Trait (abstraction)
    EmbeddingProviderType, // Enum for provider selection
    EmbeddingResponse,     // Response type (data)
    EmbeddingResult,       // Result type alias
};

#[cfg(feature = "fastembed")]
pub use reranking::FastEmbedReranker;
pub use reranking::{RerankResult, Reranker, RerankerModelInfo};

// Re-export core enrichment config types for convenience
pub use crucible_core::enrichment::{
    CohereConfig, CustomConfig, EmbeddingProviderConfig as NewEmbeddingProviderConfig,
    EnrichmentConfig, FastEmbedConfig as NewFastEmbedConfig, MockConfig as NewMockConfig,
    OllamaConfig as NewOllamaConfig, OpenAIConfig as NewOpenAIConfig, PipelineConfig,
    VertexAIConfig,
};

// Re-export model discovery
pub use model_discovery::{DiscoveredModel, DiscoveredModelType, DiscoveryConfig, ModelDiscovery};
