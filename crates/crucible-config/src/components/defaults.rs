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

// ============================================================================
// Provider Endpoints
// ============================================================================

/// Default OpenAI API endpoint
pub const DEFAULT_OPENAI_ENDPOINT: &str = "https://api.openai.com/v1";

/// Default Anthropic API endpoint
pub const DEFAULT_ANTHROPIC_ENDPOINT: &str = "https://api.anthropic.com/v1";

/// Default GitHub Copilot API endpoint
pub const DEFAULT_GITHUB_COPILOT_ENDPOINT: &str = "https://api.githubcopilot.com";

/// Default OpenRouter API endpoint
pub const DEFAULT_OPENROUTER_ENDPOINT: &str = "https://openrouter.ai/api/v1";

/// Default ZAI API endpoint
pub const DEFAULT_ZAI_ENDPOINT: &str = "https://api.z.ai/api/coding/paas/v4";

// ============================================================================
// Default Models
// ============================================================================

/// Default model for OpenAI
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o";

/// Default model for Anthropic
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-3-5-sonnet-20241022";

/// Default model for GitHub Copilot
pub const DEFAULT_GITHUB_COPILOT_MODEL: &str = "gpt-4o";

/// Default model for OpenRouter
pub const DEFAULT_OPENROUTER_MODEL: &str = "openai/gpt-4o";

/// Default model for ZAI
pub const DEFAULT_ZAI_MODEL: &str = "GLM-4.7";

// ============================================================================
// Hardcoded Model Lists
// ============================================================================

/// Available Anthropic models (hardcoded fallback when API enumeration unavailable)
pub const ANTHROPIC_MODELS: &[&str] = &[
    "claude-sonnet-4-20250514",
    "claude-3-7-sonnet-20250219",
    "claude-3-5-sonnet-20241022",
    "claude-3-5-haiku-20241022",
    "claude-3-opus-20240229",
];

/// Available ZAI models (hardcoded fallback when API enumeration unavailable)
pub const ZAI_MODELS: &[&str] = &[
    "GLM-5",
    "GLM-4.7",
    "GLM-4.6",
    "GLM-4.5",
    "GLM-4.5-Air",
    "GLM-4.5-Flash",
    "GLM-4.5v",
    "GLM-4-32b-0414-128k",
];

/// Hardcoded OpenAI models (fallback when API enumeration unavailable)
pub const OPENAI_HARDCODED_MODELS: &[&str] = &["gpt-4o", "gpt-4o-mini", "o1", "o3-mini"];

/// OpenAI model name prefixes for filtering fetched models
pub const OPENAI_MODEL_PREFIXES: &[&str] = &["gpt-", "chatgpt-", "o1", "o3", "o4"];
