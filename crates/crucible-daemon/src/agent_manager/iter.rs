use super::*;
use std::collections::HashSet;

impl AgentManager {
    pub(super) fn iter_chat_providers(
        &self,
        classification: Option<DataClassification>,
    ) -> Vec<(String, LlmProviderConfig, String)> {
        let mut providers = Vec::new();
        let mut seen_types = HashSet::new();

        if let Some(llm_config) = &self.llm_config {
            for (key, provider_config) in &llm_config.providers {
                let backend = provider_config.provider_type;
                if !backend.supports_chat()
                    || !matches_classification(provider_config, classification)
                {
                    continue;
                }

                seen_types.insert(backend.as_str().to_string());
                providers.push((key.clone(), provider_config.clone(), "config".to_string()));
            }
        }

        for (provider_key, provider_config, source_reason) in
            self.discover_env_providers(&seen_types)
        {
            if !matches_classification(&provider_config, classification) {
                continue;
            }

            providers.push((provider_key, provider_config, source_reason));
        }

        providers
    }
}

fn matches_classification(
    provider_config: &LlmProviderConfig,
    classification: Option<DataClassification>,
) -> bool {
    classification.is_none_or(|value| provider_config.effective_trust_level().satisfies(value))
}
