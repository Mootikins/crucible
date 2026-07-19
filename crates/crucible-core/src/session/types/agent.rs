//! Session agent configuration.

use crate::config::{AgentProfile, BackendType, DelegationConfig};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::config::{
    default_precognition_results, default_validation_retries, ContextStrategy, OutputValidation,
};
use crate::serde_helpers::default_true;

/// Agent configuration bound to a session.
///
/// This captures everything needed to reconstruct an agent when resuming
/// a session. The configuration is inlined (not just a reference) so that
/// sessions are self-contained and reproducible.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionAgent {
    /// Agent type: "acp" (external) or "internal" (built-in)
    pub agent_type: String,

    /// ACP agent name (e.g., "opencode", "claude") - only for ACP agents
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,

    /// Provider key (e.g., "ollama", "openai", "anthropic") - only for internal agents
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_key: Option<String>,

    /// LLM provider identifier (typed backend)
    pub provider: BackendType,

    /// Model identifier (e.g., "llama3.2", "gpt-4o", "claude-3-5-sonnet")
    pub model: String,

    /// System prompt (full text, inlined from agent card if applicable)
    pub system_prompt: String,

    /// Generation temperature (0.0 - 2.0)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    /// Maximum output tokens
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Maximum context window tokens
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_tokens: Option<usize>,

    /// Thinking/reasoning token budget for models that support extended thinking.
    /// -1 = unlimited, 0 = disabled, >0 = max tokens for thinking
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<i64>,

    /// Custom endpoint URL (for self-hosted models)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,

    /// Environment variable overrides for ACP agents
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env_overrides: HashMap<String, String>,

    /// MCP servers this agent can use
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<String>,

    /// Source agent card name (for reference, not used for reconstruction)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_card_name: Option<String>,

    /// List of capabilities this agent provides (from ACP agent profile)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,

    /// Human-readable description of this agent (from ACP agent profile)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_description: Option<String>,

    /// Delegation configuration for this agent (from ACP agent profile)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation_config: Option<DelegationConfig>,

    /// Whether Precognition (auto-RAG) is enabled for this session (default: true)
    #[serde(default = "default_true")]
    pub precognition_enabled: bool,

    /// Maximum number of unique notes to return from Precognition search (default: 5).
    #[serde(default = "default_precognition_results")]
    pub precognition_results: usize,

    /// Maximum tool-call iterations per turn. None = unlimited (default for interactive sessions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,

    /// Execution timeout in seconds per turn. None = no timeout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_timeout_secs: Option<u64>,

    /// Context window token budget. None = no limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_budget: Option<usize>,

    /// Strategy for truncating context when over budget.
    #[serde(default)]
    pub context_strategy: ContextStrategy,

    /// For SlidingWindow strategy: keep last N message pairs. None = 10 (default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<usize>,

    /// Output validation mode for agent text responses.
    #[serde(default)]
    pub output_validation: OutputValidation,

    /// Maximum retries when output validation fails (default: 3).
    #[serde(default = "default_validation_retries")]
    pub validation_retries: u32,

    /// Trigger auto-compaction when estimated message tokens exceed
    /// `context_budget * autocompact_threshold`. `None` uses the default
    /// (0.95). Set to `Some(0.0)` (or surface "off" in user-facing
    /// parsers) to disable. Range: 0.0..=1.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autocompact_threshold: Option<f32>,

    /// Session mode id ("normal" | "plan" | "auto"). Persisted so a mode set
    /// before the first message (no live handle yet) still applies when the
    /// agent handle is created, and survives handle eviction. `None` = normal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

impl SessionAgent {
    /// Construct a SessionAgent from an enriched AgentProfile.
    ///
    /// Creates an ACP-type SessionAgent with:
    /// - agent_type: "acp"
    /// - agent_name: the provided agent_name
    /// - provider: "acp"
    /// - model: the provided agent_name
    /// - capabilities, agent_description, delegation_config from profile
    /// - env_overrides: profile's env vars (isolated, parent env NOT inherited)
    ///
    /// KNOWN LIMITATION: No permission inheritance for subagents.
    /// Subagents start with a fresh permission state (empty env_overrides, no inherited
    /// permissions from the parent agent). This is intentional for security: each subagent
    /// must be explicitly configured with its own permissions. Future versions could support
    /// selective permission inheritance with explicit allowlists.
    pub fn from_profile(profile: &AgentProfile, agent_name: &str) -> Self {
        Self {
            agent_type: "acp".to_string(),
            agent_name: Some(agent_name.to_string()),
            provider_key: None,
            provider: BackendType::Custom,
            model: agent_name.to_string(),
            system_prompt: String::new(),
            temperature: None,
            max_tokens: None,
            max_context_tokens: None,
            thinking_budget: None,
            endpoint: None,
            env_overrides: profile.env.clone(),
            mcp_servers: Vec::new(),
            agent_card_name: None,
            capabilities: profile.capabilities.clone(),
            agent_description: profile.description.clone(),
            delegation_config: profile.delegation.clone(),
            precognition_enabled: true,
            precognition_results: default_precognition_results(),
            max_iterations: None,
            execution_timeout_secs: None,
            context_budget: None,
            context_strategy: ContextStrategy::default(),
            context_window: None,
            output_validation: OutputValidation::default(),
            validation_retries: default_validation_retries(),
            autocompact_threshold: None,
            mode: None,
        }
    }

    /// Canonical internal-agent defaults derived from app config.
    ///
    /// Every surface that configures a session agent from config (CLI chat,
    /// ACP bridge, web session create) goes through this one builder so they
    /// all get identical provider/model/temperature/MCP defaults.
    pub fn internal_from_config(config: &crate::config::CliAppConfig) -> Self {
        let effective_llm = config.effective_llm_provider().ok();
        let model = effective_llm
            .as_ref()
            .map(|p| p.model.clone())
            .or_else(|| config.chat.model.clone())
            .unwrap_or_else(|| crate::config::DEFAULT_CHAT_MODEL.to_string());
        let mcp_servers = config
            .mcp
            .as_ref()
            .map(|mcp| mcp.servers.iter().map(|s| s.name.clone()).collect())
            .unwrap_or_default();
        let backend_type = effective_llm
            .as_ref()
            .map(|p| p.provider_type)
            .unwrap_or(BackendType::Ollama);
        let provider_key = effective_llm
            .as_ref()
            .map(|p| p.key.clone())
            .unwrap_or_else(|| backend_type.as_str().to_string());

        Self {
            agent_type: "internal".to_string(),
            agent_name: None,
            provider_key: Some(provider_key),
            provider: backend_type,
            model,
            system_prompt: String::new(),
            temperature: effective_llm
                .as_ref()
                .map(|p| p.temperature as f64)
                .or_else(|| config.chat.temperature.map(|t| t as f64)),
            max_tokens: effective_llm
                .as_ref()
                .map(|p| p.max_tokens)
                .or(config.chat.max_tokens),
            max_context_tokens: None,
            thinking_budget: None,
            endpoint: effective_llm
                .as_ref()
                .map(|p| p.endpoint.clone())
                .or_else(|| config.chat.endpoint.clone()),
            env_overrides: HashMap::new(),
            mcp_servers,
            agent_card_name: None,
            capabilities: None,
            agent_description: None,
            delegation_config: None,
            precognition_enabled: true,
            precognition_results: default_precognition_results(),
            max_iterations: None,
            execution_timeout_secs: None,
            context_budget: None,
            context_strategy: ContextStrategy::default(),
            context_window: None,
            output_validation: OutputValidation::default(),
            validation_retries: default_validation_retries(),
            autocompact_threshold: None,
            mode: None,
        }
    }
}

/// Generate a session ID with the given type prefix.
///
/// Format: `{type}-{YYYY-MM-DDTHHMM}-{random6}`
/// Example: `chat-2025-01-08T1530-a1b2c3`
pub(super) fn generate_session_id(type_prefix: &str) -> String {
    use rand::RngExt;
    let timestamp = Utc::now().format("%Y-%m-%dT%H%M");
    let mut rng = rand::rng();
    let random: String = (0..6)
        .map(|_| {
            let idx: u8 = rng.random_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + (idx - 10)) as char
            }
        })
        .collect();
    format!("{}-{}-{}", type_prefix, timestamp, random)
}
