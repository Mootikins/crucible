//! Text generation module
//!
//! Provides text generation providers for various LLM backends:
//! - OpenAI API
//! - Ollama
//! - LlamaCpp (local GGUF models with grammar support) - requires `llama-cpp` feature

pub mod factory;
pub mod types;

#[cfg(feature = "llama-cpp")]
pub mod llama_cpp;

// Re-export all types
pub use types::*;

// Re-export LlamaCpp provider (feature-gated)
#[cfg(feature = "llama-cpp")]
pub use llama_cpp::{LlamaCppTextConfig, LlamaCppTextProvider};

// Re-export factory functions
pub use factory::{
    from_app_config, from_chat_config, from_config, from_config_by_name, from_effective_config,
    from_provider_config,
};
