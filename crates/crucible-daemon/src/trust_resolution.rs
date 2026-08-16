//! Trust resolution utilities for kiln classification lookups.

use std::path::Path;

use crucible_core::config::{read_project_config, DataClassification, LlmConfig, TrustLevel};
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
    let config = read_project_config(workspace)?;

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

/// Runtime classification for a LIVE session: the workspace's own config
/// first, then a walk up from the KILN. Session-unique scratch workspaces
/// (created for projectless sessions) carry no `.crucible` config — reading
/// only the workspace made a confidential kiln silently resolve to `None`
/// (→ Public at the trust gates), diverging from the create-time gate which
/// resolves against the kiln when no workspace is given.
pub(crate) fn resolve_session_classification(
    workspace: &Path,
    kiln: &Path,
) -> Option<DataClassification> {
    resolve_kiln_classification(workspace, kiln)
        .or_else(|| find_workspace_and_resolve_classification(kiln))
}

/// The most restrictive classification across a session's kiln set.
///
/// A session is only as shareable as its least shareable corpus, and the kiln
/// set is flat — no member's classification stands in for the rest. `None`
/// when nothing in the set is classified; callers keep their own handling of
/// that, which is not the same as `Public`.
///
/// The per-kiln resolver is the caller's, because the two that exist differ:
/// live sessions walk up from the kiln when the workspace has no config, and
/// create-time lookups do not.
pub(crate) fn most_restrictive_classification(
    kilns: &[std::path::PathBuf],
    resolve: impl Fn(&Path) -> Option<DataClassification>,
) -> Option<DataClassification> {
    fn restrictiveness(classification: DataClassification) -> u8 {
        match classification {
            DataClassification::Public => 0,
            DataClassification::Internal => 1,
            DataClassification::Confidential => 2,
        }
    }

    kilns
        .iter()
        .filter_map(|kiln| resolve(kiln))
        .max_by_key(|c| restrictiveness(*c))
}

pub fn find_workspace_and_resolve_classification(kiln: &Path) -> Option<DataClassification> {
    let mut dir = kiln.to_path_buf();
    loop {
        if dir.join(".crucible").is_dir() {
            // read_project_config handles project.toml → workspace.toml fallback
            if read_project_config(&dir).is_some() {
                return resolve_kiln_classification(&dir, kiln);
            }
        }
        if !dir.pop() {
            return None;
        }
    }
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
    // An ACP agent is an external process that picks its own model, so the
    // daemon cannot vouch for where the prompt ends up: Cloud.
    //
    // Keyed on `agent_type`, the same discriminator create time uses
    // (`resolve_provider_trust_level_for_create`) and the same one
    // `agent_factory`/`switch_model`/`scope` already branch on. Keying on
    // `agent_name.is_some()` instead — as this did — made trust follow the
    // presence of a *name* rather than the actual provider: an internal
    // session that merely carried `agent_name` was reported Cloud, which is
    // strictly below `Local` in `TrustLevel`'s ordering, so a local Ollama
    // session was refused on a confidential kiln and had its confidential
    // connected kilns silently dropped from precognition.
    if agent.agent_type == "acp" {
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
    use crucible_core::config::BackendType;
    use crucible_core::config::LlmProviderConfig;
    use std::collections::HashMap;
    use std::fs;
    use tempfile::TempDir;

    fn make_test_agent(
        agent_type: &str,
        agent_name: Option<&str>,
        provider_key: Option<&str>,
    ) -> SessionAgent {
        SessionAgent {
            mode: None,
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
            precognition_results: 5,
            max_iterations: None,
            execution_timeout_secs: None,
            context_budget: None,
            context_strategy: Default::default(),
            context_window: None,
            output_validation: Default::default(),
            validation_retries: 3,
            autocompact_threshold: None,
            tool_policy: None,
        }
    }

    fn write_workspace_config(
        workspace: &std::path::Path,
        kiln_rel: &str,
        classification: Option<&str>,
    ) {
        let dir = workspace.join(".crucible");
        fs::create_dir_all(&dir).unwrap();
        let mut toml = format!("[[kilns]]\npath = \"{kiln_rel}\"\n");
        if let Some(c) = classification {
            toml.push_str(&format!("data_classification = \"{c}\"\n"));
        }
        fs::write(dir.join("project.toml"), toml).unwrap();
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
    fn session_classification_falls_back_to_kiln_for_scratch_workspaces() {
        // Regression: a session-unique scratch workspace has NO .crucible
        // config — resolving only against it read a confidential kiln as
        // None (→ Public at the delegation trust gates).
        let tmp = TempDir::new().unwrap();
        let workspace_owner = tmp.path().join("ws");
        let kiln = workspace_owner.join("notes");
        fs::create_dir_all(&kiln).unwrap();
        write_workspace_config(&workspace_owner, "./notes", Some("confidential"));

        let scratch = tmp.path().join("workspaces").join("chat-x");
        fs::create_dir_all(&scratch).unwrap();

        assert_eq!(resolve_kiln_classification(&scratch, &kiln), None);
        assert_eq!(
            resolve_session_classification(&scratch, &kiln),
            Some(DataClassification::Confidential)
        );
        // A workspace WITH its own config still wins.
        assert_eq!(
            resolve_session_classification(&workspace_owner, &kiln),
            Some(DataClassification::Confidential)
        );
    }

    #[test]
    fn classification_missing_config_returns_none() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let kiln = workspace.join("notes");
        fs::create_dir_all(&kiln).unwrap();
        // No .crucible/project.toml or workspace.toml written

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
            crucible_dir.join("project.toml"),
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

    #[test]
    fn classification_backward_compat_workspace_toml() {
        // Verify old workspace.toml format still works via read_project_config fallback
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let kiln = workspace.join("notes");
        fs::create_dir_all(&kiln).unwrap();
        let crucible_dir = workspace.join(".crucible");
        fs::create_dir_all(&crucible_dir).unwrap();
        // Write old workspace.toml format (with [workspace] section)
        let old_format = r#"[workspace]
name = "test"

[[kilns]]
path = "./notes"
data_classification = "confidential"
"#;
        fs::write(crucible_dir.join("workspace.toml"), old_format).unwrap();

        let result = resolve_kiln_classification(&workspace, &kiln);
        assert_eq!(result, Some(DataClassification::Confidential));
    }

    // ===== resolve_provider_trust Tests =====

    #[test]
    fn provider_trust_acp_agent_returns_cloud() {
        // ACP agents (agent_type "acp") always return Cloud trust
        let agent = make_test_agent("acp", Some("claude"), None);
        let result = resolve_provider_trust(&agent, None);
        assert_eq!(result, TrustLevel::Cloud);
    }

    #[test]
    fn provider_trust_follows_the_provider_not_the_agent_name() {
        // An internal session that carries `agent_name` (the deprecated
        // card alias, or a plugin that set it after create) is still an
        // internal session: its trust is its provider's. Reading the name as
        // "this is ACP" pinned it to Cloud, which is BELOW Local, so a local
        // Ollama session was refused on a confidential kiln.
        let mut providers = HashMap::new();
        providers.insert(
            "local-ollama".to_string(),
            LlmProviderConfig {
                provider_type: BackendType::Ollama,
                endpoint: None,
                default_model: None,
                temperature: None,
                max_tokens: None,
                timeout_secs: None,
                api_key: None,
                available_models: None,
                trust_level: Some(TrustLevel::Local),
                name: None,
            },
        );
        let llm_config = LlmConfig {
            default: None,
            providers,
            models: Default::default(),
        };

        let agent = make_test_agent("internal", Some("researcher"), Some("local-ollama"));
        assert_eq!(
            resolve_provider_trust(&agent, Some(&llm_config)),
            TrustLevel::Local
        );
        assert!(TrustLevel::Local.satisfies(DataClassification::Confidential));
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
            name: None,
        };
        providers.insert("local-ollama".to_string(), provider_config);

        let llm_config = LlmConfig {
            default: None,
            providers,
            models: Default::default(),
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
            models: Default::default(),
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
            models: Default::default(),
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
