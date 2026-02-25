//! Trust resolution utilities for kiln classification lookups.

use std::path::Path;

use crucible_config::{DataClassification, LlmConfig, TrustLevel, WorkspaceConfig};
use crucible_core::session::SessionAgent;

/// Resolve the data classification for a kiln by reading the workspace config.
///
/// Returns `None` if:
/// - The workspace.toml does not exist
/// - The TOML is unparseable
/// - The kiln is not found in the [[kilns]] list
/// - The kiln entry has no `data_classification` set
///
/// Callers must handle the `None` case explicitly — no silent default to `Public`.
pub(crate) fn resolve_kiln_classification(
    workspace: &Path,
    kiln: &Path,
) -> Option<DataClassification> {
    let config_path = workspace.join(".crucible").join("workspace.toml");
    let content = std::fs::read_to_string(config_path).ok()?;
    let config = toml::from_str::<WorkspaceConfig>(&content).ok()?;

    let kiln_canonical = std::fs::canonicalize(kiln).ok();
    for attachment in &config.kilns {
        let attachment_path = if attachment.path.is_absolute() {
            attachment.path.clone()
        } else {
            workspace.join(&attachment.path)
        };

        let matches = match (
            &kiln_canonical,
            std::fs::canonicalize(&attachment_path).ok(),
        ) {
            (Some(kc), Some(ac)) => kc == &ac,
            _ => attachment_path == kiln,
        };

        if matches {
            return attachment.data_classification;
        }
    }

    None
}

/// Resolve the trust level for an LLM provider at runtime.
///
/// Returns the effective trust level based on the agent's provider configuration.
/// For ACP agents, defaults to Cloud trust. For configured providers, looks up
/// the trust level from the LLM config. Falls back to Cloud as the default.
pub(crate) fn resolve_provider_trust(
    agent: &SessionAgent,
    llm_config: Option<&LlmConfig>,
) -> TrustLevel {
    // ACP agents (identified by agent_name) default to Cloud trust
    if agent.agent_name.is_some() {
        return TrustLevel::Cloud;
    }
    // Try to look up provider by key in the LLM config
    if let (Some(key), Some(config)) = (&agent.provider_key, llm_config) {
        if let Some(provider) = config.providers.get(key) {
            return provider.effective_trust_level();
        }
    }
    // Fallback: Cloud trust
    TrustLevel::Cloud
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use crucible_config::LlmProviderConfig;
    use std::collections::HashMap;
    use crucible_config::BackendType;

    fn make_test_agent(
        agent_type: &str,
        agent_name: Option<&str>,
        provider_key: Option<&str>,
    ) -> SessionAgent {
        SessionAgent {
            agent_type: agent_type.to_string(),
            agent_name: agent_name.map(|s| s.to_string()),
            provider_key: provider_key.map(|s| s.to_string()),
            provider: BackendType::Ollama,
            model: "test-model".to_string(),
            system_prompt: "You are a test agent.".to_string(),
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
        }
    }

    fn write_workspace_config(
        workspace: &std::path::Path,
        kiln_rel: &str,
        classification: Option<&str>,
    ) {
        let dir = workspace.join(".crucible");
        fs::create_dir_all(&dir).unwrap();
        let mut toml =
            format!("[workspace]\nname = \"test\"\n\n[[kilns]]\npath = \"{kiln_rel}\"\n");
        if let Some(c) = classification {
            toml.push_str(&format!("data_classification = \"{c}\"\n"));
        }
        fs::write(dir.join("workspace.toml"), toml).unwrap();
    }

    #[test]
    fn classification_from_workspace_toml_confidential() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let kiln = workspace.join("notes");
        fs::create_dir_all(&kiln).unwrap();
        write_workspace_config(&workspace, "./notes", Some("confidential"));

        let result = resolve_kiln_classification(&workspace, &kiln);
        assert_eq!(result, Some(DataClassification::Confidential));
    }

    #[test]
    fn classification_missing_config_returns_none() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let kiln = workspace.join("notes");
        fs::create_dir_all(&kiln).unwrap();
        // No .crucible/workspace.toml written

        let result = resolve_kiln_classification(&workspace, &kiln);
        assert_eq!(result, None);
    }

    #[test]
    fn classification_bad_toml_returns_none() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let kiln = workspace.join("notes");
        fs::create_dir_all(&kiln).unwrap();
        let crucible_dir = workspace.join(".crucible");
        fs::create_dir_all(&crucible_dir).unwrap();
        fs::write(
            crucible_dir.join("workspace.toml"),
            "THIS IS NOT VALID TOML !!!@@@",
        )
        .unwrap();

        let result = resolve_kiln_classification(&workspace, &kiln);
        assert_eq!(result, None);
    }

    #[test]
    fn classification_no_matching_kiln_returns_none() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let kiln = workspace.join("notes");
        let other_kiln = workspace.join("other");
        fs::create_dir_all(&kiln).unwrap();
        fs::create_dir_all(&other_kiln).unwrap();
        // Config references "other" with confidential, not our kiln
        write_workspace_config(&workspace, "./other", Some("confidential"));

        let result = resolve_kiln_classification(&workspace, &kiln);
        assert_eq!(result, None);
    }

    #[test]
    fn classification_kiln_found_but_no_classification_returns_none() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let kiln = workspace.join("notes");
        fs::create_dir_all(&kiln).unwrap();
        // Config has the kiln but no data_classification field
        write_workspace_config(&workspace, "./notes", None);

        let result = resolve_kiln_classification(&workspace, &kiln);
        assert_eq!(result, None);
    }

    #[test]
    fn classification_explicit_public_returns_some_public() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let kiln = workspace.join("notes");
        fs::create_dir_all(&kiln).unwrap();
        write_workspace_config(&workspace, "./notes", Some("public"));

        let result = resolve_kiln_classification(&workspace, &kiln);
        assert_eq!(result, Some(DataClassification::Public));
    }

    // ===== resolve_provider_trust Tests =====

    #[test]
    fn provider_trust_acp_agent_returns_cloud() {
        // ACP agents (with agent_name set) always return Cloud trust
        let agent = make_test_agent("acp", Some("claude"), None);
        let result = resolve_provider_trust(&agent, None);
        assert_eq!(result, TrustLevel::Cloud);
    }

    #[test]
    fn provider_trust_configured_provider_returns_explicit_level() {
        // When provider_key exists and provider is found in config,
        // return the provider's effective trust level
        let mut providers = HashMap::new();
        let provider_config = LlmProviderConfig {
            provider_type: BackendType::Ollama,
            endpoint: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level: Some(TrustLevel::Local),
        };
        providers.insert("local-ollama".to_string(), provider_config);

        let llm_config = LlmConfig {
            default: None,
            providers,
        };

        let agent = make_test_agent("internal", None, Some("local-ollama"));
        let result = resolve_provider_trust(&agent, Some(&llm_config));
        assert_eq!(result, TrustLevel::Local);
    }

    #[test]
    fn provider_trust_fallback_returns_cloud() {
        // Fallback case: no agent_name, and either no provider_key or provider not found
        let agent = make_test_agent("internal", None, Some("nonexistent-provider"));
        let llm_config = LlmConfig {
            default: None,
            providers: HashMap::new(),
        };
        let result = resolve_provider_trust(&agent, Some(&llm_config));
        assert_eq!(result, TrustLevel::Cloud);
    }

    #[test]
    fn provider_trust_no_provider_key_returns_cloud() {
        // Fallback: no provider_key set
        let agent = make_test_agent("internal", None, None);
        let llm_config = LlmConfig {
            default: None,
            providers: HashMap::new(),
        };
        let result = resolve_provider_trust(&agent, Some(&llm_config));
        assert_eq!(result, TrustLevel::Cloud);
    }

    #[test]
    fn provider_trust_no_llm_config_returns_cloud() {
        // Fallback: no LlmConfig provided
        let agent = make_test_agent("internal", None, Some("ollama"));
        let result = resolve_provider_trust(&agent, None);
        assert_eq!(result, TrustLevel::Cloud);
    }
}
