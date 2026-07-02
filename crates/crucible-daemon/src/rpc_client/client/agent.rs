//! Agent and skills RPC methods
//!
//! Methods for managing agents, skills, and models.

use anyhow::Result;
use std::path::Path;

use super::session::SessionIdRequest;
use super::types::{extract_string_array, EmptyParams, NameRequest};
use super::DaemonClient;

/// Request for `session.configure_agent`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionConfigureAgentRequest {
    pub session_id: String,
    pub agent: serde_json::Value,
}

/// Request for `session.switch_model`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSwitchModelRequest {
    pub session_id: String,
    pub model_id: String,
}

/// Request for `session.set_thinking_budget`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetThinkingBudgetRequest {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<i64>,
}

/// Request for `session.set_system_prompt`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetSystemPromptRequest {
    pub session_id: String,
    pub system_prompt: String,
}

/// Request for `session.set_precognition`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetPrecognitionRequest {
    pub session_id: String,
    pub precognition_enabled: bool,
}

/// Request for `session.set_precognition_results`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetPrecognitionResultsRequest {
    pub session_id: String,
    pub precognition_results: usize,
}

/// Request for `session.undo`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionUndoRequest {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}

/// Request for `session.set_temperature`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetTemperatureRequest {
    pub session_id: String,
    pub temperature: f64,
}

/// Request for `session.set_max_tokens`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetMaxTokensRequest {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

/// Request for `session.set_max_iterations`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetMaxIterationsRequest {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
}

/// Request for `session.set_execution_timeout`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetExecutionTimeoutRequest {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

/// Request for `session.set_context_budget`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetContextBudgetRequest {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_budget: Option<usize>,
}

/// Request for `session.set_context_strategy`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetContextStrategyRequest {
    pub session_id: String,
    pub context_strategy: String,
}

/// Request for `session.set_context_window`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetContextWindowRequest {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<usize>,
}

/// Request for `session.set_output_validation`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetOutputValidationRequest {
    pub session_id: String,
    pub output_validation: String,
}

/// Request for `session.set_validation_retries`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetValidationRetriesRequest {
    pub session_id: String,
    pub validation_retries: u32,
}

/// Request for `session.set_autocompact_threshold`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetAutocompactThresholdRequest {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autocompact_threshold: Option<f32>,
}

/// Request for `models.list` (no active session required).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ListAllModelsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kiln_path: Option<String>,
}

/// Request for `providers.list` (no active session required).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ListProvidersRequest {
    pub kiln_path: Option<String>,
}

impl DaemonClient {
    pub async fn session_configure_agent(
        &self,
        session_id: &str,
        agent: &crucible_core::session::SessionAgent,
    ) -> Result<()> {
        self.typed_unit_call(
            "session.configure_agent",
            SessionConfigureAgentRequest {
                session_id: session_id.to_string(),
                agent: serde_json::to_value(agent)?,
            },
        )
        .await
    }

    pub async fn session_switch_model(&self, session_id: &str, model_id: &str) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.switch_model",
            SessionSwitchModelRequest {
                session_id: session_id.to_string(),
                model_id: model_id.to_string(),
            },
        )
        .await
    }

    pub async fn session_list_models(&self, session_id: &str) -> Result<Vec<String>> {
        let result: serde_json::Value = self
            .typed_call_with_retry(
                "session.list_models",
                SessionIdRequest {
                    session_id: session_id.to_string(),
                },
            )
            .await?;

        Ok(extract_string_array(&result, "models"))
    }

    /// List all available models without requiring an active session.
    ///
    /// If `kiln_path` is provided, the daemon resolves the kiln's data classification
    /// and filters providers whose trust level doesn't satisfy it.
    pub async fn list_all_models(&self, kiln_path: Option<&Path>) -> Result<Vec<String>> {
        let result: serde_json::Value = self
            .typed_call_with_retry(
                "models.list",
                ListAllModelsRequest {
                    kiln_path: kiln_path.map(|p| p.to_string_lossy().to_string()),
                },
            )
            .await?;

        Ok(extract_string_array(&result, "models"))
    }

    /// List all available providers without requiring an active session.
    pub async fn list_providers(
        &self,
        kiln_path: Option<&std::path::Path>,
    ) -> Result<Vec<crate::agent_manager::providers::ProviderInfo>> {
        let result: serde_json::Value = self
            .typed_call_with_retry(
                "providers.list",
                ListProvidersRequest {
                    kiln_path: kiln_path.map(|p| p.to_string_lossy().to_string()),
                },
            )
            .await?;
        let providers = result["providers"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        Ok(providers)
    }

    /// Set the thinking budget for a session's agent.
    ///
    /// The thinking budget controls reasoning token allocation for thinking models
    /// (e.g., Qwen, DeepSeek R1):
    /// - `None` - Use model's default behavior
    /// - `Some(-1)` - Unlimited thinking tokens
    /// - `Some(0)` - Disable thinking/reasoning
    /// - `Some(n)` where n > 0 - Maximum thinking tokens
    ///
    /// Changes take effect on the next message. Invalidates cached agent handles.
    pub async fn session_set_thinking_budget(
        &self,
        session_id: &str,
        budget: Option<i64>,
    ) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.set_thinking_budget",
            SessionSetThinkingBudgetRequest {
                session_id: session_id.to_string(),
                thinking_budget: budget,
            },
        )
        .await
    }

    /// Get the current thinking budget for a session's agent.
    ///
    /// Returns the configured thinking budget, or `None` if not set (using defaults).
    pub async fn session_get_thinking_budget(&self, session_id: &str) -> Result<Option<i64>> {
        self.get_session_option(
            "session.get_thinking_budget",
            session_id,
            "thinking_budget",
            |v| v.as_i64(),
        )
        .await
    }

    /// Fetch the prompt-cache aggregate for a session as a raw JSON object.
    /// Always returns a value — fields are zero before any completion has
    /// reported cache data, with `hit_rate` set to `null`.
    pub async fn session_cache_stats(&self, session_id: &str) -> Result<serde_json::Value> {
        let req = serde_json::json!({ "session_id": session_id });
        self.call_with_retry("session.cache_stats", req).await
    }

    pub async fn session_set_system_prompt(&self, session_id: &str, prompt: &str) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.set_system_prompt",
            SessionSetSystemPromptRequest {
                session_id: session_id.to_string(),
                system_prompt: prompt.to_string(),
            },
        )
        .await
    }

    pub async fn session_get_system_prompt(&self, session_id: &str) -> Result<Option<String>> {
        self.get_session_option(
            "session.get_system_prompt",
            session_id,
            "system_prompt",
            |v| v.as_str().map(|s| s.to_string()),
        )
        .await
    }

    /// Set whether Precognition (auto-RAG) is enabled for a session.
    pub async fn session_set_precognition(&self, session_id: &str, enabled: bool) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.set_precognition",
            SessionSetPrecognitionRequest {
                session_id: session_id.to_string(),
                precognition_enabled: enabled,
            },
        )
        .await
    }

    /// Get whether Precognition is enabled for a session.
    pub async fn session_get_precognition(&self, session_id: &str) -> Result<bool> {
        let result: serde_json::Value = self
            .typed_call_with_retry(
                "session.get_precognition",
                SessionIdRequest {
                    session_id: session_id.to_string(),
                },
            )
            .await?;

        let enabled = result
            .get("precognition_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        Ok(enabled)
    }

    /// Set the maximum number of Precognition search results for a session.
    pub async fn session_set_precognition_results(
        &self,
        session_id: &str,
        count: usize,
    ) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.set_precognition_results",
            SessionSetPrecognitionResultsRequest {
                session_id: session_id.to_string(),
                precognition_results: count,
            },
        )
        .await
    }

    /// Get the maximum number of Precognition search results for a session.
    pub async fn session_get_precognition_results(
        &self,
        session_id: &str,
    ) -> Result<Option<usize>> {
        self.get_session_option(
            "session.get_precognition_results",
            session_id,
            "precognition_results",
            |v| v.as_u64().map(|n| n as usize),
        )
        .await
    }

    pub async fn session_set_temperature(&self, session_id: &str, temperature: f64) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.set_temperature",
            SessionSetTemperatureRequest {
                session_id: session_id.to_string(),
                temperature,
            },
        )
        .await
    }

    pub async fn session_get_temperature(&self, session_id: &str) -> Result<Option<f64>> {
        self.get_session_option("session.get_temperature", session_id, "temperature", |v| {
            v.as_f64()
        })
        .await
    }

    pub async fn session_set_max_tokens(
        &self,
        session_id: &str,
        max_tokens: Option<u32>,
    ) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.set_max_tokens",
            SessionSetMaxTokensRequest {
                session_id: session_id.to_string(),
                max_tokens,
            },
        )
        .await
    }

    pub async fn session_get_max_tokens(&self, session_id: &str) -> Result<Option<u32>> {
        self.get_session_option("session.get_max_tokens", session_id, "max_tokens", |v| {
            v.as_u64().map(|n| n as u32)
        })
        .await
    }

    pub async fn session_set_max_iterations(
        &self,
        session_id: &str,
        max_iterations: Option<u32>,
    ) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.set_max_iterations",
            SessionSetMaxIterationsRequest {
                session_id: session_id.to_string(),
                max_iterations,
            },
        )
        .await
    }

    pub async fn session_get_max_iterations(&self, session_id: &str) -> Result<Option<u32>> {
        self.get_session_option(
            "session.get_max_iterations",
            session_id,
            "max_iterations",
            |v| v.as_u64().map(|n| n as u32),
        )
        .await
    }

    pub async fn session_set_execution_timeout(
        &self,
        session_id: &str,
        timeout_secs: Option<u64>,
    ) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.set_execution_timeout",
            SessionSetExecutionTimeoutRequest {
                session_id: session_id.to_string(),
                timeout_secs,
            },
        )
        .await
    }

    pub async fn session_get_execution_timeout(&self, session_id: &str) -> Result<Option<u64>> {
        self.get_session_option(
            "session.get_execution_timeout",
            session_id,
            "timeout_secs",
            |v| v.as_u64(),
        )
        .await
    }

    pub async fn session_set_context_budget(
        &self,
        session_id: &str,
        context_budget: Option<usize>,
    ) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.set_context_budget",
            SessionSetContextBudgetRequest {
                session_id: session_id.to_string(),
                context_budget,
            },
        )
        .await
    }

    pub async fn session_get_context_budget(&self, session_id: &str) -> Result<Option<usize>> {
        self.get_session_option(
            "session.get_context_budget",
            session_id,
            "context_budget",
            |v| v.as_u64().map(|n| n as usize),
        )
        .await
    }

    pub async fn session_set_autocompact_threshold(
        &self,
        session_id: &str,
        threshold: Option<f32>,
    ) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.set_autocompact_threshold",
            SessionSetAutocompactThresholdRequest {
                session_id: session_id.to_string(),
                autocompact_threshold: threshold,
            },
        )
        .await
    }

    pub async fn session_get_autocompact_threshold(&self, session_id: &str) -> Result<Option<f32>> {
        self.get_session_option(
            "session.get_autocompact_threshold",
            session_id,
            "autocompact_threshold",
            |v| v.as_f64().map(|n| n as f32),
        )
        .await
    }

    pub async fn session_set_context_strategy(
        &self,
        session_id: &str,
        strategy: &str,
    ) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.set_context_strategy",
            SessionSetContextStrategyRequest {
                session_id: session_id.to_string(),
                context_strategy: strategy.to_string(),
            },
        )
        .await
    }

    pub async fn session_get_context_strategy(&self, session_id: &str) -> Result<Option<String>> {
        self.get_session_option(
            "session.get_context_strategy",
            session_id,
            "context_strategy",
            |v| v.as_str().map(String::from),
        )
        .await
    }

    pub async fn session_set_context_window(
        &self,
        session_id: &str,
        context_window: Option<usize>,
    ) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.set_context_window",
            SessionSetContextWindowRequest {
                session_id: session_id.to_string(),
                context_window,
            },
        )
        .await
    }

    pub async fn session_get_context_window(&self, session_id: &str) -> Result<Option<usize>> {
        self.get_session_option(
            "session.get_context_window",
            session_id,
            "context_window",
            |v| v.as_u64().map(|n| n as usize),
        )
        .await
    }

    pub async fn session_set_output_validation(
        &self,
        session_id: &str,
        validation: &str,
    ) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.set_output_validation",
            SessionSetOutputValidationRequest {
                session_id: session_id.to_string(),
                output_validation: validation.to_string(),
            },
        )
        .await
    }

    pub async fn session_get_output_validation(&self, session_id: &str) -> Result<Option<String>> {
        self.get_session_option(
            "session.get_output_validation",
            session_id,
            "output_validation",
            |v| v.as_str().map(String::from),
        )
        .await
    }

    pub async fn session_set_validation_retries(
        &self,
        session_id: &str,
        retries: u32,
    ) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.set_validation_retries",
            SessionSetValidationRetriesRequest {
                session_id: session_id.to_string(),
                validation_retries: retries,
            },
        )
        .await
    }

    pub async fn session_get_validation_retries(&self, session_id: &str) -> Result<Option<u32>> {
        self.get_session_option(
            "session.get_validation_retries",
            session_id,
            "validation_retries",
            |v| v.as_u64().map(|n| n as u32),
        )
        .await
    }

    /// Undo the last N agent turns for a session.
    pub async fn session_undo(
        &self,
        session_id: &str,
        count: usize,
    ) -> Result<Vec<crucible_core::types::UndoSummary>> {
        let resp: serde_json::Value = self
            .typed_call_with_retry(
                "session.undo",
                SessionUndoRequest {
                    session_id: session_id.to_string(),
                    count: Some(count),
                },
            )
            .await?;
        let undone = resp
            .get("undone")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        let summaries: Vec<crucible_core::types::UndoSummary> =
            serde_json::from_value(undone).unwrap_or_default();
        Ok(summaries)
    }

    /// Check whether a session has any turns that can be undone.
    pub async fn session_can_undo(&self, session_id: &str) -> Result<bool> {
        self.get_session_option("session.can_undo", session_id, "can_undo", |v| v.as_bool())
            .await
            .map(|opt| opt.unwrap_or(false))
    }

    /// Get the number of undoable turns for a session.
    pub async fn session_undo_depth(&self, session_id: &str) -> Result<usize> {
        self.get_session_option("session.undo_depth", session_id, "undo_depth", |v| {
            v.as_u64().map(|n| n as usize)
        })
        .await
        .map(|opt| opt.unwrap_or(0))
    }

    // =========================================================================
    // Skills Discovery RPC Methods
    // =========================================================================

    /// List discovered skills with optional scope filter.
    pub async fn skills_list(
        &self,
        kiln_path: &Path,
        scope_filter: Option<&str>,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "skills.list",
            super::types::SkillsListRequest {
                kiln_path: kiln_path.to_string_lossy().to_string(),
                scope_filter: scope_filter.map(|s| s.to_string()),
            },
        )
        .await
    }

    /// Get a single skill by name with full body.
    pub async fn skills_get(&self, name: &str, kiln_path: &Path) -> Result<serde_json::Value> {
        self.typed_call(
            "skills.get",
            super::types::SkillsGetRequest {
                name: name.to_string(),
                kiln_path: kiln_path.to_string_lossy().to_string(),
            },
        )
        .await
    }

    /// Search skills by text query (case-insensitive match on name + description).
    pub async fn skills_search(
        &self,
        query: &str,
        kiln_path: &Path,
        limit: Option<usize>,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "skills.search",
            super::types::SkillsSearchRequest {
                query: query.to_string(),
                kiln_path: kiln_path.to_string_lossy().to_string(),
                limit,
            },
        )
        .await
    }

    /// List all available agent profiles (builtins + configured).
    pub async fn agents_list_profiles(&self) -> Result<serde_json::Value> {
        self.typed_call("agents.list_profiles", EmptyParams {})
            .await
    }

    /// Resolve a named agent profile.
    pub async fn agents_resolve_profile(&self, name: &str) -> Result<serde_json::Value> {
        self.typed_call(
            "agents.resolve_profile",
            NameRequest {
                name: name.to_string(),
            },
        )
        .await
    }
}
