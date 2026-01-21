//! Default configuration values used across the codebase.
//!
//! Centralizes all magic numbers to ensure consistency between components.

/// Default temperature for LLM generation (0.0 = deterministic, 2.0 = max randomness)
pub const DEFAULT_TEMPERATURE: f32 = 0.7;

/// Default max tokens for chat conversations
pub const DEFAULT_CHAT_MAX_TOKENS: u32 = 2048;

/// Default max tokens for provider/completion requests
pub const DEFAULT_PROVIDER_MAX_TOKENS: u32 = 4096;

/// Default timeout in seconds for LLM requests
pub const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// Default batch size for embedding operations
pub const DEFAULT_BATCH_SIZE: usize = 16;

/// Default chat model when none specified
pub const DEFAULT_CHAT_MODEL: &str = "llama3.2";

/// Default Ollama endpoint
pub const DEFAULT_OLLAMA_ENDPOINT: &str = "http://localhost:11434";
