use super::super::agent::SessionAgent;
use super::super::config::{
    default_precognition_results, default_validation_retries, ContextStrategy, OutputValidation,
};
use crate::config::BackendType;
use std::collections::HashMap;

#[test]
fn test_session_agent_serialization() {
    let agent = SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: BackendType::Ollama,
        model: "llama3.2".to_string(),
        system_prompt: "You are a helpful assistant.".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(4096),
        max_context_tokens: Some(8192),
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: vec!["filesystem".to_string()],
        agent_card_name: Some("default".to_string()),
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
    };

    let json = serde_json::to_string(&agent).unwrap();
    assert!(json.contains("\"agent_type\":\"internal\""));
    assert!(json.contains("\"model\":\"llama3.2\""));
    assert!(json.contains("\"temperature\":0.7"));

    let parsed: SessionAgent = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.model, "llama3.2");
    assert_eq!(parsed.temperature, Some(0.7));
    assert_eq!(parsed.mcp_servers, vec!["filesystem"]);
}

// =============================================================================
// SessionAgent Typed Provider Tests (TDD)
// =============================================================================

#[test]
fn test_session_agent_typed_provider_serialization() {
    // SessionAgent.provider should be BackendType, serialized as lowercase string
    let agent = SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: BackendType::Ollama,
        model: "llama3.2".to_string(),
        system_prompt: "Test".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: Vec::new(),
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
    };

    let json = serde_json::to_string(&agent).unwrap();
    assert!(json.contains("\"provider\":\"ollama\""));
}

#[test]
fn test_session_agent_typed_provider_deserialization() {
    // JSON with "provider": "ollama" should deserialize to BackendType::Ollama
    let json = r#"{
        "agent_type": "internal",
        "provider": "ollama",
        "model": "llama3.2",
        "system_prompt": "Test"
    }"#;

    let agent: SessionAgent = serde_json::from_str(json).unwrap();
    assert_eq!(agent.provider, BackendType::Ollama);
}

#[test]
fn test_session_agent_typed_provider_round_trip() {
    // Round-trip: BackendType → JSON → BackendType
    let original = SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("openai".to_string()),
        provider: BackendType::OpenAI,
        model: "gpt-4o".to_string(),
        system_prompt: "Test".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: Vec::new(),
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
    };

    let json = serde_json::to_string(&original).unwrap();
    let parsed: SessionAgent = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.provider, BackendType::OpenAI);
    assert_eq!(parsed.model, "gpt-4o");
}

// =============================================================================
// SessionAgent Profile Construction Tests (TDD - RED phase)
// =============================================================================

#[test]
fn test_session_agent_with_capabilities() {
    // SessionAgent should serialize/deserialize with capabilities field
    let agent = SessionAgent {
        agent_type: "acp".to_string(),
        agent_name: Some("opencode".to_string()),
        provider_key: None,
        provider: BackendType::Custom,
        model: "opencode".to_string(),
        system_prompt: "You are helpful.".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: Vec::new(),
        agent_card_name: None,
        capabilities: Some(vec!["search".to_string(), "write".to_string()]),
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
    };

    let json = serde_json::to_string(&agent).unwrap();
    assert!(json.contains("\"capabilities\""));
    assert!(json.contains("\"search\""));

    let parsed: SessionAgent = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed.capabilities,
        Some(vec!["search".to_string(), "write".to_string()])
    );
}

#[test]
fn test_session_agent_with_agent_description() {
    // SessionAgent should serialize/deserialize with agent_description field
    let agent = SessionAgent {
        agent_type: "acp".to_string(),
        agent_name: Some("claude".to_string()),
        provider_key: None,
        provider: BackendType::Custom,
        model: "claude".to_string(),
        system_prompt: "You are helpful.".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: Vec::new(),
        agent_card_name: None,
        capabilities: None,
        agent_description: Some("Claude AI assistant".to_string()),
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
    };

    let json = serde_json::to_string(&agent).unwrap();
    assert!(json.contains("\"agent_description\""));
    assert!(json.contains("Claude AI assistant"));

    let parsed: SessionAgent = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed.agent_description,
        Some("Claude AI assistant".to_string())
    );
}

#[test]
fn test_session_agent_with_delegation_config() {
    // SessionAgent should serialize/deserialize with delegation_config field
    use crate::config::DelegationConfig;

    let delegation = DelegationConfig {
        enabled: true,
        max_depth: 2,
        allowed_targets: Some(vec!["tool-agent".to_string()]),
        result_max_bytes: 102400,
        max_concurrent_delegations: 3,
            timeout_secs: 300,
    };

    let agent = SessionAgent {
        agent_type: "acp".to_string(),
        agent_name: Some("delegating-agent".to_string()),
        provider_key: None,
        provider: BackendType::Custom,
        model: "delegating-agent".to_string(),
        system_prompt: "You can delegate.".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: Vec::new(),
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: Some(delegation),
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
    };

    let json = serde_json::to_string(&agent).unwrap();
    assert!(json.contains("\"delegation_config\""));
    assert!(json.contains("\"enabled\":true"));
    assert!(json.contains("\"max_depth\":2"));

    let parsed: SessionAgent = serde_json::from_str(&json).unwrap();
    assert!(parsed.delegation_config.is_some());
    let parsed_delegation = parsed.delegation_config.unwrap();
    assert!(parsed_delegation.enabled);
    assert_eq!(parsed_delegation.max_depth, 2);
}

#[test]
fn test_session_agent_backward_compat_without_new_fields() {
    // Old JSON without new fields should deserialize correctly
    let old_json = r#"{
        "agent_type": "internal",
        "provider_key": "ollama",
        "provider": "ollama",
        "model": "llama3.2",
        "system_prompt": "You are helpful.",
        "temperature": 0.7,
        "max_tokens": 4096,
        "env_overrides": {},
        "mcp_servers": []
    }"#;

    let agent: SessionAgent = serde_json::from_str(old_json).unwrap();
    assert_eq!(agent.model, "llama3.2");
    assert_eq!(agent.temperature, Some(0.7));
    assert!(agent.capabilities.is_none());
    assert!(agent.agent_description.is_none());
    assert!(agent.delegation_config.is_none());
}

#[test]
fn test_session_agent_round_trip_with_all_fields() {
    // SessionAgent → JSON → SessionAgent should preserve all fields
    use crate::config::DelegationConfig;

    let delegation = DelegationConfig {
        enabled: true,
        max_depth: 3,
        allowed_targets: Some(vec!["agent1".to_string(), "agent2".to_string()]),
        result_max_bytes: 204800,
        max_concurrent_delegations: 3,
            timeout_secs: 300,
    };

    let original = SessionAgent {
        agent_type: "acp".to_string(),
        agent_name: Some("full-agent".to_string()),
        provider_key: None,
        provider: BackendType::Custom,
        model: "full-agent".to_string(),
        system_prompt: "Full agent.".to_string(),
        temperature: Some(0.8),
        max_tokens: Some(8192),
        max_context_tokens: Some(16384),
        thinking_budget: Some(10000),
        endpoint: Some("http://localhost:8000".to_string()),
        env_overrides: {
            let mut map = HashMap::new();
            map.insert("API_KEY".to_string(), "secret".to_string());
            map
        },
        mcp_servers: vec!["filesystem".to_string(), "web".to_string()],
        agent_card_name: Some("full-card".to_string()),
        capabilities: Some(vec![
            "search".to_string(),
            "write".to_string(),
            "execute".to_string(),
        ]),
        agent_description: Some("A full-featured agent".to_string()),
        delegation_config: Some(delegation),
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
    };

    let json = serde_json::to_string(&original).unwrap();
    let parsed: SessionAgent = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.agent_type, original.agent_type);
    assert_eq!(parsed.agent_name, original.agent_name);
    assert_eq!(parsed.model, original.model);
    assert_eq!(parsed.capabilities, original.capabilities);
    assert_eq!(parsed.agent_description, original.agent_description);
    assert!(parsed.delegation_config.is_some());
    let parsed_delegation = parsed.delegation_config.unwrap();
    assert!(parsed_delegation.enabled);
    assert_eq!(parsed_delegation.max_depth, 3);
}

#[test]
fn test_session_agent_from_profile_basic() {
    // SessionAgent::from_profile() should construct ACP agent from AgentProfile
    use crate::config::AgentProfile;

    let profile = AgentProfile {
        extends: Some("claude".to_string()),
        command: None,
        args: None,
        env: {
            let mut map = HashMap::new();
            map.insert("ANTHROPIC_API_KEY".to_string(), "key123".to_string());
            map
        },
        description: Some("Claude via profile".to_string()),
        capabilities: Some(vec!["chat".to_string(), "reasoning".to_string()]),
        delegation: None,
        permissions: None,
    };

    let agent = SessionAgent::from_profile(&profile, "claude-custom");

    assert_eq!(agent.agent_type, "acp");
    assert_eq!(agent.agent_name, Some("claude-custom".to_string()));
    assert_eq!(agent.provider, BackendType::Custom);
    assert_eq!(agent.model, "claude-custom");
    assert_eq!(
        agent.agent_description,
        Some("Claude via profile".to_string())
    );
    assert_eq!(
        agent.capabilities,
        Some(vec!["chat".to_string(), "reasoning".to_string()])
    );
    // env vars should be in env_overrides, not inherited from parent
    assert_eq!(
        agent.env_overrides.get("ANTHROPIC_API_KEY"),
        Some(&"key123".to_string())
    );
}

#[test]
fn test_session_agent_from_profile_env_isolation() {
    // Profile env vars should go into SessionAgent.env_overrides, parent env NOT inherited
    use crate::config::AgentProfile;

    let profile = AgentProfile {
        extends: None,
        command: None,
        args: None,
        env: {
            let mut map = HashMap::new();
            map.insert("CUSTOM_VAR".to_string(), "custom_value".to_string());
            map.insert("ANOTHER_VAR".to_string(), "another_value".to_string());
            map
        },
        description: None,
        capabilities: None,
        delegation: None,
        permissions: None,
    };

    let agent = SessionAgent::from_profile(&profile, "test-agent");

    // Only profile env vars should be in env_overrides
    assert_eq!(agent.env_overrides.len(), 2);
    assert_eq!(
        agent.env_overrides.get("CUSTOM_VAR"),
        Some(&"custom_value".to_string())
    );
    assert_eq!(
        agent.env_overrides.get("ANOTHER_VAR"),
        Some(&"another_value".to_string())
    );
}

#[test]
fn test_session_agent_from_profile_with_delegation() {
    // SessionAgent::from_profile() should include delegation config from profile
    use crate::config::{AgentProfile, DelegationConfig};

    let delegation = DelegationConfig {
        enabled: true,
        max_depth: 2,
        allowed_targets: Some(vec!["worker1".to_string(), "worker2".to_string()]),
        result_max_bytes: 102400,
        max_concurrent_delegations: 3,
            timeout_secs: 300,
    };

    let profile = AgentProfile {
        extends: Some("opencode".to_string()),
        command: None,
        args: None,
        env: HashMap::new(),
        description: Some("Delegating agent".to_string()),
        capabilities: Some(vec!["delegate".to_string()]),
        delegation: Some(delegation),
        permissions: None,
    };

    let agent = SessionAgent::from_profile(&profile, "delegator");

    assert_eq!(agent.agent_name, Some("delegator".to_string()));
    assert_eq!(
        agent.agent_description,
        Some("Delegating agent".to_string())
    );
    assert!(agent.delegation_config.is_some());
    let parsed_delegation = agent.delegation_config.unwrap();
    assert!(parsed_delegation.enabled);
    assert_eq!(parsed_delegation.max_depth, 2);
    assert_eq!(
        parsed_delegation.allowed_targets,
        Some(vec!["worker1".to_string(), "worker2".to_string()])
    );
}
