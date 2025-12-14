//! Unified provider layer
//!
//! This module provides a unified interface for creating providers that implement
//! the new extension traits (`Provider`, `CanEmbed`, `CanChat`) from crucible-core.
//!
//! ## Architecture
//!
//! The unified layer wraps existing provider implementations with adapters that
//! implement the new traits. This allows gradual migration while maintaining
//! backward compatibility.
//!
//! ```text
//! ProvidersConfig
//!       │
//!       ▼
//! create_provider() / create_embedding_provider() / create_chat_provider()
//!       │
//!       ▼
//! Adapter (implements Provider + CanEmbed/CanChat)
//!       │
//!       ▼
//! Legacy Provider (EmbeddingProvider / TextGenerationProvider)
//! ```

mod adapters;
mod factory;

pub use adapters::{ChatProviderAdapter, EmbeddingProviderAdapter, UnifiedProvider};
pub use factory::{
    create_chat_provider_unified, create_embedding_provider_unified, create_provider_by_name,
    create_unified_provider,
};
