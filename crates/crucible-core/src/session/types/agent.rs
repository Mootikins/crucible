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

    /// Per-tool permission policy from the source agent card. Deny = tool
    /// not advertised and refused; Ask = always prompt; Allow = never
    /// prompt. Tools not listed use default behavior. `None` = no policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_policy: Option<crate::agent::ToolPolicyMap>,
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
            tool_policy: None,
            mode: None,
        }
    }

    /// Construct an internal SessionAgent from an agent card, layered over a
    /// base config (the spawning context: the parent's agent for delegation,
    /// or the configured defaults for session creation).
    ///
    /// Model selection follows one explicit chain, most-specific first:
    /// 1. card-explicit `provider:`/`model:`
    /// 2. card `specialty:` resolved through the `[llm.models]` table
    ///    (`specialty_models`), value `"provider/model"` or bare `"model"`
    ///    (provider inherited)
    /// 3. the base — the spawning context's agent (parent session for
    ///    delegation, configured defaults for session creation)
    ///
    /// Other card fields override the base where present: system prompt (the
    /// card body — finally populating the "inlined from agent card" field),
    /// temperature, max_tokens, max_turns → max_iterations, mode,
    /// mcp_servers, and the per-tool policy. Everything else (endpoint
    /// resolution, precognition, context budget, validation) inherits from
    /// the base. An unrecognized `provider:` string falls back to the base
    /// provider (validated at use, not load).
    ///
    /// The two fields that decide what the child may EXECUTE — tool policy and
    /// MCP servers — override only when the base is the configured defaults.
    /// When the base is a delegating parent session they are intersected
    /// instead, so a delegated child's configured tool surface is never wider
    /// than the session that spawned it. Card discovery re-runs on every
    /// delegation, so without that a session running under `bash: deny` could
    /// write a card carrying `bash: allow` this turn and delegate into it the
    /// next. `mode` is NOT narrowed — see the comment at the field.
    pub fn from_card(
        card: &crate::agent::AgentCard,
        base: &SessionAgent,
        specialty_models: Option<&HashMap<String, String>>,
    ) -> Self {
        // Specialty mapping applies only when the card names no model of its
        // own: explicit card fields always win.
        let specialty_entry = if card.model.is_none() && card.provider.is_none() {
            card.specialty
                .as_deref()
                .and_then(|s| specialty_models.and_then(|m| m.get(s)))
        } else {
            None
        };
        let (mapped_provider_str, mapped_model) = match specialty_entry {
            Some(entry) => match entry.split_once('/') {
                // "provider/model" only when the prefix is a real backend;
                // otherwise the whole string is a model id (some model ids
                // contain slashes).
                Some((prefix, rest)) if prefix.parse::<BackendType>().is_ok() => {
                    (Some(prefix.to_string()), Some(rest.to_string()))
                }
                _ => (None, Some(entry.clone())),
            },
            None => (None, None),
        };

        let provider_str = card.provider.clone().or(mapped_provider_str);
        let provider = provider_str
            .as_deref()
            .and_then(|p| p.parse::<BackendType>().ok());
        // Is `base` a delegating parent the card must stay inside, or the
        // configured defaults it may freely override? Only a session whose
        // agent carries a delegation config can delegate (the spawner reads it
        // before resolving a target: `crucible-daemon/src/delegation.rs:333`),
        // and every default builder hardcodes `delegation_config: None`:
        //   - crucible-daemon/src/server/session/create.rs:367
        //     (`build_default_internal_agent`, the base at create.rs:260)
        //   - `SessionAgent::internal_from_config` below
        // A disabled config counts as a parent too: narrowing where it wasn't
        // needed is not the unsafe direction.
        let under_parent = base.delegation_config.is_some();
        Self {
            agent_type: "internal".to_string(),
            agent_name: None,
            provider_key: provider_str.or_else(|| base.provider_key.clone()),
            provider: provider.unwrap_or(base.provider),
            model: card
                .model
                .clone()
                .or(mapped_model)
                .unwrap_or_else(|| base.model.clone()),
            system_prompt: card.system_prompt.clone(),
            temperature: card.temperature.map(|t| t as f64).or(base.temperature),
            max_tokens: card.max_tokens.or(base.max_tokens),
            max_context_tokens: base.max_context_tokens,
            thinking_budget: base.thinking_budget,
            // Endpoint follows the provider: a card that switches provider
            // must not inherit the base's endpoint for a different backend.
            endpoint: if provider.is_some() && provider != Some(base.provider) {
                None
            } else {
                base.endpoint.clone()
            },
            env_overrides: HashMap::new(),
            // Gateway tools are dispatchable under prefixed names that appear
            // in no `tools:` block, so a delegated child that could add a
            // server would route around the policy narrowing below by naming
            // a server instead of a tool. An empty list means "no gateway
            // servers", not "all of them", so intersecting is exact.
            mcp_servers: if card.mcp_servers.is_empty() {
                base.mcp_servers.clone()
            } else if under_parent {
                card.mcp_servers
                    .iter()
                    .filter(|s| base.mcp_servers.contains(s))
                    .cloned()
                    .collect()
            } else {
                card.mcp_servers.clone()
            },
            agent_card_name: Some(card.name.clone()),
            capabilities: None,
            agent_description: Some(card.description.clone()),
            delegation_config: base.delegation_config.clone(),
            precognition_enabled: base.precognition_enabled,
            precognition_results: base.precognition_results,
            max_iterations: card.max_turns.or(base.max_iterations),
            execution_timeout_secs: base.execution_timeout_secs,
            context_budget: base.context_budget,
            context_strategy: base.context_strategy.clone(),
            context_window: base.context_window,
            output_validation: base.output_validation.clone(),
            validation_retries: base.validation_retries,
            autocompact_threshold: base.autocompact_threshold,
            // `mode` is deliberately left alone. Modes are an open set declared
            // in Lua (`cru.modes.<name> = ...`) whose permission stance this
            // crate cannot see, so there is no ordering here to take a minimum
            // over — and forcing the parent's mode onto a delegated child also
            // discards a card that volunteers a TIGHTER mode. Narrowing it
            // needs the mode registry's stance ordering; that is a separate
            // change, not this one.
            mode: card.mode.clone().or_else(|| base.mode.clone()),
            tool_policy: if under_parent {
                narrow_tool_policy(base.tool_policy.as_ref(), card.tools.as_ref())
            } else {
                card.tools.clone()
            },
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
            tool_policy: None,
            mode: None,
        }
    }
}

/// Restrictiveness of one tool's policy entry, with "not listed" ranked
/// between Allow and Ask: an unlisted tool faces the permission gate iff it is
/// not read-only, which is strictly tighter than Allow (never gates) and no
/// tighter than Ask (always gates). See
/// `crucible-daemon/src/agent_manager/messaging/gate_decision.rs:21-32`.
fn restrictiveness(policy: Option<crate::agent::ToolPolicy>) -> u8 {
    use crate::agent::ToolPolicy;
    match policy {
        Some(ToolPolicy::Allow) => 0,
        None => 1,
        Some(ToolPolicy::Ask) => 2,
        Some(ToolPolicy::Deny) => 3,
    }
}

/// The per-tool policy a delegated child gets: for every tool named by EITHER
/// side, the more restrictive of the two entries.
///
/// Both directions matter, and the second is the common one. A parent's Deny
/// must survive a card that allows the same tool. But a parent that names no
/// policy at all is not unconstrained — its own non-read-only tools still face
/// the gate — so a card's `allow`, which makes `requires_permission_gate`
/// answer false and skips the mode stance, the mode rules, every Lua
/// `on_request` hook and the saved patterns, must not stand either. That is
/// the stock shape: an ACP-profile parent carries no policy, so this is what
/// keeps a card written this turn from widening the child.
///
/// An entry that ends up back at "not listed" is dropped, leaving the tool
/// with default behavior rather than a synthesized one.
fn narrow_tool_policy(
    parent: Option<&crate::agent::ToolPolicyMap>,
    card: Option<&crate::agent::ToolPolicyMap>,
) -> Option<crate::agent::ToolPolicyMap> {
    let empty = crate::agent::ToolPolicyMap::new();
    let (parent, card) = (parent.unwrap_or(&empty), card.unwrap_or(&empty));
    let narrowed: crate::agent::ToolPolicyMap = parent
        .keys()
        .chain(card.keys())
        .filter_map(|name| {
            let (p, c) = (parent.get(name).copied(), card.get(name).copied());
            let stricter = if restrictiveness(p) >= restrictiveness(c) {
                p
            } else {
                c
            };
            stricter.map(|policy| (name.clone(), policy))
        })
        .collect();
    (!narrowed.is_empty()).then_some(narrowed)
}

/// Generate a session ID with the given type prefix.
///
/// Format: `{type}-{YYYY-MM-DDTHHMM}-{random6}`
/// Example: `chat-2025-01-08T1530-a1b2c3`
pub(crate) fn generate_session_id(type_prefix: &str) -> String {
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

#[cfg(test)]
mod narrowing_tests {
    use super::*;
    use crate::agent::{AgentCard, ToolPolicy, ToolPolicyMap};

    fn delegation_config() -> DelegationConfig {
        DelegationConfig {
            enabled: true,
            max_depth: 2,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
            timeout_secs: 300,
        }
    }

    /// The stock delegating parent: an ACP profile session (`cru chat -a
    /// claude`) with delegation turned on. Note `tool_policy` stays `None` —
    /// that is the shape every production delegation actually has, because
    /// `DelegationService::spawn_delegation` refuses unless the parent agent
    /// carries a delegation config, and the only constructor that produces one
    /// is `from_profile` (which hardcodes `tool_policy: None`).
    fn delegating_parent() -> SessionAgent {
        let profile = AgentProfile {
            delegation: Some(delegation_config()),
            ..AgentProfile::default()
        };
        SessionAgent::from_profile(&profile, "claude")
    }

    /// The `session.create` base: config-derived defaults, no delegation
    /// config, no policy of its own.
    fn config_defaults() -> SessionAgent {
        let mut base = SessionAgent::from_profile(&AgentProfile::default(), "defaults");
        base.agent_type = "internal".to_string();
        base
    }

    fn card_with_tools(tools: Option<ToolPolicyMap>) -> AgentCard {
        AgentCard {
            id: uuid::Uuid::nil(),
            name: "child".to_string(),
            version: "0.1.0".to_string(),
            description: "child card".to_string(),
            tags: Vec::new(),
            specialty: None,
            system_prompt: String::new(),
            mcp_servers: Vec::new(),
            provider: None,
            model: None,
            temperature: None,
            max_tokens: None,
            max_turns: None,
            mode: None,
            tools,
            config: HashMap::new(),
            loaded_at: Utc::now(),
        }
    }

    fn policy(entries: &[(&str, ToolPolicy)]) -> ToolPolicyMap {
        entries
            .iter()
            .map(|(name, p)| (name.to_string(), *p))
            .collect()
    }

    fn tool(agent: &SessionAgent, name: &str) -> Option<ToolPolicy> {
        agent
            .tool_policy
            .as_ref()
            .and_then(|p| p.get(name))
            .copied()
    }

    /// The stock-configuration escape: the parent names no tool policy at all,
    /// so its own bash calls face the permission gate. A card the agent writes
    /// this turn saying `bash: allow` must not hand the child a bash that
    /// skips the gate outright.
    #[test]
    fn a_delegated_card_cannot_auto_approve_a_tool_its_parent_must_ask_for() {
        let card = card_with_tools(Some(policy(&[("bash", ToolPolicy::Allow)])));

        let child = SessionAgent::from_card(&card, &delegating_parent(), None);

        assert_eq!(
            tool(&child, "bash"),
            None,
            "an unlisted parent entry still gates; the child must not be widened to Allow"
        );
    }

    #[test]
    fn a_parent_deny_outranks_a_card_that_allows_the_same_tool() {
        let mut parent = delegating_parent();
        parent.tool_policy = Some(policy(&[("bash", ToolPolicy::Deny)]));
        let card = card_with_tools(Some(policy(&[("bash", ToolPolicy::Allow)])));

        let child = SessionAgent::from_card(&card, &parent, None);

        assert_eq!(
            tool(&child, "bash"),
            Some(ToolPolicy::Deny),
            "a card written this turn must not delegate its way out of the parent's deny"
        );
    }

    #[test]
    fn a_parent_ask_tightens_a_card_allow() {
        let mut parent = delegating_parent();
        parent.tool_policy = Some(policy(&[("bash", ToolPolicy::Ask)]));
        let card = card_with_tools(Some(policy(&[("bash", ToolPolicy::Allow)])));

        let child = SessionAgent::from_card(&card, &parent, None);

        assert_eq!(
            tool(&child, "bash"),
            Some(ToolPolicy::Ask),
            "the parent always prompts for bash, so the child cannot auto-approve it"
        );
    }

    /// The intersection is a minimum, not a blanket "drop every Allow": where
    /// the parent has explicitly allowed a tool, a card that allows the same
    /// tool keeps the allow.
    #[test]
    fn a_parent_allow_and_a_card_allow_stay_allow() {
        let mut parent = delegating_parent();
        parent.tool_policy = Some(policy(&[("bash", ToolPolicy::Allow)]));
        let card = card_with_tools(Some(policy(&[("bash", ToolPolicy::Allow)])));

        let child = SessionAgent::from_card(&card, &parent, None);

        assert_eq!(
            tool(&child, "bash"),
            Some(ToolPolicy::Allow),
            "narrowing takes the stricter of the two, so equal entries survive"
        );
    }

    #[test]
    fn a_card_without_a_tools_block_still_inherits_the_parents_restrictions() {
        let mut parent = delegating_parent();
        parent.tool_policy = Some(policy(&[
            ("bash", ToolPolicy::Deny),
            ("read_file", ToolPolicy::Allow),
        ]));
        let card = card_with_tools(None);

        let child = SessionAgent::from_card(&card, &parent, None);

        assert_eq!(tool(&child, "bash"), Some(ToolPolicy::Deny));
        assert_eq!(
            tool(&child, "read_file"),
            None,
            "the parent's blanket allow is not a permission the child's card asked for"
        );
    }

    #[test]
    fn a_card_deny_applies_where_the_parent_is_silent() {
        let card = card_with_tools(Some(policy(&[("bash", ToolPolicy::Deny)])));

        let child = SessionAgent::from_card(&card, &delegating_parent(), None);

        assert_eq!(tool(&child, "bash"), Some(ToolPolicy::Deny));
    }

    #[test]
    fn a_card_ask_applies_where_the_parent_is_silent() {
        let card = card_with_tools(Some(policy(&[("bash", ToolPolicy::Ask)])));

        let child = SessionAgent::from_card(&card, &delegating_parent(), None);

        assert_eq!(tool(&child, "bash"), Some(ToolPolicy::Ask));
    }

    /// The other arm of `from_card`: `session.create` with an agent card, whose
    /// base is the config-derived defaults, not a parent session. There is no
    /// ceiling to narrow under, so the operator's card is authoritative.
    #[test]
    fn a_card_creating_a_session_keeps_its_own_allow() {
        let card = card_with_tools(Some(policy(&[("bash", ToolPolicy::Allow)])));

        let child = SessionAgent::from_card(&card, &config_defaults(), None);

        assert_eq!(tool(&child, "bash"), Some(ToolPolicy::Allow));
    }

    #[test]
    fn a_delegated_card_cannot_add_an_mcp_server_the_parent_lacks() {
        let mut parent = delegating_parent();
        parent.mcp_servers = vec!["notes".to_string()];
        let mut card = card_with_tools(None);
        card.mcp_servers = vec!["notes".to_string(), "shell".to_string()];

        let child = SessionAgent::from_card(&card, &parent, None);

        assert_eq!(
            child.mcp_servers,
            vec!["notes".to_string()],
            "gateway tools carry names no tool policy mentions; adding a server \
             would be a way around the policy narrowing"
        );
    }

    #[test]
    fn a_card_creating_a_session_keeps_its_own_mcp_servers() {
        let mut base = config_defaults();
        base.mcp_servers = vec!["notes".to_string()];
        let mut card = card_with_tools(None);
        card.mcp_servers = vec!["shell".to_string()];

        let child = SessionAgent::from_card(&card, &base, None);

        assert_eq!(child.mcp_servers, vec!["shell".to_string()]);
    }
}
