//! Agent and skills RPC methods
//!
//! Methods for managing agents, skills, and models.

use anyhow::Result;
use std::path::Path;
use std::time::Duration;

use super::session::SessionIdRequest;
use super::types::{extract_string_array, EmptyParams, NameRequest};
use super::DaemonClient;

/// Request for `session.configure_agent`.
///
/// `agent` stays a `Value` on purpose: the handler answers a distinct
/// `Invalid agent config: {e}` for an `agent` that is not a `SessionAgent`,
/// and typing the field here would fold that into the generic params error.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionConfigureAgentRequest {
    pub session_id: String,
    pub agent: serde_json::Value,
}

/// Request for `session.switch_model`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionSwitchModelRequest {
    pub session_id: String,
    pub model_id: String,
}

/// Request for `session.set_mode`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetModeRequest {
    pub session_id: String,
    pub mode_id: String,
}

/// Request for `session.set_thinking_budget`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetThinkingBudgetRequest {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<i64>,
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListAllModelsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kiln_path: Option<String>,
}

/// Request for `embeddings.models`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingModelsRequest {
    /// A name to resolve through the catalog, in any form the catalog accepts.
    ///
    /// The answer carries the canonical form as `resolved`, and an unknown
    /// name is an error that names the near entries. The caller therefore
    /// holds no matcher of its own, so no second matcher can drift from the
    /// catalog's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Fetch `model` into the cache before the daemon answers.
    ///
    /// One method, two questions, because the answer to the second is the
    /// first asked again: after a download the caller wants the row, and the
    /// row is what says where the files are.
    #[serde(default)]
    pub download: bool,
}

/// One local embedding model, as `embeddings.models` reports it.
///
/// The daemon owns the catalog because it links fastembed and holds the model
/// cache. This struct is the projection the CLI renders; it carries no
/// fastembed type, so a build without that feature still compiles.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingModelRow {
    /// The name to write in the config file.
    pub name: String,
    /// The width of the vector.
    pub dimensions: usize,
    /// The parameter count in millions, or `None` for a model Crucible does
    /// not curate.
    pub parameter_millions: Option<u32>,
    /// The longest input the model accepts, or `None` for a model Crucible
    /// does not curate.
    pub max_input_tokens: Option<u32>,
    /// The MTEB v1 English retrieval score, or `None` when nobody published
    /// one. Never a guess.
    pub retrieval_score: Option<f32>,
    /// Whether Crucible curates this model, so `download` can fetch it.
    pub curated: bool,
    /// One sentence on why to pick this model, or why not.
    pub note: String,
    /// Whether the files are already in the cache.
    pub downloaded: bool,
}

/// The answer to `embeddings.models`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingCatalog {
    /// Every model the daemon can run, ordered by name. Empty when the daemon
    /// was built without the `fastembed` feature.
    #[serde(default)]
    pub models: Vec<EmbeddingModelRow>,
    /// The model the daemon's own config names, when it names one.
    #[serde(default)]
    pub configured: Option<String>,
    /// The directory the daemon reads and writes models in.
    #[serde(default)]
    pub cache_dir: Option<String>,
    /// The canonical catalog name of the model the request named.
    #[serde(default)]
    pub resolved: Option<String>,
    /// The directory the requested download landed in.
    #[serde(default)]
    pub downloaded_to: Option<String>,
    /// The bytes that download occupies.
    ///
    /// Only for the model just fetched. Every row carried this once, which
    /// cost a directory walk per model on a listing that never prints it.
    #[serde(default)]
    pub downloaded_bytes: Option<u64>,
}

/// Request for `providers.list` (no active session required).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListProvidersRequest {
    #[serde(default)]
    pub kiln_path: Option<String>,
    /// `false` skips per-provider model discovery (which dials endpoints).
    /// Omitted means `true` for backward compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_models: Option<bool>,
}

/// Request for `session.connect_kiln` / `session.disconnect_kiln`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionKilnRequest {
    pub session_id: String,
    /// The kiln's registry NAME. It was `kiln_path` — a directory the caller
    /// chose — and that is the door the registration floor now stands in front
    /// of: a path here would attach a kiln nobody registered.
    ///
    /// Typed, not a `String`: both callers already hold a validated
    /// [`KilnName`] and were widening it back with `to_string()` for one hop.
    /// `KilnName` serializes as its inner string, so the wire is unchanged.
    ///
    /// [`KilnName`]: crucible_core::config::KilnName
    pub kiln: crucible_core::config::KilnName,
}

/// Request for `session.set_workspace`. `workspace: None` detaches.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionSetWorkspaceRequest {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
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

    /// Attach a kiln to a session's set. Returns the updated scope
    /// `{session_id, kilns, workspace}`.
    pub async fn session_connect_kiln(
        &self,
        session_id: &str,
        kiln: &crucible_core::config::KilnName,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "session.connect_kiln",
            SessionKilnRequest {
                session_id: session_id.to_string(),
                kiln: kiln.clone(),
            },
        )
        .await
    }

    /// Detach a kiln from the session's set. Any member may be detached — the
    /// set is flat, including the kiln the session was created with.
    pub async fn session_disconnect_kiln(
        &self,
        session_id: &str,
        kiln: &crucible_core::config::KilnName,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "session.disconnect_kiln",
            SessionKilnRequest {
                session_id: session_id.to_string(),
                kiln: kiln.clone(),
            },
        )
        .await
    }

    /// Set (Some) or detach (None) the session's workspace.
    pub async fn session_set_workspace(
        &self,
        session_id: &str,
        workspace: Option<&Path>,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "session.set_workspace",
            SessionSetWorkspaceRequest {
                session_id: session_id.to_string(),
                workspace: workspace.map(|p| p.to_string_lossy().to_string()),
            },
        )
        .await
    }

    pub async fn session_set_mode(&self, session_id: &str, mode_id: &str) -> Result<()> {
        self.typed_unit_call_with_retry(
            "session.set_mode",
            SessionSetModeRequest {
                session_id: session_id.to_string(),
                mode_id: mode_id.to_string(),
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

    /// The modes this session may enter, and the one it is in.
    ///
    /// The list is per-session because it is resolved from the session's Lua
    /// registry — two sessions in different projects can offer different modes.
    /// The settings this session's external agent advertised for itself.
    pub async fn session_list_agent_options(&self, session_id: &str) -> Result<serde_json::Value> {
        self.typed_call_with_retry(
            "session.list_agent_options",
            SessionIdRequest {
                session_id: session_id.to_string(),
            },
        )
        .await
    }

    /// Set one of the agent's own settings.
    pub async fn session_set_agent_option(
        &self,
        session_id: &str,
        option_id: &str,
        value: &str,
    ) -> Result<()> {
        #[derive(serde::Serialize)]
        struct Params<'a> {
            session_id: &'a str,
            option_id: &'a str,
            value: &'a str,
        }
        let _: serde_json::Value = self
            .typed_call_with_retry(
                "session.set_agent_option",
                Params {
                    session_id,
                    option_id,
                    value,
                },
            )
            .await?;
        Ok(())
    }

    /// Which settings this session can change.
    pub async fn session_list_knobs(
        &self,
        session_id: &str,
    ) -> Result<crucible_core::types::SessionKnobSupport> {
        self.typed_call_with_retry(
            "session.list_knobs",
            SessionIdRequest {
                session_id: session_id.to_string(),
            },
        )
        .await
    }

    pub async fn session_list_modes(
        &self,
        session_id: &str,
    ) -> Result<crucible_core::types::mode::SessionModes> {
        self.typed_call_with_retry(
            "session.list_modes",
            SessionIdRequest {
                session_id: session_id.to_string(),
            },
        )
        .await
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

    /// The local embedding catalog, and what of it is already on disk.
    ///
    /// `model` names one model to resolve through the catalog; `download`
    /// fetches it first. A fetch reads a few hundred megabytes over the
    /// network, so the timeout is the download's, not the default request's.
    pub async fn embedding_models(
        &self,
        model: Option<&str>,
        download: bool,
    ) -> Result<EmbeddingCatalog> {
        /// Long enough for the largest model on a slow link.
        const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(1800);

        let params = EmbeddingModelsRequest {
            model: model.map(str::to_string),
            download,
        };
        if download {
            self.typed_call_with_timeout("embeddings.models", params, DOWNLOAD_TIMEOUT)
                .await
        } else {
            self.typed_call_with_retry("embeddings.models", params)
                .await
        }
    }

    /// List all available providers without requiring an active session.
    pub async fn list_providers(
        &self,
        kiln_path: Option<&std::path::Path>,
    ) -> Result<Vec<crate::agent_manager::providers::ProviderInfo>> {
        self.list_providers_inner(kiln_path, None).await
    }

    /// Like [`Self::list_providers`], but skips model discovery — no endpoint
    /// is dialed, so this returns fast even when a provider is down. The
    /// `models` field comes back empty and `available` is not meaningful;
    /// use this when only the *existence* of providers matters (preflight).
    pub async fn list_providers_summary(
        &self,
        kiln_path: Option<&std::path::Path>,
    ) -> Result<Vec<crate::agent_manager::providers::ProviderInfo>> {
        self.list_providers_inner(kiln_path, Some(false)).await
    }

    async fn list_providers_inner(
        &self,
        kiln_path: Option<&std::path::Path>,
        include_models: Option<bool>,
    ) -> Result<Vec<crate::agent_manager::providers::ProviderInfo>> {
        let result: serde_json::Value = self
            .typed_call_with_retry(
                "providers.list",
                ListProvidersRequest {
                    kiln_path: kiln_path.map(|p| p.to_string_lossy().to_string()),
                    include_models,
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

    pub async fn session_get_mode(&self, session_id: &str) -> Result<Option<String>> {
        self.get_session_option("session.get_mode", session_id, "mode", |v| {
            v.as_str().map(|s| s.to_string())
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

    /// List the agent cards a session started from `workspace` would resolve.
    pub async fn agents_list_cards(
        &self,
        workspace: &Path,
        kiln_path: Option<&Path>,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            "agents.list_cards",
            super::types::AgentsListCardsRequest {
                workspace: workspace.to_string_lossy().to_string(),
                kiln_path: kiln_path.map(|p| p.to_string_lossy().to_string()),
            },
        )
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
