//! Integration tests for agent factory

#![allow(clippy::field_reassign_with_default)]

//!
//! Tests the unified agent initialization for both ACP and internal agents.

use crucible_cli::factories::{AgentInitParams, AgentType};
use crucible_config::CliAppConfig;

#[test]
fn test_agent_init_params_builder() {
    let params = AgentInitParams::new()
        .with_type(AgentType::Internal)
        .with_provider("local".to_string())
        .with_read_only(false)
        .with_max_context_tokens(8192);

    assert_eq!(params.agent_type, Some(AgentType::Internal));
    assert_eq!(params.provider_key, Some("local".to_string()));
    assert!(!params.read_only);
    assert_eq!(params.max_context_tokens, Some(8192));
}

#[test]
fn test_agent_init_params_default() {
    let params = AgentInitParams::default();
    assert_eq!(params.agent_type, None);
    assert_eq!(params.agent_name, None);
    assert_eq!(params.provider_key, None);
    assert!(!params.read_only);
    assert_eq!(params.max_context_tokens, None);
}

#[test]
fn test_agent_type_default() {
    // Default is Internal (Crucible's built-in Rig-based agents)
    assert_eq!(AgentType::default(), AgentType::Internal);
}

#[test]
fn test_agent_types_equality() {
    assert_eq!(AgentType::Acp, AgentType::Acp);
    assert_eq!(AgentType::Internal, AgentType::Internal);
    assert_ne!(AgentType::Acp, AgentType::Internal);
}

#[tokio::test]
#[ignore = "Rig uses config.chat.provider, not params.provider_key - test obsolete"]
async fn test_create_internal_agent_with_invalid_provider() {
    // NOTE: This test is obsolete after switching to Rig-only internal agents.
    // Rig reads the provider from config.chat.provider, not from params.provider_key.
    use crucible_cli::factories::create_internal_agent;

    let config = CliAppConfig::default();
    let params = AgentInitParams::new().with_provider("nonexistent_provider");

    let result = create_internal_agent(&config, params).await;

    // With Rig, this would succeed (ignores params.provider_key)
    assert!(result.is_err());
}

#[test]
fn test_agent_init_params_with_agent_name() {
    let params = AgentInitParams::new()
        .with_type(AgentType::Acp)
        .with_agent_name("claude-code");

    assert_eq!(params.agent_type, Some(AgentType::Acp));
    assert_eq!(params.agent_name, Some("claude-code".to_string()));
}

// Configuration edge case tests

#[test]
fn test_builder_chaining_all_options() {
    let params = AgentInitParams::new()
        .with_type(AgentType::Internal)
        .with_agent_name("test-agent")
        .with_provider("ollama")
        .with_read_only(false)
        .with_max_context_tokens(16384);

    assert_eq!(params.agent_type, Some(AgentType::Internal));
    assert_eq!(params.agent_name, Some("test-agent".to_string()));
    assert_eq!(params.provider_key, Some("ollama".to_string()));
    assert!(!params.read_only);
    assert_eq!(params.max_context_tokens, Some(16384));
}

#[test]
fn test_builder_override_values() {
    let params = AgentInitParams::new()
        .with_type(AgentType::Acp)
        .with_type(AgentType::Internal) // Override
        .with_provider("openai")
        .with_provider("ollama"); // Override

    // Last value should win
    assert_eq!(params.agent_type, Some(AgentType::Internal));
    assert_eq!(params.provider_key, Some("ollama".to_string()));
}

#[test]
fn test_optional_helper_methods() {
    let params = AgentInitParams::new()
        .with_agent_name_opt(Some("test".to_string()))
        .with_provider_opt(None);

    assert_eq!(params.agent_name, Some("test".to_string()));
    assert_eq!(params.provider_key, None);
}

#[test]
fn test_optional_helper_with_none() {
    let params = AgentInitParams::new()
        .with_agent_name("initial")
        .with_agent_name_opt(None); // Should override to None

    assert_eq!(params.agent_name, None);
}

#[test]
fn test_max_context_tokens_boundary_values() {
    // Zero tokens
    let params_zero = AgentInitParams::new().with_max_context_tokens(0);
    assert_eq!(params_zero.max_context_tokens, Some(0));

    // Large value (8M tokens - GPT-4 territory)
    let params_large = AgentInitParams::new().with_max_context_tokens(8_000_000);
    assert_eq!(params_large.max_context_tokens, Some(8_000_000));
}

#[test]
fn test_read_only_toggle() {
    // Default should be read-write (normal mode)
    let default_params = AgentInitParams::default();
    assert!(!default_params.read_only);

    // Explicit true
    let read_only_params = AgentInitParams::new().with_read_only(true);
    assert!(read_only_params.read_only);

    // Toggle back and forth
    let toggled = AgentInitParams::new()
        .with_read_only(true)
        .with_read_only(false)
        .with_read_only(true);
    assert!(toggled.read_only);
}

#[test]
fn test_empty_string_agent_name() {
    let params = AgentInitParams::new().with_agent_name("");

    assert_eq!(params.agent_name, Some("".to_string()));
}

#[test]
fn test_empty_string_provider() {
    let params = AgentInitParams::new().with_provider("");

    assert_eq!(params.provider_key, Some("".to_string()));
}

#[test]
fn test_agent_type_copy_trait() {
    let agent_type = AgentType::Internal;
    let copied = agent_type;

    // Both should still be valid (Copy trait)
    assert_eq!(agent_type, AgentType::Internal);
    assert_eq!(copied, AgentType::Internal);
}

#[test]
fn test_agent_type_debug_format() {
    let internal = AgentType::Internal;
    let acp = AgentType::Acp;

    // Debug format should include type name
    let internal_debug = format!("{:?}", internal);
    let acp_debug = format!("{:?}", acp);

    assert!(internal_debug.contains("Internal"));
    assert!(acp_debug.contains("Acp"));
}
