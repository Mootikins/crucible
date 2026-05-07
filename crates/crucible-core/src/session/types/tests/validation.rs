use super::super::agent::SessionAgent;
use super::super::config::{
    default_precognition_results, validate_output, ContextStrategy, OutputValidation,
};
use crate::config::BackendType;
use std::collections::HashMap;

#[test]
fn test_context_strategy_display_and_parse() {
    assert_eq!(ContextStrategy::Truncate.to_string(), "truncate");
    assert_eq!(ContextStrategy::SlidingWindow.to_string(), "sliding_window");
    assert_eq!(ContextStrategy::Summarize.to_string(), "summarize");

    assert_eq!(
        "truncate".parse::<ContextStrategy>().unwrap(),
        ContextStrategy::Truncate
    );
    assert_eq!(
        "sliding_window".parse::<ContextStrategy>().unwrap(),
        ContextStrategy::SlidingWindow
    );
    assert_eq!(
        "slidingwindow".parse::<ContextStrategy>().unwrap(),
        ContextStrategy::SlidingWindow
    );
    assert_eq!(
        "summarize".parse::<ContextStrategy>().unwrap(),
        ContextStrategy::Summarize
    );
    assert_eq!(
        "SUMMARIZE".parse::<ContextStrategy>().unwrap(),
        ContextStrategy::Summarize
    );
    assert!("nonsense".parse::<ContextStrategy>().is_err());
}

#[test]
fn test_output_validation_display_and_parse() {
    assert_eq!(OutputValidation::None.to_string(), "none");
    assert_eq!(OutputValidation::Json.to_string(), "json");
    assert_eq!(
        OutputValidation::Regex("^\\{".to_string()).to_string(),
        "regex:^\\{"
    );

    assert_eq!(
        "none".parse::<OutputValidation>().unwrap(),
        OutputValidation::None
    );
    assert_eq!(
        "off".parse::<OutputValidation>().unwrap(),
        OutputValidation::None
    );
    assert_eq!(
        "json".parse::<OutputValidation>().unwrap(),
        OutputValidation::Json
    );
    assert_eq!(
        "JSON".parse::<OutputValidation>().unwrap(),
        OutputValidation::Json
    );
    assert_eq!(
        "regex:^hello".parse::<OutputValidation>().unwrap(),
        OutputValidation::Regex("^hello".to_string())
    );
    assert!("unknown".parse::<OutputValidation>().is_err());
    assert!("regex:[invalid".parse::<OutputValidation>().is_err());
}

#[test]
fn test_validate_output_none() {
    assert!(validate_output("anything", &OutputValidation::None).is_ok());
}

#[test]
fn test_validate_output_json_valid() {
    assert!(validate_output(r#"{"key": "value"}"#, &OutputValidation::Json).is_ok());
    assert!(validate_output("42", &OutputValidation::Json).is_ok());
    assert!(validate_output(r#"[1, 2, 3]"#, &OutputValidation::Json).is_ok());
}

#[test]
fn test_validate_output_json_invalid() {
    let result = validate_output("not json at all", &OutputValidation::Json);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid JSON"));
}

#[test]
fn test_validate_output_regex_match() {
    let validation = OutputValidation::Regex("^hello".to_string());
    assert!(validate_output("hello world", &validation).is_ok());
}

#[test]
fn test_validate_output_regex_no_match() {
    let validation = OutputValidation::Regex("^hello".to_string());
    let result = validate_output("goodbye world", &validation);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("does not match pattern"));
}

#[test]
fn test_validate_output_regex_invalid_pattern() {
    let validation = OutputValidation::Regex("[invalid".to_string());
    let result = validate_output("anything", &validation);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid regex pattern"));
}

#[test]
fn test_output_validation_serde_roundtrip() {
    let agent = SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: BackendType::Ollama,
        model: "test".to_string(),
        system_prompt: String::new(),
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
        output_validation: OutputValidation::Json,
        validation_retries: 5,
        autocompact_threshold: None,
    };

    let json = serde_json::to_string(&agent).unwrap();
    let parsed: SessionAgent = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.output_validation, OutputValidation::Json);
    assert_eq!(parsed.validation_retries, 5);
}

#[test]
fn test_output_validation_serde_defaults() {
    // Deserializing without the fields should give defaults
    let json = r#"{
        "agent_type": "internal",
        "provider": "ollama",
        "model": "test",
        "system_prompt": ""
    }"#;
    let agent: SessionAgent = serde_json::from_str(json).unwrap();
    assert_eq!(agent.output_validation, OutputValidation::None);
    assert_eq!(agent.validation_retries, 3);
}
